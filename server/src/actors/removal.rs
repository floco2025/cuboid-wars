use bevy::prelude::*;

use crate::{
    actors::{ActorCrushed, ActorMap},
    combat::{PendingExplosions, kill_actor},
    config::ServerGameplayConfig,
    map::MapConfig,
    players::{PlayerMap, checkpoint_progress},
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
    map_config: Res<MapConfig>,
    mut pending_explosions: ResMut<PendingExplosions>,
    query: Query<(Entity, &ActorId, &Position, &Health, &ActorCrushed), With<ActorMarker>>,
) {
    let progress = checkpoint_progress(&players);
    let mut deaths: Vec<ActorDeath> = Vec::new();
    for (entity, id, pos, health, crushed) in query.iter() {
        let Some(info) = actors.get(id) else {
            continue;
        };
        let zone = map_config.actor_spawn_zones.get(info.spawn_zone_index);
        let kind = if !server_gameplay_config.expect_actor(&info.spawn_kind).character.flies()
            && pos.y < CHARACTER_FALL_DEATH_Y
        {
            ActorDeathKind::Void
        } else if crushed.0 {
            ActorDeathKind::Crushed
        } else if health.0 <= 0.0 {
            ActorDeathKind::Killed
        } else if let Some(until) = zone
            .filter(|zone| zone.destroys_at(progress))
            .and_then(|zone| zone.until_checkpoint)
        {
            ActorDeathKind::SelfDestruct(until)
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
            ActorDeathKind::Killed | ActorDeathKind::Crushed | ActorDeathKind::SelfDestruct(_) => {
                match death.kind {
                    ActorDeathKind::Crushed => info!(
                        "{} was crushed by moving geometry at {:?}",
                        actors.describe(&death.id),
                        death.pos
                    ),
                    ActorDeathKind::SelfDestruct(checkpoint) => info!(
                        "{} self-destructs: checkpoint {checkpoint} reached",
                        actors.describe(&death.id)
                    ),
                    _ => {}
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
            ActorDeathKind::Void => {
                info!(
                    "{} fell out of the world at {:?}",
                    actors.describe(&death.id),
                    death.pos
                );
                commands.entity(death.entity).despawn();
                actors.remove(&death.id);
            }
        }
    }
}

// Classified in this order, so a fatal hit keeps its credit over a zone's
// self-destruct at the checkpoint it names.
#[derive(Copy, Clone)]
enum ActorDeathKind {
    Void,
    Crushed,
    Killed,
    SelfDestruct(u32),
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
