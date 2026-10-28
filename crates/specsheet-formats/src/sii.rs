//! Text SII (`SiiNunit`) definitions, the format of `/def/**/*.sii` and
//! `*.sui` include fragments. Grammar evidence: `docs/formats/sii.md`.
//!
//! ```text
//! SiiNunit
//! {
//! class_name : unit.name {
//!     key: value              # scalar (or the element count of an array)
//!     key[]: value            # appended array element
//!     key[3]: value           # indexed array element
//!     tuple: (1, 2.5, &3f800000)
//! @include "fragment.sui"     # textual include, relative to this file
//! }
//! }
//! ```
//!
//! `@include` is a textual preprocessor directive and may appear inside a
//! unit body; [`parse_with_includes`] expands it before tokenizing.

use std::fmt;

/// Maximum include nesting; protects against include cycles.
const MAX_INCLUDE_DEPTH: usize = 16;

/// Errors produced while reading SII text.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SiiError {
    /// The text does not follow the SII grammar.
    #[error("{file}:{line}: {message}")]
    Syntax {
        /// The file being parsed.
        file: String,
        /// 1-based line number.
        line: usize,
        /// What was expected.
        message: String,
    },
    /// An `@include` target could not be loaded.
    #[error("include `{include}` from `{file}` failed: {reason}")]
    Include {
        /// The including file.
        file: String,
        /// The included path.
        include: String,
        /// Why loading failed.
        reason: String,
    },
    /// Includes nest deeper than the limit, usually a cycle.
    #[error("include depth exceeded at `{0}` (cycle?)")]
    IncludeDepth(String),
    /// The data is not SII text (no `SiiNunit`).
    #[error("`{0}` is not a text SII file")]
    NotText(String),
}

/// A parsed SII file: an ordered list of units.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiiDocument {
    /// The units, in file order.
    pub units: Vec<Unit>,
}

impl SiiDocument {
    /// Finds a unit by its exact name.
    pub fn unit(&self, name: &str) -> Option<&Unit> {
        self.units.iter().find(|u| u.name == name)
    }

    /// All units of the given class.
    pub fn units_of_class<'a>(&'a self, class: &'a str) -> impl Iterator<Item = &'a Unit> {
        self.units.iter().filter(move |u| u.class == class)
    }
}

/// One `class : name { ... }` block.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    /// The unit's class, e.g. `accessory_addon_data`.
    pub class: String,
    /// The unit's name, e.g. `chrome.scania.s_2016.s_mirror`.
    pub name: String,
    /// The attributes, in file order.
    pub attributes: Vec<Attribute>,
}

/// How an attribute key addresses its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// `key: value`.
    Scalar,
    /// `key[]: value`, appended to the array.
    Append,
    /// `key[n]: value`.
    Index(usize),
}

/// One `key: value` line of a unit.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    /// The key without its array suffix.
    pub key: String,
    /// How the key addresses its value.
    pub slot: Slot,
    /// The value.
    pub value: Value,
}

/// An attribute value. Bare tokens (numbers, identifiers, unit references,
/// `&hex` floats) are kept verbatim; typed accessors interpret them.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A quoted string, unescaped.
    String(String),
    /// A bare token: number, identifier, unit reference or `&hex` float.
    Token(String),
    /// The items of a `( … )` tuple.
    Tuple(Vec<String>),
}

impl Unit {
    /// The last scalar assignment of `key`.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.attributes
            .iter()
            .rev()
            .find(|a| a.key == key && a.slot == Slot::Scalar)
            .map(|a| &a.value)
    }

    /// Text of a scalar string or token attribute.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    /// A scalar attribute as a float (decimal or `&hex`).
    pub fn get_f32(&self, key: &str) -> Option<f32> {
        self.get(key).and_then(Value::as_f32)
    }

    /// A scalar attribute as an integer.
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(Value::as_i64)
    }

    /// A scalar `true`/`false` attribute.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(Value::as_bool)
    }

    /// Elements of array `key` (`key[]` / `key[n]`) in index order.
    pub fn array(&self, key: &str) -> Vec<&Value> {
        let mut items: Vec<Option<&Value>> = Vec::new();
        for attr in self.attributes.iter().filter(|a| a.key == key) {
            match attr.slot {
                Slot::Scalar => {}
                Slot::Append => items.push(Some(&attr.value)),
                Slot::Index(i) => {
                    if i >= items.len() {
                        items.resize(i + 1, None);
                    }
                    items[i] = Some(&attr.value);
                }
            }
        }
        items.into_iter().flatten().collect()
    }

    /// String/token elements of array `key`.
    pub fn array_str(&self, key: &str) -> Vec<&str> {
        self.array(key)
            .into_iter()
            .filter_map(Value::as_str)
            .collect()
    }
}

impl Value {
    /// The text of a string or token.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) | Value::Token(s) => Some(s),
            Value::Tuple(_) => None,
        }
    }

    /// A token as a float (decimal or `&hex`).
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Value::Token(t) => parse_float(t),
            _ => None,
        }
    }

    /// A token as an integer.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Token(t) => t.parse().ok(),
            _ => None,
        }
    }

    /// A `true`/`false` token.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Token(t) if t == "true" => Some(true),
            Value::Token(t) if t == "false" => Some(false),
            _ => None,
        }
    }

    /// A tuple of floats, e.g. `(1, 2.5, &3f800000)`.
    pub fn as_floats(&self) -> Option<Vec<f32>> {
        match self {
            Value::Tuple(items) => items.iter().map(|t| parse_float(t)).collect(),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "\"{s}\""),
            Value::Token(t) => f.write_str(t),
            Value::Tuple(items) => write!(f, "({})", items.join(", ")),
        }
    }
}

/// Parses a float token: decimal, or `&xxxxxxxx` (raw IEEE-754 bits).
pub fn parse_float(token: &str) -> Option<f32> {
    match token.strip_prefix('&') {
        Some(hex) => u32::from_str_radix(hex, 16).ok().map(f32::from_bits),
        None => token.parse().ok(),
    }
}

/// Resolves `@include` targets. Paths passed in are absolute game paths
/// (`/def/...`) computed relative to the including file.
pub trait IncludeLoader {
    /// The bytes of the absolute game path `path`, or why they are missing.
    fn load(&self, path: &str) -> Result<Vec<u8>, String>;
}

impl<F: Fn(&str) -> Result<Vec<u8>, String>> IncludeLoader for F {
    fn load(&self, path: &str) -> Result<Vec<u8>, String> {
        self(path)
    }
}

/// Parses SII text that contains no `@include` directives.
pub fn parse(file: &str, source: &[u8]) -> Result<SiiDocument, SiiError> {
    let text = decode_text(file, source)?;
    Parser::new(file, &text).document()
}

/// Expands `@include` directives (relative to `file`, an absolute game path)
/// using `loader`, then parses the result.
pub fn parse_with_includes(
    file: &str,
    source: &[u8],
    loader: &dyn IncludeLoader,
) -> Result<SiiDocument, SiiError> {
    let text = expand_includes(file, source, loader, 0)?;
    Parser::new(file, &text).document()
}

fn decode_text(file: &str, source: &[u8]) -> Result<String, SiiError> {
    // Encrypted (`ScsC`), binary (`BSII`) and obfuscated (`3nK`) SII start
    // with these magics; none of them occur in shipped `/def` data.
    if source.starts_with(b"ScsC") || source.starts_with(b"BSII") || source.starts_with(b"3nK") {
        return Err(SiiError::NotText(file.to_owned()));
    }
    let source = source.strip_prefix(b"\xef\xbb\xbf").unwrap_or(source);
    Ok(String::from_utf8_lossy(source).into_owned())
}

fn expand_includes(
    file: &str,
    source: &[u8],
    loader: &dyn IncludeLoader,
    depth: usize,
) -> Result<String, SiiError> {
    if depth > MAX_INCLUDE_DEPTH {
        return Err(SiiError::IncludeDepth(file.to_owned()));
    }
    let text = decode_text(file, source)?;
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let Some(rest) = line.trim_start().strip_prefix("@include") else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let include = rest.trim().trim_matches('"');
        let path = resolve_relative(file, include);
        let bytes = loader.load(&path).map_err(|reason| SiiError::Include {
            file: file.to_owned(),
            include: include.to_owned(),
            reason,
        })?;
        out.push_str(&expand_includes(&path, &bytes, loader, depth + 1)?);
        out.push('\n');
    }
    Ok(out)
}

/// Resolves an include target against the directory of `file`.
pub fn resolve_relative(file: &str, target: &str) -> String {
    if target.starts_with('/') {
        return target.to_owned();
    }
    let dir = file.rsplit_once('/').map_or("", |(dir, _)| dir);
    format!("{dir}/{target}")
}

#[derive(Clone, Debug, PartialEq)]
enum Token<'a> {
    Word(&'a str),
    Str(String),
    Colon,
    OpenBrace,
    CloseBrace,
    Tuple(Vec<String>),
}

struct Parser<'a> {
    file: &'a str,
    src: &'a str,
    pos: usize,
    line: usize,
}

impl<'a> Parser<'a> {
    fn new(file: &'a str, src: &'a str) -> Self {
        Self {
            file,
            src,
            pos: 0,
            line: 1,
        }
    }

    fn error(&self, message: impl Into<String>) -> SiiError {
        SiiError::Syntax {
            file: self.file.to_owned(),
            line: self.line,
            message: message.into(),
        }
    }

    fn document(mut self) -> Result<SiiDocument, SiiError> {
        match self.next()? {
            Some(Token::Word("SiiNunit")) => {}
            _ => return Err(self.error("expected `SiiNunit`")),
        }
        self.expect(Token::OpenBrace)?;
        let mut units = Vec::new();
        loop {
            match self.next()? {
                Some(Token::CloseBrace) => break,
                Some(Token::Word(class)) => units.push(self.unit(class)?),
                Some(other) => return Err(self.error(format!("unexpected {other:?}"))),
                None => return Err(self.error("unterminated `SiiNunit` block")),
            }
        }
        Ok(SiiDocument { units })
    }

    fn unit(&mut self, class: &str) -> Result<Unit, SiiError> {
        self.expect(Token::Colon)?;
        let name = match self.next()? {
            Some(Token::Word(name)) => name.to_owned(),
            _ => return Err(self.error("expected unit name")),
        };
        self.expect(Token::OpenBrace)?;
        let mut attributes = Vec::new();
        loop {
            match self.next()? {
                Some(Token::CloseBrace) => break,
                Some(Token::Word(key)) => {
                    self.expect(Token::Colon)?;
                    let value = match self.next()? {
                        Some(Token::Word(w)) => Value::Token(w.to_owned()),
                        Some(Token::Str(s)) => Value::String(s),
                        Some(Token::Tuple(mut items)) => {
                            // A placement continues with its rotation on the
                            // same line: `(x, y, z) (w; x, y, z)`.
                            if self.tuple_follows_on_line() {
                                items.extend(self.tuple()?);
                            }
                            Value::Tuple(items)
                        }
                        _ => return Err(self.error(format!("missing value for `{key}`"))),
                    };
                    let (key, slot) =
                        split_key(key).ok_or_else(|| self.error(format!("bad key `{key}`")))?;
                    attributes.push(Attribute {
                        key: key.to_owned(),
                        slot,
                        value,
                    });
                }
                Some(other) => {
                    return Err(self.error(format!("unexpected {other:?} in unit `{name}`")))
                }
                None => return Err(self.error(format!("unterminated unit `{name}`"))),
            }
        }
        Ok(Unit {
            class: class.to_owned(),
            name,
            attributes,
        })
    }

    fn expect(&mut self, expected: Token<'_>) -> Result<(), SiiError> {
        match self.next()? {
            Some(token) if token == expected => Ok(()),
            other => Err(self.error(format!("expected {expected:?}, found {other:?}"))),
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn skip_trivia(&mut self) -> Result<(), SiiError> {
        loop {
            let rest = &self.src[self.pos..];
            if rest.starts_with('#') || rest.starts_with("//") {
                while self.peek_char().is_some_and(|c| c != '\n') {
                    self.bump();
                }
            } else if rest.starts_with("/*") {
                let end = rest
                    .find("*/")
                    .ok_or_else(|| self.error("unterminated comment"))?;
                for _ in rest[..end + 2].chars() {
                    self.bump();
                }
            } else if self.peek_char().is_some_and(char::is_whitespace) {
                self.bump();
            } else {
                return Ok(());
            }
        }
    }

    fn next(&mut self) -> Result<Option<Token<'a>>, SiiError> {
        self.skip_trivia()?;
        let Some(c) = self.peek_char() else {
            return Ok(None);
        };
        let token = match c {
            '{' => {
                self.bump();
                Token::OpenBrace
            }
            '}' => {
                self.bump();
                Token::CloseBrace
            }
            ':' => {
                self.bump();
                Token::Colon
            }
            '"' => Token::Str(self.string()?),
            '(' => Token::Tuple(self.tuple()?),
            _ => Token::Word(self.word()),
        };
        Ok(Some(token))
    }

    /// A bare word. `::` is part of a word (`ui::group`); a single `:`
    /// terminates it (`key:value`).
    fn word(&mut self) -> &'a str {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() || matches!(c, '{' | '}' | '"' | '(' | ')' | ',') {
                break;
            }
            if c == ':' {
                if self.src[self.pos..].starts_with("::") {
                    self.bump();
                    self.bump();
                    continue;
                }
                break;
            }
            self.bump();
        }
        &self.src[start..self.pos]
    }

    fn string(&mut self) -> Result<String, SiiError> {
        self.bump(); // opening quote
                     // Bytes, because `\xNN` escapes are single bytes of a UTF-8 sequence.
        let mut out = Vec::new();
        let mut utf8 = [0u8; 4];
        loop {
            match self.bump() {
                Some('"') => return Ok(String::from_utf8_lossy(&out).into_owned()),
                Some('\\') => match self.bump() {
                    Some('n') => out.push(b'\n'),
                    Some('r') => out.push(b'\r'),
                    Some('t') => out.push(b'\t'),
                    Some('x') => match self.hex_byte() {
                        Some(byte) => out.push(byte),
                        None => out.push(b'x'),
                    },
                    Some(other) => out.extend(other.encode_utf8(&mut utf8).as_bytes()),
                    None => break,
                },
                Some(c) => out.extend(c.encode_utf8(&mut utf8).as_bytes()),
                None => break,
            }
        }
        Err(self.error("unterminated string"))
    }

    /// Two hex digits after `\x`; consumes them only when both are valid.
    fn hex_byte(&mut self) -> Option<u8> {
        let digits = self.src.get(self.pos..self.pos + 2)?;
        // `from_str_radix` alone would also accept a sign (`\x+f`).
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = u8::from_str_radix(digits, 16).ok()?;
        self.pos += 2;
        Some(byte)
    }

    /// Skips spaces and tabs; true when a `(` follows before the line ends.
    fn tuple_follows_on_line(&mut self) -> bool {
        while self.peek_char().is_some_and(|c| c == ' ' || c == '\t') {
            self.bump();
        }
        self.peek_char() == Some('(')
    }

    /// `(a, b, c)`. A `;` separates like `,` (a quaternion's `(w; x, y, z)`).
    fn tuple(&mut self) -> Result<Vec<String>, SiiError> {
        self.bump(); // '('
        let mut items = Vec::new();
        loop {
            self.skip_trivia()?;
            match self.peek_char() {
                Some(')') => {
                    self.bump();
                    return Ok(items);
                }
                Some(',' | ';') => {
                    self.bump();
                }
                Some(_) => {
                    let word = self.word();
                    let word = word.strip_suffix(';').unwrap_or(word);
                    if word.is_empty() {
                        return Err(self.error("unexpected character in tuple"));
                    }
                    items.push(word.to_owned());
                }
                None => return Err(self.error("unterminated tuple")),
            }
        }
    }
}

/// Splits `name[]` / `name[3]` / `name` into the base key and its slot.
fn split_key(key: &str) -> Option<(&str, Slot)> {
    let Some(open) = key.find('[') else {
        return Some((key, Slot::Scalar));
    };
    let index = key[open + 1..].strip_suffix(']')?;
    let slot = if index.is_empty() {
        Slot::Append
    } else {
        Slot::Index(index.parse().ok()?)
    };
    Some((&key[..open], slot))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CABIN: &str = r#"SiiNunit
{
accessory_cabin_data : normal.scania.s_2016.cabin
{
	name: "@@cabin_normal_roof@@"
	price: 16440
	model: "/vehicle/truck/scania_2016/cabin/cab_s_low.pmd"
	variant: Default
	#info[]: "commented out"
	defaults[]: "/def/a.sii"
	defaults[]: "/def/b.sii"
	suitable_for[]: "4x2.scania.s_2016.chassis"
	light: ( 0.0, -8.0, &3f800000 )
	sounds: 2
	sounds[1]: "second"
	sounds[0]: "first"
	enabled: true
}
}
"#;

    #[test]
    fn parses_unit_with_scalars_arrays_and_tuples() {
        let doc = parse("/def/cabin.sii", CABIN.as_bytes()).expect("parse");
        let unit = &doc.units[0];
        assert_eq!(unit.class, "accessory_cabin_data");
        assert_eq!(unit.name, "normal.scania.s_2016.cabin");
        assert_eq!(unit.get_str("name"), Some("@@cabin_normal_roof@@"));
        assert_eq!(unit.get_i64("price"), Some(16440));
        assert_eq!(unit.get_str("variant"), Some("Default"));
        assert_eq!(unit.array_str("defaults"), ["/def/a.sii", "/def/b.sii"]);
        assert!(unit.array("info").is_empty());
        assert_eq!(
            unit.get("light").and_then(Value::as_floats),
            Some(vec![0.0, -8.0, 1.0])
        );
        assert_eq!(unit.array_str("sounds"), ["first", "second"]);
        assert_eq!(unit.get_i64("sounds"), Some(2));
        assert_eq!(unit.get_bool("enabled"), Some(true));
    }

    #[test]
    fn parses_namespaced_class_and_glued_colons() {
        let src = "SiiNunit\n{\nui::group : _nameless.0066cfd02 {\n coords_l:-2\n my_parent: null\n}\n}\n";
        let doc = parse("/ui/x.sii", src.as_bytes()).expect("parse");
        assert_eq!(doc.units[0].class, "ui::group");
        assert_eq!(doc.units[0].get_i64("coords_l"), Some(-2));
    }

    #[test]
    fn expands_includes_relative_to_including_file() {
        let src = "SiiNunit\n{\nunit_a : a.b {\n x: 1\n@include \"part.sui\"\n}\n}\n";
        let loader = |path: &str| -> Result<Vec<u8>, String> {
            assert_eq!(path, "/def/dir/part.sui");
            Ok(b"\ty[]: \"from include\"\n".to_vec())
        };
        let doc = parse_with_includes("/def/dir/file.sii", src.as_bytes(), &loader).expect("parse");
        assert_eq!(doc.units[0].array_str("y"), ["from include"]);
    }

    #[test]
    fn reports_include_cycles() {
        let loader =
            |_: &str| -> Result<Vec<u8>, String> { Ok(b"@include \"self.sui\"\n".to_vec()) };
        let err = parse_with_includes("/def/self.sui", b"@include \"self.sui\"", &loader)
            .expect_err("cycle");
        assert!(matches!(err, SiiError::IncludeDepth(_)));
    }

    #[test]
    fn reports_syntax_errors_with_line_numbers() {
        let err =
            parse("/def/bad.sii", b"SiiNunit\n{\nclass : name {\n key 5\n}\n}").expect_err("bad");
        assert!(matches!(err, SiiError::Syntax { line: 4, .. }), "{err:?}");
    }

    #[test]
    fn decodes_hex_byte_escapes_as_utf8() {
        // Profiles and UI layouts store non-ASCII text as `\xNN` bytes of
        // its UTF-8 encoding (`/ui/truck_configuration_right.sii`: "\xe2\x82\xac").
        let src = "SiiNunit\n{\nuser_profile : p {\n a[]: \"Ver \\xe3\\x83\\x88.\"\n b[]: \"\\xzz\"\n c[]: \"\\x+f\"\n d[]: \"\\xe2\\x82\\xac 1\"\n}\n}\n";
        let doc = parse("/profile.sii", src.as_bytes()).expect("parse");
        assert_eq!(doc.units[0].array_str("a"), ["Ver \u{30c8}."]);
        assert_eq!(doc.units[0].array_str("b"), ["xzz"]);
        assert_eq!(doc.units[0].array_str("c"), ["x+f"]);
        assert_eq!(doc.units[0].array_str("d"), ["\u{20ac} 1"]);
    }

    #[test]
    fn parses_placements_as_one_seven_item_tuple() {
        // Saves write a position and a rotation quaternion `(w; x, y, z)`.
        let src = "SiiNunit\n{\nplayer : p {\n a: (&3f800000, 2, 3) (&3f01261f; 0, 0, 0)\n b: (1, 2, 3) (1 ; 0, 0, 0)\n c: (4, 5)\n d: 6\n}\n}\n";
        let doc = parse("/game.sii", src.as_bytes()).expect("parse");
        let unit = &doc.units[0];
        assert_eq!(
            unit.get("a").and_then(Value::as_floats),
            Some(vec![
                1.0,
                2.0,
                3.0,
                f32::from_bits(0x3f01_261f),
                0.0,
                0.0,
                0.0
            ])
        );
        assert_eq!(
            unit.get("b").and_then(Value::as_floats),
            Some(vec![1.0, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0])
        );
        assert_eq!(
            unit.get("c").and_then(Value::as_floats),
            Some(vec![4.0, 5.0])
        );
        assert_eq!(unit.get_i64("d"), Some(6));
    }

    #[test]
    fn rejects_encrypted_sii() {
        assert_eq!(
            parse("/save/game.sii", b"ScsC\x00\x01"),
            Err(SiiError::NotText("/save/game.sii".to_owned()))
        );
    }

    #[test]
    fn decodes_hex_floats() {
        assert_eq!(parse_float("&bf800000"), Some(-1.0));
        assert_eq!(parse_float("2.5"), Some(2.5));
        assert_eq!(parse_float("&zz"), None);
    }
}
