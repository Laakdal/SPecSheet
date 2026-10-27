//! Lays out UI markup (`specsheet_formats::ui_markup`) inside a widget box,
//! producing positioned images and text runs. The rules are inferred from
//! templates whose geometry must add up (all Hypothesis, see
//! `docs/formats/ui.md`, "Markup layout"):
//!
//! * The cursor starts at the box's top-left; coordinates grow right and
//!   down, in reference units.
//! * `<ret>` returns the cursor to the origin; `<br>` starts a new line.
//! * `<offset hshift= vshift=>` moves the cursor to an absolute position
//!   (from the margin's left edge and the box top).
//! * `<img>` takes its natural size from the texel span `left`/`right`/
//!   `top`/`bottom` (`pN` = texel N; reversed ranges mirror). `width`/
//!   `height` override it; a negative value is the space left on the line
//!   (or below) minus N; `xscale`/`yscale` `stretch`/`repeat` fill it.
//! * `<margin left= right=>` and `<align … left= right= top= bottom=>` edges:
//!   a positive value is an absolute coordinate, zero or negative an inset
//!   from the far edge.
//! * `<align>` lays its content out from the top-left of its own box (from
//!   the current line down to the bottom), then shifts it as a block.

use specsheet_formats::ui_markup::Node;

use super::parse_color;

/// A font selection: game font file (`None` = default) and scale.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub face: Option<String>,
    pub scale: [f32; 2],
}

impl Default for Font {
    fn default() -> Self {
        Self {
            face: None,
            scale: [1.0, 1.0],
        }
    }
}

/// Sizes the layout needs from the renderer.
pub trait Measure {
    /// Texel size of a UI material's texture; `None` if it cannot be read.
    fn image_size(&mut self, src: &str) -> Option<[f32; 2]>;
    /// Size of `text` set in `font`.
    fn text_size(&mut self, text: &str, font: &Font) -> [f32; 2];
}

/// `[left, top, right, bottom]`, y down from the box top.
pub type Bounds = [f32; 4];

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Image {
        bounds: Bounds,
        src: String,
        /// Texel range `[left, top, right, bottom]`; reversed ranges mirror.
        texels: Bounds,
        /// Straight RGBA tint.
        color: [u8; 4],
    },
    Text {
        bounds: Bounds,
        text: String,
        font: Font,
        color: [u8; 4],
    },
}

impl Item {
    pub fn bounds(&self) -> Bounds {
        match self {
            Item::Image { bounds, .. } | Item::Text { bounds, .. } => *bounds,
        }
    }

    fn shift(&mut self, dx: f32, dy: f32) {
        let (Item::Image { bounds, .. } | Item::Text { bounds, .. }) = self;
        bounds[0] += dx;
        bounds[2] += dx;
        bounds[1] += dy;
        bounds[3] += dy;
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Start,
    Center,
    End,
    Keep,
}

struct AlignFrame {
    first_item: usize,
    hstyle: Style,
    vstyle: Style,
    frame: Bounds,
    saved: Cursor,
}

#[derive(Clone, Copy)]
struct Cursor {
    x: f32,
    y: f32,
    line_height: f32,
    /// Horizontal flow range `[left, right]`.
    margin: [f32; 2],
    /// Box the flow fills: `[left, top, right, bottom]`.
    frame: Bounds,
}

const WHITE: [u8; 4] = [255; 4];

/// Parses `markup` and lays it out in a box of `size`.
pub fn layout_markup(
    markup: &str,
    size: [f32; 2],
    measure: &mut dyn Measure,
) -> Result<Vec<Item>, String> {
    let nodes = specsheet_formats::ui_markup::parse(markup).map_err(|e| e.to_string())?;
    Ok(layout(&nodes, size, measure))
}

/// Lays out `nodes` in a box of `size` (reference units).
pub fn layout(nodes: &[Node], size: [f32; 2], measure: &mut dyn Measure) -> Vec<Item> {
    let frame = [0.0, 0.0, size[0], size[1]];
    let mut cursor = Cursor {
        x: 0.0,
        y: 0.0,
        line_height: 0.0,
        margin: [0.0, size[0]],
        frame,
    };
    let mut color = WHITE;
    let mut fonts = vec![Font::default()];
    let mut aligns: Vec<AlignFrame> = Vec::new();
    let mut items = Vec::new();
    for node in nodes {
        match node {
            Node::Text(text) => {
                let font = fonts.last().cloned().unwrap_or_default();
                let [w, h] = measure.text_size(text, &font);
                items.push(Item::Text {
                    bounds: [cursor.x, cursor.y, cursor.x + w, cursor.y + h],
                    text: text.clone(),
                    font,
                    color,
                });
                cursor.x += w;
                cursor.line_height = cursor.line_height.max(h);
            }
            Node::Open { name, .. } => match name.as_str() {
                "color" => {
                    if let Some(c) = node.attr("value").and_then(parse_color) {
                        color = c;
                    }
                }
                "font" => {
                    let base = fonts.last().cloned().unwrap_or_default();
                    let scale = |key: &str, default: f32| {
                        node.attr(key)
                            .and_then(|v| v.parse::<f32>().ok())
                            .filter(|v| v.is_finite() && *v > 0.0)
                            .unwrap_or(default)
                    };
                    fonts.push(Font {
                        face: node.attr("face").map(str::to_owned).or(base.face),
                        scale: [scale("xscale", 1.0), scale("yscale", 1.0)],
                    });
                }
                "margin" => {
                    let [l, _, r, _] = cursor.frame;
                    cursor.margin = [
                        edge(node.attr("left"), l, l),
                        edge(node.attr("right"), l, r),
                    ];
                    cursor.x = cursor.x.max(cursor.margin[0]);
                }
                "offset" => {
                    cursor.x = cursor.margin[0] + number(node.attr("hshift")).unwrap_or(0.0);
                    cursor.y = cursor.frame[1] + number(node.attr("vshift")).unwrap_or(0.0);
                    cursor.line_height = 0.0;
                }
                "ret" => {
                    cursor.x = cursor.margin[0];
                    cursor.y = cursor.frame[1];
                    cursor.line_height = 0.0;
                }
                "br" => {
                    let font = fonts.last().cloned().unwrap_or_default();
                    let line = if cursor.line_height > 0.0 {
                        cursor.line_height
                    } else {
                        measure.text_size(" ", &font)[1]
                    };
                    cursor.x = cursor.margin[0];
                    cursor.y += line;
                    cursor.line_height = 0.0;
                }
                "align" => {
                    let [fl, _, fr, fb] = cursor.frame;
                    let top = cursor.y;
                    let frame = [
                        edge(node.attr("left"), fl, cursor.margin[0]),
                        edge(node.attr("top"), top, top),
                        edge(node.attr("right"), fl, cursor.margin[1].min(fr)),
                        edge(node.attr("bottom"), top, fb),
                    ];
                    aligns.push(AlignFrame {
                        first_item: items.len(),
                        hstyle: style(node.attr("hstyle")),
                        vstyle: style(node.attr("vstyle")),
                        frame,
                        saved: cursor,
                    });
                    cursor = Cursor {
                        x: frame[0],
                        y: frame[1],
                        line_height: 0.0,
                        margin: [frame[0], frame[2]],
                        frame,
                    };
                }
                "img" => {
                    if let Some(item) = image(node, &cursor, color, measure) {
                        let [_, t, r, b] = item.bounds();
                        cursor.x = r;
                        cursor.line_height = cursor.line_height.max(b - t);
                        items.push(item);
                    }
                }
                _ => {}
            },
            Node::Close(name) => match name.as_str() {
                "font" if fonts.len() > 1 => {
                    fonts.pop();
                }
                "margin" => {
                    cursor.margin = [cursor.frame[0], cursor.frame[2]];
                }
                "align" => {
                    if let Some(frame) = aligns.pop() {
                        align(&mut items[frame.first_item..], &frame);
                        cursor = frame.saved;
                    }
                }
                _ => {}
            },
        }
    }
    // Unclosed aligns still position their content.
    while let Some(frame) = aligns.pop() {
        align(&mut items[frame.first_item..], &frame);
    }
    items
}

fn image(node: &Node, cursor: &Cursor, color: [u8; 4], measure: &mut dyn Measure) -> Option<Item> {
    let src = node.attr("src")?.to_owned();
    let [tex_w, tex_h] = measure.image_size(&src)?;
    let texel = |key: &str, default: f32| {
        node.attr(key)
            .and_then(|v| v.strip_prefix('p'))
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    };
    let texels = [
        texel("left", 0.0),
        texel("top", 0.0),
        texel("right", tex_w),
        texel("bottom", tex_h),
    ];
    let fills = |key: &str| matches!(node.attr(key), Some("stretch" | "repeat"));
    let room_x = cursor.margin[1] - cursor.x;
    let room_y = cursor.frame[3] - cursor.y;
    let extent = |key: &str, room: f32, span: f32, fill: bool| match number(node.attr(key)) {
        Some(v) if v < 0.0 => (room + v).max(0.0),
        Some(v) => v,
        None if fill => room.max(0.0),
        None => span,
    };
    let w = extent(
        "width",
        room_x,
        (texels[2] - texels[0]).abs(),
        fills("xscale"),
    );
    let h = extent(
        "height",
        room_y,
        (texels[3] - texels[1]).abs(),
        fills("yscale"),
    );
    let color = node.attr("color").and_then(parse_color).unwrap_or(color);
    Some(Item::Image {
        bounds: [cursor.x, cursor.y, cursor.x + w, cursor.y + h],
        src,
        texels,
        color,
    })
}

fn align(items: &mut [Item], frame: &AlignFrame) {
    let Some(content) = items.iter().map(Item::bounds).reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    }) else {
        return;
    };
    let offset = |style: Style, lo: f32, hi: f32, from: f32, to: f32| match style {
        Style::Start => lo - from,
        Style::Center => (lo + hi) / 2.0 - (from + to) / 2.0,
        Style::End => hi - to,
        Style::Keep => 0.0,
    };
    let [l, t, r, b] = frame.frame;
    let dx = offset(frame.hstyle, l, r, content[0], content[2]);
    let dy = offset(frame.vstyle, t, b, content[1], content[3]);
    for item in items {
        item.shift(dx, dy);
    }
}

fn style(value: Option<&str>) -> Style {
    match value {
        Some("left" | "top") => Style::Start,
        Some("center") => Style::Center,
        Some("right" | "bottom") => Style::End,
        _ => Style::Keep,
    }
}

fn number(value: Option<&str>) -> Option<f32> {
    value?.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// An edge attribute: positive values are absolute (from `origin`), zero
/// or negative ones inset from `far`. Missing: `default`.
fn edge(value: Option<&str>, origin: f32, default: f32) -> f32 {
    match number(value) {
        Some(v) if v > 0.0 => origin + v,
        Some(v) => default + v,
        None => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specsheet_formats::ui_markup::parse;

    /// 16 × 16 corners, 1 × 1 white, 256 × 64 icons; text is 8 units per
    /// character and 10 high.
    struct Fake;

    impl Measure for Fake {
        fn image_size(&mut self, src: &str) -> Option<[f32; 2]> {
            if src.contains("corner") {
                Some([16.0, 16.0])
            } else if src.contains("white") {
                Some([1.0, 1.0])
            } else if src.contains("missing") {
                None
            } else {
                Some([256.0, 64.0])
            }
        }

        fn text_size(&mut self, text: &str, font: &Font) -> [f32; 2] {
            [
                8.0 * text.chars().count() as f32 * font.scale[0],
                10.0 * font.scale[1],
            ]
        }
    }

    fn run(markup: &str, size: [f32; 2]) -> Vec<Item> {
        layout(&parse(markup).expect("markup"), size, &mut Fake)
    }

    fn bounds(items: &[Item]) -> Vec<Bounds> {
        items.iter().map(Item::bounds).collect()
    }

    /// `txt.window.bcg_rect4` (rounded panel background) fills its box
    /// exactly: 4-unit top strip, middle, 16-unit bottom strip.
    #[test]
    fn rounded_background_fills_the_box() {
        let items = run(
            "<color value=C5000000>\
             <img src=/c/corner_4px.mat bottom=p4>\
             <img src=/c/corner_4px.mat left=p15 width=-16 bottom=p4>\
             <img src=/c/corner_4px.mat left=p16 right=p0 bottom=p4><ret>\
             <offset vshift=4><img src=/c/white.mat xscale=stretch height=-16>\
             <align vstyle=bottom>\
             <img src=/c/corner_4px.mat top=p16 bottom=p0>\
             <img src=/c/corner_4px.mat top=p16 bottom=p0 left=p16 width=-16>\
             <img src=/c/corner_4px.mat top=p16 bottom=p0 left=p16 right=p0></align>",
            [100.0, 50.0],
        );
        assert_eq!(
            bounds(&items),
            vec![
                [0.0, 0.0, 16.0, 4.0],
                [16.0, 0.0, 84.0, 4.0],
                [84.0, 0.0, 100.0, 4.0],
                [0.0, 4.0, 100.0, 34.0],
                [0.0, 34.0, 16.0, 50.0],
                [16.0, 34.0, 84.0, 50.0],
                [84.0, 34.0, 100.0, 50.0],
            ]
        );
        // The right corner is the texture mirrored; colours are AABBGGRR.
        let Item::Image { texels, color, .. } = &items[2] else {
            panic!("image")
        };
        assert_eq!(*texels, [16.0, 0.0, 0.0, 4.0]);
        assert_eq!(*color, [0, 0, 0, 0xC5]);
    }

    /// `btn.truck_acc` (accessory tile): two bands make the 96-unit item,
    /// the icon sits at (10, 30) and the price at the bottom right.
    #[test]
    fn accessory_tile_places_icon_name_and_price() {
        let items = run(
            "<nowrap><img src=/w/white.mat xscale=repeat height=22><ret>\
             <offset vshift=22><img src=/w/white.mat xscale=repeat height=74><ret>\
             <offset hshift=10 vshift=30><img src=/a/cab><ret>\
             <offset hshift=12 vshift=4><font face=/font/normal.font>Highline<ret>\
             <align vstyle=bottom hstyle=right right=-10 bottom=-7>€ 900</align></font>",
            [292.0, 96.0],
        );
        assert_eq!(
            bounds(&items),
            vec![
                [0.0, 0.0, 292.0, 22.0],
                [0.0, 22.0, 292.0, 96.0],
                [10.0, 30.0, 266.0, 94.0],
                [12.0, 4.0, 76.0, 14.0],
                [242.0, 79.0, 282.0, 89.0],
            ]
        );
        let Item::Text { font, .. } = &items[3] else {
            panic!("text")
        };
        assert_eq!(font.face.as_deref(), Some("/font/normal.font"));
    }

    #[test]
    fn margins_lines_and_unknown_images() {
        let items = run(
            "<margin left=15 right=-15>ab<br>c</margin><img src=/missing>\
             <offset hshift=5>d",
            [100.0, 40.0],
        );
        assert_eq!(
            bounds(&items),
            vec![
                [15.0, 0.0, 31.0, 10.0],
                [15.0, 10.0, 23.0, 20.0],
                [5.0, 0.0, 13.0, 10.0],
            ]
        );
    }
}
