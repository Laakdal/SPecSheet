//! ScsC, the encrypted container of profile and save files
//! (`profiles/<hex name>/profile.sii`, `save/*/game.sii`). Layout evidence:
//! `docs/formats/profiles.md`.
//!
//! ```text
//! "ScsC" | 32 bytes (unknown, not checked) | IV: 16 bytes | u32 size | body
//! body = AES-256-CBC(zlib(payload)), padded to 16 bytes
//! ```
//!
//! The payload is SII text (`SiiN`) in every profile observed and in 47 of
//! 79 encrypted saves. The other 32 saves hold binary SII (`BSII`), parsed
//! by [`crate::bsii`]; [`document`] reads any of these forms.

use std::io::Read;

use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use flate2::read::ZlibDecoder;

/// Errors produced while opening an SII container.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScscError {
    /// Neither the ScsC magic nor SII text.
    #[error("not an ScsC container or SII text (magic {0:02x?})")]
    UnknownMagic([u8; 4]),
    /// The header is shorter than its fixed size.
    #[error("truncated ScsC header")]
    Truncated,
    /// The encrypted body is not a multiple of the AES block size.
    #[error("ScsC body of {0} bytes is not whole AES blocks")]
    PartialBlock(usize),
    /// The declared plain size is over the reader's limit.
    #[error("ScsC declared size {0} exceeds limit")]
    TooLarge(u32),
    /// The decrypted payload is not valid zlib.
    #[error("ScsC payload does not inflate: {0}")]
    Inflate(String),
    /// The inflated size differs from the declared size.
    #[error("ScsC payload inflated to {actual} bytes, declared {declared}")]
    SizeMismatch {
        /// The size in the header.
        declared: u32,
        /// The inflated size.
        actual: usize,
    },
    /// The payload is binary SII, which this function does not parse.
    #[error("binary SII (BSII) is not supported")]
    BinarySii,
}

const MAGIC: &[u8; 4] = b"ScsC";
const TEXT_MAGIC: &[u8; 4] = b"SiiN";
const BINARY_MAGIC: &[u8; 4] = b"BSII";
const IV_AT: usize = 36;
const SIZE_AT: usize = 52;
const BODY_AT: usize = 56;
const BLOCK: usize = 16;
/// Observed payloads reach 7 MiB (profiles) and 12 MiB (saves). Guards
/// against a hostile size field.
const MAX_SIZE: u32 = 1 << 30;

/// The AES-256 key of ScsC containers, a format constant like a magic
/// number. Source: sii-decode-rs (MIT), as a fact only. It is not stored as
/// contiguous bytes in `eurotrucks2.exe`. Confirmed by decrypting all 4
/// profiles on the test install to zlib streams that inflate to exactly the
/// declared size (`research/evidence/profile-scsc-survey.txt`).
const KEY: [u8; 32] = [
    0x2a, 0x5f, 0xcb, 0x17, 0x91, 0xd2, 0x2f, 0xb6, 0x02, 0x45, 0xb3, 0xd8, 0x36, 0x9e, 0xd0, 0xb2,
    0xc2, 0x73, 0x71, 0x56, 0x3f, 0xbf, 0x1f, 0x3c, 0x9e, 0xdf, 0x6b, 0x11, 0x82, 0x5a, 0x5d, 0x0a,
];

/// Decrypts and inflates an ScsC container into its payload.
pub fn decrypt(bytes: &[u8]) -> Result<Vec<u8>, ScscError> {
    if magic(bytes) != *MAGIC {
        return Err(ScscError::UnknownMagic(magic(bytes)));
    }
    let body = bytes.get(BODY_AT..).ok_or(ScscError::Truncated)?;
    let iv: [u8; 16] = bytes[IV_AT..SIZE_AT]
        .try_into()
        .map_err(|_| ScscError::Truncated)?;
    let declared = u32::from_le_bytes([
        bytes[SIZE_AT],
        bytes[SIZE_AT + 1],
        bytes[SIZE_AT + 2],
        bytes[SIZE_AT + 3],
    ]);
    if body.len() % BLOCK != 0 {
        return Err(ScscError::PartialBlock(body.len()));
    }
    if declared > MAX_SIZE {
        return Err(ScscError::TooLarge(declared));
    }
    let mut plain = body.to_vec();
    // The zlib stream ends before the padding, so the padding is not checked.
    cbc::Decryptor::<aes::Aes256>::new(&KEY.into(), &iv.into())
        .decrypt_padded_mut::<NoPadding>(&mut plain)
        .map_err(|e| ScscError::Inflate(e.to_string()))?;
    let mut out = Vec::new();
    ZlibDecoder::new(&plain[..])
        .take(u64::from(declared) + 1)
        .read_to_end(&mut out)
        .map_err(|e| ScscError::Inflate(e.to_string()))?;
    if out.len() != declared as usize {
        return Err(ScscError::SizeMismatch {
            declared,
            actual: out.len(),
        });
    }
    Ok(out)
}

/// SII text from a profile or save file: decrypted when it is an ScsC
/// container, returned unchanged when it is already text (players edit
/// these files with community decryptors).
pub fn sii_text(bytes: &[u8]) -> Result<Vec<u8>, ScscError> {
    let payload = match magic(bytes) {
        m if m == *MAGIC => decrypt(bytes)?,
        _ => bytes.to_vec(),
    };
    match magic(&payload) {
        m if m == *TEXT_MAGIC => Ok(payload),
        m if m == *BINARY_MAGIC => Err(ScscError::BinarySii),
        m => Err(ScscError::UnknownMagic(m)),
    }
}

/// Errors produced by [`document`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DocumentError {
    /// The container could not be opened.
    #[error(transparent)]
    Container(#[from] ScscError),
    /// The SII text does not parse.
    #[error(transparent)]
    Text(#[from] crate::sii::SiiError),
    /// The binary SII does not parse.
    #[error(transparent)]
    Binary(#[from] crate::bsii::BsiiError),
}

/// The units of a profile or save file in any of its forms: an ScsC
/// container, SII text or binary SII. `file` names it in error messages.
pub fn document(file: &str, bytes: &[u8]) -> Result<crate::sii::SiiDocument, DocumentError> {
    let payload = match magic(bytes) {
        m if m == *MAGIC => decrypt(bytes)?,
        _ => bytes.to_vec(),
    };
    match magic(&payload) {
        m if m == *TEXT_MAGIC => Ok(crate::sii::parse(file, &payload)?),
        m if m == *BINARY_MAGIC => Ok(crate::bsii::parse(&payload)?),
        m => Err(ScscError::UnknownMagic(m).into()),
    }
}

/// The first four bytes, zero-filled when the input is shorter.
fn magic(bytes: &[u8]) -> [u8; 4] {
    let mut out = [0u8; 4];
    for (slot, byte) in out.iter_mut().zip(bytes) {
        *slot = *byte;
    }
    out
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
    use flate2::write::ZlibEncoder;
    use flate2::Compression;

    use super::*;

    const TEXT: &[u8] = b"SiiNunit\r\n{\r\nuser_profile : _nameless.1 {\r\n active_mods: 1\r\n active_mods[0]: \"pkg|Name\"\r\n}\r\n}\r\n";

    /// Builds a container the way the game's files look.
    fn container(payload: &[u8], declared: Option<u32>) -> Vec<u8> {
        let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
        zlib.write_all(payload).unwrap();
        let compressed = zlib.finish().unwrap();
        let iv = [7u8; 16];
        let mut body = compressed.clone();
        body.resize((compressed.len() / 16 + 1) * 16, 0);
        cbc::Encryptor::<aes::Aes256>::new(&KEY.into(), &iv.into())
            .encrypt_padded_mut::<Pkcs7>(&mut body, compressed.len())
            .unwrap();
        let mut out = b"ScsC".to_vec();
        out.extend([0u8; 32]);
        out.extend(iv);
        out.extend(declared.unwrap_or(payload.len() as u32).to_le_bytes());
        out.extend(body);
        out
    }

    #[test]
    fn decrypts_a_container() {
        assert_eq!(decrypt(&container(TEXT, None)).unwrap(), TEXT);
        assert_eq!(sii_text(&container(TEXT, None)).unwrap(), TEXT);
    }

    #[test]
    fn passes_plain_text_through() {
        assert_eq!(sii_text(TEXT).unwrap(), TEXT);
    }

    #[test]
    fn documents_come_from_text_binary_and_containers() {
        let text = document("/profile.sii", TEXT).unwrap();
        assert_eq!(text.units[0].class, "user_profile");
        let encrypted = document("/profile.sii", &container(TEXT, None)).unwrap();
        assert_eq!(encrypted, text);
        // An empty binary file: header and end marker only.
        let binary = b"BSII\x03\x00\x00\x00\x00\x00\x00\x00\x00";
        assert!(document("/game.sii", binary).unwrap().units.is_empty());
        assert!(document("/game.sii", &container(binary, None))
            .unwrap()
            .units
            .is_empty());
        assert!(matches!(
            document("/game.sii", b"PK\x03\x04"),
            Err(DocumentError::Container(ScscError::UnknownMagic(_)))
        ));
    }

    #[test]
    fn rejects_binary_sii() {
        let binary = b"BSII\x03\x00\x00\x00";
        assert_eq!(sii_text(binary), Err(ScscError::BinarySii));
        assert_eq!(
            sii_text(&container(binary, None)),
            Err(ScscError::BinarySii)
        );
    }

    #[test]
    fn rejects_unknown_magic_and_truncation() {
        assert_eq!(
            sii_text(b"PK\x03\x04"),
            Err(ScscError::UnknownMagic(*b"PK\x03\x04"))
        );
        assert_eq!(
            sii_text(b"Si"),
            Err(ScscError::UnknownMagic([b'S', b'i', 0, 0]))
        );
        assert_eq!(decrypt(b"ScsC\x00\x01"), Err(ScscError::Truncated));
    }

    #[test]
    fn rejects_partial_blocks_and_wrong_sizes() {
        let good = container(TEXT, None);
        assert_eq!(
            decrypt(&good[..good.len() - 1]),
            Err(ScscError::PartialBlock(good.len() - 57))
        );
        let declared = TEXT.len() as u32 + 1;
        assert_eq!(
            decrypt(&container(TEXT, Some(declared))),
            Err(ScscError::SizeMismatch {
                declared,
                actual: TEXT.len()
            })
        );
        assert_eq!(
            decrypt(&container(TEXT, Some(u32::MAX))),
            Err(ScscError::TooLarge(u32::MAX))
        );
    }

    #[test]
    fn rejects_a_body_that_is_not_zlib() {
        let mut bytes = container(TEXT, None);
        // Corrupt the first cipher block: it decrypts to garbage.
        bytes[56] ^= 0xff;
        assert!(matches!(decrypt(&bytes), Err(ScscError::Inflate(_))));
    }
}
