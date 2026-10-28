//! PMD model descriptor (version 4): looks, variants and per-part
//! attributes. Format: `docs/formats/models.md`.

use crate::binary::Reader;
use crate::FormatError;

const VERSION: u32 = 4;
/// Size of a part range record: `u32` from, `u32` to.
const PART_RANGE_LEN: usize = 8;
/// Size of an attribute header: token, `u32` kind, `u32` value offset.
const ATTRIBUTE_LEN: usize = 16;
/// The attribute that hides or shows a part.
const VISIBLE: &str = "visible";

/// A look: for each material slot of the model, the material path to use.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// The look's name, e.g. `default`.
    pub name: String,
    /// Material path per material slot.
    pub materials: Vec<String>,
}

/// A declared part attribute (in game data this is `visible`).
#[derive(Clone, Debug, PartialEq)]
pub struct AttributeDef {
    /// The attribute's name.
    pub name: String,
    /// The stored type code.
    pub kind: u32,
    /// Where this attribute's value sits inside a variant's value block, in bytes.
    pub offset: u32,
}

/// A variant: one value per declared attribute, in declaration order.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    /// The variant's name, e.g. `default`.
    pub name: String,
    /// One value per declared attribute.
    pub values: Vec<u32>,
}

/// A parsed `.pmd` model descriptor: looks, variants and part attributes.
#[derive(Clone, Debug, PartialEq)]
pub struct Pmd {
    /// The looks, in file order.
    pub looks: Vec<Look>,
    /// The variants, in file order.
    pub variants: Vec<Variant>,
    /// The declared part attributes.
    pub attributes: Vec<AttributeDef>,
    /// One range per part, in the same order as the matching `.pmg`'s parts:
    /// the indices into `attributes` that belong to that part.
    pub part_attributes: Vec<std::ops::Range<usize>>,
}

/// The fixed 0x40-byte header (models.md, "PMD").
struct Header {
    material_count: usize,
    look_count: usize,
    variant_count: usize,
    part_count: usize,
    attribute_count: usize,
    value_block_len: usize,
    material_strings_len: usize,
    looks: usize,
    variants: usize,
    part_ranges: usize,
    values: usize,
    attributes: usize,
    material_offsets: usize,
    material_strings: usize,
}

impl Header {
    fn read(r: &mut Reader<'_>) -> Result<Self, FormatError> {
        let version = r.u32()?;
        if version != VERSION {
            return Err(r.error(format!("unsupported version {version}")));
        }
        let material_count = r.count(1)?;
        let look_count = r.count(8)?;
        let _unknown = r.u32()?;
        let variant_count = r.count(8)?;
        let part_count = r.count(PART_RANGE_LEN)?;
        let attribute_count = r.count(ATTRIBUTE_LEN)?;
        let value_block_len = r.u32()? as usize;
        let material_strings_len = r.u32()? as usize;
        let mut offset = || r.u32().map(|o| o as usize);
        Ok(Self {
            material_count,
            look_count,
            variant_count,
            part_count,
            attribute_count,
            value_block_len,
            material_strings_len,
            looks: offset()?,
            variants: offset()?,
            part_ranges: offset()?,
            values: offset()?,
            attributes: offset()?,
            material_offsets: offset()?,
            material_strings: offset()?,
        })
    }
}

/// The `count × len`-byte table at `offset`, checked to lie inside the file.
fn table<'a>(
    r: &mut Reader<'a>,
    offset: usize,
    count: usize,
    len: usize,
) -> Result<&'a [u8], FormatError> {
    let total = count
        .checked_mul(len)
        .ok_or_else(|| r.error(format!("{count} records of {len} bytes overflow")))?;
    r.seek(offset)?;
    r.bytes(total)
}

/// Record `index` of a table of `len`-byte records (empty if out of range).
fn record(table: &[u8], index: usize, len: usize) -> &[u8] {
    index
        .checked_mul(len)
        .and_then(|start| table.get(start..start.checked_add(len)?))
        .unwrap_or_default()
}

/// The records of a table, in order.
fn records(table: &[u8], count: usize, len: usize) -> impl Iterator<Item = &[u8]> {
    (0..count).map(move |index| record(table, index, len))
}

/// Little-endian `u32` at `at` inside a record (0 if the record is too short;
/// callers check record sizes first).
fn u32_at(record: &[u8], at: usize) -> u32 {
    let mut word = [0; 4];
    if let Some(bytes) = at.checked_add(4).and_then(|end| record.get(at..end)) {
        word.copy_from_slice(bytes);
    }
    u32::from_le_bytes(word)
}

/// The token name at the start of a record.
fn token_at(record: &[u8]) -> String {
    let mut word = [0; 8];
    if let Some(bytes) = record.get(..8) {
        word.copy_from_slice(bytes);
    }
    crate::token::Token(u64::from_le_bytes(word)).decode()
}

impl Pmd {
    /// Parses a `.pmd` file. Malformed input returns an error and never panics.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let mut r = Reader::new("pmd", bytes);
        let h = Header::read(&mut r)?;

        // One `u32` per attribute (models.md; true of every `.pmd` in the
        // base archives). This also bounds the decoded values by the file
        // size: variants × attributes × 4 is the size of the value blocks.
        let expected_block = h.attribute_count.saturating_mul(4);
        if h.value_block_len != expected_block {
            return Err(r.error(format!(
                "value block of {} bytes for {} attributes",
                h.value_block_len, h.attribute_count
            )));
        }
        let attribute_table = table(&mut r, h.attributes, h.attribute_count, ATTRIBUTE_LEN)?;
        let attributes: Vec<AttributeDef> =
            records(attribute_table, h.attribute_count, ATTRIBUTE_LEN)
                .map(|record| AttributeDef {
                    name: token_at(record),
                    kind: u32_at(record, 8),
                    offset: u32_at(record, 12),
                })
                .collect();
        if let Some(bad) = attributes
            .iter()
            .find(|a| (a.offset as usize).saturating_add(4) > h.value_block_len)
        {
            return Err(r.error(format!(
                "attribute `{}` value offset {} outside the {}-byte value block",
                bad.name, bad.offset, h.value_block_len
            )));
        }

        let range_table = table(&mut r, h.part_ranges, h.part_count, PART_RANGE_LEN)?;
        let part_attributes = records(range_table, h.part_count, PART_RANGE_LEN)
            .map(|record| {
                let (from, to) = (u32_at(record, 0) as usize, u32_at(record, 4) as usize);
                if from <= to && to <= attributes.len() {
                    Ok(from..to)
                } else {
                    Err(r.error(format!(
                        "part attribute range {from}..{to} outside {} attributes",
                        attributes.len()
                    )))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Each variant owns one value block; an attribute's value sits at
        // its own offset inside it.
        let name_table = table(&mut r, h.variants, h.variant_count, 8)?;
        let value_table = table(&mut r, h.values, h.variant_count, h.value_block_len)?;
        let variants = records(name_table, h.variant_count, 8)
            .zip(records(value_table, h.variant_count, h.value_block_len))
            .map(|(name, block)| Variant {
                name: token_at(name),
                values: attributes
                    .iter()
                    .map(|a| u32_at(block, a.offset as usize))
                    .collect(),
            })
            .collect();

        let looks = Self::read_looks(&mut r, &h)?;
        Ok(Self {
            looks,
            variants,
            attributes,
            part_attributes,
        })
    }

    /// Reads the look names and the look-major `look × material` table of
    /// absolute offsets into the material string block. The table order is
    /// read from `sleeper_2024.pmd`: look-major, every look lists distinct
    /// materials that agree slot by slot across looks.
    fn read_looks(r: &mut Reader<'_>, h: &Header) -> Result<Vec<Look>, FormatError> {
        let name_table = table(r, h.looks, h.look_count, 8)?;
        let row_len = h
            .material_count
            .checked_mul(4)
            .ok_or_else(|| r.error("material count overflows"))?;
        let offset_table = table(r, h.material_offsets, h.look_count, row_len)?;
        let strings = Reader::new(
            "pmd material strings",
            table(r, h.material_strings, 1, h.material_strings_len)?,
        );
        // Every table entry names its own string in game data; capping the
        // decoded text at the file size stops a table that points many
        // entries at one long string from multiplying memory.
        let mut budget = r.len();
        records(name_table, h.look_count, 8)
            .zip(records(offset_table, h.look_count, row_len))
            .map(|(name, row)| {
                let materials = (0..h.material_count)
                    .map(|slot| {
                        let at = u32_at(row, slot * 4) as usize;
                        let path = at
                            .checked_sub(h.material_strings)
                            .ok_or_else(|| r.error(format!("material string offset {at:#x}")))
                            .and_then(|at| strings.c_string_at(at))?;
                        budget = budget
                            .checked_sub(path.len())
                            .ok_or_else(|| r.error("material paths exceed the file size"))?;
                        Ok(path)
                    })
                    .collect::<Result<_, FormatError>>()?;
                Ok(Look {
                    name: token_at(name),
                    materials,
                })
            })
            .collect()
    }

    /// The look with this name, compared ASCII case-insensitively.
    pub fn look(&self, name: &str) -> Option<&Look> {
        self.looks
            .iter()
            .find(|look| look.name.eq_ignore_ascii_case(name))
    }

    /// Index of the variant with this name, compared ASCII case-insensitively.
    pub fn variant_index(&self, name: &str) -> Option<usize> {
        self.variants
            .iter()
            .position(|variant| variant.name.eq_ignore_ascii_case(name))
    }

    /// Whether `part` is shown in `variant`: every attribute of that part
    /// named `visible` has a non-zero value in that variant. A part without a
    /// `visible` attribute is visible, and so is any out-of-range `variant` or
    /// `part` index. Must never panic, even on inconsistent data.
    pub fn is_part_visible(&self, variant: usize, part: usize) -> bool {
        let (Some(range), Some(variant)) =
            (self.part_attributes.get(part), self.variants.get(variant))
        else {
            return true;
        };
        // Clamp so an inconsistent range never walks past the attributes.
        let end = range.end.min(self.attributes.len());
        (range.start..end).all(|index| {
            let is_visibility = self
                .attributes
                .get(index)
                .is_some_and(|attribute| attribute.name == VISIBLE);
            !is_visibility || variant.values.get(index).is_none_or(|&value| value != 0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Token;

    /// A model description in the shape of the game's files.
    struct Fixture {
        looks: Vec<(&'static str, Vec<&'static str>)>,
        variants: Vec<(&'static str, Vec<u32>)>,
        parts: Vec<(u32, u32)>,
        attributes: Vec<&'static str>,
    }

    fn token(text: &str) -> [u8; 8] {
        Token::encode(text).unwrap().0.to_le_bytes()
    }

    fn put(out: &mut [u8], at: usize, value: u32) {
        out[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Lays the blocks out in file order (models.md, "PMD"): looks,
    /// variants, part ranges, attribute headers, values, material offsets,
    /// material strings.
    fn build(f: &Fixture) -> Vec<u8> {
        let materials = f.looks.first().map_or(0, |l| l.1.len());
        let mut out = vec![0u8; 0x40];
        // looks, variants, part ranges, values, attributes, material
        // offsets, material strings (header order).
        let mut offsets = [0u32; 7];

        offsets[0] = out.len() as u32;
        for (name, _) in &f.looks {
            out.extend(token(name));
        }
        offsets[1] = out.len() as u32;
        for (name, _) in &f.variants {
            out.extend(token(name));
        }
        offsets[2] = out.len() as u32;
        for (from, to) in &f.parts {
            out.extend(from.to_le_bytes());
            out.extend(to.to_le_bytes());
        }
        offsets[4] = out.len() as u32;
        for (i, name) in f.attributes.iter().enumerate() {
            out.extend(token(name));
            out.extend(0u32.to_le_bytes());
            out.extend((4 * i as u32).to_le_bytes());
        }
        offsets[3] = out.len() as u32;
        for (_, values) in &f.variants {
            for value in values {
                out.extend(value.to_le_bytes());
            }
        }
        offsets[5] = out.len() as u32;
        let table = out.len();
        out.resize(table + 4 * materials * f.looks.len(), 0);
        offsets[6] = out.len() as u32;
        let strings = out.len();
        for (look, (_, paths)) in f.looks.iter().enumerate() {
            for (slot, path) in paths.iter().enumerate() {
                let at = out.len() as u32;
                put(&mut out, table + 4 * (look * materials + slot), at);
                out.extend(path.bytes());
                out.push(0);
            }
        }

        let header = [
            VERSION,
            materials as u32,
            f.looks.len() as u32,
            0,
            f.variants.len() as u32,
            f.parts.len() as u32,
            f.attributes.len() as u32,
            4 * f.attributes.len() as u32,
            (out.len() - strings) as u32,
        ];
        for (i, value) in header.iter().chain(&offsets).enumerate() {
            put(&mut out, 4 * i, *value);
        }
        out
    }

    /// Modelled on `cab_s_low.pmd`: variant `default` shows parts 0 and 1,
    /// `uk` shows parts 0 and 2 (models.md, "PMD").
    fn cabin() -> Fixture {
        Fixture {
            looks: vec![
                (
                    "default",
                    vec!["/automat/aa/one.mat", "/automat/bb/two.mat"],
                ),
                ("gt", vec!["/automat/aa/one.mat", "/automat/cc/three.mat"]),
            ],
            variants: vec![("default", vec![1, 1, 0]), ("uk", vec![1, 0, 1])],
            parts: vec![(0, 1), (1, 2), (2, 3)],
            attributes: vec!["visible"; 3],
        }
    }

    #[test]
    fn parses_looks_variants_and_part_attributes() {
        let pmd = Pmd::parse(&build(&cabin())).unwrap();
        assert_eq!(pmd.looks.len(), 2);
        assert_eq!(pmd.looks[0].name, "default");
        assert_eq!(
            pmd.looks[1].materials,
            ["/automat/aa/one.mat", "/automat/cc/three.mat"]
        );
        assert_eq!(pmd.variants[1].name, "uk");
        assert_eq!(pmd.variants[1].values, [1, 0, 1]);
        assert_eq!(pmd.part_attributes, [0..1, 1..2, 2..3]);
        assert_eq!(
            pmd.attributes[2],
            AttributeDef {
                name: "visible".into(),
                kind: 0,
                offset: 8
            }
        );
    }

    #[test]
    fn values_follow_the_attribute_offsets() {
        let mut bytes = build(&Fixture {
            variants: vec![("default", vec![7, 9])],
            parts: vec![(0, 2)],
            attributes: vec!["visible", "visible"],
            ..cabin()
        });
        // Swap the two value slots: attribute 0 now reads bytes 4..8.
        let attributes = u32_at(&bytes, 0x34) as usize;
        put(&mut bytes, attributes + 12, 4);
        put(&mut bytes, attributes + 16 + 12, 0);
        let pmd = Pmd::parse(&bytes).unwrap();
        assert_eq!(pmd.variants[0].values, [9, 7]);
    }

    #[test]
    fn looks_and_variants_match_case_insensitively() {
        let pmd = Pmd::parse(&build(&cabin())).unwrap();
        assert_eq!(pmd.look("GT").map(|l| l.name.as_str()), Some("gt"));
        assert_eq!(pmd.look("missing"), None);
        assert_eq!(pmd.variant_index("Uk"), Some(1));
        assert_eq!(pmd.variant_index("eu"), None);
    }

    #[test]
    fn part_visibility_follows_the_variant() {
        let pmd = Pmd::parse(&build(&cabin())).unwrap();
        let shown = |variant| {
            (0..3)
                .map(|part| pmd.is_part_visible(variant, part))
                .collect::<Vec<_>>()
        };
        assert_eq!(shown(0), [true, true, false]);
        assert_eq!(shown(1), [true, false, true]);
        // Out-of-range indices default to visible.
        assert!(pmd.is_part_visible(5, 2));
        assert!(pmd.is_part_visible(1, 99));
    }

    #[test]
    fn parts_without_a_visible_attribute_are_visible() {
        let pmd = Pmd::parse(&build(&Fixture {
            variants: vec![("default", vec![0, 0])],
            parts: vec![(0, 1), (1, 1), (1, 2)],
            attributes: vec!["colour", "visible"],
            ..cabin()
        }))
        .unwrap();
        assert!(pmd.is_part_visible(0, 0));
        assert!(pmd.is_part_visible(0, 1));
        assert!(!pmd.is_part_visible(0, 2));
    }

    #[test]
    fn visibility_never_panics_on_inconsistent_data() {
        let mut pmd = Pmd::parse(&build(&cabin())).unwrap();
        #[allow(clippy::reversed_empty_ranges)]
        let inverted = 3..1;
        pmd.part_attributes = vec![2..usize::MAX, inverted, 0..3];
        // Variant 0 hides attribute 2; variant 1 has lost its values.
        pmd.variants[1].values.clear();
        assert!(!pmd.is_part_visible(0, 0));
        assert!(pmd.is_part_visible(1, 0));
        assert!(pmd.is_part_visible(0, 1));
        assert!(!pmd.is_part_visible(0, 2));
        assert!(pmd.is_part_visible(1, 2));
        pmd.attributes.clear();
        assert!(pmd.is_part_visible(0, 0));
    }

    #[test]
    fn accepts_a_model_without_attributes() {
        let pmd = Pmd::parse(&build(&Fixture {
            variants: vec![("default", vec![])],
            parts: vec![(0, 0)],
            attributes: vec![],
            ..cabin()
        }))
        .unwrap();
        assert_eq!(pmd.variants[0].name, "default");
        assert!(pmd.variants[0].values.is_empty());
        assert!(pmd.is_part_visible(0, 0));
    }

    #[test]
    fn rejects_a_wrong_version() {
        let mut bytes = build(&cabin());
        put(&mut bytes, 0, 5);
        assert!(Pmd::parse(&bytes).is_err());
    }

    #[test]
    fn rejects_every_truncation() {
        let bytes = build(&cabin());
        for len in 0..bytes.len() {
            assert!(Pmd::parse(&bytes[..len]).is_err(), "length {len}");
        }
    }

    #[test]
    fn rejects_bad_offsets_counts_and_ranges() {
        let good = build(&cabin());
        let patched = |at: usize, value: u32| {
            let mut bytes = good.clone();
            put(&mut bytes, at, value);
            Pmd::parse(&bytes)
        };
        for at in (0x24..0x40).step_by(4) {
            assert!(patched(at, u32::MAX).is_err(), "offset field {at:#x}");
        }
        for at in [0x04, 0x08, 0x10, 0x14, 0x18] {
            assert!(patched(at, u32::MAX).is_err(), "count field {at:#x}");
        }
        let ranges = u32_at(&good, 0x2c) as usize;
        assert!(patched(ranges + 4, 4).is_err(), "range end past attributes");
        assert!(patched(ranges, 2).is_err(), "inverted range");
        let attributes = u32_at(&good, 0x34) as usize;
        assert!(patched(attributes + 12, 12).is_err(), "value offset");
        let table = u32_at(&good, 0x38) as usize;
        assert!(patched(table, u32::MAX).is_err(), "material string offset");

        let mut unterminated = good.clone();
        let last = unterminated.len() - 1;
        unterminated[last] = b'x';
        assert!(Pmd::parse(&unterminated).is_err());
    }

    #[test]
    fn rejects_attributes_sharing_one_value_slot() {
        // 300 variants x 300 attributes, all reading the same 4-byte value:
        // a few kilobytes that would decode to 90,000 values.
        let mut bytes = build(&Fixture {
            variants: vec![("default", vec![1]); 300],
            parts: vec![(0, 300)],
            attributes: vec!["visible"; 300],
            ..cabin()
        });
        put(&mut bytes, 0x1c, 4);
        let attributes = u32_at(&bytes, 0x34) as usize;
        for i in 0..300 {
            put(&mut bytes, attributes + 16 * i + 12, 0);
        }
        assert!(Pmd::parse(&bytes).is_err());
    }

    #[test]
    fn caps_material_text_at_the_file_size() {
        // 64 looks whose single material all point at one 400-byte string.
        let looks = 64usize;
        let mut bytes = vec![0u8; 0x40];
        put(&mut bytes, 0x00, VERSION);
        put(&mut bytes, 0x04, 1);
        put(&mut bytes, 0x08, looks as u32);
        for at in [0x24, 0x28, 0x2c, 0x30, 0x34] {
            put(&mut bytes, at, 0x40);
        }
        for _ in 0..looks {
            bytes.extend(token("default"));
        }
        let table = bytes.len() as u32;
        put(&mut bytes, 0x38, table);
        let string = bytes.len() + 4 * looks;
        for _ in 0..looks {
            bytes.extend((string as u32).to_le_bytes());
        }
        put(&mut bytes, 0x3c, string as u32);
        bytes.extend([b'x'; 400]);
        bytes.push(0);
        put(&mut bytes, 0x20, 401);
        let error = Pmd::parse(&bytes).unwrap_err();
        assert!(error.message.contains("file size"), "{error}");

        // The same table with a short string is fine.
        bytes.truncate(string);
        bytes.extend(b"/a.mat\0");
        put(&mut bytes, 0x20, 7);
        let pmd = Pmd::parse(&bytes).unwrap();
        assert_eq!(pmd.looks.len(), looks);
        assert_eq!(pmd.looks[63].materials, ["/a.mat"]);
    }
}
