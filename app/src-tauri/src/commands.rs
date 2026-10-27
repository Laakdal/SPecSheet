//! Tauri commands. Each one runs its game-data work on the blocking pool so
//! the UI thread never parses files.

use std::sync::Arc;

use game_data::assembly::Drive;
use game_data::session::GameSession;
use game_data::GameInstall;
use tauri::{AppHandle, State};
use tuning_core::{AccessoryId, SlotId, TruckId};

use crate::dto::{
    slot_label, truck_name, GameInfo, OptionDto, SceneDto, SlotOptions, TruckEntry, TruckView,
};
use crate::error::AppError;
use crate::settings::{self, Settings};
use crate::state::AppState;

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| AppError::Task(e.to_string()))?
}

/// Opens the installation at `dir`; without one, the last directory that
/// worked, then automatic discovery. The opened directory is remembered.
#[tauri::command]
pub async fn open_game(
    dir: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<GameInfo, AppError> {
    let saved = settings::load(&app).game_dir;
    let game = blocking(move || {
        let install = match dir.filter(|d| !d.trim().is_empty()) {
            Some(dir) => GameInstall::at(dir.trim())?,
            None => match saved.and_then(|d| GameInstall::at(d).ok()) {
                Some(install) => install,
                None => GameInstall::discover()?,
            },
        };
        Ok(Arc::new(GameSession::open(install)?))
    })
    .await?;
    let remembered = Settings {
        game_dir: Some(game.install().root().to_path_buf()),
    };
    if let Err(error) = settings::save(&app, &remembered) {
        // Not fatal: the game is open, it just will not be remembered.
        eprintln!("{error}");
    }
    let info = GameInfo {
        root: game.install().root().display().to_string(),
        trucks: game
            .trucks()
            .iter()
            .map(|t| TruckEntry {
                id: t.id.to_string(),
                name: truck_name(&game, &t.info, t.id.as_str()),
            })
            .collect(),
        warnings: game.mount_failures.clone(),
    };
    state.set_game(game);
    Ok(info)
}

#[tauri::command]
pub async fn load_truck(truck: String, state: State<'_, AppState>) -> Result<TruckView, AppError> {
    let game = state.game()?;
    let worker_game = Arc::clone(&game);
    let session = blocking(move || Ok(worker_game.start_truck(&TruckId::new(truck))?)).await?;
    let view = TruckView::build(&game, &session);
    *state.lock_truck() = Some(session);
    Ok(view)
}

// `async` runs these on the async runtime instead of the main (UI) thread;
// selection re-assembles the truck and may parse new models.
#[tauri::command(async)]
pub fn slot_options(slot: String, state: State<'_, AppState>) -> Result<SlotOptions, AppError> {
    let game = state.game()?;
    let guard = state.lock_truck();
    let session = guard.as_ref().ok_or(AppError::NoTruck)?;
    let slot = SlotId::new(slot);
    Ok(SlotOptions {
        label: slot_label(&game, &slot),
        options: session
            .options(&slot)
            .iter()
            .map(|option| OptionDto::build(&game, option))
            .collect(),
        slot: slot.to_string(),
    })
}

/// Selects an accessory and returns the re-assembled truck.
#[tauri::command(async)]
pub fn select_accessory(
    accessory: String,
    state: State<'_, AppState>,
) -> Result<TruckView, AppError> {
    let game = state.game()?;
    let mut guard = state.lock_truck();
    let session = guard.as_mut().ok_or(AppError::NoTruck)?;
    session.select(&game, &AccessoryId::new(accessory))?;
    Ok(TruckView::build(&game, session))
}

#[tauri::command(async)]
pub fn set_right_hand_drive(
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<TruckView, AppError> {
    let game = state.game()?;
    let mut guard = state.lock_truck();
    let session = guard.as_mut().ok_or(AppError::NoTruck)?;
    session.set_drive(&game, if enabled { Drive::Right } else { Drive::Left });
    Ok(TruckView::build(&game, session))
}

/// The game's tuning scenes (garage first).
#[tauri::command]
pub async fn list_scenes(state: State<'_, AppState>) -> Result<Vec<SceneDto>, AppError> {
    let game = state.game()?;
    let worker_game = Arc::clone(&game);
    let scenes = blocking(move || Ok(worker_game.scenes()?)).await?;
    Ok(scenes
        .into_iter()
        .map(|scene| SceneDto::build(&game, scene))
        .collect())
}
