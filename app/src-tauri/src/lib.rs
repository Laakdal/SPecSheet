//! Tauri shell: a thin adapter between the frontend and `game-data`.
//! Holds no tuning rules; see `docs/ARCHITECTURE.md`.

mod commands;
mod dto;
mod error;
mod protocol;
mod settings;
mod state;
mod wire;

/// Builds and runs the application.
pub fn run() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .manage(state::AppState::default())
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::open_game,
            commands::load_truck,
            commands::slot_options,
            commands::select_accessory,
            commands::set_right_hand_drive,
            commands::list_scenes,
        ])
        .run(tauri::generate_context!())
}
