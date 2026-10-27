//! The markup language of UI templates and texts (`docs/formats/ui.md`):
//! tags such as `<img src=… width=…>`, `<color value=…>`, `</align>` and
//! plain text between them. Attribute values are bare tokens; the parser
//! only splits syntax and leaves their meaning to the caller.
//!
//! Line breaks inside a template are layout-neutral (templates wrap their
//! markup across SII string lines); they are dropped from text.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// An opening or standalone tag (`<img …>`, `<ret>`, `<align …>`).
    Open {
        name: String,
        attrs: Vec<Attr>,
    },
    /// A closing tag (`</align>`).
    Close(String),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    pub key: String,
    /// The raw token; empty for a bare key (`<nowrap>` style flags).
    pub value: String,
}

impl Node {
    /// The value of attribute `key` of an opening tag.
    pub fn attr(&self, key: &str) -> Option<&str> {
        match self {
            Node::Open { attrs, .. } => attrs
                .iter()
                .rev()
                .find(|a| a.key == key)
                .map(|a| a.value.as_str()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkupError {
    /// Byte offset of the `<` that starts the bad tag.
    pub offset: usize,
    pub message: &'static str,
}

impl fmt::Display for MarkupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "markup at byte {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for MarkupError {}

/// Splits `markup` into tags and text.
pub fn parse(markup: &str) -> Result<Vec<Node>, MarkupError> {
    let mut nodes = Vec::new();
    let mut text = String::new();
    let mut rest = markup;
    let mut offset = 0;
    while let Some(start) = rest.find('<') {
        push_text(&mut text, &rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('>').ok_or(MarkupError {
            offset: offset + start,
            message: "unterminated tag",
        })?;
        let body = &after[..end];
        if !text.is_empty() {
            nodes.push(Node::Text(std::mem::take(&mut text)));
        }
        nodes.push(tag(body).ok_or(MarkupError {
            offset: offset + start,
            message: "tag without a name",
        })?);
        let consumed = start + 1 + end + 1;
        rest = &rest[consumed..];
        offset += consumed;
    }
    push_text(&mut text, rest);
    if !text.is_empty() {
        nodes.push(Node::Text(text));
    }
    Ok(nodes)
}

fn push_text(text: &mut String, raw: &str) {
    text.extend(raw.chars().filter(|c| *c != '\n' && *c != '\r'));
}

fn tag(body: &str) -> Option<Node> {
    if let Some(name) = body.strip_prefix('/') {
        let name = name.trim();
        return (!name.is_empty()).then(|| Node::Close(name.to_owned()));
    }
    let mut words = body.split_whitespace();
    let name = words.next()?.to_owned();
    let attrs = words
        .map(|word| match word.split_once('=') {
            Some((key, value)) => Attr {
                key: key.to_owned(),
                value: value.to_owned(),
            },
            None => Attr {
                key: word.to_owned(),
                value: String::new(),
            },
        })
        .collect();
    Some(Node::Open { name, attrs })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(name: &str, attrs: &[(&str, &str)]) -> Node {
        Node::Open {
            name: name.to_owned(),
            attrs: attrs
                .iter()
                .map(|(k, v)| Attr {
                    key: (*k).to_owned(),
                    value: (*v).to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn splits_tags_attributes_and_text() {
        let nodes = parse(
            "<color value=%0><img src=/m/white.mat xscale=stretch height=-16>\n\
             <offset vshift=4>%1<ret></align>",
        )
        .expect("parse");
        assert_eq!(
            nodes,
            vec![
                open("color", &[("value", "%0")]),
                open(
                    "img",
                    &[
                        ("src", "/m/white.mat"),
                        ("xscale", "stretch"),
                        ("height", "-16")
                    ]
                ),
                open("offset", &[("vshift", "4")]),
                Node::Text("%1".to_owned()),
                open("ret", &[]),
                Node::Close("align".to_owned()),
            ]
        );
        assert_eq!(nodes[1].attr("height"), Some("-16"));
    }

    #[test]
    fn keeps_text_and_drops_line_breaks() {
        let nodes = parse("Price:\r\n 1 200 <br>€").expect("parse");
        assert_eq!(
            nodes,
            vec![
                Node::Text("Price: 1 200 ".to_owned()),
                open("br", &[]),
                Node::Text("€".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_broken_tags_without_panicking() {
        assert_eq!(parse("ok <img src=x").map_err(|e| e.offset), Err(3));
        assert!(parse("<>").is_err());
        assert!(parse("</ >").is_err());
        assert_eq!(parse("").expect("empty"), Vec::new());
    }
}
