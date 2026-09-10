use bevy::prelude::*;

use crate::{
    actors::{ActorCrushed, ActorInfo, ActorMap, navigation::NavGraphs},
    combat::{PendingExplosions, kill_actor},
    config::ServerGameplayConfig,
    players::PlayerMap,
};
use common::{
    constants::CHARACTER_FALL_DEATH_Y,
    map::Carriers,
    protocol::{ActorId, ActorMarker, CarrierId, Health, PlayerId, Position},
};

// Despawn actors that have fallen below the death threshold, left their
// nested map, been crushed by a carrier, or had their health reduced to
// zero. Health-zero deaths and crushes broadcast their cue and queue a
// blast for the shared resolver. Falls and departures are silent.
//
// Actor entities are despawned outright; the `actors_respawn_system` will
// pick the missing slots up next tick and create replacements.
pub fn actors_removal_system(
    mut commands: Commands,
    mut actors: ResMut<ActorMap>,
    players: Res<PlayerMap>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    carriers: Res<Carriers>,
    nav_graphs: Res<NavGraphs>,
    mut pending_explosions: ResMut<PendingExplosions>,
    query: Query<(Entity, &ActorId, &Position, &Health, &ActorCrushed), With<ActorMarker>>,
) {
    let mut deaths: Vec<ActorDeath> = Vec::new();
    for (entity, id, pos, health, crushed) in query.iter() {
        let Some(info) = actors.get(id) else {
            continue;
        };
        let kind = if pos.y < CHARACTER_FALL_DEATH_Y {
            ActorDeathKind::Fall
        } else if crushed.0 {
            ActorDeathKind::Crushed
        } else if left_carrier(info, pos, &carriers, &nav_graphs) {
            ActorDeathKind::LeftCarrier(info.carrier)
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
            ActorDeathKind::LeftCarrier(carrier) => {
                info!(
                    "{} left carrier {} and despawned at {:?}",
                    actors.describe(&death.id),
                    carrier.0,
                    death.pos
                );
                commands.entity(death.entity).despawn();
                actors.remove(&death.id);
            }
        }
    }
}

// A nested map's actor outside that map's volume — knocked off, or walked
// off a moving edge — has no grid to navigate; root actors have the whole
// world. The carriers are at this tick's pose, where movement left the body.
fn left_carrier(info: &ActorInfo, pos: &Position, carriers: &Carriers, nav_graphs: &NavGraphs) -> bool {
    !info.carrier.is_world()
        && !nav_graphs
            .get(info.carrier)
            .contains(&carriers.pose(info.carrier).inverse_transform_position(pos))
}

#[derive(Copy, Clone)]
enum ActorDeathKind {
    Fall,
    Crushed,
    LeftCarrier(CarrierId),
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
