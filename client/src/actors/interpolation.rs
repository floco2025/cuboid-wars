use std::collections::VecDeque;

use bevy::prelude::*;
use common::{
    map::Carriers,
    math::{angle_delta_radians, sequence_is_newer},
    protocol::{ActorMovementState, CarrierId},
};

const MAX_SAMPLES: usize = 64;

#[derive(Component)]
pub(crate) struct RemoteActorMotion {
    samples: VecDeque<MovementSample>,
    cursor: f64,
}

#[derive(Clone, Copy)]
struct MovementSample {
    tick: u32,
    at: f64,
    movement: ActorMovementState,
}

#[derive(Component, Default)]
pub(crate) struct ActorAnimationVelocity(pub Vec3);

impl RemoteActorMotion {
    pub(crate) fn new(tick: u32, movement: ActorMovementState, delay_ticks: f64) -> Self {
        Self {
            samples: VecDeque::from([MovementSample {
                tick,
                at: 0.0,
                movement,
            }]),
            cursor: -delay_ticks,
        }
    }

    pub(crate) fn push(&mut self, tick: u32, movement: ActorMovementState) {
        let last = self.samples.back().expect("actor movement buffer is empty");
        if !sequence_is_newer(tick, last.tick) {
            return;
        }
        let at = last.at + f64::from(tick.wrapping_sub(last.tick));
        self.samples.push_back(MovementSample { tick, at, movement });
        if self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    pub(crate) fn advance(
        &mut self,
        delta_ticks: f64,
        carriers: &Carriers,
        carrier_alpha: f32,
        tick_secs: f64,
    ) -> (ActorMovementState, Vec3) {
        let newest = self.samples.back().expect("actor movement buffer is empty").at;
        // Holding the newest sample avoids overshooting stops when an update is late.
        self.cursor = (self.cursor + delta_ticks).min(newest);
        while self.samples.get(1).is_some_and(|sample| sample.at <= self.cursor) {
            self.samples.pop_front();
        }
        let left = self.samples.front().expect("actor movement buffer is empty");
        let mut movement = render_movement(left.movement, carriers, carrier_alpha);
        let Some(right) = self.samples.get(1) else {
            return (movement, Vec3::ZERO);
        };
        if self.cursor < left.at {
            return (movement, Vec3::ZERO);
        }
        let span = right.at - left.at;
        let alpha = ((self.cursor - left.at) / span) as f32;
        let start = Vec3::from(movement.pos);
        let end = Vec3::from(render_movement(right.movement, carriers, carrier_alpha).pos);
        movement.pos = start.lerp(end, alpha).into();
        movement.face_yaw += angle_delta_radians(right.movement.face_yaw, movement.face_yaw) * alpha;
        movement.vertical_velocity += (right.movement.vertical_velocity - movement.vertical_velocity) * alpha;
        (movement, (end - start) * (1.0 / (span * tick_secs)) as f32)
    }
}

fn render_movement(mut movement: ActorMovementState, carriers: &Carriers, alpha: f32) -> ActorMovementState {
    movement.pos = carriers
        .pose_between(movement.carrier, alpha)
        .transform_position(&movement.pos);
    movement.carrier = CarrierId::WORLD;
    movement
}
