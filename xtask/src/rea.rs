//! `research/rea/*.txt`: the executable searches behind REA evidence. A
//! batch's `search` and `trace` lines are seeds, strings that find the code
//! again in a new build; its `function` and `xrefs` lines are the addresses
//! they led to in the build the evidence names. `cargo xtask rea` prints one
//! batched run of every seed, since each REA run pays a long Ghidra analysis.

use std::collections::BTreeMap;
use std::path::Path;

/// Where the batches live, relative to the repository root.
pub const DIR: &str = "research/rea";

/// One line of a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// A regex string search.
    Search {
        pattern: String,
        case_sensitive: bool,
    },
    /// A trace from one string to the code that uses it.
    Trace { query: String, case_sensitive: bool },
    /// A function analysed at an address of the old build.
    Function(String),
    /// Cross-references to an address of the old build.
    Xrefs(String),
}

/// Parses a batch; `#` starts a comment line.
pub fn parse(text: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let (case_sensitive, rest) = match rest.strip_prefix("case-sensitive ") {
            Some(rest) => (true, rest),
            None => (false, rest),
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(format!("line {}: `{word}` needs a value", index + 1));
        }
        let address = || {
            let hex = rest.strip_prefix("0x").unwrap_or("");
            if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                Ok(rest.to_owned())
            } else {
                Err(format!("line {}: `{rest}` is not an 0x address", index + 1))
            }
        };
        steps.push(match word {
            "search" => Step::Search {
                pattern: rest.to_owned(),
                case_sensitive,
            },
            "trace" => Step::Trace {
                query: rest.to_owned(),
                case_sensitive,
            },
            "function" => Step::Function(address()?),
            "xrefs" => Step::Xrefs(address()?),
            other => {
                return Err(format!(
                    "line {}: unknown step `{other}` (search, trace, function, xrefs)",
                    index + 1
                ))
            }
        });
    }
    Ok(steps)
}

/// The commands of one batched run over every batch: the case-insensitive
/// searches joined into one regex, the other searches and every trace on
/// their own. Arguments are quoted for a POSIX shell.
pub fn commands(batches: &BTreeMap<String, Vec<Step>>, exe: &str) -> Vec<String> {
    let common = "--provider ghidra --snapshot \"$S/eurotrucks2.snapshot\" --format json";
    let mut joined = Vec::new();
    let mut out = Vec::new();
    for (name, steps) in batches {
        let stem = name.trim_end_matches(".txt");
        for (i, step) in steps.iter().enumerate() {
            match step {
                Step::Search {
                    pattern,
                    case_sensitive: false,
                } => {
                    let bare = pattern.strip_prefix("(?i)").unwrap_or(pattern);
                    if !joined.contains(&bare) {
                        joined.push(bare);
                    }
                }
                Step::Search {
                    pattern,
                    case_sensitive: true,
                } => out.push(format!(
                    "npx -y rea-agents@6.4.0 search {} {} --mode regex {common} > \"$S/port-{stem}-{i}.json\"",
                    quote(exe),
                    quote(&format!("--pattern={pattern}"))
                )),
                Step::Trace {
                    query,
                    case_sensitive,
                } => out.push(format!(
                    "npx -y rea-agents@6.4.0 trace {} {}{} {common} > \"$S/port-{stem}-{i}.json\"",
                    quote(exe),
                    quote(&format!("--query={query}")),
                    if *case_sensitive { " --case-sensitive" } else { "" }
                )),
                Step::Function(_) | Step::Xrefs(_) => {}
            }
        }
    }
    if !joined.is_empty() {
        let pattern = format!("(?i)({})", joined.join("|"));
        out.insert(
            0,
            format!(
                "npx -y rea-agents@6.4.0 search {} {} --mode regex {common} > \"$S/port-search.json\"",
                quote(exe),
                quote(&format!("--pattern={pattern}"))
            ),
        );
    }
    out
}

/// `text` in single quotes for a POSIX shell.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Every batch (`*.txt`) in `research/rea/`, parsed.
pub fn read(root: &Path) -> Result<BTreeMap<String, Vec<Step>>, String> {
    let mut batches = BTreeMap::new();
    let dir = root.join(DIR);
    if !dir.is_dir() {
        return Ok(batches);
    }
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{DIR}: {e}"))?;
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        // Batches are `.txt`; REA's own output files may sit beside them.
        if !name.ends_with(".txt") || name.ends_with(".stderr.txt") {
            continue;
        }
        let text =
            std::fs::read_to_string(entry.path()).map_err(|e| format!("{DIR}/{name}: {e}"))?;
        let steps = parse(&text).map_err(|e| format!("{DIR}/{name}: {e}"))?;
        batches.insert(name, steps);
    }
    Ok(batches)
}

/// `cargo xtask rea`: prints the batched run and the old addresses.
pub fn print(root: &Path) -> Result<(), String> {
    let batches = read(root)?;
    let exe = std::env::var("ETS2_DIR").map_or_else(
        |_| "<game>/bin/win_x64/eurotrucks2.exe".to_owned(),
        |game| format!("{}/bin/win_x64/eurotrucks2.exe", game.replace('\\', "/")),
    );
    println!("xtask: one REA run over every seed in {DIR}/ (Git Bash; S = %USERPROFILE%/rea-ets2,");
    println!("  environment as in CLAUDE.md; a new build needs a new snapshot, about 30 min):");
    for command in commands(&batches, &exe) {
        println!("  {command}");
    }
    println!("xtask: then re-locate the old addresses from what the seeds find:");
    for (name, steps) in &batches {
        let old: Vec<String> = steps
            .iter()
            .filter_map(|s| match s {
                Step::Function(a) => Some(format!("function {a}")),
                Step::Xrefs(a) => Some(format!("xrefs {a}")),
                _ => None,
            })
            .collect();
        if !old.is_empty() {
            println!("  {name}: {}", old.join(", "));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_step_and_rejects_bad_lines() {
        let steps = parse(
            "# comment\nsearch (?i)(a|b)\ntrace case-sensitive pin_offset\nfunction 0x14082b610\nxrefs 0x1400\n",
        )
        .expect("parses");
        assert_eq!(
            steps,
            [
                Step::Search {
                    pattern: "(?i)(a|b)".into(),
                    case_sensitive: false
                },
                Step::Trace {
                    query: "pin_offset".into(),
                    case_sensitive: true
                },
                Step::Function("0x14082b610".into()),
                Step::Xrefs("0x1400".into()),
            ]
        );
        assert!(parse("serch x\n").expect_err("unknown").contains("line 1"));
        assert!(parse("function FUN_1\n").is_err());
        assert!(parse("trace\n").is_err());
    }

    #[test]
    fn one_run_joins_the_searches_and_keeps_each_trace() {
        let batches = BTreeMap::from([
            (
                "a.txt".to_owned(),
                parse("search (?i)(x|y)\ntrace it's\nfunction 0x1\n").expect("a"),
            ),
            (
                "b.txt".to_owned(),
                parse("search z\nsearch (?i)(x|y)\n").expect("b"),
            ),
        ]);
        let commands = commands(&batches, "D:/g/eurotrucks2.exe");
        assert_eq!(commands.len(), 2, "{commands:?}");
        assert!(
            commands[0].contains("'--pattern=(?i)((x|y)|z)'"),
            "{}",
            commands[0]
        );
        assert!(
            commands[1].contains("'--query=it'\\''s'"),
            "{}",
            commands[1]
        );
        assert!(!commands[1].contains("--case-sensitive"));
    }
}
