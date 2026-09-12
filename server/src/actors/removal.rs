use bevy::prelude::*;

use crate::{
    actors::{ActorCrushed, ActorMap},
    combat::{PendingExplosions, kill_actor},
    config::ServerGameplayConfig,
    players::PlayerMap,
};
use common::{
    constants::CHARACTER_FALL_DEATH_Y,
    protocol::{ActorId, ActorMarker, Health, PlayerId, Position},
};

pub fn actors_removal_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    players: Res<PlayerMap>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    mut pending_explosions: ResMut<PendingExplosions>,
    query: Query<(Entity, &ActorId, &Position, &Health, &ActorCrushed), With<ActorMarker>>,
) {
    let mut deaths: Vec<ActorDeath> = Vec::new();
    for (entity, id, pos, health, crushed) in query.iter() {
        let Some(info) = actors.get(id) else {
            continue;
        };
        let kind = if info.flight.is_none() && pos.y < CHARACTER_FALL_DEATH_Y {
            ActorDeathKind::Fall
        } else if crushed.0 {
            ActorDeathKind::Crushed
        } else if health.0 <= 0.0 {
            ActorDeathKind::Killed
        } else {
            continue;
        };
        deaths.push(ActorDeath {
            entity,
            id: *id,
            pos: *pos,
            killer: matches!(kind, ActorDeathKind::Killed)
                .then_some(info.last_damager)
                .flatten(),
            kind,
        });
    }

    if deaths.is_empty() {
        return;
    }

    for death in deaths {
        match death.kind {
            ActorDeathKind::Killed | ActorDeathKind::Crushed => {
                if matches!(death.kind, ActorDeathKind::Crushed) {
                    info!(
                        "{} was crushed by moving geometry at {:?}",
                        actors.describe(&death.id),
                        death.pos
                    );
                }
                kill_actor(
                    &mut commands,
                    &mut actors,
                    &players,
                    &mut pending_explosions,
                    &server_gameplay_config.feed,
                    death.id,
                    death.entity,
                    death.pos,
                    death.killer,
                );
            }
            ActorDeathKind::Fall => {
                info!("{} fell and despawned at {:?}", actors.describe(&death.id), death.pos);
                commands.entity(death.entity).despawn();
                actors.remove(&death.id);
            }
        }
    }
}

#[derive(Copy, Clone)]
enum ActorDeathKind {
    Fall,
    Crushed,
    Killed,
}

struct ActorDeath {
    entity: Entity,
    id: ActorId,
    pos: Position,
    killer: Option<PlayerId>,
    kind: ActorDeathKind,
}

#[cfg(test)]
#[path = "tests/removal.rs"]
mod tests;
