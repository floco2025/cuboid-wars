use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    physics::{CollisionWorld, character_hitbox_center, character_surface_distance},
    protocol::{ActorId, ActorMarker, BarrierKindId, Health, PlateState, PlayerMarker, Position},
};

use crate::{actors::ActorMap, config::ServerGameplayConfig};

// A character where this tick left it.
#[derive(Clone, Copy)]
pub(super) struct CharacterBody {
    pub entity: Entity,
    pub pos: Position,
    pub physics: CharacterPhysicsConfig,
}

pub(super) fn contact_explosions_system(
    mut health: Query<&mut Health, With<ActorMarker>>,
    actors: Res<ActorMap>,
    gameplay: Res<GameplayConfig>,
    config: Res<ServerGameplayConfig>,
    collision: Res<CollisionWorld>,
    plates: Res<PlateState>,
    players: Query<(Entity, &Position), With<PlayerMarker>>,
    actor_positions: Query<(Entity, &ActorId, &Position), With<ActorMarker>>,
) {
    let players: Vec<_> = players
        .iter()
        .map(|(entity, pos)| CharacterBody {
            entity,
            pos: *pos,
            physics: gameplay.player.physics(),
        })
        .collect();
    // Actors whose kind detonates on contact, with the kind's trigger gap.
    let contact_actors: Vec<_> = actor_positions
        .iter()
        .filter_map(|(entity, id, pos)| {
            let kind = &actors.get(id)?.spawn_kind;
            let trigger_gap = config.expect_actor(kind).attack.contact_trigger_gap()?;
            let body = CharacterBody {
                entity,
                pos: *pos,
                physics: gameplay.expect_actor(kind).physics(),
            };
            Some((body, trigger_gap))
        })
        .collect();
    detonate_actors_touching_players(
        &mut health,
        actors.peaceful,
        &players,
        &contact_actors,
        &collision,
        &plates.open_barrier_kinds,
    );
}

pub(super) fn detonate_actors_touching_players(
    actor_health: &mut Query<&mut Health, With<ActorMarker>>,
    peaceful: bool,
    players: &[CharacterBody],
    contact_actors: &[(CharacterBody, f32)],
    collision_world: &CollisionWorld,
    open_barriers: &[BarrierKindId],
) {
    if peaceful {
        return;
    }
    for player in players {
        for (actor, trigger_gap) in contact_actors {
            if character_bodies_touch(player, actor, *trigger_gap, collision_world, open_barriers)
                && let Ok(mut health) = actor_health.get_mut(actor.entity)
            {
                health.0 = 0.0;
            }
        }
    }
}

fn character_bodies_touch(
    a: &CharacterBody,
    b: &CharacterBody,
    trigger_gap: f32,
    collision_world: &CollisionWorld,
    open_barriers: &[BarrierKindId],
) -> bool {
    // Character movement blocks before colliders overlap, so contact uses a
    // configurable surface tolerance instead of requiring actual intersection.
    if character_surface_distance(a.pos, a.physics, b.pos, b.physics) > trigger_gap {
        return false;
    }
    collision_world.attack_path_clear(
        character_hitbox_center(a.pos, a.physics),
        character_hitbox_center(b.pos, b.physics),
        open_barriers,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_geometry::{WALL_HEIGHT, WALL_THICKNESS};
    use common::protocol::{Barrier, BarrierKindTable, CarrierId, MapLayout, Wall};

    #[test]
    fn touching_an_actor_does_not_detonate_it_during_peace() {
        let collision_world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let (player, mut actor, trigger_gap) = bodies();
        let mut world = World::new();
        actor.entity = world.spawn((ActorMarker, Health(100.0))).id();
        let entity = actor.entity;
        for (peaceful, expected_health) in [(true, 100.0), (false, 0.0)] {
            let mut state = world.query_filtered::<&mut Health, With<ActorMarker>>();
            detonate_actors_touching_players(
                &mut state.query_mut(&mut world),
                peaceful,
                &[player],
                &[(actor, trigger_gap)],
                &collision_world,
                &[],
            );
            assert_eq!(
                world.get::<Health>(entity).expect("actor health missing").0,
                expected_health
            );
        }
    }

    fn bodies() -> (CharacterBody, CharacterBody, f32) {
        let server = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let gameplay = server.gameplay_config();
        let player = CharacterBody {
            entity: Entity::from_bits(1),
            pos: Position {
                x: -0.3,
                y: 0.0,
                z: 0.0,
            },
            physics: gameplay.player.physics(),
        };
        let actor = CharacterBody {
            entity: Entity::from_bits(2),
            pos: Position { x: 0.3, y: 0.0, z: 0.0 },
            physics: gameplay.expect_actor("scuttler").physics(),
        };
        (
            player,
            actor,
            server
                .expect_actor("scuttler")
                .attack
                .contact_trigger_gap()
                .expect("scuttler contact attack missing from server gameplay config"),
        )
    }

    #[test]
    fn nearby_player_triggers_contact_explosion_without_cover() {
        let (player, actor, distance) = bodies();
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());

        assert!(character_bodies_touch(&player, &actor, distance, &world, &[]));
    }

    #[test]
    fn wall_blocks_contact_explosion() {
        let (player, actor, distance) = bodies();
        let layout = MapLayout {
            walls: vec![Wall {
                x1: 0.0,
                z1: -2.0,
                x2: 0.0,
                z2: 2.0,
                width: WALL_THICKNESS,
                level: 0,
                y: 0.0,
                height: WALL_HEIGHT,
                carrier: CarrierId::WORLD,
            }],
            ..default()
        };
        let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());

        assert!(!character_bodies_touch(&player, &actor, distance, &world, &[]));
    }

    #[test]
    fn vertically_separated_player_does_not_trigger_contact_explosion() {
        let (mut player, actor, distance) = bodies();
        player.pos.y = 3.0;
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());

        assert!(!character_bodies_touch(&player, &actor, distance, &world, &[]));
    }

    #[test]
    fn closed_barrier_blocks_contact_detonation() {
        let (player, actor, distance) = bodies();
        let kind = BarrierKindId(0);
        let layout = MapLayout {
            barriers: vec![Barrier {
                x1: 0.0,
                z1: -2.0,
                x2: 0.0,
                z2: 2.0,
                y: 0.0,
                height: WALL_HEIGHT,
                width: 0.1,
                level: 0,
                levels: 1,
                kind,
                carrier: CarrierId::WORLD,
            }],
            ..default()
        };
        let kinds = BarrierKindTable::from_ids(vec!["shield".into()]).expect("barrier catalog rejected");
        let world = CollisionWorld::from_map_layout(&layout, &kinds);
        assert!(!character_bodies_touch(&player, &actor, distance, &world, &[]));
        assert!(character_bodies_touch(&player, &actor, distance, &world, &[kind]));
    }
}
