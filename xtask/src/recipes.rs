//! `research/recipes.txt`: how each file in `research/evidence/` is made
//! again after a game update (docs/version-ports.md). Without the game,
//! [`check`] proves that every evidence file has a recipe, that every
//! command names a tool that exists, and that every compared recipe has a
//! baseline in `research/baseline/` matching its commands. With the game,
//! [`run`] runs the recipes and saves or compares their output.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The recipe list, relative to the repository root.
pub const RECIPES: &str = "research/recipes.txt";
/// The evidence files the recipes cover.
const EVIDENCE: &str = "research/evidence";
/// Saved outputs of the `game` and `digest` recipes.
const BASELINE: &str = "research/baseline";
/// Where the research examples live.
const EXAMPLES: &str = "crates/specsheet-research/examples";
/// Where the REA search batches live.
const REA: &str = crate::rea::DIR;

/// What a recipe line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Output depends on the install only; compared with the baseline.
    Game(Vec<String>),
    /// Output is game file content; only its digest is kept.
    Digest(Vec<String>),
    /// Reads the user's mod folder or profiles; run, never compared.
    User(Vec<String>),
    /// An REA search batch under `research/rea/`, or `None` while not saved.
    Rea(Option<String>),
    /// Rechecked by hand; the text says what to look at.
    Hand(String),
    /// Cannot be made again; the text says why.
    Historical(String),
}

/// One line of the recipe list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    /// 1-based line in the recipe list, for messages.
    pub line: usize,
    /// The evidence file, from the repository root, e.g.
    /// `research/evidence/axle-survey.txt`.
    pub evidence: String,
    pub kind: Kind,
    /// The command as written, for baseline section headers.
    pub text: String,
}

impl Recipe {
    /// The command of a recipe that runs one.
    pub fn command(&self) -> Option<&[String]> {
        match &self.kind {
            Kind::Game(c) | Kind::Digest(c) | Kind::User(c) => Some(c),
            _ => None,
        }
    }

    /// Whether the output is kept in a baseline.
    fn compared(&self) -> bool {
        matches!(self.kind, Kind::Game(_) | Kind::Digest(_))
    }
}

/// Parses the recipe list. `#` starts a comment line.
pub fn parse(text: &str) -> Result<Vec<Recipe>, Vec<String>> {
    let mut recipes = Vec::new();
    let mut problems = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.splitn(3, char::is_whitespace);
        let evidence = parts.next().unwrap_or_default().to_owned();
        let kind = parts.next().unwrap_or_default();
        let rest = parts.next().unwrap_or_default().trim().to_owned();
        if rest.is_empty() {
            problems.push(format!(
                "{RECIPES}:{line}: `{kind}` needs a command or a text"
            ));
            continue;
        }
        let command = || split_args(&rest);
        let kind = match kind {
            "game" => command().map(Kind::Game),
            "digest" => command().map(Kind::Digest),
            "user" => command().map(Kind::User),
            "rea" => Ok(Kind::Rea((rest != "-").then(|| rest.clone()))),
            "hand" => Ok(Kind::Hand(rest.clone())),
            "historical" => Ok(Kind::Historical(rest.clone())),
            other => Err(format!(
                "unknown kind `{other}` (game, digest, user, rea, hand, historical)"
            )),
        };
        match kind {
            Ok(kind) => recipes.push(Recipe {
                line,
                evidence,
                kind,
                text: rest,
            }),
            Err(e) => problems.push(format!("{RECIPES}:{line}: {e}")),
        }
    }
    if problems.is_empty() {
        Ok(recipes)
    } else {
        Err(problems)
    }
}

/// Splits a command on spaces; `"double quotes"` keep spaces in one
/// argument.
fn split_args(text: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    args.push(std::mem::take(&mut current));
                    any = false;
                }
            }
            c => {
                current.push(c);
                any = true;
            }
        }
    }
    if quoted {
        return Err(format!("unclosed quote in `{text}`"));
    }
    if any {
        args.push(current);
    }
    Ok(args)
}

/// What [`check`] looks at, gathered from the tree so the rules can be
/// tested without one.
#[derive(Debug, Default)]
pub struct Tree {
    /// Files under `research/evidence/`, from the repository root.
    pub evidence: Vec<String>,
    /// Example names in `crates/specsheet-research/examples/`.
    pub examples: Vec<String>,
    /// Files under `tools/research/`, as `tools/research/<name>`.
    pub scripts: Vec<String>,
    /// File names in `research/rea/`, with the problem of each that does not
    /// parse.
    pub rea: BTreeMap<String, Option<String>>,
    /// Baseline path under `research/baseline/` -> its contents.
    pub baselines: BTreeMap<String, String>,
}

impl Tree {
    pub fn read(root: &Path) -> Result<Self, String> {
        let names = |dir: &str, strip: &str| -> Result<Vec<String>, String> {
            let path = root.join(dir);
            if !path.is_dir() {
                return Ok(Vec::new());
            }
            let mut names: Vec<String> = std::fs::read_dir(&path)
                .map_err(|e| format!("{dir}: {e}"))?
                .flatten()
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .map(|n| n.strip_suffix(strip).map(str::to_owned).unwrap_or(n))
                .collect();
            names.sort();
            Ok(names)
        };
        let mut baselines = BTreeMap::new();
        for name in files_under(&root.join(BASELINE))? {
            let path = root.join(BASELINE).join(&name);
            let text =
                std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            baselines.insert(name, text);
        }
        Ok(Self {
            evidence: files_under(&root.join(EVIDENCE))?
                .into_iter()
                .map(|n| format!("{EVIDENCE}/{n}"))
                .collect(),
            examples: names(EXAMPLES, ".rs")?,
            scripts: names("tools/research", "")?
                .into_iter()
                .map(|n| format!("tools/research/{n}"))
                .collect(),
            rea: names(REA, "")?
                .into_iter()
                .map(|name| {
                    let text = std::fs::read_to_string(root.join(REA).join(&name));
                    let problem = match text {
                        Ok(text) => crate::rea::parse(&text).err(),
                        Err(e) => Some(e.to_string()),
                    };
                    (name, problem)
                })
                .collect(),
            baselines,
        })
    }
}

/// Every file under `dir`, as `/`-separated paths relative to it, sorted.
/// Names are taken as `read_dir` lists them, so case is exact.
fn files_under(dir: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((path, prefix)) = stack.pop() {
        if !path.is_dir() {
            continue;
        }
        let entries = std::fs::read_dir(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for entry in entries.flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let relative = format!("{prefix}{name}");
            if entry.path().is_dir() {
                stack.push((entry.path(), format!("{relative}/")));
            } else {
                out.push(relative);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// The baseline of an evidence file: the same path under
/// `research/baseline/`, with `.txt` for its extension.
pub fn baseline_name(evidence: &str) -> String {
    let relative = evidence
        .strip_prefix(EVIDENCE)
        .map_or(evidence, |r| r.trim_start_matches('/'));
    let (folder, name) = relative
        .rsplit_once('/')
        .map_or(("", relative), |(f, n)| (f, n));
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    if folder.is_empty() {
        format!("{stem}.txt")
    } else {
        format!("{folder}/{stem}.txt")
    }
}

/// The section headers of a baseline, in order: the command of each
/// compared recipe.
fn sections(baseline: &str) -> Vec<&str> {
    baseline
        .lines()
        .filter_map(|l| l.strip_prefix(SECTION))
        .collect()
}

/// Starts a baseline section; the command follows.
const SECTION: &str = "## ";

/// Every problem with the recipes against `tree`; empty when all hold.
pub fn problems(recipes: &[Recipe], tree: &Tree) -> Vec<String> {
    let mut problems = Vec::new();
    let mut by_file: BTreeMap<&str, Vec<&Recipe>> = BTreeMap::new();
    for recipe in recipes {
        by_file.entry(&recipe.evidence).or_default().push(recipe);
        let at = format!("{RECIPES}:{}", recipe.line);
        if !tree.evidence.contains(&recipe.evidence) {
            problems.push(format!("{at}: no file {}", recipe.evidence));
        }
        if let Some(command) = recipe.command() {
            if let Some(problem) = tool_problem(command, tree) {
                problems.push(format!("{at}: {problem}"));
            }
        }
        if let Kind::Rea(Some(file)) = &recipe.kind {
            match tree.rea.get(file) {
                None => problems.push(format!("{at}: no file {REA}/{file}")),
                Some(Some(problem)) => problems.push(format!("{REA}/{file}: {problem}")),
                Some(None) => {}
            }
        }
    }
    for file in &tree.evidence {
        if !by_file.contains_key(file.as_str()) {
            problems.push(format!(
                "{file}: no recipe in {RECIPES} (add one: game, digest, user, rea, hand or historical)"
            ));
        }
    }
    let mut expected: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (file, recipes) in &by_file {
        let compared: Vec<&str> = recipes
            .iter()
            .filter(|r| r.compared())
            .map(|r| r.text.as_str())
            .collect();
        if !compared.is_empty() {
            expected.insert(baseline_name(file), compared);
        }
    }
    for (name, commands) in &expected {
        match tree.baselines.get(name) {
            None => problems.push(format!(
                "{BASELINE}/{name}: missing; run `cargo xtask recipes --save` with ETS2_DIR set"
            )),
            Some(text) if sections(text) != *commands => problems.push(format!(
                "{BASELINE}/{name}: its commands differ from {RECIPES}; run `cargo xtask recipes --save`"
            )),
            Some(_) => {}
        }
    }
    for name in tree.baselines.keys() {
        if !expected.contains_key(name) {
            problems.push(format!(
                "{BASELINE}/{name}: no game or digest recipe uses it"
            ));
        }
    }
    problems
}

/// Why `command` cannot run, if it cannot.
fn tool_problem(command: &[String], tree: &Tree) -> Option<String> {
    let tool = command.first()?;
    if tool == "python" || tool == "bash" {
        let script = command.iter().skip(1).find(|a| !a.starts_with('-'));
        return match script {
            Some(s) if tree.scripts.contains(s) => None,
            Some(s) => Some(format!("no script {s}")),
            None => Some(format!("`{tool}` without a script")),
        };
    }
    (!tree.examples.contains(tool)).then(|| format!("no research example `{tool}` in {EXAMPLES}"))
}

/// `cargo xtask recipes`: the check without the game.
pub fn check(root: &Path) -> Result<(), String> {
    let text =
        std::fs::read_to_string(root.join(RECIPES)).map_err(|e| format!("{RECIPES}: {e}"))?;
    let recipes =
        parse(&text).map_err(|p| format!("recipe check failed:\n  {}", p.join("\n  ")))?;
    let tree = Tree::read(root)?;
    let problems = problems(&recipes, &tree);
    if !problems.is_empty() {
        return Err(format!("recipe check failed:\n  {}", problems.join("\n  ")));
    }
    let count = |f: fn(&Kind) -> bool| recipes.iter().filter(|r| f(&r.kind)).count();
    println!(
        "xtask: recipes ok: {} evidence files, {} recipes ({} game, {} digest, {} user, {} rea of which {} not saved, {} hand, {} historical)",
        tree.evidence.len(),
        recipes.len(),
        count(|k| matches!(k, Kind::Game(_))),
        count(|k| matches!(k, Kind::Digest(_))),
        count(|k| matches!(k, Kind::User(_))),
        count(|k| matches!(k, Kind::Rea(_))),
        count(|k| matches!(k, Kind::Rea(None))),
        count(|k| matches!(k, Kind::Hand(_))),
        count(|k| matches!(k, Kind::Historical(_))),
    );
    Ok(())
}

/// Where a run's paths come from.
pub struct Places {
    pub game: PathBuf,
    /// The ETS2 home folder, for `user` recipes.
    pub home: Option<PathBuf>,
    /// A fresh folder the run may write to and deletes afterwards.
    pub scratch: PathBuf,
    /// The built examples (`<target>/release/examples`).
    pub examples: PathBuf,
}

/// The arguments with `<game>`, `<home>` and `<scratch>` filled in, and a
/// `*` in the file name (`<game>/*.scs`) expanded to the matching files of
/// that folder, sorted.
pub fn expand(args: &[String], places: &Places) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for arg in args {
        let mut value = arg.replace("<game>", &slashed(&places.game));
        value = value.replace("<scratch>", &slashed(&places.scratch));
        if value.contains("<home>") {
            let home = places
                .home
                .as_ref()
                .ok_or("this recipe needs ETS2_HOME (the ETS2 home folder)")?;
            value = value.replace("<home>", &slashed(home));
        }
        let (dir, name) = value.rsplit_once('/').unwrap_or((".", value.as_str()));
        match name.split_once('*') {
            Some((start, end)) => {
                let mut matches: Vec<String> = std::fs::read_dir(dir)
                    .map_err(|e| format!("{dir}: {e}"))?
                    .flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| n.starts_with(start) && n.ends_with(end))
                    .map(|n| format!("{dir}/{n}"))
                    .collect();
                matches.sort();
                if matches.is_empty() {
                    return Err(format!("`{arg}` matches no file"));
                }
                out.extend(matches);
            }
            None => out.push(value),
        }
    }
    Ok(out)
}

/// A path with forward slashes, as the commands and baselines write it.
fn slashed(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Output made comparable: no carriage returns, and the run's own folders
/// written as `<game>`, `<home>` and `<scratch>`.
pub fn normalize(output: &str, places: &Places) -> String {
    let mut text = output.replace("\r\n", "\n");
    let mut swaps = vec![
        (places.scratch.clone(), "<scratch>"),
        (places.game.clone(), "<game>"),
    ];
    if let Some(home) = &places.home {
        swaps.push((home.clone(), "<home>"));
    }
    for (path, token) in swaps {
        let forward = slashed(&path);
        let back = forward.replace('/', "\\");
        text = text.replace(&forward, token).replace(&back, token);
    }
    text
}

/// FNV-1a, 64 bits: enough to notice that a game file changed.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Runs one recipe's command and returns what its baseline section holds.
pub fn output(recipe: &Recipe, root: &Path, places: &Places) -> Result<String, String> {
    let command = recipe.command().ok_or("not a command")?;
    let args = expand(command, places)?;
    let (program, rest) = args.split_first().ok_or("empty command")?;
    let mut process = if program == "python" || program == "bash" {
        let mut c = Command::new(program);
        if program == "python" {
            c.arg("-I");
        }
        c.args(rest);
        c
    } else {
        let exe = places
            .examples
            .join(format!("{program}{}", std::env::consts::EXE_SUFFIX));
        let mut c = Command::new(exe);
        c.args(rest);
        c
    };
    let out = process
        .current_dir(root)
        .env("ETS2_DIR", &places.game)
        .env("MSYS_NO_PATHCONV", "1")
        .output()
        .map_err(|e| format!("{program}: {e}"))?;
    let mut body = match &recipe.kind {
        Kind::Digest(_) => format!(
            "fnv1a64 {:016x}, {} bytes\n",
            fnv1a64(&out.stdout),
            out.stdout.len()
        ),
        _ => normalize(&String::from_utf8_lossy(&out.stdout), places),
    };
    if !body.ends_with('\n') {
        body.push('\n');
    }
    if !out.status.success() {
        body.push_str(&format!(
            "(exit status {})\n",
            out.status.code().unwrap_or(-1)
        ));
    }
    Ok(body)
}

/// A baseline file: a header, then one section per compared recipe.
pub fn baseline_text(evidence: &str, version: &str, sections: &[(&str, String)]) -> String {
    let mut text = format!(
        "# Baseline of {evidence}, ETS2 {version}: the output of its\n\
         # game and digest recipes in {RECIPES}. Made by `cargo xtask recipes --save`;\n\
         # `cargo xtask recipes --compare` runs them again and compares.\n"
    );
    for (command, body) in sections {
        text.push_str(SECTION);
        text.push_str(command);
        text.push('\n');
        text.push_str(body);
    }
    text
}

/// The section bodies of a baseline by command, without its header.
pub fn baseline_sections(text: &str) -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    for line in text.split_inclusive('\n') {
        if let Some(command) = line.strip_prefix(SECTION) {
            sections.push((command.trim_end().to_owned(), String::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push_str(line);
        }
    }
    sections
}

/// `cargo xtask recipes --save | --compare [--user]`: builds the research
/// examples, runs every `game` and `digest` recipe (and with `--user`, the
/// `user` ones), and writes the baselines or compares with them. A
/// comparison keeps the new outputs in `<target>/recipes/` and fails when any
/// differ.
pub fn run(root: &Path, save: bool, with_user: bool) -> Result<(), String> {
    let recipes = check_list(root)?;
    let game = std::env::var_os("ETS2_DIR")
        .map(PathBuf::from)
        .ok_or("this task needs ETS2_DIR (the game directory)")?;
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    crate::run(
        root,
        "cargo",
        &[
            "build",
            "--release",
            "-p",
            "specsheet-research",
            "--examples",
        ],
    )?;
    // Outside the repository: mod_extract refuses destinations inside it.
    let scratch = std::env::temp_dir().join(format!("specsheet-recipes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|e| format!("{}: {e}", scratch.display()))?;
    let places = Places {
        game,
        home: std::env::var_os("ETS2_HOME").map(PathBuf::from),
        scratch,
        examples: target.join("release").join("examples"),
    };
    let version = game_version(root, &places);
    let result = run_all(root, &recipes, &places, &target, &version, save, with_user);
    let _ = std::fs::remove_dir_all(&places.scratch);
    result
}

/// The recipes, after the check without the game passes (baselines may be
/// missing when saving).
fn check_list(root: &Path) -> Result<Vec<Recipe>, String> {
    let text =
        std::fs::read_to_string(root.join(RECIPES)).map_err(|e| format!("{RECIPES}: {e}"))?;
    let recipes =
        parse(&text).map_err(|p| format!("recipe check failed:\n  {}", p.join("\n  ")))?;
    let tree = Tree::read(root)?;
    let problems: Vec<String> = problems(&recipes, &tree)
        .into_iter()
        .filter(|p| !p.starts_with(BASELINE))
        .collect();
    if !problems.is_empty() {
        return Err(format!("recipe check failed:\n  {}", problems.join("\n  ")));
    }
    Ok(recipes)
}

/// The installed version (`scs version`), as the baselines record it.
fn game_version(root: &Path, places: &Places) -> String {
    let recipe = Recipe {
        line: 0,
        evidence: String::new(),
        kind: Kind::Game(vec!["scs".into(), "version".into()]),
        text: String::new(),
    };
    output(&recipe, root, places)
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|v| !v.is_empty() && !v.contains("exit status"))
        .unwrap_or_else(|| "(version unknown)".to_owned())
}

fn run_all(
    root: &Path,
    recipes: &[Recipe],
    places: &Places,
    target: &Path,
    version: &str,
    save: bool,
    with_user: bool,
) -> Result<(), String> {
    let mut by_file: BTreeMap<&str, Vec<&Recipe>> = BTreeMap::new();
    for recipe in recipes {
        by_file.entry(&recipe.evidence).or_default().push(recipe);
    }
    let fresh = target.join("recipes");
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh).map_err(|e| format!("{}: {e}", fresh.display()))?;
    let mut differ = Vec::new();
    let mut failed = Vec::new();
    for (file, recipes) in &by_file {
        let mut sections = Vec::new();
        for recipe in recipes {
            let compared = recipe.compared();
            let user = with_user && matches!(recipe.kind, Kind::User(_));
            if !(compared || user) {
                continue;
            }
            let started = std::time::Instant::now();
            let body = output(recipe, root, places).map_err(|e| format!("{file}: {e}"))?;
            let seconds = started.elapsed().as_secs_f32();
            if body.contains("\n(exit status ") || body.starts_with("(exit status ") {
                failed.push(format!("{file}: `{}` failed", recipe.text));
            }
            if compared {
                sections.push((recipe.text.as_str(), body));
            } else {
                println!("xtask: ran {file}: {} ({seconds:.0} s)", recipe.text);
            }
        }
        if sections.is_empty() {
            continue;
        }
        let name = baseline_name(file);
        let text = baseline_text(file, version, &sections);
        let saved = root.join(BASELINE).join(&name);
        if save {
            if let Some(dir) = saved.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            std::fs::write(&saved, &text).map_err(|e| format!("{}: {e}", saved.display()))?;
            println!("xtask: saved {BASELINE}/{name}");
            continue;
        }
        let new = fresh.join(&name);
        if let Some(dir) = new.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&new, &text).map_err(|e| format!("{name}: {e}"))?;
        let old = std::fs::read_to_string(&saved).map_err(|e| format!("{BASELINE}/{name}: {e}"))?;
        let old = baseline_sections(&old);
        for (command, body) in &sections {
            let before = old
                .iter()
                .find(|(c, _)| c == command)
                .map(|(_, b)| b.as_str());
            match before {
                Some(before) if before == body => {}
                Some(before) => differ.push(format!(
                    "{file}: `{command}`: {} lines differ",
                    changed_lines(before, body)
                )),
                None => differ.push(format!("{file}: `{command}`: not in the baseline")),
            }
        }
    }
    for recipe in recipes {
        match &recipe.kind {
            Kind::Hand(text) => println!("xtask: by hand, {}: {text}", recipe.evidence),
            Kind::Rea(None) => println!(
                "xtask: by hand, {}: REA results whose search is not saved in {REA}/",
                recipe.evidence
            ),
            _ => {}
        }
    }
    if save {
        return if failed.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "saved, but some commands failed:\n  {}",
                failed.join("\n  ")
            ))
        };
    }
    println!("xtask: new outputs in {}", fresh.display());
    let problems: Vec<String> = failed.into_iter().chain(differ).collect();
    if problems.is_empty() {
        println!("xtask: every game and digest recipe matches its baseline (ETS2 {version})");
        Ok(())
    } else {
        Err(format!(
            "{} recipe outputs differ from research/baseline/ (ETS2 {version}):\n  {}",
            problems.len(),
            problems.join("\n  ")
        ))
    }
}

/// How many lines differ between two outputs, by line multiset.
pub fn changed_lines(old: &str, new: &str) -> usize {
    let mut count: BTreeMap<&str, i64> = BTreeMap::new();
    for l in old.lines() {
        *count.entry(l).or_default() += 1;
    }
    for l in new.lines() {
        *count.entry(l).or_default() -= 1;
    }
    count.values().map(|v| v.unsigned_abs() as usize).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Tree {
        Tree {
            evidence: vec![
                "research/evidence/a.txt".into(),
                "research/evidence/b.txt".into(),
                "research/evidence/pins/c.md".into(),
            ],
            examples: vec!["axle_survey".into(), "scs".into()],
            scripts: vec!["tools/research/attr_tables.py".into()],
            rea: BTreeMap::from([("pins.txt".to_owned(), None)]),
            baselines: BTreeMap::from([(
                "a.txt".to_owned(),
                "# header\n## axle_survey\nx\n## scs cat /def/a.sii\nfnv1a64 1, 2 bytes\n"
                    .to_owned(),
            )]),
        }
    }

    const GOOD: &str = "# comment\n\
        research/evidence/a.txt game axle_survey\n\
        research/evidence/a.txt digest scs cat /def/a.sii\n\
        research/evidence/b.txt rea pins.txt\n\
        research/evidence/b.txt user python tools/research/attr_tables.py \"<home>/a b\" -x\n\
        research/evidence/pins/c.md hand compare the cockpit locators with the table\n";

    #[test]
    fn a_complete_list_passes() {
        let recipes = parse(GOOD).expect("parses");
        assert_eq!(recipes.len(), 5);
        assert_eq!(
            recipes[3].command().expect("a command"),
            [
                "python",
                "tools/research/attr_tables.py",
                "<home>/a b",
                "-x"
            ]
        );
        assert_eq!(problems(&recipes, &tree()), Vec::<String>::new());
    }

    #[test]
    fn bad_lines_are_reported_with_their_line() {
        let problems = parse("a.txt gmae axle_survey\nb.txt hand\nc.md user scs \"open\n")
            .expect_err("three bad lines");
        assert_eq!(problems.len(), 3);
        assert!(
            problems[0].contains(":1: unknown kind `gmae`"),
            "{problems:?}"
        );
        assert!(problems[1].contains(":2:"), "{problems:?}");
        assert!(problems[2].contains("unclosed quote"), "{problems:?}");
    }

    #[test]
    fn a_missing_recipe_tool_script_or_pattern_fails() {
        let mut text = GOOD.replace(
            "research/evidence/pins/c.md hand compare the cockpit locators with the table\n",
            "",
        );
        text.push_str(
            "research/evidence/a.txt game axle_srvey\n\
             research/evidence/b.txt rea gone.txt\n\
             research/evidence/b.txt user python tools/research/gone.py\n\
             research/evidence/d.txt hand x\n",
        );
        let recipes = parse(&text).expect("parses");
        let problems = problems(&recipes, &tree());
        let has = |needle: &str| problems.iter().any(|p| p.contains(needle));
        assert!(
            has("research/evidence/pins/c.md: no recipe"),
            "{problems:?}"
        );
        assert!(has("no research example `axle_srvey`"), "{problems:?}");
        assert!(has("no file research/rea/gone.txt"), "{problems:?}");
        assert!(has("no script tools/research/gone.py"), "{problems:?}");
        assert!(has("no file research/evidence/d.txt"), "{problems:?}");
    }

    #[test]
    fn an_rea_batch_that_does_not_parse_fails() {
        let recipes = parse(GOOD).expect("parses");
        let mut tree = tree();
        tree.rea.insert(
            "pins.txt".into(),
            Some("line 2: unknown step `serch`".into()),
        );
        assert_eq!(
            problems(&recipes, &tree),
            ["research/rea/pins.txt: line 2: unknown step `serch`"]
        );
    }

    #[test]
    fn a_baseline_must_exist_and_match_the_commands() {
        let recipes = parse(GOOD).expect("parses");
        let mut tree = tree();
        tree.baselines.clear();
        assert!(problems(&recipes, &tree)[0].contains("research/baseline/a.txt: missing"));

        tree.baselines
            .insert("a.txt".into(), "## axle_survey\nx\n".into());
        assert!(problems(&recipes, &tree)[0].contains("commands differ"));

        tree.baselines = self::tree().baselines;
        tree.baselines.insert("b.txt".into(), "## x\n".into());
        assert!(problems(&recipes, &tree)[0]
            .contains("research/baseline/b.txt: no game or digest recipe"));
    }

    #[test]
    fn a_baseline_mirrors_the_evidence_path() {
        assert_eq!(
            baseline_name("research/evidence/axle-survey.txt"),
            "axle-survey.txt"
        );
        assert_eq!(baseline_name("research/evidence/pins/c.md"), "pins/c.txt");
        assert_eq!(baseline_name("research/evidence/rea-x.json"), "rea-x.txt");
    }

    #[test]
    fn evidence_is_found_in_area_folders() {
        let dir = std::env::temp_dir().join(format!("xtask-under-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("pins").join("deep")).expect("temp dirs");
        for name in ["b.txt", "pins/A.txt", "pins/deep/c.json"] {
            std::fs::write(dir.join(name), b"").expect("write");
        }
        assert_eq!(
            files_under(&dir).expect("lists"),
            ["b.txt", "pins/A.txt", "pins/deep/c.json"]
        );
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn output_is_normalized_to_placeholders() {
        let places = Places {
            game: PathBuf::from("D:/Steam/ETS2"),
            home: Some(PathBuf::from("D:/SCS/ETS2")),
            scratch: PathBuf::from("C:/Temp/run"),
            examples: PathBuf::new(),
        };
        let out = "D:/Steam/ETS2/def.scs: 3\r\nD:\\SCS\\ETS2\\mod\\a.scs\r\nC:/Temp/run/x\r\n";
        assert_eq!(
            normalize(out, &places),
            "<game>/def.scs: 3\n<home>\\mod\\a.scs\n<scratch>/x\n"
        );
        let args = expand(&["<game>/a.scs".into(), "--x".into()], &places).expect("expands");
        assert_eq!(args, ["D:/Steam/ETS2/a.scs", "--x"]);
    }

    #[test]
    fn a_star_in_the_file_name_expands_to_sorted_files() {
        let dir = std::env::temp_dir().join(format!("xtask-recipes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        for name in ["b.scs", "a.scs", "c.txt"] {
            std::fs::write(dir.join(name), b"").expect("write");
        }
        let places = Places {
            game: dir.clone(),
            home: None,
            scratch: PathBuf::new(),
            examples: PathBuf::new(),
        };
        let args = expand(&["<game>/*.scs".into()], &places).expect("expands");
        let game = slashed(&dir);
        assert_eq!(args, [format!("{game}/a.scs"), format!("{game}/b.scs")]);
        assert!(expand(&["<home>/x".into()], &places).is_err());
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn baselines_round_trip_and_changes_are_counted() {
        let text = baseline_text(
            "a.txt",
            "1.60.1.7",
            &[
                ("axle_survey", "x\ny\n".into()),
                ("scs cat /a", "fnv1a64 1, 2 bytes\n".into()),
            ],
        );
        assert_eq!(sections(&text), ["axle_survey", "scs cat /a"]);
        assert_eq!(
            baseline_sections(&text),
            [
                ("axle_survey".to_owned(), "x\ny\n".to_owned()),
                ("scs cat /a".to_owned(), "fnv1a64 1, 2 bytes\n".to_owned())
            ]
        );
        assert_eq!(changed_lines("a\nb\nc\n", "a\nc\nd\n"), 2);
        assert_eq!(changed_lines("a\n", "a\n"), 0);
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(fnv1a64(b"a"), fnv1a64(b"b"));
    }
}
