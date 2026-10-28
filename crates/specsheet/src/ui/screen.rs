//! The game's truck configuration screen (`truck.config`), drawn from its
//! own layout files and templates (`docs/formats/ui-truck-config.md`,
//! `docs/app/ui.md`). The layouts give every widget's place and look; the
//! containers the game fills in code (tab row, accessory list) are filled
//! here from `specsheet-data::view`.

use bevy_egui::egui::{self, Align2, Color32, FontId, Id, Pos2, Sense};
use specsheet_core::{AccessoryId, SlotId};
use specsheet_data::localization::Localization;
use specsheet_data::session::GameSession;
use specsheet_data::ui::flow::{self, Font, Item, Measure};
use specsheet_data::ui::{
    substitute, KeepAspect, Layout, Rect, Screen, Templates, Widget, REFERENCE_HEIGHT,
    REFERENCE_WIDTH,
};
use specsheet_data::view::{tab_slots, OptionView, SlotOptions, Tab, TabSlot, TruckOverview};

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
    /// Shown only by behaviour not wired up yet: remove button, messages,
    /// conflict marker, paint channels, walk-camera notice and crosshair,
    /// cart, accessory origin badge, buy-mode price labels and the cart and
    /// checkout buttons, money, unfolded analysis, colour picker, control
    /// hints, tab-key hints, search and filter.
    pub const HIDDEN: [i64; 23] = [
        8005, 600, 620, 410, 333, 587, 355, 390, 391, 386, 210, 11, 6000, 420, 111, 575, 576, 577,
        578, 401, 402, 403, 404,
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
    prices: Prices,
    state: &mut ScreenState,
) -> (Vec<Click>, Vec<egui::Rect>) {
    let mut clicks = Vec::new();
    let mut blocking = Vec::new();
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
            if !visible(layout, index, list_open, state.tab.is_some()) {
                continue;
            }
            let target = cx.mapping.rect(layout.keep_aspect, &widget.rect);
            let size = [widget.rect.width(), widget.rect.height()];
            let painted = match widget.id {
                ids::TAB_ROW => tab_row(ui, cx, assets, widget, target, state, &mut clicks),
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
fn visible(layout: &Layout, index: usize, list_open: bool, tab_open: bool) -> bool {
    let hidden_itself = |widget: &Widget| {
        ids::HIDDEN.contains(&widget.id)
            || (widget.id == ids::FIRST_MESSAGE && (list_open || tab_open))
            || (widget.id == ids::COMPONENTS && !list_open)
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
    button: &specsheet_data::ui::ButtonTemplate,
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
                widget(ids::HIDDEN[10], Some(0), money),
                // Unaddressed background with the money's rectangle.
                widget(0, Some(0), money),
                // Unaddressed widget elsewhere stays.
                widget(0, Some(0), Rect { top: 50.0, ..money }),
            ],
        };
        assert!(!visible(&layout, 1, false, false));
        assert!(
            !visible(&layout, 2, false, false),
            "background of a hidden widget"
        );
        assert!(visible(&layout, 3, false, false));
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
