//! PMD model descriptors (version 4): looks (material sets) and variants
//! (per-part attribute values such as visibility). The geometry lives in
//! the sibling `.pmg`. Layout evidence: `docs/formats/models.md#pmd-model-descriptor-version-4`.

use crate::binary::{FormatError, Reader};
use crate::token::Token;

const VERSION: u32 = 4;
const WHAT: &str = "pmd";

/// A look: one material path per material slot of the model.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub name: String,
    pub materials: Vec<String>,
}

/// A part attribute declaration (in practice always `visible`).
#[derive(Clone, Debug, PartialEq)]
pub struct AttributeDef {
    pub name: String,
    pub kind: u32,
    /// Byte offset of the value inside each variant's value block.
    pub offset: u32,
}

/// A variant: one value for every declared attribute.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: String,
    pub values: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pmd {
    pub looks: Vec<Look>,
    pub variants: Vec<Variant>,
    pub attributes: Vec<AttributeDef>,
    /// For each part (same order as the `.pmg` parts), the range of
    /// `attributes` that belong to it.
    pub part_attributes: Vec<std::ops::Range<usize>>,
}

impl Pmd {
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let mut r = Reader::new(WHAT, bytes);
        let version = r.u32()?;
        if version != VERSION {
            return Err(r.error(format!("unsupported version {version}")));
        }
        let material_count = r.count(4)?;
        let look_count = r.count(8)?;
        let _piece_count = r.u32()?;
        let variant_count = r.count(8)?;
        let part_count = r.count(8)?;
        let attribute_count = r.count(16)?;
        let values_size = r.u32()? as usize;
        let _material_block_size = r.u32()?;
        let look_offset = r.u32()? as usize;
        let variant_offset = r.u32()? as usize;
        let part_attribs_offset = r.u32()? as usize;
        let values_offset = r.u32()? as usize;
        let attribs_header_offset = r.u32()? as usize;
        let material_offsets_offset = r.u32()? as usize;
        let _material_paths_offset = r.u32()?;

        r.seek(look_offset)?;
        let look_names = read_tokens(&mut r, look_count)?;
        r.seek(variant_offset)?;
        let variant_names = read_tokens(&mut r, variant_count)?;

        r.seek(part_attribs_offset)?;
        let mut part_attributes = Vec::with_capacity(part_count);
        for _ in 0..part_count {
            let (from, to) = (r.u32()? as usize, r.u32()? as usize);
            if from > to || to > attribute_count {
                return Err(r.error(format!("bad part attribute range {from}..{to}")));
            }
            part_attributes.push(from..to);
        }

        r.seek(attribs_header_offset)?;
        let mut attributes = Vec::with_capacity(attribute_count);
        for _ in 0..attribute_count {
            let name = r.token()?.decode();
            let kind = r.u32()?;
            let offset = r.u32()?;
            if offset as usize + 4 > values_size {
                return Err(r.error(format!(
                    "attribute `{name}` offset {offset} outside value block"
                )));
            }
            attributes.push(AttributeDef { name, kind, offset });
        }

        let mut variants = Vec::with_capacity(variant_count);
        for (index, name) in variant_names.into_iter().enumerate() {
            let mut values = Vec::with_capacity(attribute_count);
            for attribute in &attributes {
                r.seek(values_offset + index * values_size + attribute.offset as usize)?;
                values.push(r.u32()?);
            }
            variants.push(Variant { name, values });
        }

        r.seek(material_offsets_offset)?;
        let mut looks = Vec::with_capacity(look_count);
        for name in look_names {
            let mut materials = Vec::with_capacity(material_count);
            for _ in 0..material_count {
                let offset = r.u32()? as usize;
                materials.push(r.c_string_at(offset)?);
            }
            looks.push(Look { name, materials });
        }

        Ok(Self {
            looks,
            variants,
            attributes,
            part_attributes,
        })
    }

    pub fn look(&self, name: &str) -> Option<&Look> {
        self.looks
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(name))
    }

    pub fn variant_index(&self, name: &str) -> Option<usize> {
        self.variants
            .iter()
            .position(|v| v.name.eq_ignore_ascii_case(name))
    }

    /// Whether `part` is visible in variant `variant`. Parts without a
    /// `visible` attribute are always visible.
    pub fn is_part_visible(&self, variant: usize, part: usize) -> bool {
        let (Some(values), Some(range)) = (
            self.variants.get(variant).map(|v| &v.values),
            self.part_attributes.get(part),
        ) else {
            return true;
        };
        range
            .clone()
            .filter(|&i| self.attributes[i].name == "visible")
            .all(|i| values[i] != 0)
    }
}

fn read_tokens(r: &mut Reader<'_>, count: usize) -> Result<Vec<String>, FormatError> {
    (0..count).map(|_| r.token().map(Token::decode)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal two-variant, two-part, one-material PMD.
    fn sample() -> Vec<u8> {
        let mut b = Vec::new();
        let header_len = 64u32;
        let look = Token::encode("default").expect("token").0;
        let variants = [
            Token::encode("default").expect("t").0,
            Token::encode("uk").expect("t").0,
        ];
        let visible = Token::encode("visible").expect("t").0;
        let look_offset = header_len;
        let variant_offset = look_offset + 8;
        let part_attribs_offset = variant_offset + 16;
        let attribs_header_offset = part_attribs_offset + 16;
        let values_offset = attribs_header_offset + 2 * 16;
        let material_offsets_offset = values_offset + 2 * 8;
        let material_paths_offset = material_offsets_offset + 4;
        for v in [
            4,
            1,
            1,
            0,
            2,
            2,
            2,
            8,
            6,
            look_offset,
            variant_offset,
            part_attribs_offset,
            values_offset,
            attribs_header_offset,
            material_offsets_offset,
            material_paths_offset,
        ] {
            b.extend(u32::to_le_bytes(v));
        }
        b.extend(look.to_le_bytes());
        for v in variants {
            b.extend(v.to_le_bytes());
        }
        for v in [0u32, 1, 1, 2] {
            b.extend(v.to_le_bytes());
        }
        for offset in [0u32, 4] {
            b.extend(visible.to_le_bytes());
            b.extend(0u32.to_le_bytes());
            b.extend(offset.to_le_bytes());
        }
        for v in [1u32, 0, 0, 1] {
            b.extend(v.to_le_bytes());
        }
        b.extend(material_paths_offset.to_le_bytes());
        b.extend(b"/a.mat\0");
        b
    }

    #[test]
    fn parses_looks_variants_and_visibility() {
        let pmd = Pmd::parse(&sample()).expect("parse");
        assert_eq!(pmd.looks[0].name, "default");
        assert_eq!(pmd.looks[0].materials, ["/a.mat"]);
        assert_eq!(pmd.variant_index("UK"), Some(1));
        assert!(pmd.is_part_visible(0, 0));
        assert!(!pmd.is_part_visible(0, 1));
        assert!(!pmd.is_part_visible(1, 0));
        assert!(pmd.is_part_visible(1, 1));
    }

    #[test]
    fn rejects_truncated_input() {
        let bytes = sample();
        for len in [0, 10, 63, 80, bytes.len() - 3] {
            assert!(Pmd::parse(&bytes[..len]).is_err(), "len {len}");
        }
    }
}
