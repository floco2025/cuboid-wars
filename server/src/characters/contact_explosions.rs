use std::collections::HashMap;

use bevy::prelude::*;
use common::{
    config::CharacterPhysicsConfig,
    physics::{CharacterMovePlan, CollisionWorld, character_center, character_vertical_ranges_overlap},
    protocol::{ActorMarker, Health},
};

use crate::{actors::ActorMap, config::ServerGameplayConfig};

pub(super) fn detonate_actors_touching_players(
    actor_health: &mut Query<&mut Health, With<ActorMarker>>,
    actors: &ActorMap,
    planned_moves: &[CharacterMovePlan],
    server_gameplay_config: &ServerGameplayConfig,
    collision_world: &CollisionWorld,
) {
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
                character_move_plans_touch(planned_move, other, trigger_gap, collision_world)
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
) -> bool {
    // Character movement blocks before colliders overlap, so contact uses a
    // configurable surface tolerance instead of requiring actual intersection.
    if !character_vertical_ranges_overlap(a.target, a.physics, b.target, b.physics)
        || a.target.horizontal_distance_sq(&b.target) > contact_distance(a.physics, b.physics, trigger_gap).powi(2)
    {
        return false;
    }
    collision_world.line_of_sight_clear(
        character_center(a.target, a.physics),
        character_center(b.target, b.physics),
    )
}

fn contact_distance(a: CharacterPhysicsConfig, b: CharacterPhysicsConfig, trigger_gap: f32) -> f32 {
    horizontal_collider_radius(a) + horizontal_collider_radius(b) + trigger_gap
}

fn horizontal_collider_radius(physics: CharacterPhysicsConfig) -> f32 {
    physics.collider.width.max(physics.collider.depth) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{
        config::GameplayConfig,
        constants::WALL_THICKNESS,
        protocol::{BarrierKindTable, MapLayout, Position, Wall},
    };

    fn plans() -> (CharacterMovePlan, CharacterMovePlan, f32) {
        let gameplay = GameplayConfig::load_default().expect("default gameplay config should load");
        let server = ServerGameplayConfig::load_default().expect("default server gameplay config should load");
        let player_physics = gameplay.player.physics();
        let actor_physics = gameplay.expect_actor("mine").physics();
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
                .expect_actor("mine")
                .attack
                .contact_trigger_gap()
                .expect("mine contact attack missing from server gameplay config"),
        )
    }

    #[test]
    fn nearby_player_triggers_contact_explosion_without_cover() {
        let (player, actor, distance) = plans();
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());

        assert!(character_move_plans_touch(&player, &actor, distance, &world));
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
            }],
            ..default()
        };
        let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());

        assert!(!character_move_plans_touch(&player, &actor, distance, &world));
    }

    #[test]
    fn vertically_separated_player_does_not_trigger_contact_explosion() {
        let (mut player, actor, distance) = plans();
        player.target.y = 3.0;
        let world = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());

        assert!(!character_move_plans_touch(&player, &actor, distance, &world));
    }
}
