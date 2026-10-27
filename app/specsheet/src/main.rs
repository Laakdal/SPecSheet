//! SpecSheet: assembles trucks from the user's ETS2 installation and shows
//! them in the game's tuning scenes. A thin Bevy + egui front end over
//! `game-data`; see `docs/ARCHITECTURE.md`.
// Bevy systems take their dependencies as parameters.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod hotspots;
mod script;
mod settings;
mod studio;
mod ui;
mod viewer;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

use crate::studio::{Action, Actions, Studio};

fn main() -> AppExit {
    let script = match std::env::var_os(script::ENV_VAR) {
        Some(path) => match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|source| script::parse(&source))
        {
            Ok(steps) => Some(script::Script::new(steps)),
            Err(error) => {
                eprintln!("{}: {error}", script::ENV_VAR);
                return AppExit::error();
            }
        },
        None => None,
    };

    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "SpecSheet".into(),
            resolution: WindowResolution::new(1440, 900),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    .insert_resource(Studio::starting())
    .insert_resource(Actions(vec![Action::OpenGame(None)]))
    .init_resource::<viewer::ModelCache>()
    .init_resource::<viewer::TruckInstances>()
    .init_resource::<viewer::SceneState>()
    .init_resource::<viewer::ViewportInput>()
    .init_resource::<ui::UiState>()
    .add_systems(Startup, (viewer::setup, ui::setup))
    .add_systems(
        Update,
        (
            studio::finish_jobs,
            script::run,
            studio::apply_actions,
            viewer::finish_models,
            viewer::apply_scene,
            viewer::sync_truck,
            viewer::frame_truck,
            viewer::orbit_camera,
        )
            .chain(),
    )
    .add_systems(EguiPrimaryContextPass, ui::ui);
    if let Some(script) = script {
        app.insert_resource(script);
    }
    app.run()
}
