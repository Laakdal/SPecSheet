//! SCS "tokens": short lowercase identifiers packed into a `u64` as base-38
//! digits (`\0`, `0-9`, `a-z`, `_`), least significant digit first.

use std::fmt;

const ALPHABET: &[u8; 38] = b"\x000123456789abcdefghijklmnopqrstuvwxyz_";
const MAX_LEN: usize = 12;

/// A game token: up to 12 characters of `0-9`, `a-z` and `_`, packed
/// base 38 into a `u64`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Token(pub u64);

impl Token {
    /// Encodes `text`; `None` if it is too long or uses other characters.
    pub fn encode(text: &str) -> Option<Self> {
        if text.len() > MAX_LEN {
            return None;
        }
        let mut value = 0u64;
        for byte in text.bytes().rev() {
            let digit = ALPHABET.iter().position(|&c| c == byte && c != 0)? as u64;
            value = value * 38 + digit;
        }
        Some(Self(value))
    }

    /// Decodes the packed text. Invalid encodings decode to lossy text.
    pub fn decode(self) -> String {
        let mut out = String::new();
        let mut value = self.0;
        while value != 0 {
            let digit = (value % 38) as usize;
            value /= 38;
            if digit != 0 {
                out.push(ALPHABET[digit] as char);
            }
        }
        out
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Token({:?})", self.decode())
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.decode())
    }
}

#[cfg(test)]
mod tests {
    use super::Token;

    #[test]
    fn decodes_tokens_observed_in_cab_s_low_pmd() {
        // Look name and part-attribute name from
        // /vehicle/truck/scania_2016/cabin/cab_s_low.pmd (see docs/formats/models.md#tokens).
        assert_eq!(Token(0x15_73c3_e700).decode(), "default");
        assert_eq!(Token(0x0a_ed71_b9ce).decode(), "visible");
    }

    #[test]
    fn round_trips() {
        for text in ["", "a", "s_mirror", "f_light_chs", "abcdefghij_9"] {
            assert_eq!(
                Token::encode(text).map(Token::decode).as_deref(),
                Some(text)
            );
        }
        assert_eq!(Token::encode("UPPER"), None);
        assert_eq!(Token::encode("thirteen_char"), None);
    }
}
