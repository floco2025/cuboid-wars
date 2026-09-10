use bevy::prelude::*;
use common::protocol::ActorMovementState;

use crate::network::{SampleBuffer, SampleTiming};

// World travel of the interpolated body this frame, for wheel animation.
#[derive(Component, Default)]
pub(crate) struct ActorAnimationVelocity(pub Vec3);

// An actor's server samples, played back behind their tick timeline.
#[derive(Component)]
pub(crate) struct RemoteActorMotion(pub(crate) SampleBuffer<ActorMovementState>);

impl RemoteActorMotion {
    pub(crate) fn new(tick: u32, movement: ActorMovementState, timing: SampleTiming) -> Self {
        Self(SampleBuffer::new(Some(tick), movement, timing))
    }

    pub(crate) fn push(&mut self, tick: u32, movement: ActorMovementState) -> bool {
        self.0.push(tick, movement)
    }
}
