//! Dev script mode: `SPECSHEET_SCRIPT=<file>` replays UI actions and takes
//! screenshots, so changes can be seen working without a person at the
//! mouse (see `docs/DEVELOPMENT.md`). One command per line; `#` comments:
//!
//! ```text
//! wait                     until no job runs and every model has loaded
//! truck <truck id>         slot <slot id>        back
//! select <accessory id>    pick <n>              (n-th selectable option of the open slot)
//! drive left|right         scene <index>
//! tab <token>              orbit <yaw°> <pitch°> <distance>
//! sleep <frames>           echo <text>           (logs the text, to sync external drivers)
//! screenshot <png path>    exit
//! ```

use std::collections::VecDeque;
use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use specsheet_data::assembly::Drive;
use specsheet_data::view::Tab;
use specsheet_truck::{AccessoryId, SlotId, TruckId};

use crate::studio::{Action, Actions, FrameSet, Studio};
use crate::ui::UiState;
use crate::viewer::{ModelCache, Orbit};

pub(crate) const ENV_VAR: &str = "SPECSHEET_SCRIPT";

/// Replays the script named by `SPECSHEET_SCRIPT`; does nothing when the
/// variable is unset.
pub struct ScriptPlugin {
    steps: Option<Vec<Step>>,
}

impl ScriptPlugin {
    /// Reads and parses the script; errors name the variable and the line.
    pub fn from_env() -> Result<Self, String> {
        let Some(path) = std::env::var_os(ENV_VAR) else {
            return Ok(Self { steps: None });
        };
        let steps = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|source| parse(&source))
            .map_err(|error| format!("{ENV_VAR}: {error}"))?;
        Ok(Self { steps: Some(steps) })
    }
}

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        if let Some(steps) = &self.steps {
            app.insert_resource(Script::new(steps.clone()))
                .add_systems(Update, run.in_set(FrameSet::Script));
        }
    }
}

/// Frames the app must stay idle before `wait` finishes, so uploads and
/// the egui layout settle.
const SETTLE_FRAMES: u32 = 20;
/// Frames given to an asynchronous screenshot before the next command.
const SCREENSHOT_FRAMES: u32 = 30;

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Act(Action),
    /// Selects the n-th selectable option of the slot shown in the sidebar.
    Pick(usize),
    /// Opens a tab of the game screen by its tab-row token (`cabin`,
    /// `e_accessories`, …).
    Tab(Tab),
    Orbit {
        yaw: f32,
        pitch: f32,
        distance: f32,
    },
    Wait,
    Sleep(u32),
    Echo(String),
    Screenshot(PathBuf),
    Exit,
}

#[derive(Resource, Default)]
struct Script {
    steps: VecDeque<Step>,
    idle_frames: u32,
    cooldown: u32,
}

impl Script {
    fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: steps.into(),
            ..default()
        }
    }
}

/// Parses a script; errors name the offending line.
pub(crate) fn parse(source: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (number, line) in source.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let command = words.next().unwrap_or_default();
        let args: Vec<&str> = words.collect();
        let error = || format!("line {}: cannot parse `{line}`", number + 1);
        let one = || match args.as_slice() {
            [arg] => Ok(*arg),
            _ => Err(error()),
        };
        let step = match command {
            "wait" => Step::Wait,
            "exit" => Step::Exit,
            "back" => Step::Act(Action::ShowSlot(None)),
            "truck" => Step::Act(Action::LoadTruck(TruckId::new(one()?))),
            "slot" => Step::Act(Action::ShowSlot(Some(SlotId::new(one()?)))),
            "select" => Step::Act(Action::Select(AccessoryId::new(one()?))),
            "pick" => Step::Pick(one()?.parse().map_err(|_| error())?),
            "drive" => Step::Act(Action::SetDrive(match one()? {
                "left" => Drive::Left,
                "right" => Drive::Right,
                _ => return Err(error()),
            })),
            "scene" => Step::Act(Action::SetScene(one()?.parse().map_err(|_| error())?)),
            "tab" => Step::Tab(Tab::from_token(one()?).ok_or_else(error)?),
            "sleep" => Step::Sleep(one()?.parse().map_err(|_| error())?),
            "echo" => Step::Echo(args.join(" ")),
            "screenshot" => Step::Screenshot(PathBuf::from(args.join(" "))),
            "orbit" => {
                let numbers: Vec<f32> = args
                    .iter()
                    .map(|a| a.parse().map_err(|_| error()))
                    .collect::<Result<_, _>>()?;
                let [yaw, pitch, distance] = numbers[..] else {
                    return Err(error());
                };
                Step::Orbit {
                    yaw: yaw.to_radians(),
                    pitch: pitch.to_radians(),
                    distance,
                }
            }
            _ => return Err(error()),
        };
        steps.push(step);
    }
    Ok(steps)
}

fn run(
    mut commands: Commands,
    mut script: ResMut<Script>,
    studio: Res<Studio>,
    mut actions: ResMut<Actions>,
    cache: Res<ModelCache>,
    mut ui: ResMut<UiState>,
    mut orbit: Single<&mut Orbit>,
    mut exit: MessageWriter<AppExit>,
) {
    let idle = !studio.busy() && !studio.discovering && actions.0.is_empty() && !cache.loading();
    script.idle_frames = if idle { script.idle_frames + 1 } else { 0 };
    if script.cooldown > 0 {
        script.cooldown -= 1;
        return;
    }
    let Some(step) = script.steps.front().cloned() else {
        return;
    };
    match step {
        Step::Wait if script.idle_frames < SETTLE_FRAMES => return,
        Step::Wait => {}
        Step::Act(action) => actions.0.push(action),
        Step::Pick(n) => {
            let option = studio.options.as_ref().and_then(|slot| {
                slot.options
                    .iter()
                    .filter(|o| o.compatible && !o.selected)
                    .nth(n)
            });
            match option {
                Some(option) => {
                    info!("pick {}", option.id);
                    actions.0.push(Action::Select(option.id.clone()));
                }
                None => warn!("pick {n}: no such selectable option"),
            }
        }
        Step::Tab(tab) => {
            if let Some(overview) = &studio.overview {
                ui.open_tab(tab, overview, &mut actions);
            }
        }
        Step::Sleep(frames) => script.cooldown = frames,
        Step::Echo(text) => info!("script: {text}"),
        Step::Orbit {
            yaw,
            pitch,
            distance,
        } => {
            orbit.yaw = yaw;
            orbit.pitch = pitch;
            orbit.distance = distance;
        }
        Step::Screenshot(path) => {
            info!("screenshot {}", path.display());
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
            script.cooldown = SCREENSHOT_FRAMES;
        }
        Step::Exit => {
            exit.write(AppExit::Success);
        }
    }
    script.steps.pop_front();
    script.idle_frames = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands_and_skips_comments() {
        let steps = parse(
            "# demo\nwait\ntruck scania.s_2016\nslot r_grill  # light bar\ndrive right\n\
             orbit 90 10 12\nscreenshot C:/tmp/a b.png\nexit\n",
        )
        .expect("valid");
        assert_eq!(steps[0], Step::Wait);
        assert_eq!(
            steps[1],
            Step::Act(Action::LoadTruck(TruckId::new("scania.s_2016")))
        );
        assert_eq!(
            steps[2],
            Step::Act(Action::ShowSlot(Some(SlotId::new("r_grill"))))
        );
        assert_eq!(steps[3], Step::Act(Action::SetDrive(Drive::Right)));
        assert!(matches!(steps[4], Step::Orbit { distance, .. } if distance == 12.0));
        assert_eq!(steps[5], Step::Screenshot(PathBuf::from("C:/tmp/a b.png")));
        assert_eq!(steps[6], Step::Exit);
        assert_eq!(parse("pick 2").expect("valid"), vec![Step::Pick(2)]);
        assert_eq!(
            parse(
                "sleep 30
echo ready now"
            )
            .expect("valid"),
            vec![Step::Sleep(30), Step::Echo("ready now".to_owned())]
        );
    }

    #[test]
    fn reports_the_bad_line() {
        let error = parse("wait\ndrive sideways\n").expect_err("invalid");
        assert!(error.starts_with("line 2"), "{error}");
        assert!(parse("orbit 1 2\n").is_err());
        assert!(parse("teleport\n").is_err());
    }
}
