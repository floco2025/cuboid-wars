use std::collections::HashMap;

use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::{CharacterMovePlan, CollisionWorld, character_hitbox_center, character_surface_distance},
    protocol::{ActorId, ActorMarker, BarrierKindId, Health, PlateState, PlayerMarker, Position},
};

use crate::{actors::ActorMap, config::ServerGameplayConfig};

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
    let mut plans: Vec<_> = players
        .iter()
        .map(|(entity, pos)| CharacterMovePlan::stationary(entity, *pos, 0.0, gameplay.player.physics()))
        .collect();
    plans.extend(actor_positions.iter().filter_map(|(entity, id, pos)| {
        let info = actors.get(id)?;
        Some(CharacterMovePlan::stationary(
            entity,
            *pos,
            0.0,
            gameplay.expect_actor(&info.spawn_kind).physics(),
        ))
    }));
    detonate_actors_touching_players(
        &mut health,
        &actors,
        &plans,
        &config,
        &collision,
        &plates.open_barrier_kinds,
    );
}

pub(super) fn detonate_actors_touching_players(
    actor_health: &mut Query<&mut Health, With<ActorMarker>>,
    actors: &ActorMap,
    planned_moves: &[CharacterMovePlan],
    server_gameplay_config: &ServerGameplayConfig,
    collision_world: &CollisionWorld,
    open_barriers: &[BarrierKindId],
) {
    if actors.peaceful {
        return;
    }
    // Actor entity → its contact-explosion distance, resolved once. Runs in the
    // 30 Hz movement tick over players + actors; without this the nested
    // `actors.values()` scans make it O((P+A)·A) per tick. Every actor stays
    // in the map (the outer skip must recognize all actor plans); `None`
    // distance = a kind that never contact-detonates.
    let actor_contact_distance: HashMap<Entity, Option<f32>> = actors
        .values()
        .map(|actor| {
            let distance = server_gameplay_config
                .expect_actor(&actor.spawn_kind)
                .attack
                .contact_trigger_gap();
            (actor.entity, distance)
        })
        .collect();

    for planned_move in planned_moves {
        // Only players detonate actors they touch; skip actor plans.
        if actor_contact_distance.contains_key(&planned_move.entity) {
            continue;
        }

        for actor_entity in planned_moves
            .iter()
            .filter(|other| {
                let Some(&Some(trigger_gap)) = actor_contact_distance.get(&other.entity) else {
                    return false;
                };
                character_move_plans_touch(planned_move, other, trigger_gap, collision_world, open_barriers)
            })
            .map(|actor_move| actor_move.entity)
        {
            if let Ok(mut health) = actor_health.get_mut(actor_entity) {
                health.0 = 0.0;
            }
        }
    }
}

fn character_move_plans_touch(
    a: &CharacterMovePlan,
    b: &CharacterMovePlan,
    trigger_gap: f32,
    collision_world: &CollisionWorld,
    open_barriers: &[BarrierKindId],
) -> bool {
    // Character movement blocks before colliders overlap, so contact uses a
    // configurable surface tolerance instead of requiring actual intersection.
    if character_surface_distance(a.target, a.physics, b.target, b.physics) > trigger_gap {
        return false;
    }
    collision_world.attack_path_clear(
        character_hitbox_center(a.target, a.physics),
        character_hitbox_center(b.target, b.physics),
        open_barriers,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actors::ActorInfo,
        test_geometry::{WALL_HEIGHT, WALL_THICKNESS},
    };
    use common::protocol::{ActorId, Barrier, BarrierKindTable, CarrierId, MapLayout, Position, Wall};

    #[test]
    fn touching_an_actor_does_not_detonate_it_during_peace() {
        let server = ServerGameplayConfig::load_default().expect("server gameplay config rejected");
        let collision_world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let (player, mut actor, _) = plans();
        let mut world = World::new();
        actor.entity = world.spawn((ActorMarker, Health(100.0))).id();
        let mut actors = ActorMap::default();
        actors.insert(
            ActorId(1),
            ActorInfo::new(actor.entity, 0, "scuttler".into(), CarrierId::WORLD),
        );
        let entity = actor.entity;
        let moves = [player, actor];
        for (peaceful, expected_health) in [(true, 100.0), (false, 0.0)] {
            actors.set_peaceful(peaceful);
            let mut state = world.query_filtered::<&mut Health, With<ActorMarker>>();
            detonate_actors_touching_players(
                &mut state.query_mut(&mut world),
                &actors,
                &moves,
                &server,
                &collision_world,
                &[],
            );
            assert_eq!(
                world.get::<Health>(entity).expect("actor health missing").0,
                expected_health
            );
        }
    }

    fn plans() -> (CharacterMovePlan, CharacterMovePlan, f32) {
        let server = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let gameplay = server.gameplay_config();
        let player_physics = gameplay.player.physics();
        let actor_physics = gameplay.expect_actor("scuttler").physics();
        let player_pos = Position {
            x: -0.3,
            y: 0.0,
            z: 0.0,
        };
        let actor_pos = Position { x: 0.3, y: 0.0, z: 0.0 };
        (
            CharacterMovePlan::from_target(Entity::from_bits(1), player_pos, player_pos, 0.0, player_physics, false),
            CharacterMovePlan::from_target(Entity::from_bits(2), actor_pos, actor_pos, 0.0, actor_physics, false),
            server
                .expect_actor("scuttler")
                .attack
                .contact_trigger_gap()
                .expect("scuttler contact attack missing from server gameplay config"),
        )
    }

    #[test]
    fn nearby_player_triggers_contact_explosion_without_cover() {
        let (player, actor, distance) = plans();
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());

        assert!(character_move_plans_touch(&player, &actor, distance, &world, &[]));
    }

    #[test]
    fn wall_blocks_contact_explosion() {
        let (player, actor, distance) = plans();
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

        assert!(!character_move_plans_touch(&player, &actor, distance, &world, &[]));
    }

    #[test]
    fn vertically_separated_player_does_not_trigger_contact_explosion() {
        let (mut player, actor, distance) = plans();
        player.target.y = 3.0;
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());

        assert!(!character_move_plans_touch(&player, &actor, distance, &world, &[]));
    }

    #[test]
    fn closed_barrier_blocks_contact_detonation() {
        let (player, actor, distance) = plans();
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
        assert!(!character_move_plans_touch(&player, &actor, distance, &world, &[]));
        assert!(character_move_plans_touch(&player, &actor, distance, &world, &[kind]));
    }
}
