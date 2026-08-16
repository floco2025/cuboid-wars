use std::collections::VecDeque;

use bevy::prelude::*;

use common::protocol::{ActorId, PlayerId, Position};

pub enum PendingExplosion {
    Player {
        source_id: PlayerId,
        pos: Position,
    },
    Actor {
        source_id: ActorId,
        source_entity: Entity,
        spawn_kind: String,
        pos: Position,
    },
    // The only blast that isn't a death: a missile detonation. Carries the
    // shooter so blast kills credit them (death blasts credit no one).
    Missile {
        shooter: PlayerId,
        pos: Position,
    },
}

#[derive(Resource, Default)]
pub struct PendingExplosions(pub VecDeque<PendingExplosion>);

impl PendingExplosions {
    pub fn push_player(&mut self, source_id: PlayerId, pos: Position) {
        self.0.push_back(PendingExplosion::Player { source_id, pos });
    }

    pub fn push_actor(&mut self, source_id: ActorId, source_entity: Entity, spawn_kind: String, pos: Position) {
        self.0.push_back(PendingExplosion::Actor {
            source_id,
            source_entity,
            spawn_kind,
            pos,
        });
    }

    pub fn push_missile(&mut self, shooter: PlayerId, pos: Position) {
        self.0.push_back(PendingExplosion::Missile { shooter, pos });
    }
}
