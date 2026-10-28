//! Application state and the actions that change it. Every `specsheet-data`
//! call that parses files runs on the async compute pool, so the UI never
//! blocks; the truck session moves into the task and comes back with the
//! result.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, AsyncComputeTaskPool, Task};
use specsheet_data::assembly::{Drive, Placement};
use specsheet_data::paint::{piece_paint, PiecePaint};
use specsheet_data::scene::{tuning_scene, OrbitSpec, Scene};
use specsheet_data::session::{GameSession, TruckSession};
use specsheet_data::view::{self, SlotOptions, TruckEntry, TruckOverview};
use specsheet_data::GameInstall;
use specsheet_truck::{AccessoryId, SlotId, TruckId};

use crate::settings::{self, Settings};

/// The app's state and its action flow. Owns the per-frame order every
/// other plugin slots into.
pub struct StudioPlugin;

impl Plugin for StudioPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Studio::starting())
            .insert_resource(Actions(vec![Action::OpenGame(None)]))
            .configure_sets(
                Update,
                (
                    FrameSet::FinishJobs,
                    FrameSet::Script,
                    FrameSet::ApplyActions,
                    FrameSet::View,
                )
                    .chain(),
            )
            .add_systems(Update, finish_jobs.in_set(FrameSet::FinishJobs))
            .add_systems(Update, apply_actions.in_set(FrameSet::ApplyActions));
    }
}

/// Order of the `Update` work in one frame: finished jobs fold into the
/// state, the dev script queues actions, actions start jobs, then the 3D
/// view catches up.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrameSet {
    FinishJobs,
    Script,
    ApplyActions,
    View,
}

/// Something the user (or the dev script) asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Opens an installation; `None` tries the remembered one, then discovery.
    OpenGame(Option<String>),
    LoadTruck(TruckId),
    /// Shows a slot's options in the sidebar; `None` returns to the list.
    ShowSlot(Option<SlotId>),
    Select(AccessoryId),
    SetDrive(Drive),
    SetScene(usize),
}

/// Pending actions, applied in order once no job is running.
#[derive(Resource, Default)]
pub struct Actions(pub Vec<Action>);

pub struct SceneEntry {
    pub scene: Scene,
    pub name: String,
    /// The truck configuration screen's orbit camera, for its own scene.
    pub orbit: Option<OrbitSpec>,
}

/// Name of the truck configuration screen's own scene in the picker.
const TUNING_SCENE_NAME: &str = "Truck configuration";

struct Opened {
    game: Arc<GameSession>,
    trucks: Vec<TruckEntry>,
    scenes: Vec<SceneEntry>,
}

enum JobOutput {
    Opened(Result<Opened, String>),
    Truck(Result<TruckSession, String>),
    /// A selection or drive change; the session always comes back.
    Changed(TruckSession, Result<(), String>),
}

#[derive(Resource, Default)]
pub struct Studio {
    pub game: Option<Arc<GameSession>>,
    pub trucks: Vec<TruckEntry>,
    pub scenes: Vec<SceneEntry>,
    pub scene: Option<usize>,
    /// Bumped whenever `scene` changes.
    pub scene_generation: u64,
    /// `None` while a job holds it.
    session: Option<TruckSession>,
    pub overview: Option<TruckOverview>,
    pub placements: Vec<Placement>,
    /// The interior's full cockpit, shown in the interior view.
    pub cockpit: Option<Placement>,
    /// Interior-only accessories on the cockpit, shown with it.
    pub interior_parts: Vec<Placement>,
    /// The paint of every placed accessory with a paint job (F15).
    pub paints: HashMap<(SlotId, AccessoryId), PiecePaint>,
    /// Bumped whenever `placements` or the paint colour change.
    pub truck_generation: u64,
    pub options: Option<SlotOptions>,
    /// The truck's price when it was loaded, before any change.
    pub original_price: Option<i64>,
    job: Option<Task<JobOutput>>,
    pub error: Option<String>,
    /// True until the first attempt to open a game has finished.
    pub discovering: bool,
}

impl Studio {
    /// The state at start-up, before the first attempt to open a game.
    pub fn starting() -> Self {
        Self {
            discovering: true,
            ..default()
        }
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    fn spawn(&mut self, work: impl FnOnce() -> JobOutput + Send + 'static) {
        self.job = Some(AsyncComputeTaskPool::get().spawn(async move { work() }));
    }

    /// Recomputes everything derived from the session.
    fn refresh(&mut self) {
        let (Some(game), Some(session)) = (&self.game, &self.session) else {
            return;
        };
        self.overview = Some(view::overview(game, session));
        self.placements = session.assembly().placements.clone();
        self.cockpit = session.assembly().cockpit.clone();
        self.interior_parts = session.assembly().interior_parts.clone();
        self.paints = self
            .placements
            .iter()
            .chain(&self.interior_parts)
            .chain(&self.cockpit)
            .filter_map(|p| {
                let paint =
                    piece_paint(session.catalog(), session.config(), &p.slot, &p.accessory)?;
                Some(((p.slot.clone(), p.accessory.clone()), paint))
            })
            .collect();
        self.truck_generation += 1;
        if let Some(options) = &self.options {
            self.options = Some(view::slot_options(game, session, &options.slot));
        }
    }
}

/// Starts the work for queued actions, one job at a time.
fn apply_actions(mut actions: ResMut<Actions>, mut studio: ResMut<Studio>) {
    while !studio.busy() && !actions.0.is_empty() {
        let action = actions.0.remove(0);
        apply(&mut studio, action);
    }
}

fn apply(studio: &mut Studio, action: Action) {
    studio.error = None;
    match action {
        Action::OpenGame(dir) => {
            let saved = settings::load().game_dir;
            studio.spawn(move || JobOutput::Opened(open_game(dir, saved)));
        }
        Action::LoadTruck(truck) => {
            let Some(game) = studio.game.clone() else {
                return;
            };
            studio.options = None;
            studio.spawn(move || {
                JobOutput::Truck(game.start_truck(&truck).map_err(|e| e.to_string()))
            });
        }
        Action::ShowSlot(slot) => {
            studio.options = match (slot, &studio.game, &studio.session) {
                (Some(slot), Some(game), Some(session)) => {
                    Some(view::slot_options(game, session, &slot))
                }
                _ => None,
            };
        }
        Action::Select(accessory) => {
            let (Some(game), Some(mut session)) = (studio.game.clone(), studio.session.take())
            else {
                return;
            };
            studio.spawn(move || {
                let result = session
                    .select(&game, &accessory)
                    .map(|_| ())
                    .map_err(|e| e.to_string());
                JobOutput::Changed(session, result)
            });
        }
        Action::SetDrive(drive) => {
            let (Some(game), Some(mut session)) = (studio.game.clone(), studio.session.take())
            else {
                return;
            };
            studio.spawn(move || {
                session.set_drive(&game, drive);
                JobOutput::Changed(session, Ok(()))
            });
        }
        Action::SetScene(index) => {
            if index < studio.scenes.len() {
                studio.scene = Some(index);
                studio.scene_generation += 1;
            }
        }
    }
}

fn open_game(dir: Option<String>, saved: Option<std::path::PathBuf>) -> Result<Opened, String> {
    let install = match dir.filter(|d| !d.trim().is_empty()) {
        Some(dir) => GameInstall::at(dir.trim()),
        None => match saved.and_then(|d| GameInstall::at(d).ok()) {
            Some(install) => Ok(install),
            None => GameInstall::discover(),
        },
    }
    .map_err(|e| e.to_string())?;
    let game = Arc::new(GameSession::open(install).map_err(|e| e.to_string())?);
    // The truck configuration screen's own scene first, then the hub scenes.
    let tuning = tuning_scene(game.fs())
        .map_err(|e| warn!("truck configuration scene: {e}"))
        .ok()
        .map(|tuning| SceneEntry {
            scene: tuning.scene,
            name: TUNING_SCENE_NAME.to_owned(),
            orbit: tuning.orbit,
        });
    let hubs = game
        .scenes()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|scene| SceneEntry {
            name: game.localization().resolve(&scene.name),
            scene,
            orbit: None,
        });
    let scenes = tuning.into_iter().chain(hubs).collect();
    Ok(Opened {
        trucks: view::trucks(&game),
        scenes,
        game,
    })
}

/// Collects finished jobs and folds their results into the state.
fn finish_jobs(mut studio: ResMut<Studio>, mut actions: ResMut<Actions>) {
    let Some(job) = studio.job.as_mut() else {
        return;
    };
    let Some(output) = check_ready(job) else {
        return;
    };
    studio.job = None;
    match output {
        JobOutput::Opened(result) => {
            studio.discovering = false;
            match result {
                Ok(opened) => {
                    let mut remembered = settings::load();
                    remembered.game_dir = Some(opened.game.install().root().to_path_buf());
                    if let Err(error) = settings::save(&remembered) {
                        // Not fatal: the game is open, it just will not be remembered.
                        warn!("settings: {error}");
                    }
                    studio.trucks = opened.trucks;
                    studio.scenes = opened.scenes;
                    studio.game = Some(opened.game);
                    studio.session = None;
                    studio.overview = None;
                    studio.placements.clear();
                    studio.truck_generation += 1;
                    studio.scene = None;
                    if !studio.scenes.is_empty() {
                        actions.0.insert(0, Action::SetScene(0));
                    }
                    if let Some(truck) = initial_truck(&remembered, &studio.trucks) {
                        let at = 1.min(actions.0.len());
                        actions.0.insert(at, Action::LoadTruck(truck));
                    }
                }
                Err(error) => studio.error = Some(error),
            }
        }
        JobOutput::Truck(result) => match result {
            Ok(session) => {
                let mut remembered = settings::load();
                remembered.truck = Some(session.catalog().truck.to_string());
                if let Err(error) = settings::save(&remembered) {
                    warn!("settings: {error}");
                }
                studio.session = Some(session);
                studio.refresh();
                studio.original_price = studio.overview.as_ref().map(|o| o.price);
            }
            Err(error) => studio.error = Some(error),
        },
        JobOutput::Changed(session, result) => {
            studio.session = Some(session);
            if let Err(error) = result {
                studio.error = Some(error);
            }
            studio.refresh();
        }
    }
}

/// The truck tuned last time, else the first truck of the installation.
fn initial_truck(settings: &Settings, trucks: &[TruckEntry]) -> Option<TruckId> {
    settings
        .truck
        .as_deref()
        .and_then(|id| trucks.iter().find(|t| t.id.as_str() == id))
        .or_else(|| trucks.first())
        .map(|t| t.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> TruckEntry {
        TruckEntry {
            id: TruckId::new(id),
            name: id.to_owned(),
        }
    }

    #[test]
    fn starts_with_the_remembered_truck_if_it_still_exists() {
        let trucks = [entry("a.one"), entry("b.two")];
        let remembered = |truck: Option<&str>| Settings {
            game_dir: None,
            truck: truck.map(str::to_owned),
        };
        assert_eq!(
            initial_truck(&remembered(Some("b.two")), &trucks),
            Some(TruckId::new("b.two"))
        );
        assert_eq!(
            initial_truck(&remembered(Some("gone")), &trucks),
            Some(TruckId::new("a.one"))
        );
        assert_eq!(initial_truck(&remembered(None), &[]), None);
    }
}
