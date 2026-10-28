//! Extracts an archive's files to a folder the user chooses (D19).
//!
//! Only the destination is written, and never inside (or around) a
//! protected folder such as the game install, the ETS2 home folder or the
//! repository. Every file is created new; nothing is overwritten. Archive
//! paths are hostile input: each part is mapped to a name Windows can hold,
//! names that collide ignoring case get a `~N` suffix, and every changed
//! name is listed in `_renamed.txt` at the destination root. Entries
//! without a recovered name go to `_unnamed/<hash>.<extension>`, the
//! extension guessed from their content ([`specsheet_formats::sniff`]).
//! Entries that cannot be read are listed in `_unreadable.txt`.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use specsheet_archive::{city_hash_64, EntryV1, HashFsV1Archive, ZipArchive};
use specsheet_formats::sniff;

use crate::mod_names::RecoveredNames;

/// Lists written at the destination root.
const RENAMED: &str = "_renamed.txt";
const UNREADABLE: &str = "_unreadable.txt";
/// Folder for entries without a recovered name.
const UNNAMED: &str = "_unnamed";
/// Longest name part Windows file systems hold.
const MAX_PART: usize = 255;

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("destination {dest} overlaps {protected}, which is never written")]
    Protected { dest: PathBuf, protected: PathBuf },
    #[error("destination {0} is not empty; choose an empty folder or skip existing files")]
    NotEmpty(PathBuf),
    #[error("destination {0} must be an absolute path without `..`")]
    BadDestination(PathBuf),
    #[error("I/O error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// What to extract from.
pub enum Archive<'a> {
    /// A HashFS v1 package with the names recovered for it
    /// ([`crate::mod_names::recover_v1_names`]).
    V1 {
        archive: &'a HashFsV1Archive,
        names: &'a RecoveredNames,
    },
    Zip(&'a ZipArchive),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExtractOptions {
    /// Count and plan only; write nothing.
    pub dry_run: bool,
    /// Accept a non-empty destination and leave files that exist alone.
    pub skip_existing: bool,
}

/// What an extraction did (or, in a dry run, would do).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extracted {
    pub written: usize,
    pub skipped: usize,
    /// Declared bytes of the files written (or to be written).
    pub bytes: u64,
    /// Entries placed under `_unnamed/`.
    pub unnamed: usize,
    /// Archive path → written path, for names that had to change.
    pub renamed: Vec<(String, String)>,
    /// Entries that could not be read.
    pub unreadable: Vec<String>,
}

/// Extracts every file of `archive` into `dest` (see the module docs).
/// `protected` folders may neither contain `dest` nor lie inside it.
pub fn extract(
    archive: &Archive<'_>,
    dest: &Path,
    protected: &[PathBuf],
    options: ExtractOptions,
) -> Result<Extracted, ExtractError> {
    match archive {
        Archive::V1 { archive, names } => {
            let source = V1Source { archive, names };
            run(&source, dest, protected, options)
        }
        Archive::Zip(zip) => run(&ZipSource(zip), dest, protected, options),
    }
}

/// One file of an archive.
struct Item {
    /// Archive path, if known.
    path: Option<String>,
    /// Entry hash (v1) or position (ZIP); names unnamed output.
    key: u64,
    size: u64,
}

trait Source {
    fn items(&self) -> Vec<Item>;
    fn read(&self, item: &Item) -> Option<Vec<u8>>;
}

struct V1Source<'a> {
    archive: &'a HashFsV1Archive,
    names: &'a RecoveredNames,
}

impl V1Source<'_> {
    fn entry(&self, hash: u64) -> Option<&EntryV1> {
        self.archive.entries().find(|e| e.hash == hash)
    }
}

impl Source for V1Source<'_> {
    fn items(&self) -> Vec<Item> {
        self.archive
            .entries()
            .filter(|e| !e.is_directory())
            .map(|e| Item {
                path: self.names.names.get(&e.hash).cloned(),
                key: e.hash,
                size: u64::from(e.size),
            })
            .collect()
    }

    fn read(&self, item: &Item) -> Option<Vec<u8>> {
        match &item.path {
            Some(path) => self.archive.read(path).ok(),
            None => self.archive.read_entry(self.entry(item.key)?).ok(),
        }
    }
}

struct ZipSource<'a>(&'a ZipArchive);

impl Source for ZipSource<'_> {
    fn items(&self) -> Vec<Item> {
        self.0
            .files()
            .enumerate()
            .map(|(i, path)| Item {
                path: Some(path.to_owned()),
                key: i as u64,
                size: self.0.size(path).unwrap_or(0),
            })
            .collect()
    }

    fn read(&self, item: &Item) -> Option<Vec<u8>> {
        self.0.read(item.path.as_deref()?).ok()
    }
}

fn run(
    source: &impl Source,
    dest: &Path,
    protected: &[PathBuf],
    options: ExtractOptions,
) -> Result<Extracted, ExtractError> {
    let root = check_destination(dest, protected)?;
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| ExtractError::Io { path, source }
    };
    if !options.skip_existing && fs::read_dir(&root).is_ok_and(|mut d| d.next().is_some()) {
        return Err(ExtractError::NotEmpty(root));
    }
    let mut items = source.items();
    items.sort_by(|a, b| a.path.cmp(&b.path).then(a.key.cmp(&b.key)));
    let mut layout = Layout::default();
    for reserved in [RENAMED, UNREADABLE] {
        layout.taken.insert(reserved.to_lowercase());
    }
    let mut done = Extracted::default();
    for item in &items {
        let label = item
            .path
            .clone()
            .unwrap_or_else(|| format!("{:016x}", item.key));
        // Unnamed entries need their bytes for the extension; a dry run
        // reads nothing and labels them `.bin`.
        let bytes = if options.dry_run {
            None
        } else {
            match source.read(item) {
                Some(bytes) => Some(bytes),
                None => {
                    done.unreadable.push(label);
                    continue;
                }
            }
        };
        let rel = match &item.path {
            Some(path) => {
                let placed = layout.place(path);
                if placed != path.trim_start_matches('/') {
                    done.renamed.push((path.clone(), placed.clone()));
                }
                placed
            }
            None => {
                done.unnamed += 1;
                let ext = bytes.as_deref().map_or("bin", sniff::extension);
                layout.place(&format!("{UNNAMED}/{:016x}.{ext}", item.key))
            }
        };
        let Some(bytes) = bytes else {
            done.written += 1;
            done.bytes += item.size;
            continue;
        };
        let target = rel.split('/').fold(root.clone(), |p, part| p.join(part));
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io(parent))?;
        }
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => {
                if let Err(e) = file.write_all(&bytes) {
                    drop(file);
                    let _ = fs::remove_file(&target);
                    return Err(io(&target)(e));
                }
                done.written += 1;
                done.bytes += bytes.len() as u64;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && options.skip_existing => {
                done.skipped += 1;
            }
            Err(e) => return Err(io(&target)(e)),
        }
    }
    if !options.dry_run {
        let lists = [
            (
                RENAMED,
                done.renamed
                    .iter()
                    .map(|(a, b)| format!("{a} -> {b}"))
                    .collect::<Vec<_>>(),
            ),
            (UNREADABLE, done.unreadable.clone()),
        ];
        for (name, lines) in lists {
            if lines.is_empty() {
                continue;
            }
            let path = root.join(name);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(io(&path))?;
            file.write_all((lines.join("\n") + "\n").as_bytes())
                .map_err(io(&path))?;
        }
    }
    Ok(done)
}

/// `dest` made absolute and canonical (its existing part resolved), after
/// checking that it does not overlap a protected folder.
fn check_destination(dest: &Path, protected: &[PathBuf]) -> Result<PathBuf, ExtractError> {
    let bad = || ExtractError::BadDestination(dest.to_path_buf());
    if !dest.is_absolute() {
        return Err(bad());
    }
    // Resolve the deepest existing ancestor; the rest does not exist yet,
    // so it cannot be a link.
    let mut existing = dest.to_path_buf();
    let mut rest = Vec::new();
    while fs::metadata(&existing).is_err() {
        let name = existing.file_name().ok_or_else(bad)?.to_owned();
        rest.push(name);
        if !existing.pop() {
            return Err(bad());
        }
    }
    if dest.components().any(|c| c == Component::ParentDir) {
        return Err(bad());
    }
    let mut root = fs::canonicalize(&existing).map_err(|source| ExtractError::Io {
        path: existing.clone(),
        source,
    })?;
    root.extend(rest.iter().rev());
    for folder in protected {
        let Ok(folder) = fs::canonicalize(folder) else {
            continue;
        };
        if within(&root, &folder) || within(&folder, &root) {
            return Err(ExtractError::Protected {
                dest: root,
                protected: folder,
            });
        }
    }
    Ok(root)
}

/// Whether `path` is `base` or lies inside it (ignoring case on Windows).
fn within(path: &Path, base: &Path) -> bool {
    let norm = |p: &Path| -> Vec<String> {
        p.components()
            .map(|c| {
                let s = c.as_os_str().to_string_lossy();
                if cfg!(windows) {
                    s.to_lowercase()
                } else {
                    s.into_owned()
                }
            })
            .collect()
    };
    let (path, base) = (norm(path), norm(base));
    path.len() >= base.len() && path[..base.len()] == base[..]
}

/// Where each archive path goes, relative to the destination.
#[derive(Default)]
struct Layout {
    /// Archive folder (lower-case) → placed folder.
    dirs: HashMap<String, String>,
    /// Placed paths, lower-case.
    taken: HashSet<String>,
}

impl Layout {
    /// The destination-relative path (`/`-separated) for archive `path`.
    fn place(&mut self, path: &str) -> String {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let parts = if parts.is_empty() { vec!["_"] } else { parts };
        let (mut original, mut placed) = (String::new(), String::new());
        for (i, part) in parts.iter().enumerate() {
            let last = i + 1 == parts.len();
            if !original.is_empty() {
                original.push('/');
            }
            original.push_str(part);
            let key = original.to_lowercase();
            if !last {
                if let Some(dir) = self.dirs.get(&key) {
                    placed.clone_from(dir);
                    continue;
                }
            }
            let safe = safe_part(part);
            let mut n = 0;
            placed = loop {
                let name = if n == 0 {
                    safe.clone()
                } else {
                    with_suffix(&safe, n)
                };
                let full = if placed.is_empty() {
                    name
                } else {
                    format!("{placed}/{name}")
                };
                if self.taken.insert(full.to_lowercase()) {
                    break full;
                }
                n += 1;
            };
            if !last {
                self.dirs.insert(key, placed.clone());
            }
        }
        placed
    }
}

/// `part` as a name Windows can hold: `<>:"/\|?*` and control characters
/// become `_`, a trailing dot or space gets `_` appended, a reserved device
/// name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, with any
/// extension) gets `_` prepended, and an overlong part is shortened with a
/// hash of the original.
fn safe_part(part: &str) -> String {
    let mut out: String = part
        .chars()
        .map(|c| {
            if c < ' ' || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if out.ends_with(['.', ' ']) {
        out.push('_');
    }
    let stem = out
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    let numbered = |prefix: &str| {
        stem.strip_prefix(prefix).is_some_and(|n| {
            matches!(
                n,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    };
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered("COM") || numbered("LPT")
    {
        out.insert(0, '_');
    }
    if out.len() > MAX_PART {
        let mut cut = MAX_PART - 17;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out = format!("{}~{:016x}", &out[..cut], city_hash_64(part.as_bytes()));
    }
    out
}

/// `name` with `~n` before its first dot.
fn with_suffix(name: &str, n: usize) -> String {
    match name.split_once('.') {
        Some((stem, ext)) => format!("{stem}~{n}.{ext}"),
        None => format!("{name}~{n}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty folder under the system temp folder, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let unique = format!(
                "specsheet-extract-{tag}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
            );
            let path = std::env::temp_dir().join(city_hash_64(unique.as_bytes()).to_string());
            fs::create_dir_all(&path).unwrap();
            Self(fs::canonicalize(&path).unwrap())
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Files given as (path or none, bytes); `None` bytes cannot be read.
    struct Fake(Vec<(Option<&'static str>, Option<&'static [u8]>)>);

    impl Source for Fake {
        fn items(&self) -> Vec<Item> {
            self.0
                .iter()
                .enumerate()
                .map(|(i, (path, bytes))| Item {
                    path: path.map(str::to_owned),
                    key: 0x1000 + i as u64,
                    size: bytes.map_or(0, |b| b.len() as u64),
                })
                .collect()
        }

        fn read(&self, item: &Item) -> Option<Vec<u8>> {
            self.0[(item.key - 0x1000) as usize].1.map(<[u8]>::to_vec)
        }
    }

    fn files_under(root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path.strip_prefix(root).unwrap();
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn maps_hostile_names_to_safe_unique_ones() {
        let mut layout = Layout::default();
        let placed: Vec<String> = [
            "def/CON.sii",
            "def/com1",
            "a:b/c?.dds",
            "dir./x ",
            "x/../y",
            "Model/a.dds",
            "model/A.dds",
            "f",
            "f/inner",
            "//lead//slash",
            "/?30*3<*:/x.dds",
        ]
        .iter()
        .map(|p| layout.place(p))
        .collect();
        assert_eq!(
            placed,
            [
                "def/_CON.sii",
                "def/_com1",
                "a_b/c_.dds",
                "dir._/x _",
                "x/.._/y",
                "Model/a.dds",
                "Model/A~1.dds",
                "f",
                "f~1/inner",
                "lead/slash",
                "_30_3___/x.dds",
            ]
        );
        let long = "n".repeat(300) + ".dds";
        let safe = safe_part(&long);
        assert!(safe.len() <= MAX_PART && safe.contains('~'), "{safe}");
    }

    #[test]
    fn extracts_named_unnamed_and_lists_renames_and_failures() {
        let dir = TempDir::new("all");
        let dest = dir.0.join("out");
        let source = Fake(vec![
            (Some("def/a.sii"), Some(b"SiiNunit\n{\n}\n")),
            (Some("def/CON.sii"), Some(b"x")),
            (None, Some(b"DDS |\0\0\0")),
            (Some("bad.bin"), None),
        ]);
        let done = run(&source, &dest, &[], ExtractOptions::default()).unwrap();
        assert_eq!(done.written, 3);
        assert_eq!(done.unnamed, 1);
        assert_eq!(
            done.renamed,
            [("def/CON.sii".to_owned(), "def/_CON.sii".to_owned())]
        );
        assert_eq!(done.unreadable, ["bad.bin"]);
        assert_eq!(
            files_under(&dest),
            [
                "_renamed.txt",
                "_unnamed/0000000000001002.dds",
                "_unreadable.txt",
                "def/_CON.sii",
                "def/a.sii"
            ]
        );
        assert_eq!(
            fs::read(dest.join("def/a.sii")).unwrap(),
            b"SiiNunit\n{\n}\n"
        );
        assert_eq!(
            fs::read_to_string(dest.join("_renamed.txt")).unwrap(),
            "def/CON.sii -> def/_CON.sii\n"
        );
    }

    #[test]
    fn never_overwrites_and_skips_only_when_asked() {
        let dir = TempDir::new("skip");
        fs::create_dir_all(dir.0.join("def")).unwrap();
        fs::write(dir.0.join("def/a.sii"), b"mine").unwrap();
        let source = Fake(vec![
            (Some("def/a.sii"), Some(b"theirs")),
            (Some("def/b.sii"), Some(b"new")),
        ]);
        let refused = run(&source, &dir.0, &[], ExtractOptions::default());
        assert!(matches!(refused, Err(ExtractError::NotEmpty(_))));
        let options = ExtractOptions {
            skip_existing: true,
            ..Default::default()
        };
        let done = run(&source, &dir.0, &[], options).unwrap();
        assert_eq!((done.written, done.skipped), (1, 1));
        assert_eq!(fs::read(dir.0.join("def/a.sii")).unwrap(), b"mine");
    }

    #[test]
    fn refuses_destinations_overlapping_protected_folders() {
        let dir = TempDir::new("guard");
        let game = dir.0.join("Game");
        fs::create_dir_all(&game).unwrap();
        let source = Fake(vec![(Some("a.sii"), Some(b"x"))]);
        let protected = [game.clone()];
        for dest in [
            game.join("out"),
            dir.0.clone(),
            game.join("..").join("Game").join("x"),
        ] {
            let result = run(&source, &dest, &protected, ExtractOptions::default());
            assert!(
                matches!(
                    result,
                    Err(ExtractError::Protected { .. } | ExtractError::BadDestination(_))
                ),
                "{dest:?}"
            );
        }
        // Case does not hide the game folder on Windows.
        if cfg!(windows) {
            let upper = PathBuf::from(game.to_string_lossy().to_uppercase()).join("out");
            let result = run(&source, &upper, &protected, ExtractOptions::default());
            assert!(matches!(result, Err(ExtractError::Protected { .. })));
        }
        assert!(matches!(
            run(
                &source,
                Path::new("relative/out"),
                &[],
                ExtractOptions::default()
            ),
            Err(ExtractError::BadDestination(_))
        ));
        assert!(fs::read_dir(&game).unwrap().next().is_none());
    }

    #[test]
    fn a_dry_run_writes_nothing() {
        let dir = TempDir::new("dry");
        let dest = dir.0.join("out");
        let source = Fake(vec![(Some("a.sii"), Some(b"abc")), (None, Some(b"zz"))]);
        let options = ExtractOptions {
            dry_run: true,
            ..Default::default()
        };
        let done = run(&source, &dest, &[], options).unwrap();
        assert_eq!((done.written, done.unnamed, done.bytes), (2, 1, 5));
        assert!(!dest.exists());
    }
}
