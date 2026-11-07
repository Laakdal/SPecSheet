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
//!
//! Game (HashFS v2) archives give their names through their listings
//! ([`v2_names`]). Their packed textures (a `.tobj` whose image lives in
//! the archive's texture records) are listed in `_packed_textures.txt`,
//! and their size does not count against the limit. With
//! `Archive::V2 { textures: true, .. }` a named one is instead written as a
//! mod package holds it ([`loose_files`]): a standalone `.tobj` and a DX10
//! `.dds` beside it; one that is unnamed, or whose `.dds` name the archive
//! already uses, is still listed.
//!
//! Named entries are streamed to disk in 64 KiB pieces, so an entry of any
//! size passes without being held in memory; one that fails halfway leaves
//! no file, folder or reserved name behind. Unnamed entries are read whole
//! (at most 2 GiB), since their extension is guessed from all their bytes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use specsheet_archive::{
    city_hash_64, hash_path, Archive as V2Archive, Content, DirListing, EntryKind, HashFsV1Archive,
    ZipArchive,
};
use specsheet_archive::{AddressMode, PackedTexture};
use specsheet_formats::{dds, sniff, tobj};

use crate::mod_names::RecoveredNames;

/// Lists written at the destination root.
const RENAMED: &str = "_renamed.txt";
const UNREADABLE: &str = "_unreadable.txt";
const PACKED: &str = "_packed_textures.txt";
/// Folder for entries without a recovered name.
const UNNAMED: &str = "_unnamed";
/// Longest name part Windows file systems hold.
const MAX_PART: usize = 255;
/// Bytes copied per read while streaming an entry to disk.
const CHUNK: usize = 64 << 10;
/// Default limit on the bytes one extraction writes, against archives whose
/// entries share data to declare far more than the file holds. The largest
/// package on the test install unpacks to about 8.7 GB.
pub const DEFAULT_MAX_BYTES: u64 = 64 << 30;

/// Why an extraction was refused or stopped.
#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// The destination is inside, or contains, a folder never written to.
    #[error("destination {dest} overlaps {protected}, which is never written")]
    Protected {
        /// The requested destination.
        dest: PathBuf,
        /// The protected folder (the game install or the mod folder).
        protected: PathBuf,
    },
    /// The destination has files and `skip_existing` is off.
    #[error("destination {0} is not empty; choose an empty folder or skip existing files")]
    NotEmpty(PathBuf),
    #[error("the archive would write {total} bytes, over the limit of {limit}")]
    /// The archive declares more bytes than the limit.
    TooLarge {
        /// Bytes it would write.
        total: u64,
        /// The limit.
        limit: u64,
    },
    /// The destination is relative or contains `..`.
    #[error("destination {0} must be an absolute path without `..`")]
    BadDestination(PathBuf),
    /// The operating system failed to write a file.
    #[error("I/O error on {path}: {source}")]
    Io {
        /// The file involved.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// What to extract from.
pub enum Archive<'a> {
    /// A HashFS v1 package with the names recovered for it
    /// ([`crate::mod_names::recover_v1_names`]).
    V1 {
        /// The package.
        archive: &'a HashFsV1Archive,
        /// Its recovered file names.
        names: &'a RecoveredNames,
    },
    /// A ZIP package; its file names are stored.
    Zip(&'a ZipArchive),
    /// A game archive with the names found for it ([`v2_names`]).
    V2 {
        /// The archive.
        archive: &'a V2Archive,
        /// Its file names.
        names: &'a V2Names,
        /// Write named packed textures as a `.tobj` and a `.dds`
        /// ([`loose_files`]) instead of only listing them.
        textures: bool,
    },
}

/// How to extract.
#[derive(Clone, Copy, Debug)]
pub struct ExtractOptions {
    /// Count and plan only; write nothing.
    pub dry_run: bool,
    /// Accept a non-empty destination and leave files that exist alone.
    pub skip_existing: bool,
    /// Refuse before writing when the declared sizes add up to more, and
    /// stop if the bytes read do.
    pub max_bytes: u64,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            skip_existing: false,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

/// What an extraction did (or, in a dry run, would do).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Extracted {
    /// Files written (or to be written).
    pub written: usize,
    /// Existing files left alone.
    pub skipped: usize,
    /// Declared bytes of the files written (or to be written).
    pub bytes: u64,
    /// Entries placed under `_unnamed/`.
    pub unnamed: usize,
    /// Archive path → written path, for names that had to change.
    pub renamed: Vec<(String, String)>,
    /// Entries that could not be read.
    pub unreadable: Vec<String>,
    /// Packed textures left out (paths, or hashes when unnamed).
    pub packed_textures: Vec<String>,
}

/// A packed texture as a mod package would hold it: a standalone `.tobj`
/// naming `/<stem>.dds`, and that `.dds` with a DX10 header naming the
/// packed DXGI format exactly, every face and mip tightly packed. The
/// `.tobj` takes the texture's type, mipmaps, address modes and colour
/// space (`research/evidence/tobj-loose.txt`, Hypothesis, strong); its
/// Unknown bytes take their most common value, and address w is clamp to
/// edge on a cube, repeat otherwise (what mods hold; ours). `None` when
/// `path` is not a `.tobj`, the texture does not unpack or a path is too
/// long.
pub fn loose_files(path: &str, packed: &PackedTexture) -> Option<(Vec<u8>, Vec<u8>)> {
    let stem = path.strip_suffix(".tobj")?;
    let image = &packed.image;
    let surfaces = packed.unpack().ok()?;
    let data: Vec<u8> = surfaces.into_iter().flat_map(|s| s.data).collect();
    let cube = image.is_cube && image.face_count == 6;
    let header = tobj::Tobj {
        cube,
        address: [
            address_code(packed.sample.address_u),
            address_code(packed.sample.address_v),
            if cube { 2 } else { 0 },
        ],
        mipmaps: image.mip_count > 1,
        linear: !dds::is_srgb(image.dxgi_format),
        paths: vec![format!("/{}.dds", stem.trim_start_matches('/'))],
    };
    let image_file = dds::write_dx10(
        image.width,
        image.height,
        image.mip_count,
        image.face_count,
        image.dxgi_format,
        &data,
    );
    Some((tobj::write(&header)?, image_file))
}

/// The 3-bit address code of a mode (docs/formats/textures.md, "Sample
/// record"), the inverse of [`AddressMode::from_bits`].
fn address_code(mode: AddressMode) -> u8 {
    match mode {
        AddressMode::Repeat => 0,
        AddressMode::Clamp => 1,
        AddressMode::ClampToEdge => 2,
        AddressMode::ClampToBorder => 3,
        AddressMode::Mirror => 4,
        AddressMode::MirrorClamp => 5,
        AddressMode::MirrorClampToEdge => 6,
    }
}

/// Which entries to take, by archive path. In a pattern, `*` matches any
/// run of characters, `/` included (`def/*`, `*.dds`); case and a leading
/// `/` are ignored. Unnamed entries match by their label `_unnamed/<hash>`
/// (16 hex digits), as their extension is known only once they are read.
/// A filter without patterns takes every entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathFilter {
    patterns: Vec<String>,
}

impl PathFilter {
    /// A filter taking the entries that match any of `patterns`.
    pub fn new<I>(patterns: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        Self {
            patterns: patterns
                .into_iter()
                .map(|p| normalize(p.as_ref()))
                .collect(),
        }
    }

    /// Whether the entry at archive path `path` is taken.
    pub fn matches(&self, path: &str) -> bool {
        let path = normalize(path);
        self.patterns.is_empty() || self.patterns.iter().any(|p| glob(p, &path))
    }
}

/// A path as [`PathFilter`] compares it.
fn normalize(path: &str) -> String {
    path.trim_start_matches('/').to_lowercase()
}

/// Whether `text` matches `pattern`, in which `*` matches any run of
/// characters.
fn glob(pattern: &str, text: &str) -> bool {
    let mut pieces = pattern.split('*');
    let head = pieces.next().unwrap_or("");
    let Some(mut rest) = text.strip_prefix(head) else {
        return false;
    };
    let pieces: Vec<&str> = pieces.collect();
    let Some((tail, middle)) = pieces.split_last() else {
        return rest.is_empty();
    };
    // Each piece taken at its first fit leaves the most room for the rest.
    for piece in middle {
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    rest.ends_with(tail)
}

/// What [`list`] finds: the files an extraction would write, and the
/// entries it would leave out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    /// Files, in extraction order.
    pub files: Vec<Listed>,
    /// Packed textures (paths, or hashes when unnamed).
    pub packed_textures: Vec<String>,
    /// Entries the archive could not index (hashes).
    pub unreadable: Vec<String>,
}

/// One file as an extraction into an empty folder would write it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// Destination-relative path (`/`-separated). Unnamed entries are
    /// listed as `_unnamed/<hash>.bin`; extracting picks the extension
    /// from their content.
    pub path: String,
    /// The archive path, `None` for an unnamed entry.
    pub source: Option<String>,
    /// Declared size in bytes.
    pub size: u64,
}

/// Extracts the files of `archive` that `filter` takes into `dest` (see
/// the module docs). `protected` folders may neither contain `dest` nor lie
/// inside it. The size limit applies to the files taken.
pub fn extract(
    archive: &Archive<'_>,
    dest: &Path,
    protected: &[PathBuf],
    options: ExtractOptions,
    filter: &PathFilter,
) -> Result<Extracted, ExtractError> {
    match archive {
        Archive::V1 { archive, names } => {
            let source = V1Source { archive, names };
            run(&Filtered::new(&source, filter), dest, protected, options)
        }
        Archive::Zip(zip) => run(
            &Filtered::new(&ZipSource(zip), filter),
            dest,
            protected,
            options,
        ),
        Archive::V2 {
            archive,
            names,
            textures,
        } => {
            let source = V2Source {
                archive,
                names,
                textures: *textures,
            };
            run(&Filtered::new(&source, filter), dest, protected, options)
        }
    }
}

/// The files of `archive` that `filter` takes, in extraction order, named
/// as [`extract`] would name them in an empty folder, and the entries it
/// would leave out. Reads no content.
pub fn list(archive: &Archive<'_>, filter: &PathFilter) -> Listing {
    match archive {
        Archive::V1 { archive, names } => {
            let source = V1Source { archive, names };
            plan(&Filtered::new(&source, filter))
        }
        Archive::Zip(zip) => plan(&Filtered::new(&ZipSource(zip), filter)),
        Archive::V2 {
            archive,
            names,
            textures,
        } => {
            let source = V2Source {
                archive,
                names,
                textures: *textures,
            };
            plan(&Filtered::new(&source, filter))
        }
    }
}

/// File names for a game archive's entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct V2Names {
    /// Entry hash → path, without a leading slash.
    pub names: BTreeMap<u64, String>,
    /// Names found by walking the archive's listings.
    pub from_listings: usize,
    /// Names found among the candidates (entries no listing names).
    pub from_candidates: usize,
    /// Hashes of file entries left without a name, sorted.
    pub unnamed: Vec<u64>,
}

/// Names the file entries of `archive`: first by walking its listings from
/// the root, then by hashing `candidates` (e.g. every path of the mounted
/// game) for the entries no listing names. Listings are not exhaustive,
/// and some archives have no root at all (`docs/formats/hashfs.md`).
pub fn v2_names(archive: &V2Archive, candidates: &[String]) -> V2Names {
    let kinds = archive.entries().map(|e| (e.hash, e.kind)).collect();
    walk_v2_names(&|dir| archive.list_dir(dir).ok(), &kinds, candidates)
}

/// [`v2_names`] over a listing lookup and the kind of every entry. A
/// listed name counts only when an entry of that kind exists; each folder
/// is walked once.
fn walk_v2_names(
    list_dir: &dyn Fn(&str) -> Option<DirListing>,
    kinds: &HashMap<u64, EntryKind>,
    candidates: &[String],
) -> V2Names {
    let mut out = V2Names::default();
    let mut visited = HashSet::new();
    let mut stack = vec![String::new()];
    while let Some(dir) = stack.pop() {
        if !visited.insert(hash_path(&dir)) {
            continue;
        }
        let Some(listing) = list_dir(&dir) else {
            continue;
        };
        let join = |name: &str| {
            if dir.is_empty() {
                name.to_owned()
            } else {
                format!("{dir}/{name}")
            }
        };
        for file in &listing.files {
            let path = join(file);
            let hash = hash_path(&path);
            if kinds.get(&hash) == Some(&EntryKind::File) && !out.names.contains_key(&hash) {
                out.names.insert(hash, path);
                out.from_listings += 1;
            }
        }
        for sub in &listing.directories {
            let path = join(sub);
            if kinds.get(&hash_path(&path)) == Some(&EntryKind::Directory) {
                stack.push(path);
            }
        }
    }
    for candidate in candidates {
        let path = candidate.trim_start_matches('/');
        let hash = hash_path(path);
        if kinds.get(&hash) == Some(&EntryKind::File) && !out.names.contains_key(&hash) {
            out.names.insert(hash, path.to_owned());
            out.from_candidates += 1;
        }
    }
    out.unnamed = kinds
        .iter()
        .filter(|(hash, kind)| **kind == EntryKind::File && !out.names.contains_key(hash))
        .map(|(hash, _)| *hash)
        .collect();
    out.unnamed.sort_unstable();
    out
}

/// What an archive entry is, for extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ItemKind {
    /// A file to write.
    File,
    /// A packed texture, left out.
    PackedTexture,
    /// The standalone `.tobj` of a converted packed texture.
    TextureHeader,
    /// The `.dds` of a converted packed texture.
    TextureImage,
    /// An entry the archive could not index, listed as unreadable.
    Unindexed,
}

/// One file of an archive.
struct Item {
    /// Archive path, if known.
    path: Option<String>,
    /// Entry hash (v1, v2) or position (ZIP); names unnamed output.
    key: u64,
    size: u64,
    kind: ItemKind,
}

impl Item {
    /// Names the entry in the lists: its path, or its hash.
    fn label(&self) -> String {
        self.path
            .clone()
            .unwrap_or_else(|| format!("{:016x}", self.key))
    }

    /// Where the entry goes when its content is not read: its path, or
    /// `_unnamed/<hash>.bin`.
    fn unread_name(&self) -> String {
        self.path
            .clone()
            .unwrap_or_else(|| format!("{UNNAMED}/{:016x}.bin", self.key))
    }
}

trait Source {
    fn items(&self) -> Vec<Item>;
    /// The whole content of an entry, for unnamed entries: their extension
    /// is guessed from all of their bytes.
    fn read(&self, item: &Item) -> Option<Vec<u8>>;
    /// A stream of a named entry's content.
    fn open(&self, item: &Item) -> Option<Box<dyn Read + '_>>;
}

struct V1Source<'a> {
    archive: &'a HashFsV1Archive,
    names: &'a RecoveredNames,
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
                kind: ItemKind::File,
            })
            .collect()
    }

    fn read(&self, item: &Item) -> Option<Vec<u8>> {
        match &item.path {
            Some(path) => self.archive.read(path).ok(),
            None => self
                .archive
                .read_entry(self.archive.entry_by_hash(item.key)?)
                .ok(),
        }
    }

    fn open(&self, item: &Item) -> Option<Box<dyn Read + '_>> {
        let reader = self.archive.open_file(item.path.as_deref()?).ok()?;
        Some(Box::new(reader))
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
                kind: ItemKind::File,
            })
            .collect()
    }

    fn read(&self, item: &Item) -> Option<Vec<u8>> {
        self.0.read(item.path.as_deref()?).ok()
    }

    fn open(&self, item: &Item) -> Option<Box<dyn Read + '_>> {
        let reader = self.0.open_file(item.path.as_deref()?).ok()?;
        Some(Box::new(reader))
    }
}

struct V2Source<'a> {
    archive: &'a V2Archive,
    names: &'a V2Names,
    /// Convert named packed textures ([`loose_files`]).
    textures: bool,
}

impl V2Source<'_> {
    /// The `.dds` path a packed texture at `path` converts to, when it is
    /// a `.tobj` and the archive holds no entry at that path.
    fn converted_image(&self, path: &str) -> Option<String> {
        let image = format!("{}.dds", path.strip_suffix(".tobj")?);
        self.archive
            .entry_by_hash(hash_path(&image))
            .is_none()
            .then_some(image)
    }

    /// The bytes of a converted packed texture's `.tobj` or `.dds`.
    fn converted(&self, item: &Item) -> Option<Vec<u8>> {
        let path = self.names.names.get(&item.key)?;
        let packed = self.archive.read_texture(path).ok()?;
        let (header, image) = loose_files(path, &packed)?;
        Some(if item.kind == ItemKind::TextureHeader {
            header
        } else {
            image
        })
    }
}

impl Source for V2Source<'_> {
    fn items(&self) -> Vec<Item> {
        let files = self
            .archive
            .entries()
            .filter(|e| e.kind == EntryKind::File)
            .flat_map(|e| {
                let path = self.names.names.get(&e.hash).cloned();
                let item = |path, size: u64, kind| Item {
                    path,
                    key: e.hash,
                    size,
                    kind,
                };
                match &e.content {
                    Content::Stored(blob) => {
                        vec![item(path, u64::from(blob.size), ItemKind::File)]
                    }
                    Content::Texture { blob, .. } => {
                        let image = path
                            .as_deref()
                            .filter(|_| self.textures)
                            .and_then(|p| self.converted_image(p));
                        match image {
                            // The header is about 60 bytes, the image at
                            // most the padded surfaces plus 148.
                            Some(image) => vec![
                                item(path, 64, ItemKind::TextureHeader),
                                item(
                                    Some(image),
                                    u64::from(blob.size) + 148,
                                    ItemKind::TextureImage,
                                ),
                            ],
                            None => vec![item(path, u64::from(blob.size), ItemKind::PackedTexture)],
                        }
                    }
                }
            });
        let unindexed = self.archive.skipped_entries().iter().map(|s| Item {
            path: None,
            key: s.hash,
            size: 0,
            kind: ItemKind::Unindexed,
        });
        files.chain(unindexed).collect()
    }

    fn read(&self, item: &Item) -> Option<Vec<u8>> {
        if matches!(item.kind, ItemKind::TextureHeader | ItemKind::TextureImage) {
            return self.converted(item);
        }
        let entry = self.archive.entry_by_hash(item.key)?;
        self.archive.read_entry(entry).ok()
    }

    /// Whole, not streamed: a v2 entry holds at most 256 MiB.
    fn open(&self, item: &Item) -> Option<Box<dyn Read + '_>> {
        Some(Box::new(Cursor::new(self.read(item)?)))
    }
}

/// The entries of a source that a [`PathFilter`] takes.
struct Filtered<'a, S> {
    source: &'a S,
    filter: &'a PathFilter,
}

impl<'a, S> Filtered<'a, S> {
    fn new(source: &'a S, filter: &'a PathFilter) -> Self {
        Self { source, filter }
    }
}

impl<S: Source> Source for Filtered<'_, S> {
    fn items(&self) -> Vec<Item> {
        let mut items = self.source.items();
        items.retain(|item| match &item.path {
            Some(path) => self.filter.matches(path),
            None => self.filter.matches(&format!("{UNNAMED}/{:016x}", item.key)),
        });
        items
    }

    fn read(&self, item: &Item) -> Option<Vec<u8>> {
        self.source.read(item)
    }

    fn open(&self, item: &Item) -> Option<Box<dyn Read + '_>> {
        self.source.open(item)
    }
}

/// The items of `source` in extraction order: unnamed first, then by path.
fn sorted_items(source: &impl Source) -> Vec<Item> {
    let mut items = source.items();
    items.sort_by(|a, b| a.path.cmp(&b.path).then(a.key.cmp(&b.key)));
    items
}

/// The items of `source` in extraction order, split into files to write
/// and the labels of packed textures and unindexed entries.
fn partition(source: &impl Source) -> (Vec<Item>, Vec<String>, Vec<String>) {
    let (mut files, mut packed, mut unindexed) = (Vec::new(), Vec::new(), Vec::new());
    for item in sorted_items(source) {
        match item.kind {
            ItemKind::File | ItemKind::TextureHeader | ItemKind::TextureImage => files.push(item),
            ItemKind::PackedTexture => packed.push(item.label()),
            ItemKind::Unindexed => unindexed.push(item.label()),
        }
    }
    (files, packed, unindexed)
}

fn plan(source: &impl Source) -> Listing {
    let (items, packed_textures, unreadable) = partition(source);
    let mut layout = Layout::for_destination();
    let files = items
        .into_iter()
        .map(|item| Listed {
            path: layout.place(&item.unread_name(), &mut |_| false).rel,
            source: item.path,
            size: item.size,
        })
        .collect();
    Listing {
        files,
        packed_textures,
        unreadable,
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
    let (items, packed_textures, unindexed) = partition(source);
    let declared = items
        .iter()
        .fold(0u64, |sum, item| sum.saturating_add(item.size));
    if declared > options.max_bytes {
        return Err(ExtractError::TooLarge {
            total: declared,
            limit: options.max_bytes,
        });
    }
    let mut layout = Layout::for_destination();
    let mut done = Extracted {
        unreadable: unindexed,
        packed_textures,
        ..Extracted::default()
    };
    let mut buf = vec![0u8; CHUNK];
    for item in &items {
        let label = item
            .path
            .clone()
            .unwrap_or_else(|| format!("{:016x}", item.key));
        // Named entries are streamed. Unnamed ones are read whole, since
        // their extension is guessed from all of their bytes; a dry run
        // reads nothing and labels them `.bin`.
        let (wanted, mut reader): (String, Option<Box<dyn Read + '_>>) = match &item.path {
            _ if options.dry_run => (item.unread_name(), None),
            Some(path) => match source.open(item) {
                Some(reader) => (path.clone(), Some(reader)),
                None => {
                    done.unreadable.push(label);
                    continue;
                }
            },
            None => match source.read(item) {
                Some(bytes) => {
                    let ext = sniff::extension(&bytes);
                    let wanted = format!("{UNNAMED}/{:016x}.{ext}", item.key);
                    (wanted, Some(Box::new(Cursor::new(bytes))))
                }
                None => {
                    done.unreadable.push(label);
                    continue;
                }
            },
        };
        let placed = layout.place(&wanted, &mut |rel| alias_clash(&root, rel));
        let rename = item
            .path
            .as_ref()
            .filter(|path| placed.rel != path.trim_start_matches('/'))
            .map(|path| (path.clone(), placed.rel.clone()));
        let Some(reader) = reader.as_mut() else {
            done.written += 1;
            done.bytes += item.size;
            done.unnamed += usize::from(item.path.is_none());
            done.renamed.extend(rename);
            continue;
        };
        let target = placed
            .rel
            .split('/')
            .fold(root.clone(), |p, part| p.join(part));
        let new_dirs = create_parents(&root, &target).map_err(io(&target))?;
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && options.skip_existing => {
                done.skipped += 1;
                done.unnamed += usize::from(item.path.is_none());
                done.renamed.extend(rename);
                continue;
            }
            Err(e) => return Err(io(&target)(e)),
        };
        let copied = copy(reader, &mut file, &mut buf, done.bytes, options.max_bytes);
        if copied.is_err() {
            drop(file);
            let _ = fs::remove_file(&target);
        }
        match copied {
            Ok(n) => {
                done.written += 1;
                done.bytes += n;
                done.unnamed += usize::from(item.path.is_none());
                done.renamed.extend(rename);
            }
            Err(CopyError::Read) => {
                if let Some(first) = new_dirs {
                    remove_empty_dirs(&target, &first);
                }
                layout.forget(&placed);
                done.unreadable.push(label);
            }
            Err(CopyError::Write(e)) => return Err(io(&target)(e)),
            // Declared sizes may understate what an entry unpacks to.
            Err(CopyError::Limit(total)) => {
                return Err(ExtractError::TooLarge {
                    total,
                    limit: options.max_bytes,
                })
            }
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
            (PACKED, done.packed_textures.clone()),
        ];
        for (name, lines) in lists {
            if lines.is_empty() {
                continue;
            }
            // No file may have created it (everything left out).
            fs::create_dir_all(&root).map_err(io(&root))?;
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

/// Where [`Layout::place`] put one path, and what it reserved for it.
struct Placed {
    /// The destination-relative path (`/`-separated).
    rel: String,
    /// Keys added to [`Layout::taken`].
    taken: Vec<String>,
    /// Keys added to [`Layout::dirs`].
    dirs: Vec<String>,
}

impl Layout {
    /// A layout for a destination root, where the lists written there are
    /// reserved.
    fn for_destination() -> Self {
        let mut layout = Self::default();
        for reserved in [RENAMED, UNREADABLE, PACKED] {
            layout.taken.insert(reserved.to_lowercase());
        }
        layout
    }

    /// Places archive `path` (see [`Placed`]). A new folder or file name
    /// for which `clash` holds (see [`alias_clash`]) is passed over like a
    /// taken one.
    fn place(&mut self, path: &str, clash: &mut dyn FnMut(&str) -> bool) -> Placed {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let parts = if parts.is_empty() { vec!["_"] } else { parts };
        let (mut taken, mut dirs) = (Vec::new(), Vec::new());
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
                let lower = full.to_lowercase();
                if !self.taken.contains(&lower) && !clash(&full) {
                    self.taken.insert(lower.clone());
                    taken.push(lower);
                    break full;
                }
                n += 1;
            };
            if !last {
                self.dirs.insert(key.clone(), placed.clone());
                dirs.push(key);
            }
        }
        Placed {
            rel: placed,
            taken,
            dirs,
        }
    }

    /// Releases what [`Self::place`] reserved for a file that was not
    /// written, so later paths are placed as if it never came.
    fn forget(&mut self, placed: &Placed) {
        for key in &placed.taken {
            self.taken.remove(key);
        }
        for key in &placed.dirs {
            self.dirs.remove(key);
        }
    }
}

/// Whether `rel` (`/`-separated, under `root`) reaches an existing entry of
/// another name: Windows matches a name against 8.3 short aliases too, so
/// `______~1.dds` can open `_________.dds`. Writing there would fail (a
/// file) or merge into the wrong folder (a folder); an entry of the same
/// long name is not a clash, so `skip_existing` still skips it.
fn alias_clash(root: &Path, rel: &str) -> bool {
    let path = rel
        .split('/')
        .fold(root.to_path_buf(), |p, part| p.join(part));
    let Some(requested) = path.file_name() else {
        return false;
    };
    if fs::symlink_metadata(&path).is_err() {
        return false;
    }
    // The resolved path carries the entry's long name. One that exists but
    // cannot be resolved is left alone too.
    fs::canonicalize(&path).map_or(true, |real| {
        real.file_name()
            .is_none_or(|found| is_alias_collision(requested, found))
    })
}

/// Whether asking for `requested` found an entry named `found` (its long
/// name) that is another entry, not the same name in another case.
fn is_alias_collision(requested: &OsStr, found: &OsStr) -> bool {
    requested.to_string_lossy().to_lowercase() != found.to_string_lossy().to_lowercase()
}

/// Why streaming one entry to disk stopped.
enum CopyError {
    /// The archive could not be read or decoded.
    Read,
    /// The destination could not be written.
    Write(std::io::Error),
    /// The bytes written so far would pass the limit (the total).
    Limit(u64),
}

/// Copies `reader` to `file` through `buf`, keeping `already + copied`
/// within `limit`. Returns the bytes copied.
fn copy(
    reader: &mut dyn Read,
    file: &mut fs::File,
    buf: &mut [u8],
    already: u64,
    limit: u64,
) -> Result<u64, CopyError> {
    let mut copied = 0u64;
    loop {
        let n = match reader.read(buf) {
            Ok(0) => return Ok(copied),
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(CopyError::Read),
        };
        copied += n as u64;
        let total = already.saturating_add(copied);
        if total > limit {
            return Err(CopyError::Limit(total));
        }
        file.write_all(&buf[..n]).map_err(CopyError::Write)?;
    }
}

/// Creates the folders above `target`, which lies under `root`. Returns the
/// topmost folder it created below `root`, if any.
fn create_parents(root: &Path, target: &Path) -> std::io::Result<Option<PathBuf>> {
    let Some(parent) = target.parent() else {
        return Ok(None);
    };
    let mut first_new = None;
    let mut dir = parent;
    while dir != root && dir.starts_with(root) && fs::metadata(dir).is_err() {
        first_new = Some(dir.to_path_buf());
        let Some(up) = dir.parent() else { break };
        dir = up;
    }
    fs::create_dir_all(parent)?;
    Ok(first_new)
}

/// Removes the empty folders from `target`'s parent up to and including
/// `first_new` (from [`create_parents`]).
fn remove_empty_dirs(target: &Path, first_new: &Path) {
    let mut dir = target.parent();
    while let Some(d) = dir {
        if !d.starts_with(first_new) || fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
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

    /// One file of a [`Fake`] archive.
    struct FakeFile {
        path: Option<&'static str>,
        /// `None`: cannot be opened at all.
        bytes: Option<&'static [u8]>,
        /// The stream fails after yielding its bytes.
        fails_midway: bool,
        /// Declared size, if not the length of `bytes`.
        declared: Option<u64>,
        /// What the entry is.
        kind: ItemKind,
    }

    struct Fake(Vec<FakeFile>);

    /// Files given as (path or none, bytes); `None` bytes cannot be read.
    fn fake(files: Vec<(Option<&'static str>, Option<&'static [u8]>)>) -> Fake {
        Fake(
            files
                .into_iter()
                .map(|(path, bytes)| FakeFile {
                    path,
                    bytes,
                    fails_midway: false,
                    declared: None,
                    kind: ItemKind::File,
                })
                .collect(),
        )
    }

    /// Yields its bytes in small pieces, then an error.
    struct Breaking(&'static [u8]);

    impl Read for Breaking {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0.is_empty() {
                return Err(std::io::Error::other("stream broke"));
            }
            let n = buf.len().min(3).min(self.0.len());
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    impl Source for Fake {
        fn items(&self) -> Vec<Item> {
            self.0
                .iter()
                .enumerate()
                .map(|(i, f)| Item {
                    path: f.path.map(str::to_owned),
                    key: 0x1000 + i as u64,
                    size: f
                        .declared
                        .unwrap_or_else(|| f.bytes.map_or(0, |b| b.len() as u64)),
                    kind: f.kind,
                })
                .collect()
        }

        fn read(&self, item: &Item) -> Option<Vec<u8>> {
            let f = &self.0[(item.key - 0x1000) as usize];
            f.bytes.filter(|_| !f.fails_midway).map(<[u8]>::to_vec)
        }

        fn open(&self, item: &Item) -> Option<Box<dyn Read + '_>> {
            let f = &self.0[(item.key - 0x1000) as usize];
            let bytes = f.bytes?;
            Some(if f.fails_midway {
                Box::new(Breaking(bytes))
            } else {
                Box::new(bytes)
            })
        }
    }

    #[test]
    fn streams_entries_larger_than_one_chunk_intact() {
        let big: &'static [u8] = Box::leak(
            (0..(3 * CHUNK as u32 / 4 + 17))
                .flat_map(|i| i.to_le_bytes())
                .collect::<Vec<u8>>()
                .into_boxed_slice(),
        );
        let dir = TempDir::new("big");
        let dest = dir.0.join("out");
        let done = run(
            &fake(vec![(Some("big.bin"), Some(big))]),
            &dest,
            &[],
            ExtractOptions::default(),
        )
        .unwrap();
        assert_eq!((done.written, done.bytes), (1, big.len() as u64));
        assert_eq!(fs::read(dest.join("big.bin")).unwrap(), big);
    }

    #[test]
    fn a_stream_failing_midway_leaves_no_file_folder_or_name_behind() {
        let dir = TempDir::new("midway");
        let dest = dir.0.join("out");
        let broken = |path| FakeFile {
            path: Some(path),
            bytes: Some(b"0123456789"),
            fails_midway: true,
            declared: None,
            kind: ItemKind::File,
        };
        let source = Fake(vec![
            broken("def/A.sii"),
            FakeFile {
                path: Some("def/a.sii"),
                bytes: Some(b"ok"),
                fails_midway: false,
                declared: None,
                kind: ItemKind::File,
            },
            broken("only/deep/x.bin"),
        ]);
        let done = run(&source, &dest, &[], ExtractOptions::default()).unwrap();
        assert_eq!(done.unreadable, ["def/A.sii", "only/deep/x.bin"]);
        assert_eq!((done.written, done.bytes), (1, 2));
        // The failed `def/A.sii` does not push its case twin to `a~1.sii`.
        assert!(done.renamed.is_empty(), "{:?}", done.renamed);
        assert_eq!(files_under(&dest), ["_unreadable.txt", "def/a.sii"]);
        assert!(!dest.join("only").exists());
    }

    #[test]
    fn stops_when_a_stream_yields_more_than_the_limit() {
        let dir = TempDir::new("overrun");
        let dest = dir.0.join("out");
        // Declares 5 bytes, under the limit, but yields 20.
        let source = Fake(vec![FakeFile {
            path: Some("a.bin"),
            bytes: Some(b"01234567890123456789"),
            fails_midway: false,
            declared: Some(5),
            kind: ItemKind::File,
        }]);
        let options = ExtractOptions {
            max_bytes: 10,
            ..Default::default()
        };
        let result = run(&source, &dest, &[], options);
        assert!(
            matches!(result, Err(ExtractError::TooLarge { limit: 10, .. })),
            "{result:?}"
        );
        assert!(!dest.join("a.bin").exists());
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
        .map(|p| layout.place(p, &mut |_| false).rel)
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
    fn a_short_name_alias_is_a_collision_but_a_case_change_is_not() {
        let alias =
            |requested: &str, found: &str| is_alias_collision(requested.as_ref(), found.as_ref());
        // `_________.dds` holds the 8.3 alias `______~1.DDS`.
        assert!(alias("______~1.dds", "_________.dds"));
        assert!(!alias("______~1.dds", "______~1.dds"));
        assert!(!alias("Model/A.dds", "model/a.DDS"));
    }

    #[test]
    fn names_taken_on_disk_by_an_alias_get_the_next_suffix() {
        let mut layout = Layout::default();
        let on_disk = ["d", "______~1.dds", "______~2.dds"];
        let mut clash = |rel: &str| on_disk.contains(&rel);
        let placed: Vec<String> = ["______.dds", "/______.dds", "d/x.dds", "d/y.dds"]
            .iter()
            .map(|p| layout.place(p, &mut clash).rel)
            .collect();
        assert_eq!(
            placed,
            ["______.dds", "______~3.dds", "d~1/x.dds", "d~1/y.dds"]
        );
    }

    /// `_________.dds` and, when the temp volume makes 8.3 aliases, whether
    /// `______~1.dds` reaches it.
    fn file_with_alias(dir: &Path) -> bool {
        fs::write(dir.join("_________.dds"), b"long").unwrap();
        fs::metadata(dir.join("______~1.dds")).is_ok()
    }

    #[test]
    fn extraction_steps_around_8_3_aliases() {
        let probe = TempDir::new("alias-probe");
        if !file_with_alias(&probe.0) {
            eprintln!("skipped: the temp volume makes no 8.3 aliases");
            return;
        }
        // The MAN TG3 case: a suffixed name equal to an earlier file's alias.
        let dir = TempDir::new("alias");
        let dest = dir.0.join("out");
        let source = fake(vec![
            (Some("_________.dds"), Some(b"first")),
            (Some("______~1.dds"), Some(b"second")),
        ]);
        let done = run(&source, &dest, &[], ExtractOptions::default()).unwrap();
        assert_eq!((done.written, done.skipped), (2, 0));
        assert_eq!(fs::read(dest.join("_________.dds")).unwrap(), b"first");
        assert_eq!(fs::read(dest.join("______~1~1.dds")).unwrap(), b"second");

        // With `skip_existing`, an alias match is not an existing file.
        let options = ExtractOptions {
            skip_existing: true,
            ..Default::default()
        };
        let source = fake(vec![(Some("______~1.dds"), Some(b"new"))]);
        let done = run(&source, &probe.0, &[], options).unwrap();
        assert_eq!((done.written, done.skipped), (1, 0));
        assert_eq!(fs::read(probe.0.join("_________.dds")).unwrap(), b"long");
        assert_eq!(fs::read(probe.0.join("______~1~1.dds")).unwrap(), b"new");
    }

    #[test]
    fn extracts_named_unnamed_and_lists_renames_and_failures() {
        let dir = TempDir::new("all");
        let dest = dir.0.join("out");
        let source = fake(vec![
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
        let source = fake(vec![
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
        let source = fake(vec![(Some("a.sii"), Some(b"x"))]);
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
    fn refuses_an_archive_over_the_size_limit_before_writing() {
        let dir = TempDir::new("limit");
        let dest = dir.0.join("out");
        let source = fake(vec![
            (Some("a.bin"), Some(b"0123456789")),
            (Some("b.bin"), Some(b"0123456789")),
        ]);
        let options = ExtractOptions {
            max_bytes: 15,
            ..Default::default()
        };
        let result = run(&source, &dest, &[], options);
        assert!(matches!(
            result,
            Err(ExtractError::TooLarge {
                total: 20,
                limit: 15
            })
        ));
        assert!(!dest.exists());
    }

    #[test]
    fn a_dry_run_writes_nothing() {
        let dir = TempDir::new("dry");
        let dest = dir.0.join("out");
        let source = fake(vec![(Some("a.sii"), Some(b"abc")), (None, Some(b"zz"))]);
        let options = ExtractOptions {
            dry_run: true,
            ..Default::default()
        };
        let done = run(&source, &dest, &[], options).unwrap();
        assert_eq!((done.written, done.unnamed, done.bytes), (2, 1, 5));
        assert!(!dest.exists());
    }

    #[test]
    fn path_filters_match_globs_ignoring_case_and_a_leading_slash() {
        let filter = PathFilter::new(["def/*", "*.DDS", "/manifest.sii"]);
        for hit in ["def/a.sii", "/def/x/y.sii", "model/a.dds", "manifest.sii"] {
            assert!(filter.matches(hit), "{hit}");
        }
        for miss in ["manifest.sii.bak", "model/a.pmg", "xdef/a.sii", "def"] {
            assert!(!filter.matches(miss), "{miss}");
        }
        assert!(PathFilter::default().matches("anything"));
        assert!(PathFilter::new(["a*b*b"]).matches("abxb"));
        assert!(!PathFilter::new(["a*b*b"]).matches("ab"));
    }

    #[test]
    fn a_filter_extracts_and_sizes_only_the_matching_entries() {
        let source = fake(vec![
            (Some("def/a.sii"), Some(b"0123456789")),
            (Some("model/b.pmg"), Some(b"0123456789")),
            (None, Some(b"DDS |\0\0\0")),
        ]);
        // 28 bytes in all, over the limit; each selection is under it.
        let options = ExtractOptions {
            max_bytes: 15,
            ..Default::default()
        };
        let dir = TempDir::new("filter");
        for (patterns, want) in [
            (["def/*"], "def/a.sii"),
            // Unnamed entries match by their `_unnamed/<hash>` label.
            (["_unnamed/*"], "_unnamed/0000000000001002.dds"),
        ] {
            let dest = dir.0.join(patterns[0].replace(['/', '*'], "_"));
            let filter = PathFilter::new(patterns);
            let filtered = Filtered {
                source: &source,
                filter: &filter,
            };
            let done = run(&filtered, &dest, &[], options).unwrap();
            assert_eq!(done.written, 1);
            assert_eq!(files_under(&dest), [want]);
        }
    }

    #[test]
    fn a_listing_names_files_as_an_extraction_would() {
        let source = fake(vec![
            (Some("def/CON.sii"), Some(b"x")),
            (Some("def/a.sii"), Some(b"abc")),
            (None, Some(b"zz")),
            (Some("model/b.pmg"), Some(b"q")),
        ]);
        let filter = PathFilter::new(["def/*", "_unnamed/*"]);
        let listed = plan(&Filtered {
            source: &source,
            filter: &filter,
        });
        let listed: Vec<(&str, Option<&str>, u64)> = listed
            .files
            .iter()
            .map(|l| (l.path.as_str(), l.source.as_deref(), l.size))
            .collect();
        assert_eq!(
            listed,
            [
                ("_unnamed/0000000000001002.bin", None, 2),
                ("def/_CON.sii", Some("def/CON.sii"), 1),
                ("def/a.sii", Some("def/a.sii"), 3),
            ]
        );
    }

    /// A [`DirListing`] of `files` and `directories`.
    fn dir_listing(files: &[&str], directories: &[&str]) -> DirListing {
        DirListing {
            files: files.iter().map(|s| (*s).to_owned()).collect(),
            directories: directories.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn v2_names_come_from_the_listings_then_from_candidates() {
        use specsheet_archive::hash_path;
        let kinds: HashMap<u64, EntryKind> = [
            ("", EntryKind::Directory),
            ("a.sii", EntryKind::File),
            ("def", EntryKind::Directory),
            ("def/b.sii", EntryKind::File),
            ("x.sii", EntryKind::File),
            ("y.sii", EntryKind::File),
            ("notdir", EntryKind::File),
            ("notdir/evil.sii", EntryKind::File),
        ]
        .into_iter()
        .map(|(path, kind)| (hash_path(path), kind))
        .collect();
        let listings: HashMap<&str, DirListing> = [
            // A missing file, a folder listed as a file, a file listed as
            // a folder and a folder without an entry are passed over.
            (
                "",
                dir_listing(&["a.sii", "missing.sii", "def"], &["def", "notdir", "gone"]),
            ),
            ("def", dir_listing(&["b.sii"], &[])),
            ("notdir", dir_listing(&["evil.sii"], &[])),
        ]
        .into_iter()
        .collect();
        let candidates = ["/x.sii", "a.sii", "def", "zzz.sii"].map(str::to_owned);
        let names = walk_v2_names(&|dir| listings.get(dir).cloned(), &kinds, &candidates);
        let by_path: Vec<&str> = names.names.values().map(String::as_str).collect();
        let mut by_path = by_path;
        by_path.sort();
        assert_eq!(by_path, ["a.sii", "def/b.sii", "x.sii"]);
        assert_eq!((names.from_listings, names.from_candidates), (2, 1));
        let mut unnamed = vec![
            hash_path("y.sii"),
            hash_path("notdir"),
            hash_path("notdir/evil.sii"),
        ];
        unnamed.sort();
        assert_eq!(names.unnamed, unnamed);

        // No root (locale.scs, base_cfg.scs): candidates only.
        let names = walk_v2_names(&|_| None, &kinds, &candidates);
        assert_eq!((names.from_listings, names.from_candidates), (0, 2));
    }

    /// A packed texture of `faces` faces, 8x4 BC1 with two mips, padded
    /// the way an archive pads (rows on 16 bytes).
    fn packed_bc1(faces: u32, dxgi: u32) -> specsheet_archive::PackedTexture {
        use specsheet_archive::{AddressMode, ImageMeta, PackedTexture, SampleMeta};
        // Mip 0: 2x1 blocks = one 16-byte row; mip 1 (4x2): 1 block, padded.
        let face = |f: u8| {
            let mut bytes = vec![f; 16];
            bytes.extend([f + 100; 8]);
            bytes.extend([0; 8]);
            bytes
        };
        PackedTexture {
            image: ImageMeta {
                width: 8,
                height: 4,
                mip_count: 2,
                dxgi_format: dxgi,
                is_cube: faces == 6,
                face_count: faces,
                pitch_alignment: 16,
                image_alignment: 16,
            },
            sample: SampleMeta {
                mag_linear: true,
                min_linear: true,
                address_u: AddressMode::ClampToEdge,
                address_v: AddressMode::Repeat,
            },
            surfaces: (0..faces as u8).flat_map(face).collect(),
        }
    }

    /// A packed texture written as a `.tobj` and a DX10 `.dds` loads, as a
    /// mod's loose texture, into the same texture the archive gives.
    #[test]
    fn packed_textures_convert_to_loose_files_that_load_the_same() {
        use crate::render::{load_cube_texture, load_texture};
        use specsheet_archive::{Device, Vfs, ZipArchive, ZipWriter};
        let flat = packed_bc1(1, 72);
        let (tobj, dds) = loose_files("model/t.tobj", &flat).expect("2D");
        let cube = packed_bc1(6, 71);
        let (cube_tobj, cube_dds) = loose_files("env/c.tobj", &cube).expect("cube");
        assert!(loose_files("model/t.pmd", &flat).is_none());

        let mut writer = ZipWriter::new();
        for (path, bytes) in [
            ("model/t.tobj", &tobj),
            ("model/t.dds", &dds),
            ("env/c.tobj", &cube_tobj),
            ("env/c.dds", &cube_dds),
        ] {
            writer.add(path, bytes).unwrap();
        }
        let dir = TempDir::new("loose");
        let zip = dir.0.join("loose.zip");
        fs::write(&zip, writer.finish().unwrap()).unwrap();
        let mut vfs = Vfs::new();
        vfs.mount_device(Device::Zip(ZipArchive::open(&zip).unwrap()), "");
        let loose = crate::GameFs::new(vfs);

        let from_loose = load_texture(&loose, "/model/t.tobj").expect("loose 2D");
        let expected = crate::render::convert_texture("/model/t.tobj", &flat).expect("packed");
        assert_eq!(from_loose, expected);
        let cube_loose = load_cube_texture(&loose, "/env/c.tobj").expect("loose cube");
        let cube_expected = crate::render::convert_cube("/env/c.tobj", &cube).expect("cube");
        assert_eq!(cube_loose, cube_expected);
    }

    /// With texture conversion on, the two files of a packed texture are
    /// written like any other file.
    #[test]
    fn converted_textures_are_written_as_files() {
        let dir = TempDir::new("converted");
        let dest = dir.0.join("out");
        let entry = |path, bytes, kind| FakeFile {
            path: Some(path),
            bytes: Some(bytes),
            fails_midway: false,
            declared: None,
            kind,
        };
        let source = Fake(vec![
            entry("model/t.tobj", b"tobj", ItemKind::TextureHeader),
            entry("model/t.dds", b"dds!", ItemKind::TextureImage),
        ]);
        let done = run(&source, &dest, &[], ExtractOptions::default()).unwrap();
        assert_eq!((done.written, done.packed_textures.len()), (2, 0));
        assert_eq!(files_under(&dest), ["model/t.dds", "model/t.tobj"]);
    }

    /// With nothing to write but a list (a filter that takes only a packed
    /// texture), the destination is still created for the list.
    #[test]
    fn a_list_alone_creates_the_destination() {
        let dir = TempDir::new("list-only");
        let dest = dir.0.join("out");
        let source = Fake(vec![FakeFile {
            path: Some("material/x.tobj"),
            bytes: Some(b"tex"),
            fails_midway: false,
            declared: None,
            kind: ItemKind::PackedTexture,
        }]);
        let done = run(&source, &dest, &[], ExtractOptions::default()).expect("runs");
        assert_eq!((done.written, done.packed_textures.len()), (0, 1));
        assert_eq!(files_under(&dest), ["_packed_textures.txt"]);
    }

    #[test]
    fn packed_textures_and_unindexed_entries_are_listed_not_written() {
        let dir = TempDir::new("kinds");
        let dest = dir.0.join("out");
        let entry = |path, kind| FakeFile {
            path,
            bytes: Some(b"0123456789"),
            fails_midway: false,
            declared: None,
            kind,
        };
        let source = Fake(vec![
            entry(Some("def/a.sii"), ItemKind::File),
            entry(Some("model/t.tobj"), ItemKind::PackedTexture),
            entry(None, ItemKind::PackedTexture),
            entry(None, ItemKind::Unindexed),
        ]);
        // Only the 10 bytes of `def/a.sii` count against the limit.
        let options = ExtractOptions {
            max_bytes: 15,
            ..Default::default()
        };
        let done = run(&source, &dest, &[], options).unwrap();
        assert_eq!(done.written, 1);
        assert_eq!(done.packed_textures, ["0000000000001002", "model/t.tobj"]);
        assert_eq!(done.unreadable, ["0000000000001003"]);
        assert_eq!(
            files_under(&dest),
            ["_packed_textures.txt", "_unreadable.txt", "def/a.sii"]
        );
        assert_eq!(
            fs::read_to_string(dest.join("_packed_textures.txt")).unwrap(),
            "0000000000001002\nmodel/t.tobj\n"
        );
        let listing = plan(&source);
        assert_eq!(listing.files.len(), 1);
        assert_eq!(listing.packed_textures, done.packed_textures);
        assert_eq!(listing.unreadable, done.unreadable);
    }
}
