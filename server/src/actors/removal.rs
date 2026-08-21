use bevy::prelude::*;

use crate::{
    actors::ActorMap,
    combat::{PendingExplosions, kill_actor},
    players::PlayerMap,
};
use common::constants::CHARACTER_FALL_DEATH_Y;
use common::protocol::{ActorId, ActorMarker, Health, PlayerId, Position};

// Despawn actors that have either fallen below the death threshold or had
// their health reduced to zero. Health-zero death broadcasts its cue and
// queues a blast for the shared resolver. Falls are silent —
// they were teleports before, so the asymmetry is preserved.
//
// Actor entities are despawned outright; the `actors_respawn_system` will
// pick the missing slots up next tick and create replacements.
pub fn actors_removal_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    players: Res<PlayerMap>,
    mut pending_explosions: ResMut<PendingExplosions>,
    query: Query<(Entity, &ActorId, &Position, &Health), With<ActorMarker>>,
) {
    let mut deaths: Vec<ActorDeath> = Vec::new();
    for (entity, id, pos, health) in query.iter() {
        let Some(info) = actors.get(id) else {
            continue;
        };
        if pos.y < CHARACTER_FALL_DEATH_Y {
            deaths.push(ActorDeath {
                entity,
                id: *id,
                pos: *pos,
                killer: None,
                cause: DeathCause::Fall,
            });
        } else if health.0 <= 0.0 {
            deaths.push(ActorDeath {
                entity,
                id: *id,
                pos: *pos,
                killer: info.last_damager,
                cause: DeathCause::Killed,
            });
        }
    }

    if deaths.is_empty() {
        return;
    }

    for death in deaths {
        if matches!(death.cause, DeathCause::Killed) {
            kill_actor(
                &mut commands,
                &mut actors,
                &players,
                &mut pending_explosions,
                death.id,
                death.entity,
                death.pos,
                death.killer,
            );
        } else {
            info!("{} fell and despawned at {:?}", actors.describe(&death.id), death.pos);
            commands.entity(death.entity).despawn();
            actors.remove(&death.id);
        }
    }
}

#[derive(Copy, Clone)]
enum DeathCause {
    Fall,
    Killed,
}

struct ActorDeath {
    entity: Entity,
    id: ActorId,
    pos: Position,
    killer: Option<PlayerId>,
    cause: DeathCause,
}
