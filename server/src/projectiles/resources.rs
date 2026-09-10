use bevy::prelude::*;
use common::protocol::{CProjectileHit, PlayerId};

#[derive(Resource, Default)]
pub struct PendingProjectileHits(pub Vec<(PlayerId, CProjectileHit)>);

impl PendingProjectileHits {
    pub fn push(&mut self, shooter: PlayerId, hit: CProjectileHit) {
        if hit.direction.iter().all(|value| value.is_finite()) {
            self.0.push((shooter, hit));
        }
    }
}
