//! The likely file extension of an entry from its first bytes, for files
//! whose path is unknown (unnamed HashFS v1 entries). Signatures are the
//! ones counted on the test install's mod packages
//! (`research/evidence/mod-name-recovery.txt`, section 9); a guess only
//! labels a file, it never names one.

/// Extension (without the dot) that the content of `bytes` suggests:
/// `"bin"` for unrecognised binary data and `"txt"` for other text.
pub fn extension(bytes: &[u8]) -> &'static str {
    let text = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let trimmed = text.trim_ascii_start();
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"DDS ") {
        "dds"
    } else if starts(&[0x01, 0x0a, 0xb1, 0x70]) {
        "tobj"
    } else if starts(b"\x15gmP") {
        "pmg"
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"FEV ") {
        "bank"
    } else if starts(b"\xff\xd8\xff") {
        "jpg"
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if starts(b"ScsC") || starts(b"BSII") || starts(b"3nK") || text.starts_with(b"SiiNunit")
    {
        "sii"
    } else if trimmed.starts_with(b"material") || trimmed.starts_with(b"effect") {
        "mat"
    } else if text.starts_with(b"source=") {
        "soundref"
    } else if text.starts_with(b"[{000214A0-") || text.starts_with(b"[InternetShortcut]") {
        "url"
    } else if !bytes.is_empty()
        && bytes
            .iter()
            .all(|&b| b.is_ascii_graphic() || b.is_ascii_whitespace() || b >= 0x80)
    {
        "txt"
    } else {
        "bin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_counted_signatures() {
        assert_eq!(extension(b"DDS |\0\0\0"), "dds");
        assert_eq!(extension(&[0x01, 0x0a, 0xb1, 0x70, 0, 0]), "tobj");
        assert_eq!(extension(b"\x15gmP\x04\0\0\0"), "pmg");
        assert_eq!(extension(b"RIFF\x10\0\0\0FEV FMT "), "bank");
        assert_eq!(extension(b"RIFF\x10\0\0\0WAVEfmt "), "bin");
        assert_eq!(extension(b"\xff\xd8\xff\xe0\0\x10JFIF"), "jpg");
        assert_eq!(extension(b"\x89PNG\r\n\x1a\n"), "png");
        assert_eq!(extension(b"\xef\xbb\xbfSiiNunit\n{\n}"), "sii");
        assert_eq!(extension(b"ScsC\0\0"), "sii");
        assert_eq!(extension(b"3nK\x01\0"), "sii");
        assert_eq!(extension(b"\nmaterial : \"ui\" {\n}"), "mat");
        assert_eq!(extension(b"effect : \"eut2.dif\" {\n}"), "mat");
        assert_eq!(
            extension(b"\xef\xbb\xbfsource=\"/sound/a.bank#e\""),
            "soundref"
        );
        assert_eq!(
            extension(b"[{000214A0-0000-0000-C000-000000000046}]\r\n"),
            "url"
        );
        assert_eq!(extension(b"NSKYBOX readme\r\n"), "txt");
        assert_eq!(extension(b"\0\x01\x02"), "bin");
        assert_eq!(extension(b""), "bin");
    }
}
