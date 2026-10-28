//! The egui layer: the game's truck configuration screen drawn from its own
//! layout files (`screen`), the pins over the 3D view, and a small
//! SpecSheet window for what the game's screen has no place for (truck,
//! drive, scene, messages). It renders what `specsheet-data` reports and
//! turns clicks into [`Action`]s; it holds no tuning rules.

mod screen;
mod textures;
mod theme;

use bevy::camera::visibility::RenderLayers;
use bevy::camera::CameraOutputMode;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy_egui::{
    egui, EguiContexts, EguiGlobalSettings, EguiPrimaryContextPass, PrimaryEguiContext,
};
use egui::{Align2, Color32, Id, Order, Pos2, RichText, Sense, Stroke};
use specsheet_assembly::Drive;
use specsheet_gameui::{load_screen, load_templates};
use specsheet_session::view::{Hotspot, HotspotCategory, Tab, TruckOverview};

use crate::hotspots::{opacity, open_after_click, shown};
use crate::studio::{Action, Actions, Studio};
use crate::viewer::{ModelCache, TruckRoot, ViewportInput};

use self::screen::{Click, Mapping, Prices, ScreenAssets, ScreenState};
use self::textures::UiTextures;
use self::theme::{color, PALETTE};

/// The egui panels and hotspot markers, drawn through their own camera.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiState>()
            .add_systems(Startup, setup)
            .add_systems(EguiPrimaryContextPass, ui);
    }
}

const BG: Color32 = color(PALETTE.page);
const PANEL: Color32 = color(PALETTE.plate_soft);
const RAISED: Color32 = color(PALETTE.surface);
const LINE: Color32 = color(PALETTE.rule);
const TEXT: Color32 = color(PALETTE.ink);
const MUTED: Color32 = color(PALETTE.muted);
const ACCENT: Color32 = color(PALETTE.accent);
const DANGER: Color32 = color(PALETTE.danger);
const MARKER_SIZE: f32 = specsheet_theme::PIN_SIZE;
/// Reference units one wheel notch scrolls a list: about half a tile.
const WHEEL_LINE: f32 = 50.0;
/// The game screen this app reproduces (`/ui/desc/truck.sui`).
const SCREEN: &str = "truck.config";

#[derive(Resource, Default)]
pub struct UiState {
    game_dir: String,
    /// The open pin (index into the overview's hotspots): a wheel's disc
    /// pin, whose hub, cover and nut pins show while it is open.
    open_pin: Option<usize>,
    styled: bool,
    /// The game screen's layouts, loaded once a game is open.
    assets: Option<Result<ScreenAssets, String>>,
    textures: UiTextures,
    pub screen: ScreenState,
}

impl UiState {
    /// Opens a tab of the game screen: base tabs show their slot's options
    /// at once, accessory tabs wait for a pin.
    pub fn open_tab(&mut self, tab: Tab, overview: &TruckOverview, actions: &mut Actions) {
        self.screen.tab = Some(tab);
        self.screen.scroll = 0.0;
        self.open_pin = None;
        actions
            .0
            .push(Action::ShowSlot(screen::tab_slot(overview, tab)));
    }
}

fn marker_color(category: HotspotCategory) -> Color32 {
    match category {
        HotspotCategory::Base => color(PALETTE.pin_base),
        HotspotCategory::Addon => ACCENT,
        HotspotCategory::Wheel => color(PALETTE.pin_wheel),
        HotspotCategory::Interior => color(PALETTE.pin_interior),
    }
}

/// egui draws through its own full-window camera over the 3D view.
fn setup(mut commands: Commands, mut settings: ResMut<EguiGlobalSettings>) {
    settings.auto_create_primary_context = false;
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 1,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

fn style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.inactive.weak_bg_fill = RAISED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    ctx.set_visuals(visuals);
}

fn load_assets(studio: &Studio) -> Option<Result<ScreenAssets, String>> {
    let game = studio.game.as_ref()?;
    let screen = load_screen(game.fs(), SCREEN).map_err(|e| e.to_string());
    let templates = load_templates(game.fs()).map_err(|e| e.to_string());
    Some(screen.and_then(|screen| {
        let (templates, failed) = templates?;
        for path in failed {
            warn!("UI templates: {path} failed to parse");
        }
        Ok(ScreenAssets { screen, templates })
    }))
}

fn ui(
    mut contexts: EguiContexts,
    studio: Res<Studio>,
    mut actions: ResMut<Actions>,
    mut state: ResMut<UiState>,
    cache: Res<ModelCache>,
    scroll: Res<AccumulatedMouseScroll>,
    mut input: ResMut<ViewportInput>,
    camera: Single<(&mut Camera, &GlobalTransform), With<Camera3d>>,
    truck: Single<&GlobalTransform, With<TruckRoot>>,
) -> Result {
    let ctx = contexts.ctx_mut()?.clone();
    let state = &mut *state;
    if !state.styled {
        style(&ctx);
        state.styled = true;
    }
    if state.assets.is_none() {
        state.assets = load_assets(&studio);
    }
    let window = ctx.viewport_rect();
    let mapping = Mapping::new(window);
    state.screen.wheel = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y * WHEEL_LINE,
        MouseScrollUnit::Pixel => scroll.delta.y / mapping.scale,
    };
    // The game draws its screen over a full-window 3D view.
    let (mut camera, camera_transform) = camera.into_inner();
    camera.viewport = None;

    if studio.game.is_none() && !studio.busy() && !studio.discovering {
        locate(&ctx, &studio, state, &mut actions);
    }
    let loading = studio.busy() || cache.loading();
    specsheet_window(&ctx, &studio, &mut actions, state, loading);

    let mut blocking = Vec::new();
    if let (Some(overview), Some(game), Some(Ok(assets))) =
        (&studio.overview, &studio.game, &state.assets)
    {
        let clicks = egui::Area::new(Id::new("game-screen"))
            .order(Order::Background)
            .fixed_pos(window.min)
            .show(&ctx, |ui| {
                ui.set_clip_rect(window);
                let painter = ui.painter().clone();
                let mut cx = screen::Context {
                    painter: &painter,
                    game,
                    textures: &mut state.textures,
                    localization: game.localization(),
                    mapping,
                };
                let (clicks, rects) = screen::draw(
                    ui,
                    &mut cx,
                    assets,
                    overview,
                    studio.options.as_ref(),
                    Prices {
                        original: studio.original_price.unwrap_or(overview.price),
                        total: overview.price,
                    },
                    &mut state.screen,
                );
                blocking = rects;
                clicks
            })
            .inner;
        for click in clicks {
            match click {
                Click::Tab(tab) => state.open_tab(tab, overview, &mut actions),
                Click::Slot(slot) => {
                    state.screen.scroll = 0.0;
                    actions.0.push(Action::ShowSlot(Some(slot)));
                }
                Click::Select(id) => actions.0.push(Action::Select(id)),
            }
        }

        // Interior tabs look from the driver's seat.
        input.interior_view = studio.cockpit.is_some()
            && matches!(
                state.screen.tab,
                Some(Tab::Interior | Tab::InteriorAccessories)
            );
        // Pins: only on the open accessory tab, each fading as it turns
        // away from the camera (F5).
        let view_axis = *camera_transform.back();
        let tab = state.screen.tab.filter(|t| t.has_pins());
        let open = state
            .open_pin
            .filter(|&i| overview.hotspots.get(i).is_some_and(|h| h.expands));
        state.open_pin = open;
        let active = studio.options.as_ref().map(|o| &o.slot);
        for (index, hotspot) in overview.hotspots.iter().enumerate() {
            if Some(hotspot.tab) != tab || !shown(hotspot.link, open) {
                continue;
            }
            let facing = hotspot
                .facing
                .map(|f| truck.affine().transform_vector3(Vec3::from(f)));
            // From inside the cab every pin faces the camera.
            let alpha = if input.interior_view {
                1.0
            } else {
                opacity(facing, view_axis)
            };
            if alpha <= 0.0 {
                continue;
            }
            let world = truck.transform_point(Vec3::from(hotspot.position));
            let Ok(p) = camera.world_to_viewport(camera_transform, world) else {
                continue;
            };
            let pos = window.min + egui::vec2(p.x, p.y);
            if window.contains(pos) && !blocking.iter().any(|r| r.contains(pos)) {
                let pin = Pin {
                    index,
                    hotspot,
                    center: pos,
                    alpha,
                    active: Some(&hotspot.slot) == active,
                };
                marker(&ctx, &pin, state, &mut actions);
            }
        }
    } else if let (Some(Err(error)), Some(_)) = (&state.assets, &studio.game) {
        egui::Area::new(Id::new("screen-error"))
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .order(Order::Foreground)
            .show(&ctx, |ui| {
                ui.label(
                    RichText::new(format!("Truck configuration screen: {error}")).color(DANGER),
                );
            });
    }

    let pointer = ctx.input(|i| i.pointer.hover_pos());
    input.hovered = pointer.is_some_and(|pos| {
        window.contains(pos)
            && !blocking.iter().any(|r| r.contains(pos))
            && ctx
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order == Order::Background)
    });
    Ok(())
}

/// Truck, drive and scene pickers plus messages: SpecSheet's own controls,
/// kept out of the game's screen in a small collapsible window.
fn specsheet_window(
    ctx: &egui::Context,
    studio: &Studio,
    actions: &mut Actions,
    state: &mut UiState,
    loading: bool,
) {
    egui::Window::new("SpecSheet")
        .anchor(Align2::CENTER_BOTTOM, egui::vec2(0.0, -8.0))
        .collapsible(true)
        .resizable(false)
        .default_open(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let enabled = !studio.busy();
                if let Some(overview) = &studio.overview {
                    egui::ComboBox::from_id_salt("truck")
                        .selected_text(&overview.name)
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for truck in &studio.trucks {
                                let selected = truck.id == overview.truck;
                                if ui.selectable_label(selected, &truck.name).clicked() && !selected
                                {
                                    actions.0.push(Action::LoadTruck(truck.id.clone()));
                                }
                            }
                        });
                    let (label, other) = match overview.drive {
                        Drive::Left => ("Left-hand drive", Drive::Right),
                        Drive::Right => ("Right-hand drive", Drive::Left),
                    };
                    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                        actions.0.push(Action::SetDrive(other));
                    }
                }
                if let Some(current) = studio.scene.and_then(|i| studio.scenes.get(i)) {
                    egui::ComboBox::from_id_salt("scene")
                        .selected_text(&current.name)
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for (index, scene) in studio.scenes.iter().enumerate() {
                                let selected = studio.scene == Some(index);
                                if ui.selectable_label(selected, &scene.name).clicked() && !selected
                                {
                                    actions.0.push(Action::SetScene(index));
                                }
                            }
                        });
                }
                if loading {
                    ui.spinner();
                }
            });
            if let Some(game) = &studio.game {
                ui.label(
                    RichText::new(game.install().root().display().to_string())
                        .color(MUTED)
                        .small(),
                );
            }
            if let Some(error) = &studio.error {
                ui.label(RichText::new(error).color(DANGER));
            }
            let warnings = studio
                .overview
                .iter()
                .flat_map(|o| o.warnings.iter())
                .chain(state.textures.warnings.iter())
                .chain(state.assets.as_ref().and_then(|a| a.as_ref().err()))
                .take(3);
            for warning in warnings {
                ui.label(RichText::new(warning).color(MUTED).small());
            }
        });
}

fn locate(ctx: &egui::Context, studio: &Studio, state: &mut UiState, actions: &mut Actions) {
    egui::Area::new(Id::new("locate"))
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(
                    studio
                        .error
                        .as_deref()
                        .unwrap_or("Locating Euro Truck Simulator 2…"),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.game_dir)
                            .hint_text(r"C:\Steam\steamapps\common\Euro Truck Simulator 2")
                            .desired_width(360.0),
                    );
                    if ui.button("Open").clicked() {
                        actions
                            .0
                            .push(Action::OpenGame(Some(state.game_dir.clone())));
                    }
                });
            });
        });
}

/// A pin on screen.
struct Pin<'a> {
    /// Index into the overview's hotspots.
    index: usize,
    hotspot: &'a Hotspot,
    center: Pos2,
    /// 0 to 1: pins fade as they turn away from the camera.
    alpha: f32,
    /// The pin's slot is open in the sidebar.
    active: bool,
}

/// One pin: a dot in its category colour, opening its slot when clicked.
fn marker(ctx: &egui::Context, pin: &Pin, state: &mut UiState, actions: &mut Actions) {
    let hotspot = pin.hotspot;
    let size = MARKER_SIZE;
    let response = egui::Area::new(Id::new(("pin", pin.index)))
        .fixed_pos(pin.center - egui::vec2(size, size) / 2.0)
        .order(Order::Middle)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click());
            let painter = ui.painter();
            let fade = |c: Color32| c.gamma_multiply(pin.alpha);
            let fill = marker_color(hotspot.category);
            let radius = size / 2.0 * if response.hovered() { 1.15 } else { 1.0 };
            painter.circle_filled(rect.center(), radius + 4.0, fade(fill.linear_multiply(0.2)));
            if hotspot.empty {
                // Nothing fitted yet: a ring (ours, not the game's look).
                painter.circle(
                    rect.center(),
                    radius,
                    fade(BG.linear_multiply(0.6)),
                    Stroke::new(2.5, fade(fill)),
                );
            } else {
                painter.circle(
                    rect.center(),
                    radius,
                    fade(fill),
                    Stroke::new(2.0, fade(Color32::WHITE)),
                );
            }
            if pin.active {
                painter.circle_stroke(rect.center(), radius + 5.0, Stroke::new(2.0, fade(ACCENT)));
            }
            response
        })
        .inner
        .on_hover_text(&hotspot.label);
    if response.clicked() {
        state.open_pin = open_after_click(pin.index, hotspot.expands, hotspot.link, state.open_pin);
        actions.0.push(Action::ShowSlot(Some(hotspot.slot.clone())));
    }
}
