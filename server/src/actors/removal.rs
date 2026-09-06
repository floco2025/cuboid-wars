use bevy::prelude::*;

use crate::{
    actors::{ActorCrushed, ActorInfo, ActorMap, navigation::NavGraphs},
    combat::{PendingExplosions, kill_actor},
    config::ServerGameplayConfig,
    players::PlayerMap,
};
use common::constants::CHARACTER_FALL_DEATH_Y;
use common::map::Carriers;
use common::protocol::{ActorId, ActorMarker, CarrierId, Health, PlayerId, Position};

// Despawn actors that have fallen below the death threshold, left their
// nested map, been crushed by a carrier, or had their health reduced to
// zero. Health-zero death broadcasts its cue and queues a blast for the
// shared resolver. Falls, departures, and crushes are silent — falls were
// teleports before, so the asymmetry is preserved, and an actor off its
// carrier has no grid to navigate.
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
            ActorDeathKind::Killed => {
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
            ActorDeathKind::Crushed => {
                info!(
                    "{} was crushed by moving geometry at {:?}",
                    actors.describe(&death.id),
                    death.pos
                );
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
mod tests {
    use super::*;
    use crate::map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig};
    use crate::test_geometry::{CELL, LEVEL_HEIGHT, geometry};
    use common::protocol::{Carrier, MapLayout};

    // A world grid and a 2x2 two-storey nested grid whose carrier rests at
    // x = 20 on the ground storey.
    fn fixture() -> (NavGraphs, Carriers) {
        let level = |cols, rows| {
            let mut cells = CellGrid::new(cols, rows);
            for row in &mut cells.rows {
                for cell in row {
                    cell.has_floor = true;
                }
            }
            LevelGrid {
                cells,
                edges: EdgeGrid::new(cols, rows),
                barrier_edges: EdgeGrid::new(cols, rows),
            }
        };
        let mut map = MapConfig::for_grid(vec![level(8, 8)], geometry(8, 8));
        map.grids.push(CarrierGrid::new(
            CarrierId(1),
            geometry(2, 2),
            vec![level(2, 2), level(2, 2)],
        ));
        let rest = Position {
            x: 20.0,
            y: 0.0,
            z: 0.0,
        };
        let carriers = Carriers::from_layout(&MapLayout {
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: rest,
                to: rest,
                travel_ticks: 1,
                pause_ticks: 0,
                phase_ticks: 0,
            }],
            ..MapLayout::default()
        });
        (NavGraphs::new(&map), carriers)
    }

    fn actor(carrier: CarrierId) -> ActorInfo {
        ActorInfo::new(Entity::from_bits(1), 0, "mine".to_owned(), carrier)
    }

    #[test]
    fn an_actor_below_its_carriers_floor_has_left_it() {
        let (graphs, carriers) = fixture();
        let below = Position {
            x: 20.0,
            y: -1.0,
            z: 0.0,
        };
        assert!(left_carrier(&actor(CarrierId(1)), &below, &carriers, &graphs));
        assert!(!left_carrier(&actor(CarrierId::WORLD), &below, &carriers, &graphs));
    }

    #[test]
    fn an_actor_beside_its_carrier_has_left_it() {
        let (graphs, carriers) = fixture();
        let beside = Position {
            x: 20.0 + CELL * 1.5,
            y: 0.0,
            z: 0.0,
        };
        assert!(left_carrier(&actor(CarrierId(1)), &beside, &carriers, &graphs));
    }

    #[test]
    fn an_actor_on_its_carriers_upper_storey_is_still_aboard() {
        let (graphs, carriers) = fixture();
        let upstairs = Position {
            x: 20.0,
            y: LEVEL_HEIGHT,
            z: 0.0,
        };
        assert!(!left_carrier(&actor(CarrierId(1)), &upstairs, &carriers, &graphs));
        let above_the_roof = Position {
            y: 2.0 * LEVEL_HEIGHT,
            ..upstairs
        };
        assert!(left_carrier(&actor(CarrierId(1)), &above_the_roof, &carriers, &graphs));
    }
}
