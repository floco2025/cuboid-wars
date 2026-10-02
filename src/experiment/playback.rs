use std::{f32::consts::PI, time::Duration};

use anyhow::Result;
use bevy::math::Vec2;
use client::{constants::CAMERA_MAX_PITCH, network::PlaybackFrame};
use common::{config::NetworkConfig, math::angle_delta_radians};

use super::{
    executor::Executor,
    script::{Action, Script},
};

// Slower than this, a body has no travel direction and keeps its facing.
const FACING_MIN_SPEED: f32 = 0.5;
// An aim this close to vertical has no heading either.
const AIM_MIN_HORIZONTAL: f32 = 0.01;
// Exponential rate, per second, at which the view closes on the player's.
const VIEW_TURN_RATE: f32 = 6.0;
// Wall time continuous playback waits after an aim or a portal shot, so the
// view arrives on the target and stays for the result. It simulates nothing.
const ACTION_HOLD: Duration = Duration::from_millis(600);

#[derive(Default)]
pub(super) struct Controls {
    pub next: bool,
    pub play_pause: bool,
    pub restart: bool,
}

pub(super) struct Playback {
    pub executor: Executor,
    pub paused: bool,
    // Presentation only: where a player running this route would look. An
    // aim holds the view on its target until the next move; otherwise it is
    // level along the body's travel, starting on the first move so the
    // opening frame looks down the route.
    pub facing: f32,
    pub pitch: f32,
    aiming: bool,
    // Camera yaw and pitch easing onto the player's view; see `view`.
    eye: Option<Vec2>,
    resumed: bool,
    continuous: bool,
    budget: Duration,
    hold: Duration,
    dirty: bool,
    reset: bool,
}

impl Playback {
    pub fn new(script: Script) -> Result<Self> {
        let mut executor = Executor::new(script)?;
        executor.session.visual_messages = Some(Vec::new());
        let facing = executor
            .script
            .actions
            .iter()
            .find_map(|action| match action {
                Action::Move { direction, .. } if *direction != [0.0, 0.0] => Some(direction[0].atan2(direction[1])),
                _ => None,
            })
            .unwrap_or(executor.session.owner.motion.face_yaw.0);
        Ok(Self {
            executor,
            paused: true,
            facing,
            pitch: 0.0,
            aiming: false,
            eye: None,
            resumed: false,
            continuous: false,
            budget: Duration::ZERO,
            hold: Duration::ZERO,
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
            self.resumed = true;
            return Ok(Duration::ZERO);
        }
        self.resumed |= controls.next || controls.play_pause;
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
            self.hold = Duration::ZERO;
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
                let held = self.hold.min(self.budget);
                self.hold -= held;
                self.budget -= held;
                if !self.hold.is_zero() {
                    break;
                }
                // Portal/fire actions may advance one tick inside start_next.
                // Do not execute that tick ahead of the playback clock. Instant
                // actions can still finish the sequence with no extra wait.
                let action = &self.executor.script.actions[self.executor.steps.len()];
                if matches!(action, Action::Portal { .. } | Action::Place { .. } | Action::Fire)
                    && self.budget < tick_duration
                {
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
                self.look();
                self.dirty = true;
            }
        }
        Ok(simulated)
    }

    fn start_next(&mut self, tick_duration: Duration) -> Result<Duration> {
        let action = &self.executor.script.actions[self.executor.steps.len()];
        let resetting = matches!(action, Action::Reset { .. });
        self.aiming = match action {
            Action::Aim { .. } => true,
            Action::Reset { .. } => false,
            Action::Move { direction, .. } if *direction != [0.0, 0.0] => false,
            _ => self.aiming,
        };
        if self.continuous
            && matches!(
                action,
                Action::Aim { .. } | Action::Portal { .. } | Action::Place { .. }
            )
        {
            self.hold = ACTION_HOLD;
        }
        let before = self.executor.session.tick();
        self.executor.start_next()?;
        self.look();
        self.dirty = true;
        if resetting {
            self.reset = true;
            Ok(Duration::ZERO)
        } else {
            Ok(tick_duration * self.executor.session.tick().wrapping_sub(before))
        }
    }

    fn look(&mut self) {
        let session = &self.executor.session;
        let (heading, min_length, pitch) = if self.aiming {
            let aim = session.direction;
            (aim, AIM_MIN_HORIZONTAL, aim.y.clamp(-1.0, 1.0).asin())
        } else {
            (session.owner.motion.horizontal_velocity.0, FACING_MIN_SPEED, 0.0)
        };
        if heading.x * heading.x + heading.z * heading.z > min_length * min_length {
            self.facing = heading.x.atan2(heading.z);
        }
        self.pitch = pitch.clamp(-CAMERA_MAX_PITCH, CAMERA_MAX_PITCH);
    }

    // The camera yaw and pitch to show, given the ones on screen. The eye
    // eases onto the player's view whatever the mouse does and the screen
    // turns by the eye's step, so a mouse look (or --look) holds as an offset.
    // A control or a running tick restarts the eye from the screen, which
    // eases that offset away.
    pub fn view(&mut self, current: Vec2, elapsed: Duration) -> Vec2 {
        let player = Vec2::new(self.facing + PI, self.pitch);
        let eye = if std::mem::take(&mut self.resumed) || !self.paused {
            current
        } else {
            self.eye.unwrap_or(player)
        };
        let step = Vec2::new(angle_delta_radians(player.x, eye.x), player.y - eye.y)
            * (1.0 - (-VIEW_TURN_RATE * elapsed.as_secs_f32()).exp());
        self.eye = Some(eye + step);
        current + step
    }

    pub fn take_frame(&mut self) -> Option<PlaybackFrame> {
        if !std::mem::take(&mut self.dirty) {
            return None;
        }
        let session = &mut self.executor.session;
        let mut snapshot = server::network::capture_snapshot(session.server.world_mut());
        if let Some((_, player)) = snapshot.players.iter_mut().find(|(id, _)| *id == session.id) {
            player.movement = session.owner.state();
            player.movement.face_yaw = self.facing;
        }
        Some(PlaybackFrame {
            snapshot,
            projectiles: session
                .projectiles
                .iter()
                .map(|shot| (shot.id, shot.position))
                .collect(),
            cues: std::mem::take(
                session
                    .visual_messages
                    .as_mut()
                    .expect("visual messages missing from a graphical session"),
            ),
            reset: std::mem::take(&mut self.reset),
        })
    }
}
