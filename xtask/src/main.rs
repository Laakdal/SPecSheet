//! Repository tasks, run as `cargo xtask <task>` (alias in `.cargo/config.toml`):
//! * `check`: the "Done means" checks of AGENTS.md: the game-file guard,
//!   format, clippy, tests and the layer check; with `--real`, also the
//!   real-install tests (needs `ETS2_DIR`).
//! * `layers`: fails when a crate depends on a crate at its own layer or
//!   above, or when a crate outside the top layers uses Bevy (D17).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Each workspace crate's layer. A crate may depend only on crates in lower
/// layers (`docs/ARCHITECTURE.md`, D17). A crate missing here fails the
/// check, so a new crate has to be placed deliberately.
const LAYERS: &[(&str, u8)] = &[
    ("specsheet-core", 0),
    ("specsheet-archive", 0),
    ("specsheet-theme", 0),
    ("specsheet-formats", 1),
    ("specsheet-truck", 1),
    ("specsheet-data", 2),
    ("specsheet-gameui", 3),
    ("specsheet-assembly", 3),
    ("specsheet-session", 4),
    ("specsheet-render", 5),
    ("specsheet-research", 8),
    ("specsheet", 9),
    ("xtask", 0),
];

/// Only these crates may depend on Bevy, so a Bevy upgrade never touches
/// the game logic.
const BEVY_CRATES: &[&str] = &["specsheet", "specsheet-render"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = workspace_root();
    let result = match args.first().map(String::as_str) {
        Some("layers") => layers(&root),
        Some("check") => check(&root, args.iter().any(|a| a == "--real")),
        _ => Err("usage: cargo xtask <check [--real] | layers>".to_owned()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map_or(manifest.clone(), Path::to_path_buf)
}

/// The "Done means" checks, stopping at the first failure.
fn check(root: &Path, real: bool) -> Result<(), String> {
    run(root, "python", &["tools/check_public_tree.py", "--all"])?;
    run(root, "cargo", &["fmt", "--all", "--check"])?;
    run(
        root,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run(root, "cargo", &["test", "--workspace"])?;
    layers(root)?;
    if real {
        if std::env::var_os("ETS2_DIR").is_none() {
            return Err("--real needs ETS2_DIR (the game directory)".to_owned());
        }
        run(
            root,
            "cargo",
            &[
                "test",
                "--release",
                "-p",
                REAL_INSTALL_CRATE,
                "--test",
                "real_install",
            ],
        )?;
    }
    println!("xtask: all checks passed");
    Ok(())
}

/// The crate that holds `tests/real_install.rs`.
const REAL_INSTALL_CRATE: &str = "specsheet-session";

fn run(root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    println!("xtask: {program} {}", args.join(" "));
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {} failed ({status})", args.join(" ")))
    }
}

/// Checks every crate's dependencies against [`LAYERS`] and [`BEVY_CRATES`].
fn layers(root: &Path) -> Result<(), String> {
    let layer: BTreeMap<&str, u8> = LAYERS.iter().copied().collect();
    let mut manifests = vec![root.join("xtask").join("Cargo.toml")];
    let crates = std::fs::read_dir(root.join("crates")).map_err(|e| format!("crates/: {e}"))?;
    for entry in crates.flatten() {
        let manifest = entry.path().join("Cargo.toml");
        if manifest.is_file() {
            manifests.push(manifest);
        }
    }
    let mut problems = Vec::new();
    for path in manifests {
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let manifest = Manifest::parse(&text);
        let Some(name) = manifest.name else {
            problems.push(format!("{}: no package name", path.display()));
            continue;
        };
        let Some(&own) = layer.get(name.as_str()) else {
            problems.push(format!(
                "{name}: not in the layer table (xtask/src/main.rs)"
            ));
            continue;
        };
        for dep in &manifest.dependencies {
            if dep.starts_with("bevy") && !BEVY_CRATES.contains(&name.as_str()) {
                problems.push(format!(
                    "{name} depends on {dep}: only the app layers use Bevy"
                ));
            }
            if let Some(&theirs) = layer.get(dep.as_str()) {
                if theirs >= own {
                    problems.push(format!(
                        "{name} (layer {own}) depends on {dep} (layer {theirs})"
                    ));
                }
            } else if dep.starts_with("specsheet") {
                problems.push(format!(
                    "{name} depends on {dep}, which is not in the layer table"
                ));
            }
        }
    }
    if problems.is_empty() {
        println!("xtask: layers ok");
        Ok(())
    } else {
        Err(format!("layer check failed:\n  {}", problems.join("\n  ")))
    }
}

/// The parts of a Cargo.toml the layer check needs.
#[derive(Debug, Default, PartialEq)]
struct Manifest {
    name: Option<String>,
    /// Dependency names from every `[*dependencies]` table.
    dependencies: Vec<String>,
}

impl Manifest {
    fn parse(text: &str) -> Self {
        let mut manifest = Self::default();
        let mut section = String::new();
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = header.trim().to_owned();
                // `[dependencies.foo]` names a dependency in its header.
                if let Some((table, dep)) = section.rsplit_once('.') {
                    if table.ends_with("dependencies") {
                        manifest.dependencies.push(dep.to_owned());
                    }
                }
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if section == "package" && key == "name" {
                manifest.name = Some(value.trim().trim_matches('"').to_owned());
            } else if section.ends_with("dependencies") && !key.contains('.') {
                manifest.dependencies.push(key.to_owned());
            } else if section.ends_with("dependencies") {
                // `foo.workspace = true`
                if let Some((dep, _)) = key.split_once('.') {
                    manifest.dependencies.push(dep.to_owned());
                }
            }
        }
        manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_name_and_every_kind_of_dependency() {
        let manifest = Manifest::parse(
            r#"
[package]
name = "specsheet-data"
version = "0.1.0"

[dependencies]
specsheet-archive = { path = "../specsheet-archive" }
thiserror.workspace = true

[dev-dependencies]
bevy = "0.19"

[target.'cfg(windows)'.dependencies.winapi]
version = "0.3"
"#,
        );
        assert_eq!(manifest.name.as_deref(), Some("specsheet-data"));
        assert_eq!(
            manifest.dependencies,
            ["specsheet-archive", "thiserror", "bevy", "winapi"]
        );
    }

    #[test]
    fn the_layer_table_names_each_crate_once() {
        let mut names: Vec<&str> = LAYERS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LAYERS.len());
    }
}
