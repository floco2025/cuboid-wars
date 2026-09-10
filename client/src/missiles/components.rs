use bevy::prelude::*;
use common::protocol::{MissileMovementState, Position};

use super::MissileFlight;
use crate::network::{SampleBuffer, SampleTiming};

#[derive(Component, Debug, Clone, Copy)]
pub struct MissileVelocity(pub Vec3);

// The shooter's flight simulation of its own missile.
#[derive(Component)]
pub(crate) struct OwnedMissile {
    pub flight: MissileFlight,
    pub seq: u32,
}

// Another shooter's reported flight, played back behind its sample timeline.
#[derive(Component)]
pub(crate) struct RemoteMissileMotion {
    pub(crate) buffer: SampleBuffer<MissileMovementState>,
    detonation: Option<Position>,
}

// Playback reached the reported impact; the explosion goes here.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct MissileImpact(pub Position);

impl RemoteMissileMotion {
    pub(crate) fn new(seq: u32, movement: MissileMovementState, timing: SampleTiming) -> Self {
        Self {
            buffer: SampleBuffer::new(Some(seq), movement, timing),
            detonation: None,
        }
    }

    pub(crate) fn push(&mut self, seq: u32, movement: MissileMovementState) -> bool {
        self.buffer.push(seq, movement)
    }

    // The reported impact ends the flight after the travel there at the newest sample's speed.
    pub(crate) fn detonate_at(&mut self, pos: Position, tick_secs: f32) {
        let newest = *self.buffer.newest();
        let distance = Vec3::from(pos).distance(Vec3::from(newest.pos));
        let travel_ticks = if newest.speed > f32::EPSILON {
            f64::from(distance / (newest.speed * tick_secs))
        } else {
            0.0
        };
        self.buffer.seal(travel_ticks, MissileMovementState { pos, ..newest });
        self.detonation = Some(pos);
    }

    pub(crate) fn detonation_reached(&self) -> Option<Position> {
        self.detonation.filter(|_| self.buffer.at_end())
    }
}
