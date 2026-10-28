//! Binary SII (`BSII`), the form of many save files once their ScsC
//! container is decrypted. Layout evidence: `docs/formats/bsii.md`.
//!
//! ```text
//! "BSII" | u32 version (3)
//! blocks until the end marker:
//!   structure: u32 0 | u8 1 | u32 structure id | string name
//!              | fields: (u32 type | string name [| enum table]) ... | u32 0
//!   unit:      u32 structure id | id | one value per field, in field order
//!   end:       u32 0 | u8 0
//! string = u32 length | bytes
//! ```
//!
//! The output is the same [`SiiDocument`] the text parser produces: one unit
//! per data block, floats as `&xxxxxxxx` tokens, integers as decimal tokens,
//! arrays as `key: n` plus `key[i]` attributes, like the text form.

use crate::sii::{Attribute, SiiDocument, Slot, Unit, Value};

/// Errors produced while reading binary SII.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BsiiError {
    #[error("not binary SII (missing `BSII` magic)")]
    NotBinary,
    #[error("unsupported BSII version {0}")]
    UnsupportedVersion(u32),
    #[error("BSII truncated at byte {0}")]
    Truncated(usize),
    #[error("unit at byte {at} uses undefined structure {id}")]
    UnknownStructure { at: usize, id: u32 },
    #[error("field `{structure}.{field}` has unsupported value type {type_id:#x}")]
    UnknownValueType {
        structure: String,
        field: String,
        type_id: u32,
    },
    #[error("bad encoded string at byte {0}")]
    BadEncodedString(usize),
    #[error("count {count} at byte {at} exceeds the remaining data")]
    CountTooLarge { at: usize, count: u32 },
    #[error("{0} bytes after the end marker")]
    TrailingBytes(usize),
}

use std::collections::HashMap;

const MAGIC: &[u8; 4] = b"BSII";
/// Every binary save observed is version 3; others are not verified.
const VERSION: u32 = 3;
/// Characters of the base-38 token encoding; digit `d` (1..=37) is
/// `CHARS[d - 1]`, digit 0 has no character.
const CHARS: &[u8; 37] = b"0123456789abcdefghijklmnopqrstuvwxyz_";
/// Bit 63 of an encoded token is ignored (meaning unknown).
const TOKEN_MASK: u64 = !(1 << 63);
const NAMELESS: u8 = 0xff;
const ENUM_TYPE: u32 = 0x37;

/// One field of a structure: its value type and name.
struct Field {
    type_id: u32,
    name: String,
    /// Names of an enum field's values (type 0x37).
    names: HashMap<u32, String>,
}

struct Structure {
    name: String,
    fields: Vec<Field>,
}

/// Parses binary SII into units.
pub fn parse(bytes: &[u8]) -> Result<SiiDocument, BsiiError> {
    if bytes.get(0..4) != Some(MAGIC.as_slice()) {
        return Err(BsiiError::NotBinary);
    }
    let mut r = Reader { bytes, pos: 4 };
    let version = r.u32()?;
    if version != VERSION {
        return Err(BsiiError::UnsupportedVersion(version));
    }
    let mut structures: HashMap<u32, Structure> = HashMap::new();
    let mut doc = SiiDocument::default();
    loop {
        let at = r.pos;
        let block = r.u32()?;
        if block == 0 {
            if r.u8()? == 0 {
                break;
            }
            let id = r.u32()?;
            let structure = read_structure(&mut r)?;
            structures.insert(id, structure);
            continue;
        }
        let structure = structures
            .get(&block)
            .ok_or(BsiiError::UnknownStructure { at, id: block })?;
        let name = r.id()?;
        let mut attributes = Vec::new();
        for field in &structure.fields {
            read_value(&mut r, structure, field, &mut attributes)?;
        }
        doc.units.push(Unit {
            class: structure.name.clone(),
            name,
            attributes,
        });
    }
    match bytes.len() - r.pos {
        0 => Ok(doc),
        extra => Err(BsiiError::TrailingBytes(extra)),
    }
}

fn read_structure(r: &mut Reader) -> Result<Structure, BsiiError> {
    let name = r.string()?;
    let mut fields = Vec::new();
    loop {
        let type_id = r.u32()?;
        if type_id == 0 {
            return Ok(Structure { name, fields });
        }
        let field_name = r.string()?;
        let mut names = HashMap::new();
        if type_id == ENUM_TYPE {
            let count = r.count(8)?;
            for _ in 0..count {
                let key = r.u32()?;
                names.insert(key, r.string()?);
            }
        }
        fields.push(Field {
            type_id,
            name: field_name,
            names,
        });
    }
}

/// How one element of a value type is read.
#[derive(Clone, Copy)]
enum Element {
    String,
    Token,
    Floats(usize),
    I32s(usize),
    U16,
    U32,
    I32,
    I64,
    U64,
    Bool,
    Enum,
    Id,
}

impl Element {
    /// The smallest encoding, used to bound array counts.
    fn min_size(self) -> usize {
        match self {
            Element::String => 4,
            Element::Token | Element::I64 | Element::U64 => 8,
            Element::Floats(n) | Element::I32s(n) => 4 * n,
            Element::U16 => 2,
            Element::U32 | Element::I32 | Element::Enum => 4,
            Element::Bool | Element::Id => 1,
        }
    }
}

/// Value types seen in the saves (`research/evidence/bsii-derivation.txt`):
/// the element, and whether the value is an array of it.
fn layout(type_id: u32) -> Option<(Element, bool)> {
    Some(match type_id {
        0x01 => (Element::String, false),
        0x02 => (Element::String, true),
        0x03 => (Element::Token, false),
        0x04 => (Element::Token, true),
        0x05 => (Element::Floats(1), false),
        0x06 => (Element::Floats(1), true),
        0x07 => (Element::Floats(2), false),
        0x09 => (Element::Floats(3), false),
        0x11 => (Element::I32s(3), false),
        0x12 => (Element::I32s(3), true),
        0x18 => (Element::Floats(4), true),
        0x19 => (Element::Floats(8), false),
        0x1a => (Element::Floats(8), true),
        0x25 => (Element::I32, false),
        0x26 => (Element::I32, true),
        0x27 | 0x2f => (Element::U32, false),
        0x28 => (Element::U32, true),
        0x2b => (Element::U16, false),
        0x2c => (Element::U16, true),
        0x31 => (Element::I64, false),
        0x32 => (Element::I64, true),
        0x33 => (Element::U64, false),
        0x34 => (Element::U64, true),
        0x35 => (Element::Bool, false),
        0x36 => (Element::Bool, true),
        ENUM_TYPE => (Element::Enum, false),
        0x39 | 0x3b | 0x3d => (Element::Id, false),
        0x3a | 0x3c => (Element::Id, true),
        _ => return None,
    })
}

fn read_value(
    r: &mut Reader,
    structure: &Structure,
    field: &Field,
    out: &mut Vec<Attribute>,
) -> Result<(), BsiiError> {
    let (element, array) = layout(field.type_id).ok_or_else(|| BsiiError::UnknownValueType {
        structure: structure.name.clone(),
        field: field.name.clone(),
        type_id: field.type_id,
    })?;
    if !array {
        let value = read_element(r, element, field)?;
        out.push(Attribute {
            key: field.name.clone(),
            slot: Slot::Scalar,
            value,
        });
        return Ok(());
    }
    let count = r.count(element.min_size())?;
    out.push(Attribute {
        key: field.name.clone(),
        slot: Slot::Scalar,
        value: Value::Token(count.to_string()),
    });
    for index in 0..count {
        let value = read_element(r, element, field)?;
        out.push(Attribute {
            key: field.name.clone(),
            slot: Slot::Index(index),
            value,
        });
    }
    Ok(())
}

fn read_element(r: &mut Reader, element: Element, field: &Field) -> Result<Value, BsiiError> {
    Ok(match element {
        Element::String => Value::String(r.string()?),
        Element::Token => Value::Token(r.token()?),
        Element::Floats(1) => Value::Token(float_token(r.u32()?)),
        Element::Floats(n) => {
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                items.push(float_token(r.u32()?));
            }
            Value::Tuple(items)
        }
        Element::I32s(n) => {
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                items.push((r.u32()? as i32).to_string());
            }
            Value::Tuple(items)
        }
        Element::U16 => Value::Token(r.u16()?.to_string()),
        Element::U32 => Value::Token(r.u32()?.to_string()),
        Element::I32 => Value::Token((r.u32()? as i32).to_string()),
        Element::I64 => Value::Token((r.u64()? as i64).to_string()),
        Element::U64 => Value::Token(r.u64()?.to_string()),
        Element::Bool => Value::Token((r.u8()? != 0).to_string()),
        // An index missing from the enum's table is kept as its number.
        Element::Enum => {
            let index = r.u32()?;
            Value::Token(
                field
                    .names
                    .get(&index)
                    .cloned()
                    .unwrap_or_else(|| index.to_string()),
            )
        }
        Element::Id => Value::Token(r.id()?),
    })
}

/// A float as the text form's exact `&xxxxxxxx` token.
fn float_token(bits: u32) -> String {
    format!("&{bits:08x}")
}

/// A nameless id as `_nameless.` and its 16-bit groups in hex, high to low,
/// without leading zero groups (Hypothesis, see `docs/formats/bsii.md`).
fn nameless(value: u64) -> String {
    let groups: Vec<String> = (0..4)
        .rev()
        .map(|i| (value >> (16 * i)) & 0xffff)
        .skip_while(|&g| g == 0)
        .map(|g| format!("{g:x}"))
        .collect();
    if groups.is_empty() {
        "_nameless.0".to_owned()
    } else {
        format!("_nameless.{}", groups.join("."))
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], BsiiError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or(BsiiError::Truncated(self.pos))?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, BsiiError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, BsiiError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, BsiiError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, BsiiError> {
        let b = self.take(8)?;
        let mut word = [0u8; 8];
        word.copy_from_slice(b);
        Ok(u64::from_le_bytes(word))
    }

    /// An element count, bounded by what the remaining bytes can hold.
    fn count(&mut self, min_size: usize) -> Result<usize, BsiiError> {
        let at = self.pos;
        let count = self.u32()?;
        let remaining = self.bytes.len() - self.pos;
        if count as usize > remaining / min_size.max(1) {
            return Err(BsiiError::CountTooLarge { at, count });
        }
        Ok(count as usize)
    }

    fn string(&mut self) -> Result<String, BsiiError> {
        let len = self.u32()? as usize;
        Ok(String::from_utf8_lossy(self.take(len)?).into_owned())
    }

    fn token(&mut self) -> Result<String, BsiiError> {
        let at = self.pos;
        let mut rest = self.u64()? & TOKEN_MASK;
        let mut text = String::new();
        while rest > 0 {
            let digit = (rest % 38) as usize;
            let c = digit
                .checked_sub(1)
                .and_then(|i| CHARS.get(i))
                .ok_or(BsiiError::BadEncodedString(at))?;
            text.push(char::from(*c));
            rest /= 38;
        }
        Ok(text)
    }

    /// A unit id: `null`, a dotted name of encoded tokens, or a nameless id.
    fn id(&mut self) -> Result<String, BsiiError> {
        let parts = self.u8()?;
        if parts == NAMELESS {
            return Ok(nameless(self.u64()?));
        }
        if parts == 0 {
            return Ok("null".to_owned());
        }
        let mut names = Vec::with_capacity(usize::from(parts));
        for _ in 0..parts {
            names.push(self.token()?);
        }
        Ok(names.join("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz_";

    /// Inverse of the token encoding: first character in the lowest digit.
    fn token(text: &str) -> u64 {
        text.bytes().rev().fold(0u64, |acc, c| {
            let digit = CHARS.iter().position(|&x| x == c).unwrap() as u64 + 1;
            acc * 38 + digit
        })
    }

    struct Out(Vec<u8>);

    impl Out {
        fn new() -> Self {
            let mut out = Out(b"BSII".to_vec());
            out.u32(3);
            out
        }
        fn u8(&mut self, v: u8) -> &mut Self {
            self.0.push(v);
            self
        }
        fn u32(&mut self, v: u32) -> &mut Self {
            self.0.extend(v.to_le_bytes());
            self
        }
        fn bytes(&mut self, b: &[u8]) -> &mut Self {
            self.0.extend(b);
            self
        }
        fn str(&mut self, s: &str) -> &mut Self {
            self.u32(s.len() as u32).bytes(s.as_bytes())
        }
        fn field(&mut self, type_id: u32, name: &str) -> &mut Self {
            self.u32(type_id).str(name)
        }
        fn named_id(&mut self, parts: &[&str]) -> &mut Self {
            self.u8(parts.len() as u8);
            for p in parts {
                self.bytes(&token(p).to_le_bytes());
            }
            self
        }
        fn end(&mut self) -> Vec<u8> {
            self.u32(0).u8(0);
            self.0.clone()
        }
    }

    /// One structure with every value type the saves use, and one unit.
    fn sample() -> Vec<u8> {
        let mut o = Out::new();
        o.u32(0).u8(1).u32(7).str("economy");
        o.field(0x01, "name")
            .field(0x02, "names")
            .field(0x03, "brand")
            .field(0x04, "brands")
            .field(0x05, "speed")
            .field(0x06, "speeds")
            .field(0x07, "uv")
            .field(0x09, "pos")
            .field(0x11, "cell")
            .field(0x12, "cells")
            .field(0x18, "rots")
            .field(0x19, "place")
            .field(0x1a, "places")
            .field(0x25, "delta")
            .field(0x26, "deltas")
            .field(0x27, "money")
            .field(0x28, "moneys")
            .field(0x2b, "small")
            .field(0x2c, "smalls")
            .field(0x2f, "other")
            .field(0x31, "big")
            .field(0x32, "bigs")
            .field(0x33, "ubig")
            .field(0x34, "ubigs")
            .field(0x35, "flag")
            .field(0x36, "flags")
            .field(0x37, "state");
        o.u32(2).u32(0).str("idle").u32(5).str("busy");
        o.field(0x39, "owner")
            .field(0x3a, "trucks")
            .field(0x3b, "garage")
            .field(0x3c, "drivers")
            .field(0x3d, "job")
            .u32(0);
        // The unit.
        o.u32(7).named_id(&["economy", "player"]);
        o.str("D\u{e9}j\u{e0}");
        o.u32(2).str("a").str("b");
        o.bytes(&token("scania").to_le_bytes());
        o.u32(1).bytes(&token("volvo").to_le_bytes());
        o.bytes(&1.0f32.to_le_bytes());
        o.u32(2)
            .bytes(&0.5f32.to_le_bytes())
            .bytes(&(-2.0f32).to_le_bytes());
        o.bytes(&1.0f32.to_le_bytes()).bytes(&2.0f32.to_le_bytes());
        for v in [1.0f32, 2.0, 3.0] {
            o.bytes(&v.to_le_bytes());
        }
        for v in [-1i32, 0, 5] {
            o.bytes(&v.to_le_bytes());
        }
        o.u32(1);
        for v in [7i32, 8, 9] {
            o.bytes(&v.to_le_bytes());
        }
        o.u32(1);
        for v in [0.0f32, 0.0, 0.0, 1.0] {
            o.bytes(&v.to_le_bytes());
        }
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0] {
            o.bytes(&v.to_le_bytes());
        }
        o.u32(0); // empty vec8 array
        o.bytes(&(-3i32).to_le_bytes());
        o.u32(0); // empty int32 array
        o.u32(4_000_000_000);
        o.u32(2).u32(1).u32(2);
        o.bytes(&65535u16.to_le_bytes());
        o.u32(1).bytes(&3u16.to_le_bytes());
        o.u32(9);
        o.bytes(&(-5i64).to_le_bytes());
        o.u32(1).bytes(&6i64.to_le_bytes());
        o.bytes(&u64::MAX.to_le_bytes());
        o.u32(0);
        o.u8(1);
        o.u32(2).u8(0).u8(1);
        o.u32(5); // enum: "busy"
        o.named_id(&["company", "volatile", "tesco"]);
        o.u32(2).named_id(&[]).u8(0xff);
        o.bytes(&0x0177_f825_8278u64.to_le_bytes());
        o.named_id(&["garage", "berlin"]);
        o.u32(0);
        o.named_id(&[]);
        o.end()
    }

    fn tokens<'a>(unit: &'a Unit, key: &str) -> Vec<&'a str> {
        unit.array(key)
            .into_iter()
            .filter_map(Value::as_str)
            .collect()
    }

    #[test]
    fn decodes_every_value_type_into_sii_units() {
        let doc = parse(&sample()).unwrap();
        assert_eq!(doc.units.len(), 1);
        let u = &doc.units[0];
        assert_eq!(u.class, "economy");
        assert_eq!(u.name, "economy.player");
        assert_eq!(u.get_str("name"), Some("D\u{e9}j\u{e0}"));
        assert_eq!(tokens(u, "names"), ["a", "b"]);
        assert_eq!(u.get_i64("names"), Some(2));
        assert_eq!(u.get_str("brand"), Some("scania"));
        assert_eq!(tokens(u, "brands"), ["volvo"]);
        assert_eq!(u.get_f32("speed"), Some(1.0));
        assert_eq!(u.get_str("speed"), Some("&3f800000"));
        let speeds: Vec<f32> = u
            .array("speeds")
            .iter()
            .filter_map(|v| v.as_f32())
            .collect();
        assert_eq!(speeds, [0.5, -2.0]);
        assert_eq!(u.get("uv").and_then(Value::as_floats), Some(vec![1.0, 2.0]));
        assert_eq!(
            u.get("pos").and_then(Value::as_floats),
            Some(vec![1.0, 2.0, 3.0])
        );
        assert_eq!(
            u.get("cell"),
            Some(&Value::Tuple(vec!["-1".into(), "0".into(), "5".into()]))
        );
        assert_eq!(
            u.array("cells"),
            [&Value::Tuple(vec!["7".into(), "8".into(), "9".into()])]
        );
        assert_eq!(
            u.array("rots")[0].as_floats(),
            Some(vec![0.0, 0.0, 0.0, 1.0])
        );
        assert_eq!(
            u.get("place").and_then(Value::as_floats),
            Some(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])
        );
        assert_eq!(u.get_i64("places"), Some(0));
        assert!(u.array("places").is_empty());
        assert_eq!(u.get_i64("delta"), Some(-3));
        assert_eq!(u.get_i64("money"), Some(4_000_000_000));
        assert_eq!(tokens(u, "moneys"), ["1", "2"]);
        assert_eq!(u.get_i64("small"), Some(65535));
        assert_eq!(tokens(u, "smalls"), ["3"]);
        assert_eq!(u.get_i64("other"), Some(9));
        assert_eq!(u.get_i64("big"), Some(-5));
        assert_eq!(tokens(u, "bigs"), ["6"]);
        assert_eq!(u.get_str("ubig"), Some("18446744073709551615"));
        assert_eq!(u.get_bool("flag"), Some(true));
        assert_eq!(tokens(u, "flags"), ["false", "true"]);
        assert_eq!(u.get_str("state"), Some("busy"));
        assert_eq!(u.get_str("owner"), Some("company.volatile.tesco"));
        assert_eq!(tokens(u, "trucks"), ["null", "_nameless.177.f825.8278"]);
        assert_eq!(u.get_str("garage"), Some("garage.berlin"));
        assert_eq!(u.get_i64("drivers"), Some(0));
        assert_eq!(u.get_str("job"), Some("null"));
    }

    #[test]
    fn rejects_other_files_and_versions() {
        assert_eq!(parse(b"SiiNunit"), Err(BsiiError::NotBinary));
        assert_eq!(parse(b"BS"), Err(BsiiError::NotBinary));
        assert_eq!(
            parse(b"BSII\x02\x00\x00\x00"),
            Err(BsiiError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn every_truncation_is_an_error_not_a_panic() {
        let full = sample();
        for len in 8..full.len() {
            assert!(parse(&full[..len]).is_err(), "prefix of {len} bytes");
        }
    }

    #[test]
    fn rejects_trailing_bytes_after_the_end_marker() {
        let mut bytes = sample();
        bytes.push(0);
        assert_eq!(parse(&bytes), Err(BsiiError::TrailingBytes(1)));
    }

    #[test]
    fn rejects_unknown_structures_and_value_types() {
        let mut o = Out::new();
        o.u32(9).named_id(&["x"]);
        assert!(matches!(
            parse(&o.end()),
            Err(BsiiError::UnknownStructure { id: 9, .. })
        ));
        let mut o = Out::new();
        o.u32(0)
            .u8(1)
            .u32(1)
            .str("thing")
            .field(0x17, "quat")
            .u32(0);
        o.u32(1).named_id(&["t"]).bytes(&[0; 16]);
        assert_eq!(
            parse(&o.end()),
            Err(BsiiError::UnknownValueType {
                structure: "thing".into(),
                field: "quat".into(),
                type_id: 0x17,
            })
        );
    }

    #[test]
    fn rejects_counts_larger_than_the_data_and_bad_tokens() {
        let mut o = Out::new();
        o.u32(0).u8(1).u32(1).str("s").field(0x26, "ints").u32(0);
        o.u32(1).named_id(&["s"]).u32(u32::MAX);
        assert!(matches!(
            parse(&o.end()),
            Err(BsiiError::CountTooLarge {
                count: u32::MAX,
                ..
            })
        ));
        // A base-38 digit of 0 has no character.
        let mut o = Out::new();
        o.u32(0).u8(1).u32(1).str("s").field(0x03, "tok").u32(0);
        o.u32(1).named_id(&["s"]).bytes(&38u64.to_le_bytes());
        assert!(matches!(
            parse(&o.end()),
            Err(BsiiError::BadEncodedString(_))
        ));
    }
}
