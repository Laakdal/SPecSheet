//! Material files (`.mat`), e.g. `/automat/fe/fe81458fc4e29314.mat`:
//!
//! ```text
//! effect : "eut2.dif.spec.weight.add.env.rfx" {
//!     diffuse : { 1.000000 , 1.000000 , 1.000000 }
//!     shininess : 20.000000
//!     texture : "texture_base" {
//!         source : "/vehicle/truck/scania_2016/plastic_hard.tobj"
//!         sampler : default
//!     }
//! }
//! ```
//!
//! Older UI materials use the legacy form
//! `material : "ui" { texture : "icon.tobj" texture_name : "texture" }`;
//! it parses to the same structure (effect `ui`, one texture).

use crate::binary::FormatError;

const WHAT: &str = "mat";

/// A scalar, string or `{ … }` tuple value.
#[derive(Clone, Debug, PartialEq)]
pub enum MatValue {
    Text(String),
    Tuple(Vec<String>),
}

impl MatValue {
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            MatValue::Text(t) => t.parse().ok(),
            MatValue::Tuple(_) => None,
        }
    }

    pub fn as_floats(&self) -> Option<Vec<f32>> {
        match self {
            MatValue::Tuple(items) => items.iter().map(|i| i.parse().ok()).collect(),
            MatValue::Text(t) => t.parse().ok().map(|v| vec![v]),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            MatValue::Text(t) => Some(t),
            MatValue::Tuple(_) => None,
        }
    }
}

/// One `texture : "<slot>" { … }` block.
#[derive(Clone, Debug, PartialEq)]
pub struct MatTexture {
    /// Shader slot, e.g. `texture_base`, `texture_nmap`.
    pub name: String,
    /// Path of the `.tobj`, absolute or relative to the material file.
    pub source: Option<String>,
    pub attributes: Vec<(String, MatValue)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// Shader effect, e.g. `eut2.truckpaint.rfx`.
    pub effect: String,
    pub attributes: Vec<(String, MatValue)>,
    pub textures: Vec<MatTexture>,
}

impl Material {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let text = String::from_utf8_lossy(bytes);
        let mut lexer = Lexer::new(&text);
        match lexer.next()? {
            Some(Tok::Word(w)) if w == "effect" || w == "material" => {}
            other => {
                return Err(lexer.error(format!("expected `effect`, found {other:?}")));
            }
        }
        lexer.expect(Tok::Colon)?;
        let effect = lexer.string()?;
        lexer.expect(Tok::Open)?;
        let mut attributes = Vec::new();
        let mut textures = Vec::new();
        loop {
            match lexer.next()? {
                Some(Tok::Close) => break,
                Some(Tok::Word(key)) => {
                    lexer.expect(Tok::Colon)?;
                    if key == "texture" {
                        let name = lexer.string()?;
                        if lexer.peek()? == Some(Tok::Open) {
                            textures.push(texture_block(&mut lexer, name)?);
                        } else {
                            // Legacy form: the string is the source; a
                            // later `texture_name` names the slot.
                            textures.push(MatTexture {
                                name: String::new(),
                                source: Some(name),
                                attributes: Vec::new(),
                            });
                        }
                    } else if key == "texture_name"
                        && textures.last().is_some_and(|t| t.name.is_empty())
                    {
                        let value = lexer.value()?;
                        if let Some(texture) = textures.last_mut() {
                            texture.name = value.as_str().unwrap_or_default().to_owned();
                        }
                    } else {
                        attributes.push((key, lexer.value()?));
                    }
                }
                other => return Err(lexer.error(format!("unexpected {other:?}"))),
            }
        }
        Ok(Self {
            effect,
            attributes,
            textures,
        })
    }

    pub fn attribute(&self, key: &str) -> Option<&MatValue> {
        self.attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn texture(&self, slot: &str) -> Option<&MatTexture> {
        self.textures.iter().find(|t| t.name == slot)
    }
}

fn texture_block(lexer: &mut Lexer<'_>, name: String) -> Result<MatTexture, FormatError> {
    lexer.expect(Tok::Open)?;
    let mut texture = MatTexture {
        name,
        source: None,
        attributes: Vec::new(),
    };
    loop {
        match lexer.next()? {
            Some(Tok::Close) => return Ok(texture),
            Some(Tok::Word(key)) => {
                lexer.expect(Tok::Colon)?;
                let value = lexer.value()?;
                if key == "source" {
                    texture.source = value.as_str().map(str::to_owned);
                } else {
                    texture.attributes.push((key, value));
                }
            }
            other => return Err(lexer.error(format!("unexpected {other:?} in texture"))),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Colon,
    Comma,
    Open,
    Close,
}

#[derive(Clone)]
struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    src: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            chars: src.char_indices().peekable(),
            src,
        }
    }

    fn error(&self, message: impl Into<String>) -> FormatError {
        FormatError::new(WHAT, message)
    }

    fn next(&mut self) -> Result<Option<Tok>, FormatError> {
        while let Some(&(_, c)) = self.chars.peek() {
            if c.is_whitespace() {
                self.chars.next();
            } else if c == '#' || (c == '/' && self.src[self.pos()..].starts_with("//")) {
                while self.chars.peek().is_some_and(|&(_, c)| c != '\n') {
                    self.chars.next();
                }
            } else {
                break;
            }
        }
        let Some((start, c)) = self.chars.next() else {
            return Ok(None);
        };
        Ok(Some(match c {
            ':' => Tok::Colon,
            ',' => Tok::Comma,
            '{' => Tok::Open,
            '}' => Tok::Close,
            '"' => {
                let mut out = String::new();
                loop {
                    match self.chars.next() {
                        Some((_, '"')) => break Tok::Str(out),
                        Some((_, c)) => out.push(c),
                        None => return Err(self.error("unterminated string")),
                    }
                }
            }
            _ => {
                let mut end = start + c.len_utf8();
                while let Some(&(i, c)) = self.chars.peek() {
                    if c.is_whitespace() || matches!(c, ':' | ',' | '{' | '}' | '"') {
                        break;
                    }
                    end = i + c.len_utf8();
                    self.chars.next();
                }
                Tok::Word(self.src[start..end].to_owned())
            }
        }))
    }

    /// The next token without consuming it.
    fn peek(&mut self) -> Result<Option<Tok>, FormatError> {
        self.clone().next()
    }

    fn pos(&mut self) -> usize {
        self.chars.peek().map_or(self.src.len(), |&(i, _)| i)
    }

    fn expect(&mut self, expected: Tok) -> Result<(), FormatError> {
        match self.next()? {
            Some(t) if t == expected => Ok(()),
            other => Err(self.error(format!("expected {expected:?}, found {other:?}"))),
        }
    }

    fn string(&mut self) -> Result<String, FormatError> {
        match self.next()? {
            Some(Tok::Str(s)) => Ok(s),
            other => Err(self.error(format!("expected string, found {other:?}"))),
        }
    }

    fn value(&mut self) -> Result<MatValue, FormatError> {
        match self.next()? {
            Some(Tok::Str(s)) | Some(Tok::Word(s)) => Ok(MatValue::Text(s)),
            Some(Tok::Open) => {
                let mut items = Vec::new();
                loop {
                    match self.next()? {
                        Some(Tok::Close) => return Ok(MatValue::Tuple(items)),
                        Some(Tok::Comma) => {}
                        Some(Tok::Word(w)) | Some(Tok::Str(w)) => items.push(w),
                        other => return Err(self.error(format!("unexpected {other:?} in tuple"))),
                    }
                }
            }
            other => Err(self.error(format!("expected value, found {other:?}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"effect : "eut2.dif.spec.weight.add.env.tsnmapuv.rfx" {
	diffuse : { 0.554222 , 0.554222 , 0.554222 }
	shininess : 90.000000
	texture : "texture_base" {
		source : "/vehicle/truck/scania_2016/plastic_glossy.tobj"
		sampler : default
	}
	texture : "texture_reflection" {
		source : "/material/environment/fuzzy_reflection/fuzzy_reflection.tobj"
		u_address : clamp
	}
}"#;

    #[test]
    fn parses_effect_attributes_and_textures() {
        let mat = Material::parse(SAMPLE.as_bytes()).expect("parse");
        assert_eq!(mat.effect, "eut2.dif.spec.weight.add.env.tsnmapuv.rfx");
        assert_eq!(
            mat.attribute("diffuse").and_then(MatValue::as_floats),
            Some(vec![0.554222; 3])
        );
        assert_eq!(
            mat.attribute("shininess").and_then(MatValue::as_f32),
            Some(90.0)
        );
        let base = mat.texture("texture_base").expect("base");
        assert_eq!(
            base.source.as_deref(),
            Some("/vehicle/truck/scania_2016/plastic_glossy.tobj")
        );
        assert_eq!(mat.textures.len(), 2);
    }

    #[test]
    fn parses_the_legacy_ui_material_form() {
        let src = br#"material : "ui" {
    texture : "cab_highline.tobj"
    texture_name : "texture"
}"#;
        let mat = Material::parse(src).expect("parse");
        assert_eq!(mat.effect, "ui");
        let texture = mat.texture("texture").expect("texture");
        assert_eq!(texture.source.as_deref(), Some("cab_highline.tobj"));
    }

    #[test]
    fn rejects_malformed_material() {
        assert!(Material::parse(b"effect : \"x\" { diffuse : { 1 , 2 }").is_err());
        assert!(Material::parse(b"shader : \"x\" { }").is_err());
    }
}
