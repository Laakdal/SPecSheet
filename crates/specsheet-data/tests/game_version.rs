//! Pins the game version the research and tests were checked against.
//! Skipped unless `ETS2_DIR` is set.

use specsheet_data::{GameInstall, TESTED_GAME_VERSION};

#[test]
fn installed_game_is_the_tested_version() {
    if std::env::var_os("ETS2_DIR").is_none() {
        return;
    }
    let install = GameInstall::discover().expect("ETS2_DIR is an install");
    let version = install.version().expect("version.scs is readable");
    assert_eq!(
        version, TESTED_GAME_VERSION,
        "ETS2 is {version}, but the research and tests were checked against \
         {TESTED_GAME_VERSION}. Rerun the real-install tests, recheck docs/PARITY.md \
         and docs/REVERSE_ENGINEERING.md, then update TESTED_GAME_VERSION."
    );
}
