use std::collections::VecDeque;

use bevy::prelude::*;
use common::{
    math::{angle_delta_radians, sequence_is_newer},
    protocol::MissileMovementState,
};

const MAX_SAMPLES: usize = 64;

#[derive(Component)]
pub(crate) struct RemoteMissileMotion {
    samples: VecDeque<MovementSample>,
    cursor: f64,
}

#[derive(Clone, Copy)]
struct MovementSample {
    seq: u32,
    at: f64,
    movement: MissileMovementState,
}

impl RemoteMissileMotion {
    pub(crate) fn new(seq: u32, movement: MissileMovementState, delay_ticks: f64) -> Self {
        Self {
            samples: VecDeque::from([MovementSample { seq, at: 0.0, movement }]),
            cursor: -delay_ticks,
        }
    }

    pub(crate) fn push(&mut self, seq: u32, movement: MissileMovementState) {
        let last = self.samples.back().expect("missile movement buffer is empty");
        if !sequence_is_newer(seq, last.seq) {
            return;
        }
        let at = last.at + f64::from(seq.wrapping_sub(last.seq));
        self.samples.push_back(MovementSample { seq, at, movement });
        if self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    pub(crate) fn advance(&mut self, delta_ticks: f64) -> MissileMovementState {
        let newest = self.samples.back().expect("missile movement buffer is empty").at;
        self.cursor = (self.cursor + delta_ticks).min(newest);
        while self.samples.get(1).is_some_and(|sample| sample.at <= self.cursor) {
            self.samples.pop_front();
        }
        let left = self.samples.front().expect("missile movement buffer is empty");
        let mut movement = left.movement;
        let Some(right) = self.samples.get(1) else {
            return movement;
        };
        if self.cursor < left.at {
            return movement;
        }
        let alpha = ((self.cursor - left.at) / (right.at - left.at)) as f32;
        let start = Vec3::from(movement.pos);
        movement.pos = (start + (Vec3::from(right.movement.pos) - start) * alpha).into();
        movement.yaw += angle_delta_radians(right.movement.yaw, movement.yaw) * alpha;
        movement.pitch += (right.movement.pitch - movement.pitch) * alpha;
        movement.speed += (right.movement.speed - movement.speed) * alpha;
        movement
    }
}
