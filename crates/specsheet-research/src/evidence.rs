//! What a research tool prints at the top of its output, and its tables as
//! CSV, so that `research/evidence/` files share one header and a table can
//! be read by other tools. The command and the game version are not part of
//! the header: `research/recipes.txt` holds the command, and the baseline run
//! (`cargo xtask recipes --save`) records the version.

/// The header of an evidence file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header<'a> {
    /// One line saying what the file shows.
    pub title: &'a str,
    /// The SHA-256 of `eurotrucks2.exe`, when the executable was read.
    pub exe_sha256: Option<&'a str>,
    /// Whether the file belongs to a clean-room pass (D13).
    pub clean_room: bool,
}

impl Header<'_> {
    /// `# key: value` lines in a fixed order, then an empty line.
    pub fn render(&self) -> String {
        let mut out = format!("# title: {}\n", self.title);
        if let Some(sha) = self.exe_sha256 {
            out.push_str(&format!("# exe-sha256: {sha}\n"));
        }
        let clean_room = if self.clean_room { "yes" } else { "no" };
        out.push_str(&format!("# clean-room: {clean_room}\n\n"));
        out
    }
}

/// One CSV record with its line ending (RFC 4180): a field holding a comma,
/// a quote or a line break is quoted, and its quotes are doubled.
pub fn csv_line(fields: &[&str]) -> String {
    let mut out = String::new();
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        if field.contains([',', '"', '\n', '\r']) {
            out.push('"');
            out.push_str(&field.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(field);
        }
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_the_header_lines_in_order() {
        let header = Header {
            title: "Pin survey",
            exe_sha256: None,
            clean_room: false,
        };
        assert_eq!(header.render(), "# title: Pin survey\n# clean-room: no\n\n");
        let header = Header {
            exe_sha256: Some("b7df"),
            clean_room: true,
            ..header
        };
        assert_eq!(
            header.render(),
            "# title: Pin survey\n# exe-sha256: b7df\n# clean-room: yes\n\n"
        );
    }

    #[test]
    fn csv_quotes_commas_quotes_and_newlines() {
        assert_eq!(
            csv_line(&["a", "b,c", "d\"e", "f\ng"]),
            "a,\"b,c\",\"d\"\"e\",\"f\ng\"\n"
        );
        assert_eq!(csv_line(&[]), "\n");
        assert_eq!(csv_line(&["", "x"]), ",x\n");
    }
}
