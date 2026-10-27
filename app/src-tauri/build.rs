use std::path::PathBuf;

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows_attributes()))
        .unwrap_or_else(|error| panic!("tauri build failed: {error:#}"));
}

/// tauri-winres escapes `'` as `\'` in the generated resource script, which
/// RC.EXE rejects, so the icon cannot be referenced from a directory whose
/// absolute path contains an apostrophe (e.g. "C:\Mudrock's Headquarters").
/// In that case the icon is copied to the system temp directory first.
fn windows_attributes() -> tauri_build::WindowsAttributes {
    let icon = PathBuf::from("icons").join("icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    if !manifest_dir.contains('\'') {
        return tauri_build::WindowsAttributes::new();
    }
    let copy = std::env::temp_dir().join("ets2-tuning-app-icon.ico");
    std::fs::copy(&icon, &copy)
        .unwrap_or_else(|error| panic!("copying {}: {error}", icon.display()));
    tauri_build::WindowsAttributes::new().window_icon_path(copy)
}
