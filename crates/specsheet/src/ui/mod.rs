//! The egui layer: the game's truck configuration screen drawn from its own
//! layout files (`screen`), the pins over the 3D view, and a small
//! SpecSheet window for what the game's screen has no place for (truck,
//! drive, scene, messages). It renders what `specsheet-data` reports and
//! turns clicks into [`Action`]s; it holds no tuning rules.

mod screen;
mod textures;

use bevy::camera::visibility::RenderLayers;
use bevy::camera::CameraOutputMode;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy_egui::{
    egui, EguiContexts, EguiGlobalSettings, EguiPrimaryContextPass, PrimaryEguiContext,
};
use egui::{Align2, Color32, Id, Order, Pos2, RichText, Sense, Stroke};
use specsheet_data::assembly::Drive;
use specsheet_data::ui::{load_screen, load_templates};
use specsheet_data::view::{Hotspot, HotspotCategory, Tab, TruckOverview};

use crate::hotspots::{
    cluster, side_facing, Cluster, Projected, CLUSTER_RADIUS, TRUCK_CENTER_HEIGHT,
};
use crate::studio::{Action, Actions, Studio};
use crate::viewer::{ModelCache, TruckRoot, ViewportInput};

use self::screen::{Click, Mapping, Prices, ScreenAssets, ScreenState};
use self::textures::UiTextures;

/// The egui panels and hotspot markers, drawn through their own camera.
pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiState>()
            .add_systems(Startup, setup)
            .add_systems(EguiPrimaryContextPass, ui);
    }
}

const BG: Color32 = Color32::from_rgb(0x11, 0x13, 0x17);
const PANEL: Color32 = Color32::from_rgb(0x1a, 0x1d, 0x23);
const RAISED: Color32 = Color32::from_rgb(0x22, 0x26, 0x2e);
const LINE: Color32 = Color32::from_rgb(0x2e, 0x33, 0x3d);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xea, 0xee);
const MUTED: Color32 = Color32::from_rgb(0x8d, 0x94, 0xa1);
const ACCENT: Color32 = Color32::from_rgb(0xf5, 0xa6, 0x23);
const DANGER: Color32 = Color32::from_rgb(0xef, 0x6b, 0x6b);
const MARKER_SIZE: f32 = 16.0;
const GROUP_SIZE: f32 = 20.0;
/// The game screen this app reproduces (`/ui/desc/truck.sui`).
const SCREEN: &str = "truck.config";

#[derive(Resource, Default)]
pub struct UiState {
    game_dir: String,
    /// Key of the cluster whose slot list is open.
    open_cluster: Option<String>,
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
        self.open_cluster = None;
        actions
            .0
            .push(Action::ShowSlot(screen::tab_slot(overview, tab)));
    }
}

fn marker_color(category: HotspotCategory) -> Color32 {
    match category {
        HotspotCategory::Base => Color32::from_rgb(0x8b, 0xd1, 0x7c),
        HotspotCategory::Addon => ACCENT,
        HotspotCategory::Wheel => Color32::from_rgb(0x5f, 0xb3, 0xf9),
        HotspotCategory::Interior => Color32::from_rgb(0xc9, 0xa2, 0xf5),
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
                    mapping: Mapping::new(window),
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

        // Pins: only on the open accessory tab (F5).
        let truck_center = truck.transform_point(Vec3::Y * TRUCK_CENTER_HEIGHT);
        let eye = camera_transform.translation();
        let tab = state.screen.tab.filter(|t| t.has_pins());
        let projected: Vec<Projected> = overview
            .hotspots
            .iter()
            .enumerate()
            .filter(|(_, h)| Some(h.tab) == tab)
            .filter_map(|(index, hotspot)| {
                let world = truck.transform_point(Vec3::from(hotspot.position));
                if !side_facing(world, truck_center, eye) {
                    return None;
                }
                let p = camera.world_to_viewport(camera_transform, world).ok()?;
                let pos = window.min + egui::vec2(p.x, p.y);
                let free = window.contains(pos) && !blocking.iter().any(|r| r.contains(pos));
                free.then_some(Projected {
                    index,
                    position: Vec2::new(pos.x, pos.y),
                })
            })
            .collect();
        let active = studio.options.as_ref().map(|o| &o.slot);
        for group in cluster(&projected, CLUSTER_RADIUS) {
            marker(&ctx, overview, &group, active, state, &mut actions);
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
    if input.hovered && ctx.input(|i| i.pointer.primary_pressed()) {
        state.open_cluster = None;
    }
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

/// One marker: a dot for a single slot, a numbered dot that opens a slot
/// list for a cluster.
fn marker(
    ctx: &egui::Context,
    overview: &TruckOverview,
    group: &Cluster,
    active: Option<&specsheet_core::SlotId>,
    state: &mut UiState,
    actions: &mut Actions,
) {
    let mut members: Vec<&Hotspot> = group
        .members
        .iter()
        .filter_map(|&i| overview.hotspots.get(i))
        .collect();
    members.sort_by(|a, b| a.label.cmp(&b.label));
    let Some(first) = members.first() else {
        return;
    };
    let key = members
        .iter()
        .map(|h| h.slot.as_str())
        .collect::<Vec<_>>()
        .join("+");
    let single = members.len() == 1;
    let is_active = members.iter().any(|h| Some(&h.slot) == active);
    let size = if single { MARKER_SIZE } else { GROUP_SIZE };
    let center = Pos2::new(group.position.x, group.position.y);
    let area = egui::Area::new(Id::new(("marker", &key)))
        .fixed_pos(center - egui::vec2(size, size) / 2.0)
        .order(Order::Middle)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click());
            let painter = ui.painter();
            // A group (one wheel axle) takes its members' colour; a mixed
            // cluster is white.
            let grouped = members
                .iter()
                .all(|h| h.group.is_some() && h.group == first.group);
            let fill = if single || grouped {
                marker_color(first.category)
            } else {
                Color32::WHITE
            };
            let radius = size / 2.0 * if response.hovered() { 1.15 } else { 1.0 };
            painter.circle_filled(rect.center(), radius + 4.0, fill.linear_multiply(0.2));
            if members.iter().all(|h| h.empty) {
                // Empty slots sit on bare locators, away from any part: a ring.
                painter.circle(
                    rect.center(),
                    radius,
                    BG.linear_multiply(0.6),
                    Stroke::new(2.5, fill),
                );
            } else {
                painter.circle(
                    rect.center(),
                    radius,
                    fill,
                    Stroke::new(2.0, Color32::WHITE),
                );
            }
            if is_active {
                painter.circle_stroke(rect.center(), radius + 5.0, Stroke::new(2.0, ACCENT));
            }
            if !single {
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    members.len().to_string(),
                    egui::FontId::proportional(11.0),
                    Color32::from_rgb(0x11, 0x11, 0x11),
                );
            }
            response
        });
    let response = if single {
        area.inner.on_hover_text(&first.label)
    } else {
        area.inner
    };
    if response.clicked() {
        if single {
            state.open_cluster = None;
            actions.0.push(Action::ShowSlot(Some(first.slot.clone())));
        } else if state.open_cluster.as_deref() == Some(key.as_str()) {
            state.open_cluster = None;
        } else {
            state.open_cluster = Some(key.clone());
        }
    }
    if single || state.open_cluster.as_deref() != Some(key.as_str()) {
        return;
    }
    egui::Area::new(Id::new(("cluster-menu", &key)))
        .fixed_pos(center + egui::vec2(size, -size / 2.0))
        .order(Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                for hotspot in &members {
                    let text = match &hotspot.selected {
                        Some(selected) => format!("{}  ·  {selected}", hotspot.label),
                        None => hotspot.label.clone(),
                    };
                    if ui.selectable_label(false, text).clicked() {
                        state.open_cluster = None;
                        actions.0.push(Action::ShowSlot(Some(hotspot.slot.clone())));
                    }
                }
            });
        });
}
