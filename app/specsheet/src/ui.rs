//! The egui layer: top bar, sidebar, messages and the hotspot markers over
//! the 3D view. It renders what `game-data::view` reports and turns clicks
//! into [`Action`]s; it holds no tuning rules.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraOutputMode, Viewport};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy_egui::{egui, EguiContexts, EguiGlobalSettings, PrimaryEguiContext};
use egui::{Align2, Color32, Id, LayerId, Order, Pos2, RichText, Sense, Stroke, UiBuilder};
use game_data::assembly::Drive;
use game_data::view::{Hotspot, HotspotCategory, OptionView, SlotOptions, TruckOverview};

use crate::hotspots::{
    cluster, side_facing, Cluster, Projected, CLUSTER_RADIUS, TRUCK_CENTER_HEIGHT,
};
use crate::studio::{Action, Actions, Studio};
use crate::viewer::{ModelCache, TruckRoot, ViewportInput};

const BG: Color32 = Color32::from_rgb(0x11, 0x13, 0x17);
const PANEL: Color32 = Color32::from_rgb(0x1a, 0x1d, 0x23);
const RAISED: Color32 = Color32::from_rgb(0x22, 0x26, 0x2e);
const LINE: Color32 = Color32::from_rgb(0x2e, 0x33, 0x3d);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xea, 0xee);
const MUTED: Color32 = Color32::from_rgb(0x8d, 0x94, 0xa1);
const ACCENT: Color32 = Color32::from_rgb(0xf5, 0xa6, 0x23);
const DANGER: Color32 = Color32::from_rgb(0xef, 0x6b, 0x6b);
const SIDEBAR_WIDTH: f32 = 320.0;
const MARKER_SIZE: f32 = 16.0;
const GROUP_SIZE: f32 = 20.0;

#[derive(Resource, Default)]
pub struct UiState {
    game_dir: String,
    pub show_interior: bool,
    /// Key of the cluster whose slot list is open.
    open_cluster: Option<String>,
    styled: bool,
}

fn marker_color(category: HotspotCategory) -> Color32 {
    match category {
        HotspotCategory::Base => Color32::from_rgb(0x8b, 0xd1, 0x7c),
        HotspotCategory::Addon => ACCENT,
        HotspotCategory::Wheel => Color32::from_rgb(0x5f, 0xb3, 0xf9),
        HotspotCategory::Interior => Color32::from_rgb(0xc9, 0xa2, 0xf5),
    }
}

/// egui draws through its own full-window camera over the 3D view, so the
/// 3D camera's viewport can shrink to the area between the panels.
pub fn setup(mut commands: Commands, mut settings: ResMut<EguiGlobalSettings>) {
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

pub fn ui(
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
    if !state.styled {
        style(&ctx);
        state.styled = true;
    }
    let mut root = egui::Ui::new(
        ctx.clone(),
        "root".into(),
        UiBuilder::new()
            .layer_id(LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    let loading = studio.busy() || cache.loading();
    top_bar(&mut root, &studio, &mut actions, &mut state, loading);
    messages(&mut root, &studio, &cache);
    if let Some(overview) = &studio.overview {
        egui::Panel::right("sidebar")
            .exact_size(SIDEBAR_WIDTH)
            .resizable(false)
            .show(&mut root, |ui| match &studio.options {
                Some(options) => option_list(ui, options, studio.busy(), &mut actions),
                None => slot_list(ui, overview, &mut actions),
            });
    }
    let central = root.available_rect_before_wrap();

    let (mut camera, camera_transform) = camera.into_inner();
    let scale = ctx.pixels_per_point();
    if central.width() >= 1.0 && central.height() >= 1.0 {
        camera.viewport = Some(Viewport {
            physical_position: UVec2::new(
                (central.min.x * scale) as u32,
                (central.min.y * scale) as u32,
            ),
            physical_size: UVec2::new(
                (central.width() * scale) as u32,
                (central.height() * scale) as u32,
            ),
            ..default()
        });
    }

    if studio.game.is_none() && !studio.busy() && !studio.discovering {
        locate(&ctx, &studio, &mut state, &mut actions);
    }

    if let Some(overview) = &studio.overview {
        let truck_center = truck.transform_point(Vec3::Y * TRUCK_CENTER_HEIGHT);
        let eye = camera_transform.translation();
        let projected: Vec<Projected> = overview
            .hotspots
            .iter()
            .enumerate()
            .filter(|(_, h)| state.show_interior || h.category != HotspotCategory::Interior)
            .filter_map(|(index, hotspot)| {
                let world = truck.transform_point(Vec3::from(hotspot.position));
                if !side_facing(world, truck_center, eye) {
                    return None;
                }
                let p = camera.world_to_viewport(camera_transform, world).ok()?;
                let pos = central.min + egui::vec2(p.x, p.y);
                central.contains(pos).then_some(Projected {
                    index,
                    position: Vec2::new(pos.x, pos.y),
                })
            })
            .collect();
        let active = studio.options.as_ref().map(|o| &o.slot);
        for group in cluster(&projected, CLUSTER_RADIUS) {
            marker(&ctx, overview, &group, active, &mut state, &mut actions);
        }
    }

    let pointer = ctx.input(|i| i.pointer.hover_pos());
    input.hovered = pointer.is_some_and(|pos| {
        central.contains(pos)
            && ctx
                .layer_id_at(pos)
                .is_none_or(|layer| layer.order == Order::Background)
    });
    if input.hovered && ctx.input(|i| i.pointer.primary_pressed()) {
        state.open_cluster = None;
    }
    Ok(())
}

fn top_bar(
    root: &mut egui::Ui,
    studio: &Studio,
    actions: &mut Actions,
    state: &mut UiState,
    loading: bool,
) {
    egui::Panel::top("topbar").show(root, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let (mark, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), Sense::hover());
            ui.painter()
                .circle_stroke(mark.center(), 5.5, Stroke::new(3.0, ACCENT));
            ui.label(RichText::new("SpecSheet").strong());
            ui.add_space(12.0);
            let enabled = !studio.busy();
            if let Some(overview) = &studio.overview {
                egui::ComboBox::from_id_salt("truck")
                    .selected_text(&overview.name)
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for truck in &studio.trucks {
                            let selected = truck.id == overview.truck;
                            if ui.selectable_label(selected, &truck.name).clicked() && !selected {
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
                    .width(180.0)
                    .show_ui(ui, |ui| {
                        for (index, scene) in studio.scenes.iter().enumerate() {
                            let selected = studio.scene == Some(index);
                            if ui.selectable_label(selected, &scene.name).clicked() && !selected {
                                actions.0.push(Action::SetScene(index));
                            }
                        }
                    });
            }
            if studio.overview.is_some() {
                ui.checkbox(&mut state.show_interior, "Interior markers");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(game) = &studio.game {
                    let root = game.install().root().display().to_string();
                    ui.label(RichText::new(&root).color(MUTED))
                        .on_hover_text(&root);
                }
                if loading {
                    ui.spinner();
                }
            });
        });
        ui.add_space(4.0);
    });
}

fn messages(root: &mut egui::Ui, studio: &Studio, cache: &ModelCache) {
    let warnings: Vec<&String> = studio
        .overview
        .iter()
        .flat_map(|o| o.warnings.iter())
        .chain(cache.warnings.iter())
        .take(3)
        .collect();
    if studio.game.is_none() || (studio.error.is_none() && warnings.is_empty()) {
        return;
    }
    egui::Panel::bottom("messages").show(root, |ui| {
        if let Some(error) = &studio.error {
            ui.label(RichText::new(error).color(DANGER));
        }
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

fn slot_list(ui: &mut egui::Ui, overview: &TruckOverview, actions: &mut Actions) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.heading("Parts");
        ui.label(
            RichText::new((overview.hotspots.len() + overview.general.len()).to_string())
                .color(MUTED),
        );
    });
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        let by_label =
            |a: &(&str, &str, Option<&str>), b: &(&str, &str, Option<&str>)| a.1.cmp(b.1);
        let mut groups: Vec<(&str, Vec<(&str, &str, Option<&str>)>)> = HotspotCategory::ALL
            .iter()
            .map(|category| {
                let mut items: Vec<_> = overview
                    .hotspots
                    .iter()
                    .filter(|h| h.category == *category)
                    .map(|h| (h.slot.as_str(), h.label.as_str(), h.selected.as_deref()))
                    .collect();
                items.sort_by(by_label);
                (category.title(), items)
            })
            .collect();
        let mut general: Vec<_> = overview
            .general
            .iter()
            .map(|s| (s.slot.as_str(), s.label.as_str(), s.selected.as_deref()))
            .collect();
        general.sort_by(by_label);
        groups.push(("General", general));
        for (title, items) in groups {
            if items.is_empty() {
                continue;
            }
            ui.add_space(6.0);
            ui.label(RichText::new(title.to_uppercase()).small().color(MUTED));
            for (slot, label, selected) in items {
                let response = ui.add(
                    egui::Button::new(RichText::new(label))
                        .right_text(RichText::new(selected.unwrap_or("None")).color(MUTED))
                        .min_size(egui::vec2(ui.available_width(), 26.0)),
                );
                if response.clicked() {
                    actions
                        .0
                        .push(Action::ShowSlot(Some(tuning_core::SlotId::new(slot))));
                }
            }
        }
    });
}

fn option_list(ui: &mut egui::Ui, options: &SlotOptions, busy: bool, actions: &mut Actions) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .button("‹ Parts")
            .on_hover_text("Back to all parts")
            .clicked()
        {
            actions.0.push(Action::ShowSlot(None));
        }
        ui.heading(&options.label);
        ui.label(RichText::new(options.options.len().to_string()).color(MUTED));
    });
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        for option in &options.options {
            option_row(ui, option, busy, actions);
        }
    });
}

fn option_row(ui: &mut egui::Ui, option: &OptionView, busy: bool, actions: &mut Actions) {
    let mut meta = Vec::new();
    if let Some(price) = option.price {
        meta.push(format!("€{price}"));
    }
    if let Some(level) = option.unlock_level.filter(|l| *l > 0) {
        meta.push(format!("Lvl {level}"));
    }
    let enabled = !busy && !option.selected && option.compatible;
    let name = if option.selected {
        RichText::new(&option.name).color(ACCENT).strong()
    } else if option.compatible {
        RichText::new(&option.name)
    } else {
        RichText::new(&option.name).color(MUTED)
    };
    let response = ui
        .add_enabled(
            enabled,
            egui::Button::new(name)
                .right_text(RichText::new(meta.join("  ")).color(MUTED))
                .selected(option.selected)
                .min_size(egui::vec2(ui.available_width(), 28.0)),
        )
        .on_hover_text(option.id.as_str())
        .on_disabled_hover_text(option.reason.as_deref().unwrap_or(option.id.as_str()));
    if !option.compatible {
        if let Some(reason) = &option.reason {
            ui.label(RichText::new(reason).small().color(DANGER));
        }
    }
    if response.clicked() {
        actions.0.push(Action::Select(option.id.clone()));
    }
}

/// One marker: a dot for a single slot, a numbered dot that opens a slot
/// list for a cluster.
fn marker(
    ctx: &egui::Context,
    overview: &TruckOverview,
    group: &Cluster,
    active: Option<&tuning_core::SlotId>,
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
