use bevy::prelude::*;

mod health;
mod spawning;

pub use health::characters_health_regeneration_system;
pub use spawning::{generate_actor_spawn_position_in_zone, generate_player_spawn_position};

use crate::{
    config::ServerGameplayConfig,
    resources::{ActorMap, PlayerInfo, PlayerMap},
    systems::actors::{ActorMovementQuery, apply_actor_moves, plan_actor_moves},
};
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::PHYSICS_EPSILON,
    markers::{ActorMarker, PlayerMarker},
    physics::{
        CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, overlapping_character, step_character_movement,
    },
    protocol::{Health, PlayerId, PlayerMoveIntent, Position},
};

type PlayerMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Position,
        &'static mut CharacterVerticalVelocity,
        &'static PlayerMoveIntent,
        &'static PlayerId,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

pub fn characters_movement_system(
    time: Res<Time>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    players: Res<PlayerMap>,
    mut actors: ResMut<ActorMap>,
    mut player_query: PlayerMovementQuery,
    mut actor_health: Query<&mut Health, With<ActorMarker>>,
    mut actor_query: ActorMovementQuery,
) {
    let delta = time.delta_secs();
    let mut planned_moves = Vec::new();
    let actor_starts: Vec<(Entity, Position)> = actor_query
        .iter()
        .map(|(entity, _, pos, _, _, _)| (entity, *pos))
        .collect();

    plan_player_moves(
        delta,
        &collision_world,
        &gameplay_config,
        &players,
        &player_query,
        &mut planned_moves,
    );
    plan_actor_moves(
        delta,
        &collision_world,
        &gameplay_config,
        &server_gameplay_config,
        &players,
        &mut actors,
        &actor_starts,
        &mut actor_query,
        &mut planned_moves,
    );
    apply_player_moves(&mut player_query, &planned_moves);
    detonate_actors_touching_players(
        &mut actor_health,
        &actors,
        &planned_moves,
        server_gameplay_config.actors.contact_explosion_distance,
    );
    apply_actor_moves(&mut actor_query, &planned_moves);
}

fn plan_player_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    query: &PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_config = &gameplay_config.characters.player;
    let player_physics = player_config.physics();
    for (entity, pos, motion, move_intent, player_id) in query.iter() {
        let is_stunned = players.0.get(player_id).is_some_and(|info| info.stun_timer > 0.0);
        let has_speed_power_up = players.0.get(player_id).is_some_and(PlayerInfo::has_speed);
        let velocity =
            move_intent.to_horizontal_velocity(player_config.walk_speed, player_config.run_speed, has_speed_power_up);
        let velocity_sq = velocity.x.mul_add(velocity.x, velocity.z * velocity.z);
        let is_standing_still = velocity_sq < PHYSICS_EPSILON * PHYSICS_EPSILON;
        let suppress_horizontal = is_stunned || is_standing_still;

        let target_xz = if suppress_horizontal {
            *pos
        } else {
            Position {
                x: velocity.x.mul_add(delta, pos.x),
                y: pos.y,
                z: velocity.z.mul_add(delta, pos.z),
            }
        };

        let has_phasing = players.0.get(player_id).is_some_and(PlayerInfo::has_phasing);
        let step = step_character_movement(
            pos,
            motion.0,
            collision_world,
            has_phasing,
            player_physics,
            target_xz.x,
            target_xz.z,
            delta,
        );

        planned_moves.push(CharacterMovePlan {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            physics: player_physics,
            blocked: step.blocked,
        });
    }
}

fn apply_player_moves(query: &mut PlayerMovementQuery, planned_moves: &[CharacterMovePlan]) {
    for planned_move in planned_moves {
        let Ok((_, mut pos, mut motion, _, _)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        if overlapping_character(planned_move, planned_moves).is_some() {
            pos.y = planned_move.target.y;
        } else {
            *pos = planned_move.target;
        }
        motion.0 = planned_move.target_vertical_velocity;
    }
}

fn detonate_actors_touching_players(
    actor_health: &mut Query<&mut Health, With<ActorMarker>>,
    actors: &ActorMap,
    planned_moves: &[CharacterMovePlan],
    contact_explosion_distance: f32,
) {
    for planned_move in planned_moves {
        if actors.0.values().any(|actor| actor.entity == planned_move.entity) {
            continue;
        }

        for actor_entity in planned_moves
            .iter()
            .filter(|other| {
                actors.0.values().any(|actor| actor.entity == other.entity)
                    && character_move_plans_touch(planned_move, other, contact_explosion_distance)
            })
            .map(|actor_move| actor_move.entity)
        {
            if let Ok(mut health) = actor_health.get_mut(actor_entity) {
                health.0 = 0.0;
            }
        }
    }
}

fn character_move_plans_touch(a: &CharacterMovePlan, b: &CharacterMovePlan, contact_explosion_distance: f32) -> bool {
    // Character movement blocks before colliders overlap, so contact uses a
    // configurable surface tolerance instead of requiring actual intersection.
    vertical_ranges_overlap(a.target, a.physics, b.target, b.physics)
        && horizontal_distance_sq(&a.target, &b.target)
            <= contact_distance(a.physics, b.physics, contact_explosion_distance).powi(2)
}

fn contact_distance(a: CharacterPhysicsConfig, b: CharacterPhysicsConfig, contact_explosion_distance: f32) -> f32 {
    horizontal_collider_radius(a) + horizontal_collider_radius(b) + contact_explosion_distance
}

fn horizontal_collider_radius(physics: CharacterPhysicsConfig) -> f32 {
    physics.collider.width.max(physics.collider.depth) / 2.0
}

fn vertical_ranges_overlap(
    a_pos: Position,
    a_physics: CharacterPhysicsConfig,
    b_pos: Position,
    b_physics: CharacterPhysicsConfig,
) -> bool {
    let a_bottom = a_pos.y + a_physics.collider.bottom_y_offset();
    let a_top = a_pos.y + a_physics.collider.top_y_offset();
    let b_bottom = b_pos.y + b_physics.collider.bottom_y_offset();
    let b_top = b_pos.y + b_physics.collider.top_y_offset();
    a_bottom <= b_top && b_bottom <= a_top
}

fn horizontal_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    dx.mul_add(dx, dz * dz)
}
