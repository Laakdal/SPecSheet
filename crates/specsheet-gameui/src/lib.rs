//! The game's UI screens (`docs/formats/ui.md`): layout files of widget
//! units in a 1440 × 900 reference space, the templates their looks come
//! from, UI colours and UI materials. Presentation-neutral: front ends turn
//! this into drawing and fill the containers the game fills in code.

pub mod flow;

use std::collections::HashMap;

use specsheet_formats::mat::Material;
use specsheet_formats::sii::{resolve_relative, SiiDocument, Unit};

use specsheet_data::GameFs;
use specsheet_data::{GameDataError, Result};

/// Screen descriptions (`screen_desc` units, with includes).
const SCREEN_DESC: &str = "/ui/!desc.sii";
const TEMPLATE_DIR: &str = "/ui/template";
const ACCESSORY_ICON_DIR: &str = "/material/ui/accessory";
const DEFAULT_LIST_DELIMITER: &str = "~~";

/// The UI reference size (Confirmed: `game.log.txt` prints "Setting UI
/// reference mode to: 1440x900"; root screens span exactly this).
pub const REFERENCE_WIDTH: f32 = 1440.0;
pub const REFERENCE_HEIGHT: f32 = 900.0;

/// How a window keeps its 1440-wide layout on wider screens (Hypothesis:
/// the UI scales by height and the window is pinned to one side).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeepAspect {
    /// Stretched to fill.
    #[default]
    None,
    Left,
    Center,
    Right,
}

/// A rectangle in reference units, y pointing up from the bottom left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Rect {
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.top - self.bottom
    }
}

/// One widget unit of a layout file.
#[derive(Clone, Debug, PartialEq)]
pub struct Widget {
    /// SII unit name; children refer to it through `my_parent`.
    pub name: String,
    /// Widget type, e.g. `ui::text_common`, `ui::button_row`.
    pub class: String,
    /// 0 = not addressed by code.
    pub id: i64,
    pub parent: Option<usize>,
    pub rect: Rect,
    /// Draw order within the window (Hypothesis: higher on top).
    pub layer: i64,
    pub look_template: Option<String>,
    /// Template arguments, `|`-separated.
    pub value: Option<String>,
    /// Raw markup (`ui::text`).
    pub text: Option<String>,
    /// Button rows: one button per item, with `id_list` parallel.
    pub value_list: Vec<String>,
    pub id_list: Vec<i64>,
    /// Lists: item size and gap.
    pub item_size: Option<[f32; 2]>,
    pub item_gap: Option<f32>,
    /// Sliders: smallest handle length (`minimal_handle_size`).
    pub min_handle: Option<f32>,
}

/// A layout file: one window root and its widgets.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub path: String,
    pub keep_aspect: KeepAspect,
    pub widgets: Vec<Widget>,
}

impl Layout {
    /// The first widget with `id`.
    pub fn by_id(&self, id: i64) -> Option<&Widget> {
        self.widgets.iter().find(|w| w.id == id)
    }

    /// The direct children of widget `index`.
    pub fn children(&self, index: usize) -> impl Iterator<Item = (usize, &Widget)> {
        self.widgets
            .iter()
            .enumerate()
            .filter(move |(_, w)| w.parent == Some(index))
    }
}

/// A screen: its layout files in `ui_script[]` order, plus `background`.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// Localisation reference of the title (`@@…@@`).
    pub title: Option<String>,
    pub layouts: Vec<Layout>,
}

/// Loads screen `name` (e.g. `truck.config`) from the screen descriptions.
pub fn load_screen(fs: &GameFs, name: &str) -> Result<Screen> {
    let doc = fs.read_sii(SCREEN_DESC)?;
    let unit = doc.unit(name).ok_or_else(|| GameDataError::Definition {
        path: SCREEN_DESC.to_owned(),
        message: format!("no screen_desc {name}"),
    })?;
    let paths = unit
        .array_str("ui_script")
        .into_iter()
        .filter_map(|script| script.split_once('|').map(|(_, path)| path))
        .chain(unit.get_str("background"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let layouts = paths
        .iter()
        .map(|path| load_layout(fs, path))
        .collect::<Result<_>>()?;
    Ok(Screen {
        title: unit.get_str("title").map(str::to_owned),
        layouts,
    })
}

pub fn load_layout(fs: &GameFs, path: &str) -> Result<Layout> {
    let doc = fs.read_sii(path)?;
    Ok(layout_from_doc(path, &doc))
}

fn layout_from_doc(path: &str, doc: &SiiDocument) -> Layout {
    let units: Vec<&Unit> = doc
        .units
        .iter()
        .filter(|u| u.get("coords_l").is_some())
        .collect();
    let index: HashMap<&str, usize> = units
        .iter()
        .enumerate()
        .map(|(i, u)| (u.name.as_str(), i))
        .collect();
    let mut keep_aspect = KeepAspect::None;
    let widgets = units
        .iter()
        .map(|unit| {
            let parent = unit
                .get_str("my_parent")
                .and_then(|name| index.get(name).copied());
            if parent.is_none() {
                if let Some(aspect) = unit.get_str("keep_aspect") {
                    keep_aspect = match aspect {
                        "left" => KeepAspect::Left,
                        "center" => KeepAspect::Center,
                        "right" => KeepAspect::Right,
                        _ => KeepAspect::None,
                    };
                }
            }
            widget(unit, parent)
        })
        .collect();
    Layout {
        path: path.to_owned(),
        keep_aspect,
        widgets,
    }
}

fn widget(unit: &Unit, parent: Option<usize>) -> Widget {
    let f = |key: &str| unit.get_f32(key).unwrap_or(0.0);
    let text = |key: &str| {
        unit.get_str(key)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let delimiter = unit
        .get_str("list_delimiter")
        .filter(|d| !d.is_empty())
        .unwrap_or(DEFAULT_LIST_DELIMITER);
    let list = |key: &str| -> Vec<String> {
        unit.get_str(key)
            .filter(|s| !s.is_empty())
            .map(|s| s.split(delimiter).map(str::to_owned).collect())
            .unwrap_or_default()
    };
    Widget {
        name: unit.name.clone(),
        class: unit.class.clone(),
        id: unit.get_i64("id").unwrap_or(0),
        parent,
        rect: Rect {
            left: f("coords_l"),
            right: f("coords_r"),
            top: f("coords_t"),
            bottom: f("coords_b"),
        },
        layer: unit.get_i64("layer").unwrap_or(0),
        look_template: text("look_template"),
        value: text("value"),
        text: text("text"),
        id_list: list("id_list")
            .iter()
            .filter_map(|id| id.trim().parse().ok())
            .collect(),
        value_list: list("value_list"),
        item_size: unit
            .get_f32("item_width")
            .zip(unit.get_f32("item_height"))
            .map(|(w, h)| [w, h]),
        item_gap: unit.get_f32("item_gap"),
        min_handle: unit.get_f32("minimal_handle_size"),
    }
}

/// A text look (`ui::text_template`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextTemplate {
    pub markup: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// A button look per state (`ui::button_template`).
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonTemplate {
    pub normal: String,
    /// Selected or hovered.
    pub selected: String,
    pub pressed: String,
    pub disabled: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

/// A vertical scrollbar look (`ui::slider_template`, `*_vert` fields).
#[derive(Clone, Debug, PartialEq)]
pub struct SliderTemplate {
    pub handle: String,
    pub handle_hovered: String,
    pub handle_pressed: String,
    pub track: String,
    /// Space kept free at both ends of the track.
    pub margin: f32,
}

/// Every text and button template under `/ui/template`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Templates {
    text: HashMap<String, TextTemplate>,
    buttons: HashMap<String, ButtonTemplate>,
    sliders: HashMap<String, SliderTemplate>,
    /// Every template by name with its class, including kinds without a
    /// typed model yet (sliders, input lines, tables, tooltips).
    classes: HashMap<String, String>,
}

impl Templates {
    pub fn text(&self, name: &str) -> Option<&TextTemplate> {
        self.text.get(name)
    }

    pub fn button(&self, name: &str) -> Option<&ButtonTemplate> {
        self.buttons.get(name)
    }

    pub fn slider(&self, name: &str) -> Option<&SliderTemplate> {
        self.sliders.get(name)
    }

    /// Every text template and button state as `(name, markup)`.
    pub fn markups(&self) -> impl Iterator<Item = (&str, &str)> {
        let text = self
            .text
            .iter()
            .map(|(name, t)| (name.as_str(), t.markup.as_str()));
        let buttons = self.buttons.iter().flat_map(|(name, b)| {
            [&b.normal, &b.selected, &b.pressed, &b.disabled]
                .map(|markup| (name.as_str(), markup.as_str()))
        });
        text.chain(buttons)
    }

    /// The class of template `name`, e.g. `slider_template`.
    pub fn class(&self, name: &str) -> Option<&str> {
        self.classes.get(name).map(String::as_str)
    }

    fn add(&mut self, doc: &SiiDocument) {
        let markup = |unit: &Unit, key: &str| unit.get_str(key).unwrap_or_default().to_owned();
        for unit in &doc.units {
            if unit.class.ends_with("_template") {
                self.classes.insert(unit.name.clone(), unit.class.clone());
            }
            match unit.class.as_str() {
                "ui::text_template" => {
                    self.text.insert(
                        unit.name.clone(),
                        TextTemplate {
                            markup: markup(unit, "text"),
                            width: unit.get_f32("width"),
                            height: unit.get_f32("height"),
                        },
                    );
                }
                "ui::slider_template" => {
                    let handle = markup(unit, "n_pml_vert");
                    let or_handle = |key: &str| {
                        unit.get_str(key)
                            .filter(|s| !s.is_empty())
                            .map_or_else(|| handle.clone(), str::to_owned)
                    };
                    self.sliders.insert(
                        unit.name.clone(),
                        SliderTemplate {
                            handle_hovered: or_handle("s_pml_vert"),
                            handle_pressed: or_handle("p_pml_vert"),
                            track: markup(unit, "n_background_vert"),
                            margin: unit
                                .get_f32("margin_vert")
                                .filter(|m| m.is_finite())
                                .unwrap_or(0.0),
                            handle: handle.clone(),
                        },
                    );
                }
                "ui::button_template" => {
                    let normal = markup(unit, "n_pml");
                    let or_normal = |key: &str| {
                        unit.get_str(key)
                            .filter(|s| !s.is_empty())
                            .map_or_else(|| normal.clone(), str::to_owned)
                    };
                    self.buttons.insert(
                        unit.name.clone(),
                        ButtonTemplate {
                            selected: or_normal("s_pml"),
                            pressed: or_normal("p_pml"),
                            disabled: or_normal("d_pml"),
                            normal: normal.clone(),
                            width: unit.get_f32("width"),
                            height: unit.get_f32("height"),
                        },
                    );
                }
                _ => {}
            }
        }
    }
}

/// Loads every template file. Files that fail to parse are skipped and
/// their names returned, so one broken file does not hide the rest.
pub fn load_templates(fs: &GameFs) -> Result<(Templates, Vec<String>)> {
    let mut templates = Templates::default();
    let mut failed = Vec::new();
    for file in fs.list_dir(TEMPLATE_DIR)?.files {
        if !file.ends_with(".sii") {
            continue;
        }
        let path = format!("{TEMPLATE_DIR}/{file}");
        match fs.read_sii(&path) {
            Ok(doc) => templates.add(&doc),
            Err(_) => failed.push(path),
        }
    }
    Ok((templates, failed))
}

/// Substitutes the template parameters `%0`…`%9` with `args`; missing
/// arguments become empty. Other `%` sequences are kept.
pub fn substitute(template: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek().and_then(|d| d.to_digit(10))) {
            ('%', Some(digit)) => {
                chars.next();
                out.push_str(args.get(digit as usize).copied().unwrap_or_default());
            }
            _ => out.push(c),
        }
    }
    out
}

/// A UI colour `AABBGGRR` (Hypothesis, docs/formats/ui.md) as straight
/// RGBA bytes. Accepts an optional `0x` prefix.
pub fn parse_color(text: &str) -> Option<[u8; 4]> {
    let hex = text
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if hex.len() != 8 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    let [a, b, g, r] = value.to_be_bytes();
    Some([r, g, b, a])
}

/// A UI material: what an `<img src=…>` draws.
#[derive(Clone, Debug, PartialEq)]
pub struct UiMaterial {
    pub effect: String,
    /// Absolute `.tobj` path.
    pub texture: Option<String>,
    /// Signed-distance glyphs tinted by `color=` (`ui.sdf.rfx`,
    /// `ui.font.msdf.rfx`) rather than plain images.
    pub distance_field: bool,
}

/// Loads the material an `<img src=…>` names; a missing extension means
/// `.mat` (the accessory tile template writes `…/accessory/%1`).
pub fn load_ui_material(fs: &GameFs, src: &str) -> Result<UiMaterial> {
    let path = if src.rsplit('/').next().is_some_and(|f| f.contains('.')) {
        src.to_owned()
    } else {
        format!("{src}.mat")
    };
    let material = Material::parse(&fs.read(&path)?).map_err(|source| GameDataError::Format {
        path: path.clone(),
        source,
    })?;
    let texture = material
        .textures
        .iter()
        .find_map(|t| t.source.as_deref())
        .map(|source| resolve_relative(&path, source));
    Ok(UiMaterial {
        distance_field: material.effect.contains("sdf"),
        effect: material.effect,
        texture,
    })
}

/// The material of an accessory's `icon` attribute (Confirmed for 5,848 of
/// 5,851 truck icons).
pub fn accessory_icon(icon: &str) -> String {
    format!("{ACCESSORY_ICON_DIR}/{icon}.mat")
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_formats::sii;

    #[test]
    fn substitutes_single_digit_parameters_only() {
        assert_eq!(
            substitute("<color value=%0>%1 %2", &["FF00FF00", "Cab"]),
            "<color value=FF00FF00>Cab "
        );
        // `%0FFFFFF` is parameter 0 followed by literal digits.
        assert_eq!(substitute("color=%0FFFFFF", &["80"]), "color=80FFFFFF");
        assert_eq!(substitute("100% %#", &[]), "100% %#");
    }

    #[test]
    fn reads_colours_as_aabbggrr() {
        // `clr_red` = FF4C4CFC is red only in this byte order.
        assert_eq!(parse_color("FF4C4CFC"), Some([0xFC, 0x4C, 0x4C, 0xFF]));
        assert_eq!(parse_color("0xC5000000"), Some([0, 0, 0, 0xC5]));
        assert_eq!(parse_color("FFF"), None);
        assert_eq!(parse_color("GG000000"), None);
    }

    #[test]
    fn builds_the_widget_tree_with_rows_and_lists() {
        let src = br#"SiiNunit
{
ui::window : _nameless.root {
 keep_aspect: left
 coords_l: 0
 coords_r: 1440
 coords_t: 900
 coords_b: 0
 id: 0
 layer: 0
 my_parent: null
}
ui::button_row : _nameless.tabs {
 value_list: "cabin~~chassis"
 id_list: "510~~500"
 list_delimiter: "~~"
 look_template: btn.truck_grp
 coords_l: 464
 coords_r: 976
 coords_t: 888
 coords_b: 824
 id: 499
 layer: 4
 my_parent: _nameless.root
}
ui::slider_common : _nameless.slider {
 look_template: sld.slider0d
 minimal_handle_size: 32
 coords_l: 309
 coords_r: 343
 coords_t: 700
 coords_b: 100
 my_parent: _nameless.acclist
}
ui::list_common : _nameless.acclist {
 item_width: 292
 item_height: 96
 item_gap: 4
 coords_l: 10
 coords_r: 310
 coords_t: 700
 coords_b: 100
 id: 300
 my_parent: _nameless.root
}
truck_config_hdl : _nameless.handler {
 window_link: _nameless.root
}
}"#;
        let doc = sii::parse("/ui/x.sii", src).expect("parse");
        let layout = layout_from_doc("/ui/x.sii", &doc);
        assert_eq!(layout.keep_aspect, KeepAspect::Left);
        assert_eq!(layout.widgets.len(), 4, "the handler is not a widget");
        let tabs = layout.by_id(499).expect("tabs");
        assert_eq!(tabs.value_list, ["cabin", "chassis"]);
        assert_eq!(tabs.id_list, [510, 500]);
        assert_eq!(tabs.parent, Some(0));
        assert_eq!(tabs.rect.height(), 64.0);
        let list = layout.by_id(300).expect("list");
        assert_eq!(list.item_size, Some([292.0, 96.0]));
        let slider = layout
            .widgets
            .iter()
            .find(|w| w.class == "ui::slider_common")
            .expect("slider");
        assert_eq!(slider.min_handle, Some(32.0));
        assert_eq!(slider.parent.map(|p| layout.widgets[p].id), Some(300));
        assert_eq!(layout.children(0).count(), 2);
        let mut templates = Templates::default();
        let doc = sii::parse(
            "/ui/template/slider.sii",
            br#"SiiNunit
{
ui::slider_template : sld.v {
 n_pml_vert: "<img src=/h2.mat>"
 s_pml_vert: "<img src=/h1.mat>"
 p_pml_vert: ""
 n_background_vert: "<img src=/white.mat height=-1 width=2>"
 margin_vert: 2
}
}"#,
        )
        .expect("parse");
        templates.add(&doc);
        let slider = templates.slider("sld.v").expect("slider");
        assert_eq!(slider.handle_hovered, "<img src=/h1.mat>");
        assert_eq!(
            slider.handle_pressed, slider.handle,
            "empty states fall back"
        );
        assert_eq!(slider.margin, 2.0);
    }
}
