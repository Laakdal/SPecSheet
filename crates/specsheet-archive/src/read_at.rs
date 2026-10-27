//! Positional reads so one open archive can serve concurrent readers without
//! sharing a seek cursor.

use std::fs::File;
use std::io;

pub(crate) fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_exact_at(buf, offset)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut filled = 0;
        while filled < buf.len() {
            let n = file.seek_read(&mut buf[filled..], offset + filled as u64)?;
            if n == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            filled += n;
        }
        Ok(())
    }
}
