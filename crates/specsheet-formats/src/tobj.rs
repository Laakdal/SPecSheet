//! Standalone `.tobj` files, as mod packages hold them next to their
//! `.dds` (game archives pack the two into one entry instead,
//! `specsheet-archive::texture`). Layout from our own bytes:
//! `research/evidence/mod-name-recovery.txt` section 8 (the path records)
//! and `research/evidence/tobj-loose.txt` (the header).

/// First bytes of a standalone `.tobj` file.
pub const MAGIC: [u8; 4] = [0x01, 0x0a, 0xb1, 0x70];
/// Where the texture path records start.
const PATHS_AT: usize = 0x28;
/// The longest path a record may hold.
const MAX_PATH: usize = 512;

/// The texture paths of a standalone `.tobj` file: from 0x28, records of a
/// little-endian u64 length and that many path bytes, filling the file
/// exactly (one record; six for a cube map). Read whole, since a path may
/// hold characters such as `?*<>:#`. `None` when the bytes are not a
/// `.tobj` or the records do not fill them.
pub fn texture_paths(bytes: &[u8]) -> Option<Vec<&str>> {
    if !bytes.starts_with(&MAGIC) {
        return None;
    }
    let mut rest = bytes.get(PATHS_AT..)?;
    let mut out = Vec::new();
    while !rest.is_empty() {
        let (len, tail) = rest.split_first_chunk::<8>()?;
        let len = usize::try_from(u64::from_le_bytes(*len)).ok()?;
        if len > MAX_PATH || len > tail.len() {
            return None;
        }
        let (path, tail) = tail.split_at(len);
        out.push(std::str::from_utf8(path).ok()?);
        rest = tail;
    }
    (!out.is_empty()).then_some(out)
}

/// What a standalone `.tobj` header says about its texture
/// (`research/evidence/tobj-loose.txt`, all Hypothesis, strong).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tobj {
    /// A cube map (byte 0x18 = 5; 2 is 2D).
    pub cube: bool,
    /// Address modes for u, v and w (bytes 0x1e, 0x1f, 0x20), in the code
    /// space of a packed texture's sample record: 0 repeat, 2 clamp to edge.
    pub address: [u8; 3],
    /// Whether the texture has mipmaps (byte 0x1c is not 2).
    pub mipmaps: bool,
    /// Colour data is linear, not sRGB (byte 0x26 or 0x16 is 1). Only
    /// matters for a `.dds` whose header names no colour space.
    pub linear: bool,
    /// The texture paths: one, or six for a cube map.
    pub paths: Vec<String>,
}

/// Header byte offsets (`tobj-loose.txt` section 2).
const TYPE_AT: usize = 0x18;
const MIPMAPS_AT: usize = 0x1c;
const ADDRESS_AT: usize = 0x1e;
const LINEAR_AT: usize = 0x26;
const LINEAR_TOO_AT: usize = 0x16;
/// Type byte of a cube map.
const TYPE_CUBE: u8 = 5;
/// Mipmap byte meaning none.
const NO_MIPMAPS: u8 = 2;

/// Reads a standalone `.tobj`. `None` when the bytes are not one.
pub fn parse(bytes: &[u8]) -> Option<Tobj> {
    let paths = texture_paths(bytes)?;
    let header = bytes.get(..PATHS_AT)?;
    Some(Tobj {
        cube: header[TYPE_AT] == TYPE_CUBE,
        address: [
            header[ADDRESS_AT],
            header[ADDRESS_AT + 1],
            header[ADDRESS_AT + 2],
        ],
        mipmaps: header[MIPMAPS_AT] != NO_MIPMAPS,
        linear: header[LINEAR_AT] == 1 || header[LINEAR_TOO_AT] == 1,
        paths: paths.into_iter().map(str::to_owned).collect(),
    })
}

/// Header bytes whose meaning is Unknown, written with the value most mod
/// files hold (`tobj-loose.txt` section 1): 0x14, 0x1a/0x1b and 0x25.
const UNKNOWN_DEFAULTS: [(usize, u8); 4] = [(0x14, 1), (0x1a, 3), (0x1b, 3), (0x25, 1)];
/// Type byte of a 2D texture.
const TYPE_2D: u8 = 2;
/// Mipmap byte the most mod files with mipmaps hold.
const WITH_MIPMAPS: u8 = 3;

/// A standalone `.tobj` for `tobj`, as a mod package would hold it; the
/// bytes whose meaning is Unknown take their most common value. `None`
/// when a path is longer than a record may hold or there is none.
pub fn write(tobj: &Tobj) -> Option<Vec<u8>> {
    if tobj.paths.is_empty() || tobj.paths.iter().any(|p| p.len() > MAX_PATH) {
        return None;
    }
    let mut bytes = vec![0u8; PATHS_AT];
    bytes[..4].copy_from_slice(&MAGIC);
    for (at, value) in UNKNOWN_DEFAULTS {
        bytes[at] = value;
    }
    bytes[TYPE_AT] = if tobj.cube { TYPE_CUBE } else { TYPE_2D };
    bytes[MIPMAPS_AT] = if tobj.mipmaps {
        WITH_MIPMAPS
    } else {
        NO_MIPMAPS
    };
    bytes[ADDRESS_AT..ADDRESS_AT + 3].copy_from_slice(&tobj.address);
    bytes[LINEAR_AT] = u8::from(tobj.linear);
    for path in &tobj.paths {
        bytes.extend((path.len() as u64).to_le_bytes());
        bytes.extend(path.as_bytes());
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `.tobj` header shaped like the Sanax Volvo FH2022's
    /// (`tobj-loose.txt`), with `changes` applied, and one path record.
    fn tobj(changes: &[(usize, u8)], paths: &[&str]) -> Vec<u8> {
        let mut bytes = vec![0u8; PATHS_AT];
        bytes[..4].copy_from_slice(&MAGIC);
        for (at, value) in [
            (0x14, 1),
            (0x18, 2),
            (0x1a, 1),
            (0x1b, 1),
            (0x1c, 3),
            (0x25, 1),
        ] {
            bytes[at] = value;
        }
        for &(at, value) in changes {
            bytes[at] = value;
        }
        for path in paths {
            bytes.extend((path.len() as u64).to_le_bytes());
            bytes.extend(path.as_bytes());
        }
        bytes
    }

    #[test]
    fn reads_the_header_and_paths() {
        let plain = parse(&tobj(&[], &["/vehicle/x.dds"])).unwrap();
        assert_eq!(
            plain,
            Tobj {
                cube: false,
                address: [0, 0, 0],
                mipmaps: true,
                linear: false,
                paths: vec!["/vehicle/x.dds".to_owned()],
            }
        );
        let clamped = parse(&tobj(
            &[(0x1e, 2), (0x1f, 2), (0x1c, 2), (0x26, 1)],
            &["/ui/y.dds"],
        ))
        .unwrap();
        assert_eq!(clamped.address, [2, 2, 0]);
        assert!(!clamped.mipmaps && clamped.linear);
        let faces = ["/a/1.dds"; 6];
        let cube = parse(&tobj(&[(0x18, 5), (0x20, 2)], &faces)).unwrap();
        assert!(cube.cube && cube.paths.len() == 6);
        assert!(parse(&tobj(&[(0x16, 1)], &["/n.dds"])).unwrap().linear);
    }

    /// What `write` makes reads back as the same header and paths.
    #[test]
    fn writes_a_header_that_reads_back() {
        for tobj in [
            Tobj {
                cube: false,
                address: [2, 0, 0],
                mipmaps: true,
                linear: false,
                paths: vec!["/vehicle/x.dds".to_owned()],
            },
            Tobj {
                cube: true,
                address: [2, 2, 2],
                mipmaps: false,
                linear: true,
                paths: vec!["/env/cube.dds".to_owned()],
            },
        ] {
            let bytes = write(&tobj).expect("writes");
            assert_eq!(parse(&bytes), Some(tobj));
        }
        let too_long = Tobj {
            cube: false,
            address: [0; 3],
            mipmaps: true,
            linear: false,
            paths: vec!["x".repeat(MAX_PATH + 1)],
        };
        assert_eq!(write(&too_long), None);
    }

    #[test]
    fn rejects_what_is_not_a_tobj() {
        assert_eq!(parse(b"DDS "), None);
        assert_eq!(parse(&tobj(&[], &[])), None);
        let mut short = tobj(&[], &["/x.dds"]);
        short.pop();
        assert_eq!(parse(&short), None);
    }
}
