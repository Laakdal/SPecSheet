//! SpecSheet: assembles trucks from the user's ETS2 installation and shows
//! them in the game's tuning scenes. A thin Bevy + egui front end over
//! `specsheet-data`; see `docs/ARCHITECTURE.md`.
// Bevy systems take their dependencies as parameters.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hotspots;
mod parity;
mod script;
mod settings;
mod studio;
mod ui;
mod viewer;

use bevy::prelude::*;
use bevy::window::WindowResolution;
use bevy_egui::EguiPlugin;

use crate::script::ScriptPlugin;
use crate::studio::StudioPlugin;
use crate::ui::UiPlugin;
use crate::viewer::ViewerPlugin;
use specsheet_render::GameMaterialsPlugin;

fn main() -> AppExit {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--parity") {
        return match parity::run(&args[1..]) {
            Ok(true) => AppExit::Success,
            Ok(false) => AppExit::error(),
            Err(error) => {
                eprintln!("parity: {error}");
                AppExit::error()
            }
        };
    }
    let script = match ScriptPlugin::from_env() {
        Ok(plugin) => plugin,
        Err(error) => {
            eprintln!("{error}");
            return AppExit::error();
        }
    };
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "SpecSheet".into(),
                resolution: WindowResolution::new(1440, 900),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            EguiPlugin::default(),
            StudioPlugin,
            ViewerPlugin,
            GameMaterialsPlugin,
            UiPlugin,
            script,
        ))
        .run()
}
