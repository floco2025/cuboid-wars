use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use common::protocol::Position;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use server::{
    app::{NetworkOverrides, ServerAppOptions, build_server_app_from_files},
    network::LocalLink,
};

use super::executor::Executor;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Script {
    pub gameplay: PathBuf,
    pub settings: PathBuf,
    pub layout: PathBuf,
    pub spawn: [f32; 3],
    pub actions: Vec<Action>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Action {
    Aim {
        target: [f32; 3],
    },
    Portal {
        end: End,
    },
    // The portal a shot from `eye` at `target` opens, wherever the player
    // stands: a sweep tries the pairs a `probe` has shown reachable.
    Place {
        end: End,
        eye: [f32; 3],
        target: [f32; 3],
    },
    // What portal shots from `eye`, or from the player's, would do.
    Probe {
        #[serde(default)]
        eye: Option<[f32; 3]>,
        targets: Vec<[f32; 3]>,
    },
    Fire,
    Move {
        direction: [f32; 2],
        ticks: u32,
        #[serde(default)]
        crouch: bool,
        #[serde(default)]
        jump: bool,
    },
    Check {
        min: [f32; 3],
        max: [f32; 3],
        #[serde(default = "require_ground")]
        grounded: bool,
    },
    Advance {
        ticks: u32,
    },
    Inspect,
    // Recreate the session at `spawn`, or at the script's without one.
    Reset {
        #[serde(default)]
        spawn: Option<[f32; 3]>,
    },
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum End {
    A,
    B,
}

fn require_ground() -> bool {
    true
}

impl Script {
    pub fn build_server(&self, local: LocalLink, logging: bool) -> Result<bevy::prelude::App> {
        build_server_app_from_files(
            ServerAppOptions {
                map: None,
                god: false,
                peace: false,
                initial_spawn: Some(Position {
                    x: self.spawn[0],
                    y: self.spawn[1],
                    z: self.spawn[2],
                }),
                checkpoint: None,
                network: NetworkOverrides::default(),
                logging,
            },
            &self.gameplay,
            &self.settings,
            &self.layout,
            local,
        )
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("reading experiment {}", path.display()))?;
        let mut script: Self = serde_json::from_str(&text).context("invalid experiment script")?;
        let directory = path.parent().context("experiment directory missing")?;
        script.gameplay = directory.join(&script.gameplay);
        script.settings = directory.join(&script.settings);
        script.layout = directory.join(&script.layout);
        script.validate()?;
        Ok(script)
    }

    fn validate(&self) -> Result<()> {
        ensure!(self.spawn.iter().all(|n| n.is_finite()), "spawn must be finite");
        for (index, action) in self.actions.iter().enumerate() {
            match action {
                Action::Aim { target } => ensure!(
                    target.iter().all(|n| n.is_finite()),
                    "action {index}: aim target must be finite"
                ),
                Action::Place { eye, target, .. } => ensure!(
                    eye.iter().chain(target).all(|n| n.is_finite()) && eye != target,
                    "action {index}: place needs a finite eye and a different finite target"
                ),
                Action::Reset { spawn: Some(spawn) } => ensure!(
                    spawn.iter().all(|n| n.is_finite()),
                    "action {index}: reset spawn must be finite"
                ),
                Action::Probe { eye, targets } => ensure!(
                    !targets.is_empty() && eye.iter().chain(targets).flatten().all(|n| n.is_finite()),
                    "action {index}: probe needs finite targets and a finite eye"
                ),
                Action::Advance { ticks } | Action::Move { ticks, .. } => {
                    ensure!(*ticks > 0, "action {index}: ticks must be positive");
                    if let Action::Move { direction, .. } = action {
                        ensure!(
                            direction.iter().all(|n| n.is_finite()),
                            "action {index}: direction must be finite"
                        );
                    }
                }
                Action::Check { min, max, .. } => ensure!(
                    (0..3).all(|axis| min[axis].is_finite() && max[axis].is_finite() && min[axis] <= max[axis]),
                    "action {index}: invalid check bounds"
                ),
                _ => {}
            }
        }
        Ok(())
    }

    pub fn run(&self) -> Result<Value> {
        self.validate()?;
        let mut executor = Executor::new(self.clone())?;
        while !executor.finished() {
            executor.start_next()?;
            while executor.running() {
                executor.tick()?;
            }
        }
        Ok(json!({"initial": executor.initial, "steps": executor.steps}))
    }
}

pub fn run_file(path: &Path) -> Result<()> {
    let report = Script::load(path)?.run()?;
    serde_json::to_writer_pretty(io::stdout().lock(), &report)?;
    println!();
    Ok(())
}
