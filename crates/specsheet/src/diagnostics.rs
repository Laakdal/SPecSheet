//! The app's log file and the opt-in performance log (TASKS item 13).
//!
//! - Every run writes its log to `specsheet.log` in the app config directory
//!   (next to `settings.json`), keeping the previous run as
//!   `specsheet.prev.log`. Release builds have no console, so this is their
//!   only log.
//! - `SPECSHEET_PERF=1` adds frame time and FPS every few seconds, and the
//!   duration of each background job (opening the game, loading a truck, a
//!   selection). It only logs; nothing is drawn, so screenshots are unchanged.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::log::tracing_subscriber::{self, Layer};
use bevy::log::{BoxedLayer, LogPlugin};
use bevy::prelude::*;

const LOG_FILE: &str = "specsheet.log";
const PREVIOUS_LOG_FILE: &str = "specsheet.prev.log";
/// How often the performance log prints frame time and FPS.
const PERF_INTERVAL: Duration = Duration::from_secs(5);

/// Bevy's log plugin with the log file as an extra output.
pub fn log_plugin() -> LogPlugin {
    LogPlugin {
        custom_layer: file_layer,
        ..default()
    }
}

/// Writes the log, without colour codes, to the app's log file. No file
/// (no config directory, or it cannot be written) leaves the console only.
fn file_layer(_app: &mut App) -> Option<BoxedLayer> {
    let dir = crate::settings::app_dir()?;
    std::fs::create_dir_all(&dir).ok()?;
    let (current, previous) = log_paths(&dir);
    // The previous run's log is kept for one more run; failing to keep it is
    // not a reason to skip this run's log.
    let _ = std::fs::rename(&current, &previous);
    let file = File::create(&current).ok()?;
    Some(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(Mutex::new(file))
            .boxed(),
    )
}

/// The current and the previous log file in `dir`.
fn log_paths(dir: &Path) -> (PathBuf, PathBuf) {
    (dir.join(LOG_FILE), dir.join(PREVIOUS_LOG_FILE))
}

/// Whether `SPECSHEET_PERF` asks for the performance log: set, non-empty and
/// not `0`.
fn perf_requested(value: Option<&str>) -> bool {
    value.is_some_and(|v| !v.trim().is_empty() && v.trim() != "0")
}

/// Whether this run logs performance (read once from `SPECSHEET_PERF`).
pub fn perf_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| perf_requested(std::env::var("SPECSHEET_PERF").ok().as_deref()))
}

/// Frame time and FPS in the log when the performance log is on.
pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        if !perf_enabled() {
            return;
        }
        app.add_plugins((
            FrameTimeDiagnosticsPlugin::default(),
            LogDiagnosticsPlugin {
                wait_duration: PERF_INTERVAL,
                filter: Some(
                    [
                        FrameTimeDiagnosticsPlugin::FRAME_TIME,
                        FrameTimeDiagnosticsPlugin::FPS,
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..default()
            },
        ));
        info!("perf: performance log on (SPECSHEET_PERF)");
    }
}

/// A background job's start, for its duration in the performance log.
#[derive(Clone, Copy, Debug)]
pub struct JobTimer {
    label: &'static str,
    started: Instant,
}

impl JobTimer {
    /// Starts timing `label`.
    pub fn start(label: &'static str) -> Self {
        Self {
            label,
            started: Instant::now(),
        }
    }

    /// Logs the job's duration when the performance log is on.
    pub fn finish(self) {
        if perf_enabled() {
            info!(
                "perf: {} took {} ms",
                self.label,
                self.started.elapsed().as_millis()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_performance_log_is_off_unless_asked_for() {
        assert!(!perf_requested(None));
        assert!(!perf_requested(Some("")));
        assert!(!perf_requested(Some(" ")));
        assert!(!perf_requested(Some("0")));
        assert!(perf_requested(Some("1")));
        assert!(perf_requested(Some("yes")));
    }

    #[test]
    fn the_log_and_the_previous_log_sit_side_by_side() {
        let (current, previous) = log_paths(Path::new("cfg"));
        assert_eq!(current, Path::new("cfg").join("specsheet.log"));
        assert_eq!(previous, Path::new("cfg").join("specsheet.prev.log"));
    }
}
