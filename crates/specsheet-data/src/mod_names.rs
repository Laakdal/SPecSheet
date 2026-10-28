//! Recovers file names in HashFS v1 mod packages.
//!
//! A v1 package indexes files by a hash of their path. Names live only in
//! directory listings, which are optional: most packages lack the root
//! listing, and some ship none at all (`docs/formats/mods.md`). The game
//! never needs them, since it always looks a file up by its full path. To
//! show a package's contents we hash candidate paths and keep the ones that
//! match, from these sources:
//! - paths the caller supplies: every path of the mounted game
//!   ([`game_paths`]), since mods replace game files at their own paths,
//!   and the files of the other mod packages, since a model package is
//!   often named only by a separate definitions package;
//! - the listings of every directory entry found, and the parents of every
//!   candidate, which reach listings below a missing root;
//! - paths mentioned inside the package's files, named or not
//!   ([`specsheet_formats::paths`], `.tobj` texture paths read exactly),
//!   the conventional files of the addon units they declare, and the icon
//!   and description that `manifest.sii` names;
//! - the content hash of each unnamed entry, which names a generated
//!   `automat/` material;
//! - the stems of the named files packed next to an unnamed one, with the
//!   package's extensions;
//! - variants of found names by word swaps learned from the game
//!   ([`learn_word_swaps`]), and a vehicle folder's files tried in the
//!   other vehicle folders of its kind;
//! - names that sort between an unnamed file's named neighbours in packing
//!   order, built from words of paths with the same extension.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use specsheet_archive::{city_hash_64, DirListing, EntryV1, HashFsV1Archive, Vfs};
use specsheet_formats::{paths, sii};

/// Files larger than this are not scanned for paths (sound banks and
/// textures reach hundreds of MB and name nothing). Listings too; the
/// largest in 21 real packages is 87 KB.
const MAX_SCAN: u32 = 1 << 20;
/// Extensions tried per packing-order neighbour, the package's most used
/// first; a bound against packages built to multiply guesses.
const MAX_EXTENSIONS: usize = 64;

/// Bounds on one package's search, so hostile content cannot grow it
/// without end, set well above what the test install needs
/// (`research/evidence/mod-name-recovery.txt`, section 6).
#[derive(Clone, Copy, Debug)]
struct Limits {
    /// Paths ever queued, tried or waiting. A full run with the game and
    /// the ZIP mods as candidates queues about 1.27 million.
    candidates: usize,
    /// Total length of those paths; about 59 MB in that run.
    candidate_bytes: usize,
    /// Bytes read to scan for paths, listings included. The largest real
    /// package holds 1.18 GB of files small enough to scan, and a file is
    /// read at most twice (unnamed, then named).
    scan_bytes: u64,
}

const LIMITS: Limits = Limits {
    candidates: 4_000_000,
    candidate_bytes: 256 << 20,
    scan_bytes: 4 << 30,
};

/// Names recovered for a package's entries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecoveredNames {
    /// Entry hash → path, without a leading slash.
    pub names: BTreeMap<u64, String>,
    /// Hashes of entries no candidate matched, sorted.
    pub unnamed: Vec<u64>,
}

/// Names the entries of `archive`, trying `candidates` first (see the
/// module docs), and variants of found names by `swaps`. Unreadable entries
/// are skipped, never fatal.
pub fn recover_v1_names(
    archive: &HashFsV1Archive,
    candidates: impl IntoIterator<Item = String>,
    swaps: &WordSwaps,
) -> RecoveredNames {
    let by_hash = archive.entries().map(|e| (e.hash, e)).collect();
    recover_with(&V1 { archive, by_hash }, candidates, LIMITS, swaps)
}

/// Word swaps between file names, learned from a set of paths (normally the
/// game's, [`game_paths`]): two files in one folder, with one extension,
/// whose names differ in exactly one `_`-separated word, as `blinker_on`
/// and `blinker_off`. A swap seen in at least [`MIN_SWAP_COUNT`] such
/// pairs is kept (13,462 in the 1.60 game, `research/evidence/
/// mod-name-recovery.txt` section 10).
#[derive(Clone, Debug, Default)]
pub struct WordSwaps {
    /// Word → the words it is swapped with.
    to: HashMap<String, Vec<String>>,
}

/// Swaps seen fewer times than this are not used.
pub const MIN_SWAP_COUNT: usize = 20;
/// Folders with more files than this are skipped when learning, to bound
/// the pairwise comparison.
const MAX_LEARN_FOLDER: usize = 200;

/// Learns [`WordSwaps`] from `paths`.
pub fn learn_word_swaps(paths: &[String]) -> WordSwaps {
    let mut folders: HashMap<(&str, &str), Vec<Vec<&str>>> = HashMap::new();
    for path in paths {
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
        if let Some((stem, ext)) = file.split_once('.') {
            folders
                .entry((dir, ext))
                .or_default()
                .push(stem.split('_').collect());
        }
    }
    let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
    for stems in folders.values().filter(|s| s.len() <= MAX_LEARN_FOLDER) {
        for (i, a) in stems.iter().enumerate() {
            for b in stems[i + 1..].iter().filter(|b| b.len() == a.len()) {
                let mut differ = (0..a.len()).filter(|&k| a[k] != b[k]);
                if let (Some(k), None) = (differ.next(), differ.next()) {
                    *counts.entry((a[k], b[k])).or_default() += 1;
                    *counts.entry((b[k], a[k])).or_default() += 1;
                }
            }
        }
    }
    let mut to: HashMap<String, Vec<String>> = HashMap::new();
    for ((from, into), n) in counts {
        if n >= MIN_SWAP_COUNT {
            to.entry(from.to_owned()).or_default().push(into.to_owned());
        }
    }
    for words in to.values_mut() {
        words.sort();
    }
    WordSwaps { to }
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
    /// Hashes of the file entries in the order their data is stored.
    fn packing_order(&self) -> Vec<u64>;
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

    fn packing_order(&self) -> Vec<u64> {
        let mut files: Vec<&EntryV1> = self
            .archive
            .entries()
            .filter(|e| !e.is_directory())
            .collect();
        files.sort_by_key(|e| e.offset);
        files.into_iter().map(|e| e.hash).collect()
    }
}

/// Paths waiting to be tried, each checked and queued once, within
/// [`Limits`].
struct Search {
    limits: Limits,
    queue: VecDeque<String>,
    /// Every path ever queued.
    seen: HashSet<String>,
    seen_bytes: usize,
    scanned_bytes: u64,
}

impl Search {
    fn new(limits: Limits) -> Self {
        Self {
            limits,
            queue: VecDeque::new(),
            seen: HashSet::new(),
            seen_bytes: 0,
            scanned_bytes: 0,
        }
    }

    /// Queues `path` if it is a plausible path ([`paths::clean`]), new, and
    /// within the candidate limits; otherwise drops it.
    fn push(&mut self, path: &str) {
        let Some(path) = paths::clean(path) else {
            return;
        };
        let full = self.seen.len() >= self.limits.candidates
            || self.seen_bytes + path.len() > self.limits.candidate_bytes;
        if full || self.seen.contains(path) {
            return;
        }
        self.seen_bytes += path.len();
        self.seen.insert(path.to_owned());
        self.queue.push_back(path.to_owned());
    }

    /// Whether no more paths can be queued.
    fn is_full(&self) -> bool {
        self.seen.len() >= self.limits.candidates || self.seen_bytes >= self.limits.candidate_bytes
    }

    fn extend<S: AsRef<str>>(&mut self, paths: impl IntoIterator<Item = S>) {
        for path in paths {
            self.push(path.as_ref());
        }
    }

    /// Whether an entry of `size` bytes may be read to scan it, counting
    /// it against the scan limit if so.
    fn may_scan(&mut self, size: u32) -> bool {
        let total = self.scanned_bytes + u64::from(size);
        if size > MAX_SCAN || total > self.limits.scan_bytes {
            return false;
        }
        self.scanned_bytes = total;
        true
    }
}

#[cfg(test)]
fn recover(package: &impl Package, candidates: impl IntoIterator<Item = String>) -> RecoveredNames {
    recover_with(package, candidates, LIMITS, &WordSwaps::default())
}

fn recover_with(
    package: &impl Package,
    candidates: impl IntoIterator<Item = String>,
    limits: Limits,
    swaps: &WordSwaps,
) -> RecoveredNames {
    let mut search = Search::new(limits);
    search.extend(["manifest.sii", "versions.sii"]);
    let mut vehicles = Vehicles::default();
    let mut words: Option<Words> = None;
    search.extend(candidates.into_iter().inspect(|p| vehicles.note(p)));
    let mut names = BTreeMap::new();
    let mut scanned: HashSet<u64> = HashSet::new();
    loop {
        drain(package, &mut search, &mut names);
        // Files nothing names yet may still mention others by absolute path.
        let before = names.len();
        for (hash, is_dir, size) in package.entries() {
            if is_dir
                || names.contains_key(&hash)
                || !scanned.insert(hash)
                || !search.may_scan(size)
            {
                continue;
            }
            if let Some(bytes) = package.read_hash(hash) {
                search.extend(paths::referenced_paths("", &bytes));
                search.extend(paths::unit_paths(&bytes));
                search.push(&automat_path(&bytes));
            }
        }
        drain(package, &mut search, &mut names);
        if names.len() == before {
            neighbour_guesses(package, &names, &mut search);
            swap_guesses(&names, swaps, &mut search);
            vehicle_guesses(&names, &mut vehicles, &mut search);
            gap_guesses(package, &names, &mut words, &mut search);
            drain(package, &mut search, &mut names);
        }
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
fn drain(package: &impl Package, search: &mut Search, names: &mut BTreeMap<u64, String>) {
    while let Some(path) = search.queue.pop_front() {
        // A parent may be a directory entry whose listing names more files.
        let mut parent = path.as_str();
        while let Some((up, _)) = parent.rsplit_once('/') {
            search.push(up);
            parent = up;
        }
        // Companions too, even of a missing file: a package may keep a
        // texture's descriptor without the texture.
        search.extend(paths::companions(&path));
        let Some((hash, is_dir, size)) = package.find(&path) else {
            continue;
        };
        if names.contains_key(&hash) {
            continue;
        }
        names.insert(hash, path.clone());
        if is_dir {
            if search.may_scan(size) {
                if let Some(listing) = package.list_dir(&path) {
                    for name in listing.files.iter().chain(&listing.directories) {
                        search.push(&format!("{path}/{name}"));
                    }
                }
            }
            continue;
        }
        if !search.may_scan(size) {
            continue;
        }
        let Some(bytes) = package.read(&path) else {
            continue;
        };
        search.extend(paths::referenced_paths(&path, &bytes));
        search.extend(paths::unit_paths(&bytes));
        if path == "manifest.sii" {
            search.extend(manifest_files(&bytes));
        }
    }
}

/// Guesses from the packing order. Packers write a folder's files together,
/// often sorted by name, so the named files packed just before and after an
/// unnamed one give its likely folder and stem (`research/evidence/
/// mod-name-recovery.txt`, section 7). Each such `<folder>/<stem>` is tried
/// with the [`MAX_EXTENSIONS`] extensions the package's named files use
/// most. Guesses go straight into `search`, so its limits hold, and stop
/// once it is full.
fn neighbour_guesses(package: &impl Package, names: &BTreeMap<u64, String>, search: &mut Search) {
    let split = |path: &str| -> Option<(usize, usize)> {
        let name_at = path.rfind('/').map_or(0, |i| i + 1);
        let dot = path[name_at..].find('.')?;
        Some((name_at, name_at + dot))
    };
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for path in names.values() {
        if let Some((_, dot)) = split(path) {
            *counts.entry(&path[dot + 1..]).or_default() += 1;
        }
    }
    let mut extensions: Vec<(&str, usize)> = counts.into_iter().collect();
    extensions.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    extensions.truncate(MAX_EXTENSIONS);
    let order: Vec<Option<&String>> = package
        .packing_order()
        .iter()
        .map(|h| names.get(h))
        .collect();
    // The nearest named file before each position.
    let mut before = Vec::with_capacity(order.len());
    let mut last = None;
    for name in &order {
        before.push(last);
        if name.is_some() {
            last = *name;
        }
    }
    let mut next = None;
    for (i, name) in order.iter().enumerate().rev() {
        if name.is_some() {
            next = *name;
            continue;
        }
        for neighbour in before[i].into_iter().chain(next) {
            let Some((_, dot)) = split(neighbour) else {
                continue;
            };
            for (ext, _) in &extensions {
                if search.is_full() {
                    return;
                }
                search.push(&format!("{}.{ext}", &neighbour[..dot]));
            }
        }
    }
}

/// Vehicle folders (`def/vehicle/<kind>/<name>/`) seen among the
/// candidates and found names, by kind.
#[derive(Default)]
struct Vehicles(HashMap<String, HashSet<String>>);

impl Vehicles {
    /// Records the vehicle folder of `path`, if it is in one.
    fn note(&mut self, path: &str) {
        if let Some((kind, name, _)) = vehicle_parts(path) {
            if !self.0.get(kind).is_some_and(|names| names.contains(name)) {
                self.0
                    .entry(kind.to_owned())
                    .or_default()
                    .insert(name.to_owned());
            }
        }
    }
}

/// `(kind, name, rest)` of `def/vehicle/<kind>/<name>/<rest>`.
fn vehicle_parts(path: &str) -> Option<(&str, &str, &str)> {
    let rest = path.trim_start_matches('/').strip_prefix("def/vehicle/")?;
    let (kind, rest) = rest.split_once('/')?;
    let (name, rest) = rest.split_once('/')?;
    (!rest.is_empty()).then_some((kind, name, rest))
}

/// A vehicle folder's files tried in its sibling vehicles of the same kind:
/// a sound mod lays out each truck it covers alike (`scania.s_2016/
/// interior/sound.sui`, `scania.t_2016/interior/sound.sui`; `research/
/// evidence/mod-name-recovery.txt` section 11).
fn vehicle_guesses(names: &BTreeMap<u64, String>, vehicles: &mut Vehicles, search: &mut Search) {
    for path in names.values() {
        vehicles.note(path);
    }
    for path in names.values() {
        let Some((kind, own, rest)) = vehicle_parts(path) else {
            continue;
        };
        for other in vehicles.0.get(kind).into_iter().flatten() {
            if search.is_full() {
                return;
            }
            if other != own {
                search.push(&format!("def/vehicle/{kind}/{other}/{rest}"));
            }
        }
    }
}

/// Words of the candidates' folder and file names, split at `_` and `.`,
/// by the extension of the path they come from, for [`gap_names`]: a gap
/// between two `.sui` files is filled with words of `.sui` paths.
#[derive(Default)]
struct Words(HashMap<String, BTreeSet<String>>, usize);

/// Longest word kept, and most words kept (a bound against hostile
/// candidates).
const MAX_WORD: usize = 64;
const MAX_WORDS: usize = 1_000_000;
/// Guesses per gap.
const MAX_GAP_GUESSES: usize = 20_000;

impl Words {
    fn note(&mut self, path: &str) {
        let file = path.rsplit('/').next().unwrap_or(path);
        let Some((_, ext)) = file.split_once('.') else {
            return;
        };
        let set = self.0.entry(ext.to_owned()).or_default();
        for word in path.split(['/', '_', '.']) {
            if !word.is_empty()
                && word.len() <= MAX_WORD
                && self.1 < MAX_WORDS
                && !set.contains(word)
            {
                set.insert(word.to_owned());
                self.1 += 1;
            }
        }
    }
}

/// Names that sort strictly between two files of one folder: their shared
/// leading `_` words, one more word, and their shared extension
/// (`sound.sui` < `sound_2024.sui` < `sound_effects.sui`).
fn gap_names(before: &str, after: &str, words: &Words) -> Vec<String> {
    let split = |path: &str| -> (String, String) {
        match path.rsplit_once('/') {
            Some((dir, file)) => (format!("{dir}/"), file.to_owned()),
            None => (String::new(), path.to_owned()),
        }
    };
    let ((dir, low), (dir_after, high)) = (split(before), split(after));
    let (Some((stem_low, ext)), Some((stem_high, ext_high))) =
        (low.split_once('.'), high.split_once('.'))
    else {
        return Vec::new();
    };
    // A package that is not sorted may pack a later name first.
    if dir != dir_after || ext != ext_high || low >= high {
        return Vec::new();
    }
    let shared: Vec<&str> = stem_low
        .split('_')
        .zip(stem_high.split('_'))
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a)
        .collect();
    let lead = if shared.is_empty() {
        String::new()
    } else {
        format!("{}_", shared.join("_"))
    };
    // Words whose name can fall in the gap: from the low name's next word
    // (or the start) up to the high name's rest (or the end).
    let from = low
        .strip_prefix(&lead)
        .map_or("", |rest| rest.split(['_', '.']).next().unwrap_or(""));
    let to = match high.strip_prefix(&lead) {
        Some(rest) => rest.to_owned(),
        None if high > lead => char::MAX.to_string(),
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    if from > to.as_str() {
        return out;
    }
    let range = (
        std::ops::Bound::Included(from),
        std::ops::Bound::Included(to.as_str()),
    );
    let Some(set) = words.0.get(ext) else {
        return out;
    };
    for word in set.range::<str, _>(range) {
        let name = format!("{lead}{word}.{ext}");
        if low < name && name < high {
            out.push(format!("{dir}{name}"));
            if out.len() == MAX_GAP_GUESSES {
                break;
            }
        }
    }
    out
}

/// For each unnamed file packed between two named files of one folder, the
/// names that sort between theirs (packers sort a folder's names;
/// `research/evidence/mod-name-recovery.txt` section 12). The words come
/// from every path queued so far; they are gathered once, and only when a
/// package has such a gap.
fn gap_guesses(
    package: &impl Package,
    names: &BTreeMap<u64, String>,
    words: &mut Option<Words>,
    search: &mut Search,
) {
    let order: Vec<Option<&String>> = package
        .packing_order()
        .iter()
        .map(|h| names.get(h))
        .collect();
    let mut gaps = Vec::new();
    let mut before = None;
    for (i, name) in order.iter().enumerate() {
        if name.is_some() {
            before = *name;
            continue;
        }
        if let (Some(low), Some(high)) = (before, order[i + 1..].iter().flatten().next()) {
            gaps.push((String::clone(low), String::clone(high)));
        }
    }
    if gaps.is_empty() {
        return;
    }
    let words = words.get_or_insert_with(|| {
        let mut words = Words::default();
        for path in search.seen.iter().chain(names.values()) {
            words.note(path);
        }
        words
    });
    for (low, high) in gaps {
        for guess in gap_names(&low, &high, words) {
            if search.is_full() {
                return;
            }
            search.push(&guess);
        }
    }
}

/// Variants of the found names by [`WordSwaps`], in the same folder.
fn swap_guesses(names: &BTreeMap<u64, String>, swaps: &WordSwaps, search: &mut Search) {
    for path in names.values() {
        let (dir, file) = path
            .rsplit_once('/')
            .map_or(("", path.as_str()), |(d, f)| (d, f));
        let Some((stem, ext)) = file.split_once('.') else {
            continue;
        };
        let words: Vec<&str> = stem.split('_').collect();
        for (k, word) in words.iter().enumerate() {
            for into in swaps.to.get(*word).into_iter().flatten() {
                if search.is_full() {
                    return;
                }
                let mut variant = words.clone();
                variant[k] = into;
                let name = format!("{}.{ext}", variant.join("_"));
                search.push(&if dir.is_empty() {
                    name
                } else {
                    format!("{dir}/{name}")
                });
            }
        }
    }
}

/// Where a generated material with these bytes lives: the game names every
/// `automat/<xx>/<hash>.mat` by the CityHash64 of its content, `xx` being
/// the first two hex digits (65,473 of 65,473,
/// `research/evidence/mod-name-recovery.txt` section 8).
fn automat_path(bytes: &[u8]) -> String {
    let digest = format!("{:016x}", city_hash_64(bytes));
    format!("automat/{}/{digest}.mat", &digest[..2])
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
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    use super::*;

    /// A package keyed like v1 (by path hash) with optional listings.
    #[derive(Default)]
    struct Fake {
        files: HashMap<String, Vec<u8>>,
        dirs: HashMap<String, DirListing>,
        /// Paths the package holds but no source names.
        hidden: Vec<String>,
        /// Every path the search looked up.
        asked: RefCell<Vec<String>>,
        /// Bytes handed out by `read` and `read_hash`.
        bytes_read: Cell<u64>,
        /// File paths in packing order.
        order: Vec<String>,
    }

    fn hash(path: &str) -> u64 {
        specsheet_archive::city_hash_64(path.as_bytes())
    }

    impl Package for Fake {
        fn find(&self, path: &str) -> Option<(u64, bool, u32)> {
            self.asked.borrow_mut().push(path.to_owned());
            if let Some(bytes) = self.files.get(path) {
                return Some((hash(path), false, bytes.len() as u32));
            }
            self.dirs.contains_key(path).then(|| (hash(path), true, 0))
        }

        fn read(&self, path: &str) -> Option<Vec<u8>> {
            let bytes = self.files.get(path).cloned()?;
            self.bytes_read
                .set(self.bytes_read.get() + bytes.len() as u64);
            Some(bytes)
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
            let bytes = self
                .files
                .iter()
                .find(|(p, _)| hash(p) == wanted)
                .map(|(_, b)| b.clone())?;
            self.bytes_read
                .set(self.bytes_read.get() + bytes.len() as u64);
            Some(bytes)
        }

        fn packing_order(&self) -> Vec<u64> {
            self.order.iter().map(|p| hash(p)).collect()
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

    #[test]
    fn guesses_from_named_neighbours_in_packing_order() {
        // Nothing names dlc/a.dds, but it is packed beside dlc/a.pmd, and
        // the package uses the .dds extension elsewhere.
        let mut package = Fake::default()
            .file("dlc/a.dds", b"junk")
            .file("dlc/a.pmd", b"")
            .file("dlc/a.pmg", b"")
            .file("m/x.dds", b"");
        package.order = ["dlc/a.dds", "dlc/a.pmd", "dlc/a.pmg", "m/x.dds"]
            .map(str::to_owned)
            .to_vec();
        let result = recover(&package, ["dlc/a.pmd".to_owned(), "m/x.dds".to_owned()]);
        assert_eq!(
            named(&result),
            ["dlc/a.dds", "dlc/a.pmd", "dlc/a.pmg", "m/x.dds"]
        );
    }

    #[test]
    fn neighbour_guesses_keep_to_common_extensions_and_the_limits() {
        // 100 named files with 100 extensions, then one unnamed file.
        let mut package = Fake::default().file("u/unnamed.bin", b"x");
        let mut names = BTreeMap::new();
        for i in 0..100 {
            let path = format!("e/f{i}.x{i}");
            names.insert(hash(&path), path.clone());
            package = package.file(&path, b"");
            package.order.push(path);
        }
        package.order.push("u/unnamed.bin".to_owned());
        let mut search = Search::new(LIMITS);
        neighbour_guesses(&package, &names, &mut search);
        assert_eq!(search.queue.len(), MAX_EXTENSIONS);
        // A full search takes no more guesses.
        let mut search = Search::new(Limits {
            candidates: 10,
            ..LIMITS
        });
        neighbour_guesses(&package, &names, &mut search);
        assert_eq!(search.queue.len(), 10);
    }

    #[test]
    fn learns_word_swaps_from_folders_and_applies_them() {
        // 20 folders each hold an on/off pair: the swap is learned.
        let mut paths = Vec::new();
        for i in 0..MIN_SWAP_COUNT {
            paths.push(format!("d{i}/lamp_on.soundref"));
            paths.push(format!("d{i}/lamp_off.soundref"));
        }
        // Seen once only: not learned.
        paths.push("x/left_a.sii".to_owned());
        paths.push("x/left_b.sii".to_owned());
        let swaps = learn_word_swaps(&paths);
        assert_eq!(
            swaps.to.get("on").map(Vec::as_slice),
            Some(&["off".to_owned()][..])
        );
        assert!(!swaps.to.contains_key("a"));

        let package = Fake::default()
            .file("f/blinker_on.soundref", b"")
            .file("f/blinker_off.soundref", b"");
        let result = recover_with(
            &package,
            ["f/blinker_on.soundref".to_owned()],
            LIMITS,
            &swaps,
        );
        assert_eq!(
            named(&result),
            ["f/blinker_off.soundref", "f/blinker_on.soundref"]
        );
    }

    #[test]
    fn tries_a_vehicle_folders_files_in_its_sibling_vehicles() {
        // The package's Scania S interior is named; its Scania T copy is
        // not, but another candidate shows that folder exists.
        let package = Fake::default()
            .file(
                "def/vehicle/truck/scania.s/interior/sound.sui",
                b"sounds: 1",
            )
            .file(
                "def/vehicle/truck/scania.t/interior/sound.sui",
                b"sounds: 2",
            );
        let result = recover(
            &package,
            [
                "def/vehicle/truck/scania.s/interior/sound.sui".to_owned(),
                "def/vehicle/truck/scania.t/engine/other.sii".to_owned(),
            ],
        );
        assert_eq!(
            named(&result),
            [
                "def/vehicle/truck/scania.s/interior/sound.sui",
                "def/vehicle/truck/scania.t/interior/sound.sui"
            ]
        );
    }

    #[test]
    fn guesses_names_that_sort_between_packed_neighbours() {
        // Packed between sound.sui and sound_effects.sui (names sorted), so
        // the name sorts between them; "2024" is a word another path uses.
        let mut package = Fake::default()
            .file("f/sound.sui", b"a")
            .file("f/sound_2024.sui", b"b")
            .file("f/sound_effects.sui", b"c");
        package.order = ["f/sound.sui", "f/sound_2024.sui", "f/sound_effects.sui"]
            .map(str::to_owned)
            .to_vec();
        let result = recover(
            &package,
            [
                "f/sound.sui".to_owned(),
                "f/sound_effects.sui".to_owned(),
                "def/vehicle/truck/volvo.fh_2024/x.sui".to_owned(),
            ],
        );
        assert_eq!(
            named(&result),
            ["f/sound.sui", "f/sound_2024.sui", "f/sound_effects.sui"]
        );
    }

    #[test]
    fn gap_names_keep_to_the_gap() {
        let mut words = Words::default();
        for w in ["2024", "alpha", "dash", "effects", "zeta", "a.b"] {
            words.note(&format!("x/{w}/y.sui"));
        }
        // Words of other extensions do not fill a .sui gap.
        words.note("x/beta/y.pmd");
        let guesses = gap_names("f/sound.sui", "f/sound_effects.sui", &words);
        assert_eq!(
            guesses,
            [
                "f/sound_2024.sui",
                "f/sound_a.sui",
                "f/sound_alpha.sui",
                "f/sound_b.sui",
                "f/sound_dash.sui"
            ]
        );
        // Different folders: no gap.
        assert!(gap_names("f/sound.sui", "g/sound_effects.sui", &words).is_empty());
        // An unsorted package packs a later name first: no gap, no panic.
        assert!(gap_names("f/sound_zeta.sui", "f/sound_alpha.sui", &words).is_empty());
        assert!(gap_names("f/sound_dash_x.sui", "f/sound_alpha.sui", &words).is_empty());
    }

    #[test]
    fn names_automat_materials_by_their_content() {
        let content = b"material : \"eut2.dif\" {\n texture : \"/x.tobj\"\n}\n";
        let digest = format!("{:016x}", specsheet_archive::city_hash_64(content));
        let path = format!("automat/{}/{digest}.mat", &digest[..2]);
        let package = Fake::default().file(&path, content);
        let result = recover(&package, Vec::new());
        assert_eq!(named(&result), [path.as_str()]);
    }

    #[test]
    fn names_a_descriptor_whose_texture_is_missing() {
        // Nothing names the .tobj, and the texture it points to is not in
        // the package; its name is still the texture's with `.tobj`.
        let package = Fake::default().file(
            "model/sky/a.tobj",
            b"\x01\x0a\xb1\x70\x00\x00\x00\x00/model/sky/a.dds",
        );
        let result = recover(&package, Vec::new());
        assert_eq!(named(&result), ["model/sky/a.tobj"]);
    }

    #[test]
    fn never_looks_up_unsafe_or_overlong_paths() {
        // Listing names and caller candidates go through the same checks as
        // paths found in files.
        let mut package = Fake::default().file("def/a/ok.sii", b"");
        package.dirs.insert(
            "def/a".to_owned(),
            DirListing {
                files: vec![
                    "ok.sii".to_owned(),
                    "../up.sii".to_owned(),
                    String::new(),
                    "x".repeat(600),
                ],
                directories: vec![".".to_owned(), "b/../c".to_owned()],
            },
        );
        let overlong = format!("def/{}x.sii", "b/".repeat(400));
        let result = recover(
            &package,
            ["def/a".to_owned(), overlong, "def//c.sii".to_owned()],
        );
        assert_eq!(named(&result), ["def/a", "def/a/ok.sii"]);
        for path in package.asked.borrow().iter() {
            assert_eq!(
                paths::clean(path),
                Some(path.as_str()),
                "looked up {path:?}"
            );
        }
    }

    #[test]
    fn the_candidate_budget_counts_paths_still_waiting() {
        let limits = Limits {
            candidates: 10,
            ..LIMITS
        };
        let mut search = Search::new(limits);
        search.extend((0..50).map(|i| format!("r/{i}.sii")));
        assert_eq!(search.queue.len(), 10);
        // Repeats cost nothing.
        let mut search = Search::new(limits);
        search.extend(std::iter::repeat_n("r/same.sii", 50));
        assert_eq!(search.queue.len(), 1);
        // The total length is bounded too.
        let mut search = Search::new(Limits {
            candidate_bytes: 100,
            ..LIMITS
        });
        search.extend((0..50).map(|i| format!("r/{i:08}.sii")));
        assert_eq!(search.queue.len(), 100 / "r/00000000.sii".len());
    }

    #[test]
    fn stops_scanning_at_the_byte_budget() {
        // 20 files nothing names, 100 bytes each, 450 bytes of budget.
        let mut package = Fake::default();
        for i in 0..20 {
            package
                .files
                .insert(format!("hidden/{i}.sii"), vec![b'x'; 100]);
        }
        let limits = Limits {
            scan_bytes: 450,
            ..LIMITS
        };
        let result = recover_with(&package, Vec::new(), limits, &WordSwaps::default());
        assert_eq!(result.unnamed.len(), 20);
        assert!(
            package.bytes_read.get() <= 450,
            "{}",
            package.bytes_read.get()
        );
    }
}
