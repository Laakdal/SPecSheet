//! Bounds-checked little-endian reader used by the binary parsers.

use crate::token::Token;

/// A malformed binary file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{what}: {message}")]
pub struct FormatError {
    pub what: &'static str,
    pub message: String,
}

impl FormatError {
    pub fn new(what: &'static str, message: impl Into<String>) -> Self {
        Self {
            what,
            message: message.into(),
        }
    }
}

pub(crate) struct Reader<'a> {
    what: &'static str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(what: &'static str, bytes: &'a [u8]) -> Self {
        Self {
            what,
            bytes,
            pos: 0,
        }
    }

    pub fn error(&self, message: impl Into<String>) -> FormatError {
        FormatError::new(self.what, message)
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn seek(&mut self, pos: usize) -> Result<(), FormatError> {
        if pos > self.bytes.len() {
            return Err(self.error(format!(
                "offset {pos:#x} beyond end ({:#x})",
                self.bytes.len()
            )));
        }
        self.pos = pos;
        Ok(())
    }

    pub fn bytes(&mut self, len: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| {
                self.error(format!(
                    "read of {len} bytes at {:#x} out of bounds",
                    self.pos
                ))
            })?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], FormatError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.bytes(N)?);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16(&mut self) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32, FormatError> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> Result<u64, FormatError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32, FormatError> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    pub fn vec3(&mut self) -> Result<[f32; 3], FormatError> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    pub fn vec4(&mut self) -> Result<[f32; 4], FormatError> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }

    pub fn token(&mut self) -> Result<Token, FormatError> {
        Ok(Token(self.u64()?))
    }

    /// A `u32` count, rejected if it could not possibly fit in the file.
    pub fn count(&mut self, min_item_size: usize) -> Result<usize, FormatError> {
        let count = self.u32()? as usize;
        if count.saturating_mul(min_item_size.max(1)) > self.bytes.len() {
            return Err(self.error(format!("implausible count {count}")));
        }
        Ok(count)
    }

    /// A NUL-terminated string starting at absolute `offset`.
    pub fn c_string_at(&self, offset: usize) -> Result<String, FormatError> {
        let tail = self
            .bytes
            .get(offset..)
            .ok_or_else(|| self.error(format!("string offset {offset:#x} out of bounds")))?;
        let len = tail
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| self.error(format!("unterminated string at {offset:#x}")))?;
        Ok(String::from_utf8_lossy(&tail[..len]).into_owned())
    }
}
