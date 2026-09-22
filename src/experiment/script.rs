use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
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
    Fire,
    Move {
        direction: [f32; 2],
        ticks: u32,
        #[serde(default)]
        run: bool,
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
    Reset,
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
                initial_spawn: Some(common::protocol::Position {
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
