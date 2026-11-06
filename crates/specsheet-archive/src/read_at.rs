//! Positional reads so one open archive can serve concurrent readers without
//! sharing a seek cursor, and [`EntryReader`], which streams one entry.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use flate2::read::{DeflateDecoder, ZlibDecoder};

use crate::error::ArchiveError;

pub(crate) fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = read_at(file, &mut buf[filled..], offset + filled as u64)?;
        if n == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        filled += n;
    }
    Ok(())
}

/// One positional read; may return fewer bytes than `buf` holds.
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_at(buf, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        file.seek_read(buf, offset)
    }
}

/// How an entry's stored bytes are encoded.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Codec {
    Stored,
    Zlib,
    Deflate,
}

/// Where an entry's bytes are and what they decode to.
pub(crate) struct Stream<'a> {
    pub file: &'a File,
    pub file_len: u64,
    /// The archive file, for errors.
    pub archive: &'a Path,
    /// Names the entry in errors.
    pub label: String,
    pub offset: u64,
    pub stored_size: u64,
    pub size: u64,
    pub codec: Codec,
}

/// A failure of the archive file itself, kept apart from decoding failures
/// as it passes through a decoder.
#[derive(Debug)]
struct FileFault(io::Error);

impl fmt::Display for FileFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for FileFault {}

/// The bytes `offset..end` of a file, read positionally.
struct Range<'a> {
    file: &'a File,
    pos: u64,
    end: u64,
}

impl Read for Range<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let left = self.end - self.pos;
        if left == 0 || buf.is_empty() {
            return Ok(0);
        }
        let want = usize::try_from(left).map_or(buf.len(), |left| left.min(buf.len()));
        let fault = |e: io::Error| io::Error::new(e.kind(), FileFault(e));
        let n = read_at(self.file, &mut buf[..want], self.pos).map_err(fault)?;
        if n == 0 {
            // The file shrank after it was opened.
            return Err(fault(io::ErrorKind::UnexpectedEof.into()));
        }
        self.pos += n as u64;
        Ok(n)
    }
}

enum Inner<'a> {
    Stored(Range<'a>),
    Zlib(ZlibDecoder<Range<'a>>),
    Deflate(DeflateDecoder<Range<'a>>),
}

/// Streams the content of one archive entry without holding it in memory
/// (from [`crate::ZipArchive::open_file`] or
/// [`crate::HashFsV1Archive::open_entry`]).
///
/// It yields exactly [`size`](Self::size) bytes: a stream that decodes to
/// more or fewer is an error of kind [`io::ErrorKind::InvalidData`]. Every
/// error carries an [`ArchiveError`] (`Corrupt` for bad data, `Io` for a
/// failed read of the archive file), reachable through
/// [`io::Error::into_inner`].
pub struct EntryReader<'a> {
    inner: Inner<'a>,
    archive: PathBuf,
    label: String,
    size: u64,
    produced: u64,
}

impl fmt::Debug for EntryReader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntryReader")
            .field("archive", &self.archive)
            .field("entry", &self.label)
            .field("size", &self.size)
            .field("produced", &self.produced)
            .finish_non_exhaustive()
    }
}

impl<'a> EntryReader<'a> {
    /// Checks that the stored bytes lie in the file and, for stored
    /// entries, that both sizes agree.
    pub(crate) fn new(s: Stream<'a>) -> Result<Self, ArchiveError> {
        let corrupt = |reason: String| ArchiveError::Corrupt {
            path: s.archive.to_path_buf(),
            reason,
        };
        let end = s
            .offset
            .checked_add(s.stored_size)
            .filter(|&end| end <= s.file_len)
            .ok_or_else(|| {
                corrupt(format!(
                    "{}: {} bytes at {:#x} lie outside the {}-byte file",
                    s.label, s.stored_size, s.offset, s.file_len
                ))
            })?;
        let range = Range {
            file: s.file,
            pos: s.offset,
            end,
        };
        let inner = match s.codec {
            Codec::Stored if s.stored_size != s.size => {
                return Err(corrupt(format!("{}: uncompressed with two sizes", s.label)))
            }
            Codec::Stored => Inner::Stored(range),
            Codec::Zlib => Inner::Zlib(ZlibDecoder::new(range)),
            Codec::Deflate => Inner::Deflate(DeflateDecoder::new(range)),
        };
        Ok(Self {
            inner,
            archive: s.archive.to_path_buf(),
            label: s.label,
            size: s.size,
            produced: 0,
        })
    }

    /// Bytes the entry decodes to, as the archive declares them.
    pub fn size(&self) -> u64 {
        self.size
    }

    fn corrupt(&self, reason: String) -> io::Error {
        let error = ArchiveError::Corrupt {
            path: self.archive.clone(),
            reason: format!("{}: {reason}", self.label),
        };
        io::Error::new(io::ErrorKind::InvalidData, error)
    }

    /// `e` from the inner reader as an error carrying an [`ArchiveError`].
    fn wrap(&self, e: io::Error) -> io::Error {
        if e.get_ref().is_some_and(|inner| inner.is::<FileFault>()) {
            let kind = e.kind();
            let source = match e.into_inner().map(|inner| inner.downcast::<FileFault>()) {
                Some(Ok(fault)) => fault.0,
                _ => kind.into(),
            };
            let error = ArchiveError::Io {
                path: self.archive.clone(),
                source,
            };
            return io::Error::new(kind, error);
        }
        let codec = match self.inner {
            Inner::Stored(_) => "read",
            Inner::Zlib(_) => "zlib",
            Inner::Deflate(_) => "deflate",
        };
        self.corrupt(format!("{codec}: {e}"))
    }
}

impl Read for EntryReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        // One byte past the declared size is enough to see an overrun.
        let room = (self.size - self.produced).saturating_add(1);
        let cap = usize::try_from(room).map_or(buf.len(), |room| room.min(buf.len()));
        let buf = &mut buf[..cap];
        let result = match &mut self.inner {
            Inner::Stored(r) => r.read(buf),
            Inner::Zlib(r) => r.read(buf),
            Inner::Deflate(r) => r.read(buf),
        };
        let n = match result {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => return Err(e),
            Err(e) => return Err(self.wrap(e)),
        };
        self.produced += n as u64;
        if self.produced > self.size {
            return Err(self.corrupt(format!(
                "decodes to more than the declared {} bytes",
                self.size
            )));
        }
        if n == 0 && self.produced < self.size {
            return Err(self.corrupt(format!(
                "decoded to {} bytes, expected {}",
                self.produced, self.size
            )));
        }
        Ok(n)
    }
}

/// The [`ArchiveError`] an [`EntryReader`] error carries.
pub(crate) fn archive_error(e: io::Error, archive: &Path) -> ArchiveError {
    let kind = e.kind();
    match e.into_inner().map(|inner| inner.downcast::<ArchiveError>()) {
        Some(Ok(error)) => *error,
        Some(Err(other)) => ArchiveError::Io {
            path: archive.to_path_buf(),
            source: io::Error::new(kind, other),
        },
        None => ArchiveError::Io {
            path: archive.to_path_buf(),
            source: kind.into(),
        },
    }
}

/// Reads all of `reader` into memory, with errors as [`ArchiveError`].
pub(crate) fn read_whole(mut reader: EntryReader<'_>) -> Result<Vec<u8>, ArchiveError> {
    let mut out = Vec::new();
    let archive = reader.archive.clone();
    reader
        .read_to_end(&mut out)
        .map_err(|e| archive_error(e, &archive))?;
    Ok(out)
}
