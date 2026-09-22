use std::time::Duration;

use anyhow::Result;
use client::network::PlaybackFrame;
use common::config::NetworkConfig;

use super::{
    executor::Executor,
    script::{Action, Script},
};

#[derive(Default)]
pub(super) struct Controls {
    pub next: bool,
    pub play_pause: bool,
    pub restart: bool,
}

pub(super) struct Playback {
    pub executor: Executor,
    pub paused: bool,
    continuous: bool,
    budget: Duration,
    dirty: bool,
    reset: bool,
}

impl Playback {
    pub fn new(script: Script) -> Result<Self> {
        let mut executor = Executor::new(script)?;
        executor.session.visual_messages = Some(Vec::new());
        Ok(Self {
            executor,
            paused: true,
            continuous: false,
            budget: Duration::ZERO,
            dirty: true,
            reset: false,
        })
    }

    pub fn pause(&mut self) {
        self.paused = true;
        self.budget = Duration::ZERO;
    }

    // Continuous playback carries unused tick budget across action boundaries.
    // Enter switches to one action (or the remainder of the current action).
    // Pauses discard wall time, and neither mode ever skips a simulation tick.
    pub fn update(&mut self, controls: Controls, elapsed: Duration) -> Result<Duration> {
        if controls.restart {
            *self = Self::new(self.executor.script.clone())?;
            self.reset = true;
            return Ok(Duration::ZERO);
        }
        let mut simulated = Duration::ZERO;
        let tick_duration = self
            .executor
            .session
            .server
            .world()
            .resource::<NetworkConfig>()
            .tick_duration();
        if controls.next && !self.executor.finished() {
            self.continuous = false;
            self.paused = false;
            self.budget = Duration::ZERO;
            if !self.executor.running() {
                simulated += self.start_next(tick_duration)?;
            }
        } else if controls.play_pause {
            if !self.paused {
                self.pause();
            } else if !self.executor.finished() {
                self.continuous = true;
                self.paused = false;
            }
        }
        if self.paused {
            return Ok(simulated);
        }
        self.budget += elapsed;
        loop {
            if !self.executor.running() {
                if !self.continuous || self.executor.finished() {
                    self.pause();
                    break;
                }
                // Portal/fire actions may advance one tick inside start_next.
                // Do not execute that tick ahead of the playback clock. Instant
                // actions can still finish the sequence with no extra wait.
                let action = &self.executor.script.actions[self.executor.steps.len()];
                if matches!(action, Action::Portal { .. } | Action::Fire) && self.budget < tick_duration {
                    break;
                }
                let advanced = self.start_next(tick_duration)?;
                simulated += advanced;
                self.budget -= advanced;
            } else {
                if self.budget < tick_duration {
                    break;
                }
                let before = self.executor.session.tick();
                self.executor.tick()?;
                simulated += tick_duration * self.executor.session.tick().wrapping_sub(before);
                self.budget -= tick_duration;
                self.dirty = true;
            }
        }
        Ok(simulated)
    }

    fn start_next(&mut self, tick_duration: Duration) -> Result<Duration> {
        let resetting = matches!(self.executor.script.actions[self.executor.steps.len()], Action::Reset);
        let before = self.executor.session.tick();
        self.executor.start_next()?;
        self.dirty = true;
        if resetting {
            self.reset = true;
            Ok(Duration::ZERO)
        } else {
            Ok(tick_duration * self.executor.session.tick().wrapping_sub(before))
        }
    }

    pub fn take_frame(&mut self) -> Option<PlaybackFrame> {
        if !std::mem::take(&mut self.dirty) {
            return None;
        }
        let session = &mut self.executor.session;
        let mut snapshot = server::network::capture_snapshot(session.server.world_mut());
        if let Some((_, player)) = snapshot.players.iter_mut().find(|(id, _)| *id == session.id) {
            player.movement = session.owner.state();
        }
        Some(PlaybackFrame {
            snapshot,
            projectiles: session
                .projectiles
                .iter()
                .map(|shot| (shot.id, shot.position))
                .collect(),
            cues: std::mem::take(session.visual_messages.as_mut().expect("visual session")),
            reset: std::mem::take(&mut self.reset),
        })
    }
}
