//! Application state shared by commands and the asset protocol.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use game_data::session::{GameSession, TruckSession};

use crate::error::AppError;

#[derive(Default)]
pub struct AppState {
    game: RwLock<Option<Arc<GameSession>>>,
    truck: Mutex<Option<TruckSession>>,
}

impl AppState {
    pub fn set_game(&self, game: Arc<GameSession>) {
        *self.game.write().unwrap_or_else(PoisonError::into_inner) = Some(game);
        *self.lock_truck() = None;
    }

    pub fn game(&self) -> Result<Arc<GameSession>, AppError> {
        self.game
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or(AppError::NoGame)
    }

    pub fn lock_truck(&self) -> MutexGuard<'_, Option<TruckSession>> {
        self.truck.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
