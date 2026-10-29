//! The game's truck configuration screen (`truck.config`), drawn from its
//! own layout files and templates (`docs/formats/ui-truck-config.md`,
//! `docs/app/ui.md`). The layouts give every widget's place and look; the
//! containers the game fills in code (tab row, accessory list) are filled
//! here from `specsheet-data::view`.

use bevy_egui::egui::{self, Align2, Color32, FontId, Id, Pos2, Sense};
use specsheet_core::colour::{linear_to_srgb, srgb_to_linear};
use specsheet_core::{AccessoryId, SlotId};
use specsheet_data::localization::Localization;
use specsheet_gameui::flow::{self, Font, Item, Measure};
use specsheet_gameui::{
    substitute, KeepAspect, Layout, Rect, Screen, Templates, Widget, REFERENCE_HEIGHT,
    REFERENCE_WIDTH,
};
use specsheet_session::view::{
    tab_slots, OptionView, PaintColorView, SlotOptions, Tab, TabSlot, TruckOverview,
};
use specsheet_session::GameSession;
use specsheet_truck::PaintChannel;

use super::textures::UiTextures;

/// The screen's layouts and the templates they use, loaded with the game.
pub struct ScreenAssets {
    pub screen: Screen,
    pub templates: Templates,
}

/// Widgets the screen handler shows, hides or fills in code. Ids are the
/// layout files' own (`docs/formats/ui-truck-config.md`); which are shown
/// when is our Design until the handler is traced (`docs/app/ui.md`).
mod ids {
    /// Category tab row (`ui::button_row`, F5).
    pub const TAB_ROW: i64 = 499;
    /// Accessory list, filled with option tiles.
    pub const ACCESSORY_LIST: i64 = 300;
    /// Accessory browser: list, its title, search and filter.
    pub const COMPONENTS: i64 = 305;
    /// List caption, set from the tab.
    pub const LIST_TITLE: i64 = 310;
    /// "Select a category", shown until a list is open.
    pub const FIRST_MESSAGE: i64 = 380;
    /// Price box values in customize mode: original price, extra price,
    /// new total (rows of the `tc_*` labels).
    pub const ORIGINAL_PRICE: i64 = 364;
    pub const EXTRA_PRICE: i64 = 362;
    pub const NEW_TOTAL: i64 = 360;
    /// The colour picker group, in the right column; shown while the paint
    /// job list offers colours, in place of the folded analysis.
    pub const PICKER_GROUP: i64 = 420;
    /// The folded vehicle analysis, which the picker replaces.
    pub const FOLDED_ANALYSIS: i64 = 6001;
    /// The picker's hue bar and saturation/value square (`ui_color_picker`).
    pub const PICKER: i64 = 430;
    /// Paint channel buttons (`btn.truck_chnl_s`); also the id of the
    /// centre layout's own channel row, which stays hidden.
    pub const CHANNELS: i64 = 410;
    /// The remove button (`btn.remove_acc`): a bubble whose arrow points up
    /// at the open slot's pin, shown while its selection may be removed.
    pub const REMOVE: i64 = 8005;
    /// The R, G and B values of the picked colour.
    pub const RGB_FIELDS: [i64; 3] = [460, 461, 462];
    /// Picker widgets not wired up yet: preset and save buttons, user
    /// colours, the channel overflow list, the RGB/HSV switch and the
    /// button beside the channels.
    pub const PICKER_UNWIRED: [i64; 7] = [440, 445, 446, 450, 415, 465, 473];
    /// Shown only by behaviour not wired up yet: messages,
    /// conflict marker, walk-camera notice and crosshair, cart, accessory
    /// origin badge, buy-mode price labels and the cart and checkout
    /// buttons, money, unfolded analysis, control hints, tab-key hints,
    /// search and filter.
    pub const HIDDEN: [i64; 20] = [
        600, 620, 333, 587, 355, 390, 391, 386, 210, 11, 6000, 111, 575, 576, 577, 578, 401, 402,
        403, 404,
    ];
}

/// Reference text heights per game font (Design: egui's font stands in
/// for the game's MSDF fonts until those are rendered).
fn font_size(font: &Font) -> f32 {
    let base = match font.face.as_deref() {
        Some(face) if face.contains("title") => 24.0,
        Some(face) if face.contains("big") => 20.0,
        Some(face) if face.contains("small") => 12.5,
        _ => 15.0,
    };
    base * font.scale[1]
}

/// Maps the 1440 × 900 reference space (y up) onto the window: scaled by
/// height, each window pinned by its `keep_aspect` (Hypothesis,
/// `docs/formats/ui.md`).
#[derive(Clone, Copy, Debug)]
pub struct Mapping {
    pub scale: f32,
    pub window: egui::Rect,
}

impl Mapping {
    pub fn new(window: egui::Rect) -> Self {
        Self {
            scale: window.height() / REFERENCE_HEIGHT,
            window,
        }
    }

    pub fn rect(&self, aspect: KeepAspect, r: &Rect) -> egui::Rect {
        let virtual_width = self.window.width() / self.scale;
        let (offset, stretch) = match aspect {
            KeepAspect::Left => (0.0, 1.0),
            KeepAspect::Right => (virtual_width - REFERENCE_WIDTH, 1.0),
            KeepAspect::Center => ((virtual_width - REFERENCE_WIDTH) / 2.0, 1.0),
            KeepAspect::None => (0.0, virtual_width / REFERENCE_WIDTH),
        };
        let x = |v: f32| self.window.left() + (offset + v * stretch) * self.scale;
        let y = |v: f32| self.window.top() + (REFERENCE_HEIGHT - v) * self.scale;
        egui::Rect::from_min_max(
            Pos2::new(x(r.left), y(r.top)),
            Pos2::new(x(r.right), y(r.bottom)),
        )
    }
}

/// What the screen shows besides the layouts.
#[derive(Default)]
pub struct ScreenState {
    pub tab: Option<Tab>,
    /// Accessory list scroll, in reference units.
    pub scroll: f32,
    /// Mouse wheel this frame, in reference units (positive: up). Read from
    /// Bevy's input, as the 3D camera does; egui's scroll delta stays empty
    /// for this screen.
    pub wheel: f32,
    /// The colour picker's open channel (index into the editable colours).
    pub channel: usize,
    /// The colour being dragged in the picker, as sRGB HSV; applied when
    /// the mouse button is released.
    pub dragging: Option<[f32; 3]>,
}

/// The price box in customize mode: the truck's price when loaded and
/// now (`Configuration::price`).
#[derive(Clone, Copy, Debug)]
pub struct Prices {
    pub original: i64,
    pub total: i64,
}

/// What the user did on the screen this frame.
pub enum Click {
    Tab(Tab),
    /// A slot picked from an accessory tab's list.
    Slot(SlotId),
    Select(AccessoryId),
    /// The remove button: empty the open slot.
    Remove(SlotId),
    /// A colour picked for a paint channel (linear RGB).
    PaintColor(PaintChannel, [f32; 3]),
}

struct EguiMeasure<'a> {
    painter: &'a egui::Painter,
    game: &'a GameSession,
    textures: &'a mut UiTextures,
    scale: f32,
}

impl Measure for EguiMeasure<'_> {
    fn image_size(&mut self, src: &str) -> Option<[f32; 2]> {
        let ctx = self.painter.ctx().clone();
        self.textures.get(&ctx, self.game, src).map(|t| t.size)
    }

    fn text_size(&mut self, text: &str, font: &Font) -> [f32; 2] {
        let size = self
            .painter
            .layout_no_wrap(
                text.to_owned(),
                FontId::proportional(font_size(font) * self.scale),
                Color32::WHITE,
            )
            .size();
        [size.x / self.scale, size.y / self.scale]
    }
}

pub struct Context<'a> {
    pub painter: &'a egui::Painter,
    pub game: &'a GameSession,
    pub textures: &'a mut UiTextures,
    pub localization: &'a Localization,
    pub mapping: Mapping,
}

impl Context<'_> {
    /// Lays out `markup` (template already substituted) in `target` and
    /// paints it; returns the painted bounds.
    fn paint(&mut self, markup: &str, size: [f32; 2], target: egui::Rect) -> Option<egui::Rect> {
        let markup = self.localization.resolve(markup);
        let mut measure = EguiMeasure {
            painter: self.painter,
            game: self.game,
            textures: self.textures,
            scale: self.mapping.scale,
        };
        let items = flow::layout_markup(&markup, size, &mut measure).ok()?;
        let sx = target.width() / size[0].max(1.0);
        let sy = target.height() / size[1].max(1.0);
        let to_screen = |b: [f32; 4]| {
            egui::Rect::from_min_max(
                Pos2::new(target.left() + b[0] * sx, target.top() + b[1] * sy),
                Pos2::new(target.left() + b[2] * sx, target.top() + b[3] * sy),
            )
        };
        let mut painted: Option<egui::Rect> = None;
        let ctx = self.painter.ctx().clone();
        for item in items {
            let rect = to_screen(item.bounds());
            match item {
                Item::Image {
                    src, texels, color, ..
                } => {
                    let Some(texture) = self.textures.get(&ctx, self.game, &src) else {
                        continue;
                    };
                    let [w, h] = texture.size;
                    let uv = egui::Rect::from_min_max(
                        Pos2::new(texels[0] / w, texels[1] / h),
                        Pos2::new(texels[2] / w, texels[3] / h),
                    );
                    let [r, g, b, a] = color;
                    self.painter.image(
                        texture.id,
                        rect,
                        uv,
                        Color32::from_rgba_unmultiplied(r, g, b, a),
                    );
                }
                Item::Text {
                    text, font, color, ..
                } => {
                    let [r, g, b, a] = color;
                    self.painter.text(
                        rect.min,
                        Align2::LEFT_TOP,
                        text,
                        FontId::proportional(font_size(&font) * self.mapping.scale),
                        Color32::from_rgba_unmultiplied(r, g, b, a),
                    );
                }
            }
            painted = Some(painted.map_or(rect, |p| p.union(rect)));
        }
        painted
    }
}

/// Draws the screen's left, center and right layouts and fills its code
/// containers. Returns the clicks and the rectangles that block the 3D
/// view's pointer.
pub fn draw(
    ui: &mut egui::Ui,
    cx: &mut Context<'_>,
    assets: &ScreenAssets,
    overview: &TruckOverview,
    options: Option<&SlotOptions>,
    colours: &[PaintColorView],
    remove_under: Option<Pos2>,
    prices: Prices,
    state: &mut ScreenState,
) -> (Vec<Click>, Vec<egui::Rect>) {
    let mut clicks = Vec::new();
    let mut blocking = Vec::new();
    let picking = !colours.is_empty();
    if state.channel >= colours.len() {
        state.channel = 0;
    }
    // The browser shows a slot's options, or an accessory tab's slots
    // while no slot is open.
    let slot_list = options
        .is_none()
        .then_some(state.tab)
        .flatten()
        .filter(|t| t.has_pins());
    let list_open = options.is_some() || slot_list.is_some();
    let list_items = match (options, slot_list) {
        (Some(o), _) => o.options.len(),
        (None, Some(tab)) => tab_slots(overview, tab).len(),
        (None, None) => 0,
    };
    for layout in &assets.screen.layouts {
        let mut order: Vec<usize> = (0..layout.widgets.len()).collect();
        order.sort_by_key(|&i| layout.widgets[i].layer);
        for index in order {
            let widget = &layout.widgets[index];
            if !visible(layout, index, list_open, state.tab.is_some(), picking) {
                continue;
            }
            if widget.id == ids::REMOVE && remove_under.is_none() {
                continue;
            }
            let target = cx.mapping.rect(layout.keep_aspect, &widget.rect);
            let size = [widget.rect.width(), widget.rect.height()];
            let painted = match widget.id {
                ids::TAB_ROW => tab_row(ui, cx, assets, widget, target, state, &mut clicks),
                ids::CHANNELS => channel_row(ui, cx, assets, widget, target, colours, state),
                ids::PICKER => colour_picker(ui, cx, widget, target, colours, state, &mut clicks),
                ids::REMOVE => {
                    // Hung under the pin, centred, at the template's size.
                    let anchor = remove_under.unwrap_or(target.center_top());
                    let target = egui::Rect::from_min_size(
                        Pos2::new(anchor.x - target.width() / 2.0, anchor.y),
                        target.size(),
                    );
                    let response = ui.interact(target, Id::new("remove"), Sense::click());
                    if response.clicked() {
                        if let Some(o) = options {
                            clicks.push(Click::Remove(o.slot.clone()));
                        }
                    }
                    widget_markup(assets, widget, None, &mut || response.hovered())
                        .and_then(|markup| cx.paint(&markup, size, target))
                }
                id if ids::RGB_FIELDS.contains(&id) => {
                    let channel = ids::RGB_FIELDS.iter().position(|&f| f == id);
                    rgb_field(cx, target, colours, state, channel)
                }
                _ if widget.class == SLIDER_CLASS
                    && widget
                        .parent
                        .is_some_and(|p| layout.widgets[p].id == ids::ACCESSORY_LIST) =>
                {
                    let list = widget.parent.map(|p| &layout.widgets[p]);
                    list.and_then(|list| {
                        list_slider(ui, cx, assets, widget, target, list, list_items, state)
                    })
                }
                ids::ACCESSORY_LIST => match (options, slot_list) {
                    (Some(o), _) => {
                        accessory_list(ui, cx, assets, widget, target, o, state, &mut clicks)
                    }
                    (None, Some(tab)) => slot_tiles(
                        ui,
                        cx,
                        assets,
                        widget,
                        target,
                        &tab_slots(overview, tab),
                        state,
                        &mut clicks,
                    ),
                    (None, None) => None,
                },
                _ => {
                    // The caption template is `<color value=%0>%1`; the colour
                    // follows the sibling "select a category" message (Design).
                    let value = match widget.id {
                        ids::LIST_TITLE => state
                            .tab
                            .map(|t| format!("@@clr_sel@@|@@{}@@", t.caption_key())),
                        ids::ORIGINAL_PRICE => Some(price(Some(prices.original))),
                        ids::EXTRA_PRICE => Some(price(Some(prices.total - prices.original))),
                        ids::NEW_TOTAL => Some(price(Some(prices.total))),
                        _ => widget.value.clone(),
                    };
                    widget_markup(assets, widget, value.as_deref(), &mut || {
                        ui.rect_contains_pointer(target)
                    })
                    .and_then(|markup| cx.paint(&markup, size, target))
                }
            };
            if let Some(rect) = painted {
                if widget.parent.is_some() {
                    blocking.push(rect);
                }
            }
        }
    }
    (clicks, blocking)
}

/// Whether `index` and all its ancestors are shown.
fn visible(layout: &Layout, index: usize, list_open: bool, tab_open: bool, picking: bool) -> bool {
    let in_picker = |widget: &Widget| {
        widget
            .parent
            .is_some_and(|p| layout.widgets[p].id == ids::PICKER_GROUP)
    };
    let hidden_itself = |widget: &Widget| {
        ids::HIDDEN.contains(&widget.id)
            || (widget.id == ids::FIRST_MESSAGE && (list_open || tab_open))
            || (widget.id == ids::COMPONENTS && !list_open)
            || (widget.id == ids::PICKER_GROUP && !picking)
            || (widget.id == ids::FOLDED_ANALYSIS && picking)
            || (widget.id == ids::CHANNELS && !in_picker(widget))
            || (ids::PICKER_UNWIRED.contains(&widget.id) && in_picker(widget))
    };
    let mut current = Some(index);
    while let Some(i) = current {
        let widget = &layout.widgets[i];
        // An unaddressed sibling with the same rectangle is the hidden
        // widget's background (e.g. the money box) and goes with it.
        let background_of_hidden = widget.id == 0
            && layout.widgets.iter().any(|other| {
                other.id != 0
                    && other.parent == widget.parent
                    && other.rect == widget.rect
                    && hidden_itself(other)
            });
        if hidden_itself(widget) || background_of_hidden {
            return false;
        }
        current = widget.parent;
    }
    true
}

/// The markup a plain widget draws: its text template with its value as
/// arguments, a button template's normal or hovered state, or raw markup.
fn widget_markup(
    assets: &ScreenAssets,
    widget: &Widget,
    value: Option<&str>,
    hovered: &mut dyn FnMut() -> bool,
) -> Option<String> {
    let args: Vec<&str> = value.map(|v| v.split('|').collect()).unwrap_or_default();
    let template = widget.look_template.as_deref();
    if let Some(text) = template.and_then(|t| assets.templates.text(t)) {
        return Some(fill(&text.markup, &args));
    }
    if let Some(button) = template.and_then(|t| assets.templates.button(t)) {
        let markup = if hovered() {
            &button.selected
        } else {
            &button.normal
        };
        return Some(fill(markup, &args));
    }
    widget.text.clone()
}

/// The paint channel buttons: one swatch per editable colour, the open one
/// underlined (`btn.truck_chnl_s`: `%0` the channel icon, `%1` its colour).
/// Which icon a channel takes is ours: they are numbered in order.
fn channel_row(
    ui: &mut egui::Ui,
    cx: &mut Context<'_>,
    assets: &ScreenAssets,
    widget: &Widget,
    target: egui::Rect,
    colours: &[PaintColorView],
    state: &mut ScreenState,
) -> Option<egui::Rect> {
    let button = widget
        .look_template
        .as_deref()
        .and_then(|t| assets.templates.button(t))?;
    let (width, height) = (CHANNEL_BUTTON[0], CHANNEL_BUTTON[1]);
    let step = (width + CHANNEL_SPACING) * cx.mapping.scale;
    let mut painted = None::<egui::Rect>;
    for (i, colour) in colours.iter().enumerate().take(CHANNEL_ICONS) {
        let rect = egui::Rect::from_min_size(
            Pos2::new(target.left() + step * i as f32, target.top()),
            egui::vec2(width * cx.mapping.scale, height * cx.mapping.scale),
        );
        let response = ui.interact(rect, Id::new(("paint-channel", i)), Sense::click());
        let markup = if state.channel == i || response.hovered() {
            &button.selected
        } else {
            &button.normal
        };
        let colour_arg = match state.dragging.filter(|_| state.channel == i) {
            Some(hsv) => ui_hex(hsv_to_rgb(hsv)),
            None => colour.ui_color.clone(),
        };
        let icon = i.to_string();
        if let Some(r) = cx.paint(&fill(markup, &[&icon, &colour_arg]), [width, height], rect) {
            painted = Some(painted.map_or(r, |p| p.union(r)));
        }
        if response.clicked() && state.channel != i {
            state.channel = i;
            state.dragging = None;
        }
    }
    painted
}

/// `btn.truck_chnl_s`'s size and the row's `item_distance`.
const CHANNEL_BUTTON: [f32; 2] = [48.0, 53.0];
const CHANNEL_SPACING: f32 = 10.0;
/// `clr_chnl_0` … `clr_chnl_3`.
const CHANNEL_ICONS: usize = 4;
/// `ui_color_picker [430]`: the hue bar's height (`hue_width`, at the top)
/// and the gap to the saturation/value square (`padding`).
const HUE_BAR: f32 = 27.0;
const PICKER_PADDING: f32 = 5.0;
const HUE_CURSOR: &str = "/material/ui/color_picker/hue_cursor.mat";
const SAT_CURSOR: &str = "/material/ui/color_picker/sat_cursor.mat";

/// The open channel's colour as sRGB HSV: the one being dragged, else the
/// paint job's current one.
fn picked_hsv(colours: &[PaintColorView], state: &ScreenState) -> Option<[f32; 3]> {
    state.dragging.or_else(|| {
        let colour = colours.get(state.channel)?;
        Some(rgb_to_hsv(colour.color.map(linear_to_srgb)))
    })
}

/// The hue bar and the saturation/value square of `ui_color_picker`.
/// Dragging either changes the colour; it applies on release.
fn colour_picker(
    ui: &mut egui::Ui,
    cx: &mut Context<'_>,
    widget: &Widget,
    target: egui::Rect,
    colours: &[PaintColorView],
    state: &mut ScreenState,
    clicks: &mut Vec<Click>,
) -> Option<egui::Rect> {
    let channel = colours.get(state.channel)?.channel;
    let [mut h, mut s, mut v] = picked_hsv(colours, state)?;
    let scale = target.height() / widget.rect.height().max(1.0);
    let hue = egui::Rect::from_min_max(
        target.min,
        Pos2::new(target.right(), target.top() + HUE_BAR * scale),
    );
    let square = egui::Rect::from_min_max(
        Pos2::new(target.left(), hue.bottom() + PICKER_PADDING * scale),
        target.max,
    );
    // The hue bar: six segments around the colour wheel.
    let mut mesh = egui::Mesh::default();
    for i in 0..6 {
        let (h0, h1) = (i as f32 / 6.0, (i + 1) as f32 / 6.0);
        let x0 = hue.left() + hue.width() * h0;
        let x1 = hue.left() + hue.width() * h1;
        let base = mesh.vertices.len() as u32;
        for (x, hh, y) in [
            (x0, h0, hue.top()),
            (x1, h1, hue.top()),
            (x1, h1, hue.bottom()),
            (x0, h0, hue.bottom()),
        ] {
            mesh.colored_vertex(Pos2::new(x, y), color32(hsv_to_rgb([hh, 1.0, 1.0])));
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base, base + 2, base + 3);
    }
    // The square: saturation along x, value along y (top bright).
    const GRID: usize = 16;
    let base = mesh.vertices.len() as u32;
    for row in 0..=GRID {
        for col in 0..=GRID {
            let (sx, vy) = (col as f32 / GRID as f32, 1.0 - row as f32 / GRID as f32);
            let pos = Pos2::new(
                square.left() + square.width() * col as f32 / GRID as f32,
                square.top() + square.height() * row as f32 / GRID as f32,
            );
            mesh.colored_vertex(pos, color32(hsv_to_rgb([h, sx, vy])));
        }
    }
    let width = GRID as u32 + 1;
    for row in 0..GRID as u32 {
        for col in 0..GRID as u32 {
            let a = base + row * width + col;
            mesh.add_triangle(a, a + 1, a + width + 1);
            mesh.add_triangle(a, a + width + 1, a + width);
        }
    }
    cx.painter.add(egui::Shape::mesh(mesh));
    // Interaction.
    let hue_response = ui.interact(hue, Id::new("picker-hue"), Sense::click_and_drag());
    let square_response = ui.interact(square, Id::new("picker-square"), Sense::click_and_drag());
    if let Some(p) = hue_response.interact_pointer_pos() {
        h = ((p.x - hue.left()) / hue.width()).clamp(0.0, 1.0);
        state.dragging = Some([h, s, v]);
    }
    if let Some(p) = square_response.interact_pointer_pos() {
        s = ((p.x - square.left()) / square.width()).clamp(0.0, 1.0);
        v = 1.0 - ((p.y - square.top()) / square.height()).clamp(0.0, 1.0);
        state.dragging = Some([h, s, v]);
    }
    let released = hue_response.drag_stopped()
        || hue_response.clicked()
        || square_response.drag_stopped()
        || square_response.clicked();
    if released {
        if let Some(hsv) = state.dragging.take() {
            let linear = hsv_to_rgb(hsv).map(srgb_to_linear);
            clicks.push(Click::PaintColor(channel, linear));
        }
    }
    // The game's cursors.
    let ctx = cx.painter.ctx().clone();
    let cursor_size = egui::vec2(16.0, 16.0) * cx.mapping.scale;
    if let Some(t) = cx.textures.get(&ctx, cx.game, HUE_CURSOR) {
        let at = Pos2::new(hue.left() + hue.width() * h, hue.center().y);
        cx.painter.image(
            t.id,
            egui::Rect::from_center_size(at, egui::vec2(cursor_size.x, hue.height())),
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    if let Some(t) = cx.textures.get(&ctx, cx.game, SAT_CURSOR) {
        let at = Pos2::new(
            square.left() + square.width() * s,
            square.top() + square.height() * (1.0 - v),
        );
        cx.painter.image(
            t.id,
            egui::Rect::from_center_size(at, cursor_size),
            egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    Some(target)
}

/// One of the picked colour's R, G, B values (0–255, sRGB), centred in its
/// input field (read-only for now).
fn rgb_field(
    cx: &mut Context<'_>,
    target: egui::Rect,
    colours: &[PaintColorView],
    state: &ScreenState,
    channel: Option<usize>,
) -> Option<egui::Rect> {
    let rgb = hsv_to_rgb(picked_hsv(colours, state)?);
    let value = (rgb.get(channel?)? * 255.0).round() as u8;
    cx.painter
        .rect_filled(target, 2.0, Color32::from_black_alpha(140));
    cx.painter.text(
        target.center(),
        Align2::CENTER_CENTER,
        value.to_string(),
        FontId::proportional(13.0 * cx.mapping.scale),
        Color32::WHITE,
    );
    Some(target)
}

fn color32([r, g, b]: [f32; 3]) -> Color32 {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color32::from_rgb(byte(r), byte(g), byte(b))
}

/// sRGB as the UI's `FFBBGGRR` hex.
fn ui_hex([r, g, b]: [f32; 3]) -> String {
    let byte = |c: f32| (c.clamp(0.0, 1.0) * 255.0) as u8;
    format!("FF{:02X}{:02X}{:02X}", byte(b), byte(g), byte(r))
}

/// HSV in [0, 1] to RGB in [0, 1].
fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [f32; 3] {
    let h = (h.rem_euclid(1.0)) * 6.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// RGB in [0, 1] to HSV in [0, 1].
fn rgb_to_hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta <= f32::EPSILON {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    let s = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    [h, s, max]
}

fn tab_row(
    ui: &mut egui::Ui,
    cx: &mut Context<'_>,
    assets: &ScreenAssets,
    widget: &Widget,
    target: egui::Rect,
    state: &mut ScreenState,
    clicks: &mut Vec<Click>,
) -> Option<egui::Rect> {
    let button = widget
        .look_template
        .as_deref()
        .and_then(|t| assets.templates.button(t))?;
    let count = widget.value_list.len().max(1) as f32;
    // Dynamic buttons share the row; the eight 64-unit icons fill its 512
    // units exactly in 1.60.
    let width = widget.rect.width() / count;
    let height = widget.rect.height();
    let step = target.width() / count;
    for (i, token) in widget.value_list.iter().enumerate() {
        let Some(tab) = Tab::from_token(token) else {
            continue;
        };
        let rect = egui::Rect::from_min_size(
            Pos2::new(target.left() + step * i as f32, target.top()),
            egui::vec2(step, target.height()),
        );
        let id = Id::new(("tab", i));
        let response = ui.interact(rect, id, Sense::click());
        let selected = state.tab == Some(tab);
        let markup = if selected || response.hovered() {
            &button.selected
        } else {
            &button.normal
        };
        cx.paint(&fill(markup, &[token]), [width, height], rect);
        if response.clicked() {
            clicks.push(Click::Tab(tab));
        }
    }
    Some(target)
}

/// Fills a template's parameters. An `<img>` whose tag uses an empty or
/// missing argument is dropped first: it would name an image that does not
/// exist (`accessory_%9.mat` with no `%9`). Design: the game draws nothing
/// there either, as far as screenshots show.
fn fill(template: &str, args: &[&str]) -> String {
    let empty = |tag: &str| {
        let bytes = tag.as_bytes();
        bytes.windows(2).any(|w| {
            w[0] == b'%'
                && w[1].is_ascii_digit()
                && args
                    .get(usize::from(w[1] - b'0'))
                    .is_none_or(|a| a.is_empty())
        })
    };
    let mut kept = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("<img") {
        kept.push_str(&rest[..start]);
        let Some(len) = rest[start..].find('>') else {
            rest = &rest[start..];
            break;
        };
        let tag = &rest[start..start + len + 1];
        if !empty(tag) {
            kept.push_str(tag);
        }
        rest = &rest[start + len + 1..];
    }
    kept.push_str(rest);
    substitute(&kept, args)
}

/// Price as the game prints it (`€ 22 563,-` in the layout placeholders).
fn price(value: Option<i64>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(c);
    }
    let sign = if value < 0 { "-" } else { "" };
    format!("€ {sign}{grouped},-")
}

/// Alpha of the "owned" badge in the tile template (`%5`).
const SHOWN: &str = "FF";
const HIDDEN_ALPHA: &str = "00";

#[allow(clippy::too_many_arguments)]
fn accessory_list(
    ui: &mut egui::Ui,
    cx: &mut Context<'_>,
    assets: &ScreenAssets,
    widget: &Widget,
    target: egui::Rect,
    options: &SlotOptions,
    state: &mut ScreenState,
    clicks: &mut Vec<Click>,
) -> Option<egui::Rect> {
    let button = widget
        .look_template
        .as_deref()
        .and_then(|t| assets.templates.button(t))?;
    // The tiles take egui's hover and the screen's area claims no layer
    // space, so test the pointer position against the list itself.
    if ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| target.contains(p))
    {
        state.scroll -= state.wheel;
    }
    let ([item_w, item_h], pitch) = list_geometry(widget, options.options.len(), state)?;
    let scale = cx.mapping.scale;
    let painter = cx.painter.with_clip_rect(target);
    let mut list_cx = Context {
        painter: &painter,
        game: cx.game,
        textures: cx.textures,
        localization: cx.localization,
        mapping: cx.mapping,
    };
    for (i, option) in options.options.iter().enumerate() {
        let top = i as f32 * pitch - state.scroll;
        if top + item_h < 0.0 || top > widget.rect.height() {
            continue;
        }
        let rect = egui::Rect::from_min_size(
            Pos2::new(target.left(), target.top() + top * scale),
            egui::vec2(item_w * scale, item_h * scale),
        );
        let response = ui.interact(rect.intersect(target), Id::new(("tile", i)), Sense::click());
        let markup = tile_markup(button, option, response.hovered());
        list_cx.paint(&markup, [item_w, item_h], rect);
        if response.clicked() && option.compatible && !option.selected {
            clicks.push(Click::Select(option.id.clone()));
        }
    }
    Some(target)
}

const SLIDER_CLASS: &str = "ui::slider_common";

/// The accessory list's scrollbar from its `ui::slider_template`: the track,
/// and a handle whose position follows the list's scroll and which can be
/// dragged. Hidden when the list fits (the template's disabled state is
/// empty). The handle keeps `minimal_handle_size`, as the widget's
/// `dynamic_handle: false` suggests (Hypothesis).
#[allow(clippy::too_many_arguments)]
fn list_slider(
    ui: &mut egui::Ui,
    cx: &mut Context<'_>,
    assets: &ScreenAssets,
    widget: &Widget,
    target: egui::Rect,
    list: &Widget,
    items: usize,
    state: &mut ScreenState,
) -> Option<egui::Rect> {
    let template = widget
        .look_template
        .as_deref()
        .and_then(|t| assets.templates.slider(t))?;
    let [_, item_h] = list.item_size?;
    let pitch = item_h + list.item_gap.unwrap_or(0.0);
    let max_scroll = (pitch * items as f32 - list.rect.height()).max(0.0);
    if max_scroll <= 0.0 {
        return None;
    }
    let size = [widget.rect.width(), widget.rect.height()];
    cx.paint(&template.track, size, target);
    let length = size[1] - 2.0 * template.margin;
    let handle = widget
        .min_handle
        .unwrap_or(length / 4.0)
        .clamp(1.0, length.max(1.0));
    let travel = (length - handle).max(1.0);
    let scale = target.height() / size[1].max(1.0);
    let top = template.margin + travel * (state.scroll / max_scroll).clamp(0.0, 1.0);
    let rect = egui::Rect::from_min_size(
        Pos2::new(target.left(), target.top() + top * scale),
        egui::vec2(target.width(), handle * scale),
    );
    let response = ui.interact(rect, Id::new("list-slider"), Sense::drag());
    if response.dragged() {
        state.scroll += response.drag_delta().y / scale * (max_scroll / travel);
        state.scroll = state.scroll.clamp(0.0, max_scroll);
    }
    let markup = if response.dragged() {
        &template.handle_pressed
    } else if response.hovered() {
        &template.handle_hovered
    } else {
        &template.handle
    };
    cx.paint(markup, [size[0], handle], rect);
    Some(target)
}

/// List geometry shared by option and slot tiles: `(item size, pitch)`
/// in reference units, scroll clamped to the content.
fn list_geometry(
    widget: &Widget,
    count: usize,
    state: &mut ScreenState,
) -> Option<([f32; 2], f32)> {
    let size = widget.item_size?;
    let pitch = size[1] + widget.item_gap.unwrap_or(0.0);
    let max_scroll = (pitch * count as f32 - widget.rect.height()).max(0.0);
    state.scroll = state.scroll.clamp(0.0, max_scroll);
    Some((size, pitch))
}

/// An accessory tab's slots as tiles while no slot is open (Design: the
/// game reaches every accessory through pins; slots without an anchor yet
/// would otherwise be unreachable). Arguments of `btn.truck_acc`: slot
/// name, the fitted part's icon, and its name as the info line.
#[allow(clippy::too_many_arguments)]
fn slot_tiles(
    ui: &mut egui::Ui,
    cx: &mut Context<'_>,
    assets: &ScreenAssets,
    widget: &Widget,
    target: egui::Rect,
    slots: &[TabSlot<'_>],
    state: &mut ScreenState,
    clicks: &mut Vec<Click>,
) -> Option<egui::Rect> {
    let button = widget
        .look_template
        .as_deref()
        .and_then(|t| assets.templates.button(t))?;
    // The tiles take egui's hover and the screen's area claims no layer
    // space, so test the pointer position against the list itself.
    if ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| target.contains(p))
    {
        state.scroll -= state.wheel;
    }
    let ([item_w, item_h], pitch) = list_geometry(widget, slots.len(), state)?;
    let scale = cx.mapping.scale;
    let painter = cx.painter.with_clip_rect(target);
    let mut list_cx = Context {
        painter: &painter,
        game: cx.game,
        textures: cx.textures,
        localization: cx.localization,
        mapping: cx.mapping,
    };
    for (i, slot) in slots.iter().enumerate() {
        let top = i as f32 * pitch - state.scroll;
        if top + item_h < 0.0 || top > widget.rect.height() {
            continue;
        }
        let rect = egui::Rect::from_min_size(
            Pos2::new(target.left(), target.top() + top * scale),
            egui::vec2(item_w * scale, item_h * scale),
        );
        let response = ui.interact(rect.intersect(target), Id::new(("slot", i)), Sense::click());
        let markup = if response.hovered() {
            &button.selected
        } else {
            &button.normal
        };
        let args = [
            slot.label,
            slot.selected_icon.unwrap_or_default(),
            slot.selected.unwrap_or_default(),
            "",
            HIDDEN_ALPHA,
            HIDDEN_ALPHA,
        ];
        list_cx.paint(&fill(markup, &args), [item_w, item_h], rect);
        if response.clicked() {
            clicks.push(Click::Slot(slot.slot.clone()));
        }
    }
    Some(target)
}

/// `btn.truck_acc` arguments: name, icon, info, price, new badge alpha,
/// owned badge alpha, (disabled) reason; `%8`/`%9` are not filled yet.
fn tile_markup(
    button: &specsheet_gameui::ButtonTemplate,
    option: &OptionView,
    hovered: bool,
) -> String {
    let icon = option.icon.clone().unwrap_or_default();
    let price = price(option.price);
    let reason = option.reason.clone().unwrap_or_default();
    let owned = if option.selected { SHOWN } else { HIDDEN_ALPHA };
    let args = [
        option.name.as_str(),
        icon.as_str(),
        "",
        price.as_str(),
        HIDDEN_ALPHA,
        owned,
        reason.as_str(),
    ];
    let markup = if !option.compatible {
        &button.disabled
    } else if hovered {
        &button.selected
    } else {
        &button.normal
    };
    fill(markup, &args)
}

/// The slot a base tab shows directly, if any.
pub fn tab_slot(overview: &TruckOverview, tab: Tab) -> Option<SlotId> {
    if tab.has_pins() {
        return None;
    }
    overview
        .general
        .iter()
        .find(|s| s.tab == tab && s.option_count > 1)
        .or_else(|| overview.general.iter().find(|s| s.tab == tab))
        .map(|s| s.slot.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widget(id: i64, parent: Option<usize>, rect: Rect) -> Widget {
        Widget {
            name: format!("w{id}"),
            class: "ui::text_common".to_owned(),
            id,
            parent,
            rect,
            layer: 0,
            look_template: None,
            value: None,
            text: None,
            value_list: Vec::new(),
            id_list: Vec::new(),
            item_size: None,
            item_gap: None,
            min_handle: None,
        }
    }

    #[test]
    fn a_hidden_widget_takes_its_background_along() {
        let money = Rect {
            left: 1113.0,
            right: 1433.0,
            top: 893.0,
            bottom: 853.0,
        };
        let layout = Layout {
            path: String::new(),
            keep_aspect: KeepAspect::Right,
            widgets: vec![
                widget(0, None, Rect::default()),
                // `_.money` [11].
                widget(11, Some(0), money),
                // Unaddressed background with the money's rectangle.
                widget(0, Some(0), money),
                // Unaddressed widget elsewhere stays.
                widget(0, Some(0), Rect { top: 50.0, ..money }),
            ],
        };
        assert!(!visible(&layout, 1, false, false, false));
        assert!(
            !visible(&layout, 2, false, false, false),
            "background of a hidden widget"
        );
        assert!(visible(&layout, 3, false, false, false));
    }

    #[test]
    fn the_picker_replaces_the_folded_analysis_while_picking() {
        let layout = Layout {
            path: String::new(),
            keep_aspect: KeepAspect::Right,
            widgets: vec![
                widget(0, None, Rect::default()),
                widget(ids::PICKER_GROUP, Some(0), Rect::default()),
                widget(ids::PICKER, Some(1), Rect::default()),
                widget(ids::CHANNELS, Some(1), Rect::default()),
                widget(445, Some(1), Rect::default()),
                widget(ids::FOLDED_ANALYSIS, Some(0), Rect::default()),
                // The centre layout's own channel row.
                widget(ids::CHANNELS, Some(0), Rect::default()),
            ],
        };
        let shown = |i, picking| visible(&layout, i, true, true, picking);
        assert!(!shown(2, false) && shown(2, true), "hue and square");
        assert!(shown(3, true), "the picker's channel buttons");
        assert!(!shown(4, true), "save colour is not wired up");
        assert!(shown(5, false) && !shown(5, true), "folded analysis");
        assert!(!shown(6, true), "the centre channel row stays hidden");
    }

    #[test]
    fn hsv_round_trips_and_formats_as_the_ui_hex() {
        for rgb in [[1.0, 0.0, 0.0], [0.2, 0.4, 0.6], [0.5, 0.5, 0.5], [0.0; 3]] {
            let back = hsv_to_rgb(rgb_to_hsv(rgb));
            for (a, b) in rgb.iter().zip(back) {
                assert!((a - b).abs() < 1e-5, "{rgb:?} -> {back:?}");
            }
        }
        assert_eq!(hsv_to_rgb([1.0 / 3.0, 1.0, 1.0]), [0.0, 1.0, 0.0]);
        assert_eq!(ui_hex([1.0, 0.5, 0.0]), "FF007FFF");
    }

    #[test]
    fn maps_reference_space_by_height_and_aspect() {
        // 1920 × 900 window: scale 1, 480 extra units of width.
        let mapping = Mapping::new(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(1920.0, 900.0),
        ));
        let r = Rect {
            left: 0.0,
            right: 100.0,
            top: 900.0,
            bottom: 800.0,
        };
        let left = mapping.rect(KeepAspect::Left, &r);
        assert_eq!((left.left(), left.top(), left.bottom()), (0.0, 0.0, 100.0));
        assert_eq!(mapping.rect(KeepAspect::Right, &r).left(), 480.0);
        assert_eq!(mapping.rect(KeepAspect::Center, &r).left(), 240.0);
        let stretched = mapping.rect(KeepAspect::None, &r).width();
        assert!(
            (stretched - 100.0 * 1920.0 / 1440.0).abs() < 1e-3,
            "{stretched}"
        );
        // Half the height halves everything.
        let half = Mapping::new(egui::Rect::from_min_size(
            Pos2::ZERO,
            egui::vec2(720.0, 450.0),
        ));
        assert_eq!(half.rect(KeepAspect::Left, &r).width(), 50.0);
    }

    #[test]
    fn drops_images_named_by_empty_arguments() {
        let markup = "<img src=/a/%1>x<img src=/t/accessory_%9.mat color=FF>%0";
        assert_eq!(
            fill(markup, &["Cab", "cab_icon"]),
            "<img src=/a/cab_icon>xCab"
        );
    }

    #[test]
    fn prints_prices_like_the_game() {
        assert_eq!(price(Some(22563)), "€ 22 563,-");
        assert_eq!(price(Some(900)), "€ 900,-");
        assert_eq!(price(Some(1_000_000)), "€ 1 000 000,-");
        assert_eq!(price(None), "");
    }
}
