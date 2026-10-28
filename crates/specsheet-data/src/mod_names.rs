//! Recovers file names in HashFS v1 mod packages.
//!
//! A v1 package indexes files by a hash of their path. Names live only in
//! directory listings, which are optional: most packages lack the root
//! listing, and some ship none at all (`docs/formats/mods.md`). The game
//! never needs them, since it always looks a file up by its full path. To
//! show a package's contents we hash candidate paths and keep the ones that
//! match, from three sources:
//! - paths the caller supplies: every path of the mounted game
//!   ([`game_paths`]), since mods replace game files at their own paths,
//!   and the files of the other mod packages, since a model package is
//!   often named only by a separate definitions package;
//! - the listings of every directory entry found, and the parents of every
//!   candidate, which reach listings below a missing root;
//! - paths mentioned inside the package's files, named or not
//!   ([`specsheet_formats::paths`]), the conventional files of the addon
//!   units they declare, and the icon and description that `manifest.sii`
//!   names.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use specsheet_archive::{DirListing, EntryV1, HashFsV1Archive, Vfs};
use specsheet_formats::{paths, sii};

/// Files larger than this are not scanned for paths (sound banks and
/// textures reach hundreds of MB and name nothing).
const MAX_SCAN: u32 = 1 << 20;
/// Upper bound on candidate paths tried per package, so hostile content
/// cannot grow the search without end. The game has about 610,000 paths.
const MAX_CANDIDATES: usize = 4_000_000;

/// Names recovered for a package's entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveredNames {
    /// Entry hash → path, without a leading slash.
    pub names: BTreeMap<u64, String>,
    /// Hashes of entries no candidate matched, sorted.
    pub unnamed: Vec<u64>,
}

/// Names the entries of `archive`, trying `candidates` first (see the
/// module docs). Unreadable entries are skipped, never fatal.
pub fn recover_v1_names(
    archive: &HashFsV1Archive,
    candidates: impl IntoIterator<Item = String>,
) -> RecoveredNames {
    let by_hash = archive.entries().map(|e| (e.hash, e)).collect();
    recover(&V1 { archive, by_hash }, candidates)
}

/// Every file and directory path of the mounted game, without a leading
/// slash. Walks the listings from the root; about 2 s for the base game and
/// DLC.
pub fn game_paths(vfs: &Vfs) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![String::new()];
    while let Some(dir) = stack.pop() {
        let Ok(listing) = vfs.list_dir(&dir) else {
            continue;
        };
        let join = |name: &str| {
            if dir.is_empty() {
                name.to_owned()
            } else {
                format!("{dir}/{name}")
            }
        };
        out.extend(listing.files.iter().map(|f| join(f)));
        for sub in &listing.directories {
            let path = join(sub);
            out.push(path.clone());
            stack.push(path);
        }
    }
    out
}

/// What the search needs from a package.
trait Package {
    /// Hash, directory flag and size of the entry at `path`.
    fn find(&self, path: &str) -> Option<(u64, bool, u32)>;
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    fn list_dir(&self, path: &str) -> Option<DirListing>;
    /// Hash, directory flag and size of every entry.
    fn entries(&self) -> Vec<(u64, bool, u32)>;
    fn read_hash(&self, hash: u64) -> Option<Vec<u8>>;
}

struct V1<'a> {
    archive: &'a HashFsV1Archive,
    by_hash: HashMap<u64, &'a EntryV1>,
}

impl Package for V1<'_> {
    fn find(&self, path: &str) -> Option<(u64, bool, u32)> {
        self.archive
            .entry(path)
            .map(|e| (e.hash, e.is_directory(), e.size))
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.archive.read(path).ok()
    }

    fn list_dir(&self, path: &str) -> Option<DirListing> {
        self.archive.list_dir(path).ok()
    }

    fn entries(&self) -> Vec<(u64, bool, u32)> {
        self.archive
            .entries()
            .map(|e| (e.hash, e.is_directory(), e.size))
            .collect()
    }

    fn read_hash(&self, hash: u64) -> Option<Vec<u8>> {
        let entry = self.by_hash.get(&hash)?;
        self.archive.read_entry(entry).ok()
    }
}

fn recover(package: &impl Package, candidates: impl IntoIterator<Item = String>) -> RecoveredNames {
    let mut queue: VecDeque<String> = ["manifest.sii", "versions.sii"]
        .into_iter()
        .map(str::to_owned)
        .chain(candidates)
        .collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut names = BTreeMap::new();
    let mut scanned: HashSet<u64> = HashSet::new();
    loop {
        drain(package, &mut queue, &mut seen, &mut names);
        // Files nothing names yet may still mention others by absolute path.
        let before = names.len();
        for (hash, is_dir, size) in package.entries() {
            if is_dir || size > MAX_SCAN || names.contains_key(&hash) || !scanned.insert(hash) {
                continue;
            }
            if let Some(bytes) = package.read_hash(hash) {
                queue.extend(paths::referenced_paths("", &bytes));
                queue.extend(paths::unit_paths(&bytes));
            }
        }
        drain(package, &mut queue, &mut seen, &mut names);
        if names.len() == before {
            break;
        }
    }
    let mut unnamed: Vec<u64> = package
        .entries()
        .into_iter()
        .map(|(hash, _, _)| hash)
        .filter(|h| !names.contains_key(h))
        .collect();
    unnamed.sort_unstable();
    RecoveredNames { names, unnamed }
}

/// Tries every queued path, adding what each match leads to.
fn drain(
    package: &impl Package,
    queue: &mut VecDeque<String>,
    seen: &mut HashSet<String>,
    names: &mut BTreeMap<u64, String>,
) {
    while let Some(path) = queue.pop_front() {
        let path = path.trim_start_matches('/').to_owned();
        if path.is_empty() || seen.contains(&path) {
            continue;
        }
        if seen.len() >= MAX_CANDIDATES {
            break;
        }
        seen.insert(path.clone());
        // A parent may be a directory entry whose listing names more files.
        let mut parent = path.as_str();
        while let Some((up, _)) = parent.rsplit_once('/') {
            if !seen.contains(up) {
                queue.push_back(up.to_owned());
            }
            parent = up;
        }
        let Some((hash, is_dir, size)) = package.find(&path) else {
            continue;
        };
        if names.contains_key(&hash) {
            continue;
        }
        names.insert(hash, path.clone());
        if is_dir {
            if let Some(listing) = package.list_dir(&path) {
                let children = listing.files.iter().chain(&listing.directories);
                queue.extend(children.map(|name| format!("{path}/{name}")));
            }
            continue;
        }
        queue.extend(paths::companions(&path));
        if size > MAX_SCAN {
            continue;
        }
        let Some(bytes) = package.read(&path) else {
            continue;
        };
        queue.extend(paths::referenced_paths(&path, &bytes));
        queue.extend(paths::unit_paths(&bytes));
        if path == "manifest.sii" {
            queue.extend(manifest_files(&bytes));
        }
    }
}

/// The root-relative icon and description files a manifest names.
fn manifest_files(bytes: &[u8]) -> Vec<String> {
    let Ok(document) = sii::parse("/manifest.sii", bytes) else {
        return Vec::new();
    };
    document
        .units_of_class("mod_package")
        .flat_map(|unit| ["icon", "description_file"].map(|key| unit.get_str(key)))
        .flatten()
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// A package keyed like v1 (by path hash) with optional listings.
    #[derive(Default)]
    struct Fake {
        files: HashMap<String, Vec<u8>>,
        dirs: HashMap<String, DirListing>,
        /// Paths the package holds but no source names.
        hidden: Vec<String>,
    }

    fn hash(path: &str) -> u64 {
        specsheet_archive::city_hash_64(path.as_bytes())
    }

    impl Package for Fake {
        fn find(&self, path: &str) -> Option<(u64, bool, u32)> {
            if let Some(bytes) = self.files.get(path) {
                return Some((hash(path), false, bytes.len() as u32));
            }
            self.dirs.contains_key(path).then(|| (hash(path), true, 0))
        }

        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.files.get(path).cloned()
        }

        fn list_dir(&self, path: &str) -> Option<DirListing> {
            self.dirs.get(path).cloned()
        }

        fn entries(&self) -> Vec<(u64, bool, u32)> {
            let files = self
                .files
                .iter()
                .map(|(p, b)| (hash(p), false, b.len() as u32));
            let dirs = self.dirs.keys().map(|p| (hash(p), true, 0));
            let hidden = self.hidden.iter().map(|p| (hash(p), false, 0));
            files.chain(dirs).chain(hidden).collect()
        }

        fn read_hash(&self, wanted: u64) -> Option<Vec<u8>> {
            self.files
                .iter()
                .find(|(p, _)| hash(p) == wanted)
                .map(|(_, b)| b.clone())
        }
    }

    impl Fake {
        fn file(mut self, path: &str, bytes: &[u8]) -> Self {
            self.files.insert(path.to_owned(), bytes.to_vec());
            self
        }
    }

    fn named(result: &RecoveredNames) -> Vec<&str> {
        let mut names: Vec<&str> = result.names.values().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn names_files_from_candidates_references_and_the_manifest() {
        let package = Fake::default()
            .file(
                "manifest.sii",
                b"SiiNunit\n{\nmod_package : .m {\n icon: \"icon.jpg\"\n description_file: \"d.txt\"\n}\n}\n",
            )
            .file("icon.jpg", b"\xff\xd8\xff")
            .file("d.txt", b"text")
            // Named only by the caller's candidates (a game path).
            .file(
                "def/vehicle/truck/t/sound/engine.sii",
                b"sounds[]: \"engine|/def/x/engine.soundref\"\n",
            )
            // Named only by a reference inside a named file.
            .file("def/x/engine.soundref", b"source=\"/sound/truck/e.bank#engine\"\n")
            .file("sound/truck/e.bank", b"RIFF")
            // Named by the companion rule.
            .file("sound/truck/e.bank.guids", b"{guid} bank:/e");
        let result = recover(
            &package,
            [
                "def/vehicle/truck/t/sound/engine.sii".to_owned(),
                "def/not/here.sii".to_owned(),
            ],
        );
        assert_eq!(
            named(&result),
            [
                "d.txt",
                "def/vehicle/truck/t/sound/engine.sii",
                "def/x/engine.soundref",
                "icon.jpg",
                "manifest.sii",
                "sound/truck/e.bank",
                "sound/truck/e.bank.guids",
            ]
        );
        assert!(result.unnamed.is_empty());
    }

    #[test]
    fn follows_relative_includes() {
        let package = Fake::default()
            .file(
                "def/t/cabin.sii",
                b"x : .y {\n @include \"cabin_sound.sui\"\n}\n",
            )
            .file("def/t/cabin_sound.sui", b"sounds: 0\n");
        let result = recover(&package, ["def/t/cabin.sii".to_owned()]);
        assert_eq!(named(&result), ["def/t/cabin.sii", "def/t/cabin_sound.sui"]);
    }

    #[test]
    fn reaches_listings_below_a_missing_root_through_parents() {
        let mut package = Fake::default().file("def/a/b/deep.sii", b"");
        package.dirs.insert(
            "def/a".to_owned(),
            DirListing {
                files: vec![],
                directories: vec!["b".to_owned()],
            },
        );
        package.dirs.insert(
            "def/a/b".to_owned(),
            DirListing {
                files: vec!["deep.sii".to_owned()],
                directories: vec![],
            },
        );
        // Only a path that does not exist, in a folder that does.
        let result = recover(&package, ["def/a/other.sii".to_owned()]);
        assert_eq!(named(&result), ["def/a", "def/a/b", "def/a/b/deep.sii"]);
    }

    #[test]
    fn follows_references_inside_files_nothing_names() {
        // An unnamed definition names its model; the model gets a name even
        // though the definition does not.
        let mut package =
            Fake::default().file("vehicle/badge/b.pmd", b"\x00/vehicle/badge/b.tobj\x00");
        package.files.insert(
            "def/hidden/badge.sii".to_owned(),
            b"model: \"/vehicle/badge/b.pmd\"".to_vec(),
        );
        let package = package
            .file("vehicle/badge/b.pmg", b"")
            .file("vehicle/badge/b.tobj", b"");
        let result = recover(&package, Vec::new());
        assert_eq!(
            named(&result),
            [
                "vehicle/badge/b.pmd",
                "vehicle/badge/b.pmg",
                "vehicle/badge/b.tobj"
            ]
        );
        assert_eq!(result.unnamed, [hash("def/hidden/badge.sii")]);
    }

    #[test]
    fn reports_entries_no_source_names() {
        let mut package = Fake::default().file("manifest.sii", b"SiiNunit\n{\n}\n");
        package.hidden.push("secret/file.sii".to_owned());
        let result = recover(&package, Vec::new());
        assert_eq!(named(&result), ["manifest.sii"]);
        assert_eq!(result.unnamed, [hash("secret/file.sii")]);
    }

    #[test]
    fn never_scans_large_files_and_terminates_on_cycles() {
        let big = vec![b'/'; MAX_SCAN as usize + 1];
        let package = Fake::default()
            .file("a.sii", b"\"/b.sii\"")
            .file("b.sii", b"\"/a.sii\"")
            .file("big.bin", &big);
        let result = recover(&package, ["a.sii".to_owned(), "big.bin".to_owned()]);
        assert_eq!(named(&result), ["a.sii", "b.sii", "big.bin"]);
    }
}
