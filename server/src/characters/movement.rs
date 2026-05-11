use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    constants::PHYSICS_EPSILON,
    physics::{
        CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, overlapping_character, step_character_movement,
    },
    protocol::{ActorMarker, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

use crate::{
    actors::{ActorMovementQuery, apply_actor_moves, plan_actor_moves},
    config::ServerGameplayConfig,
    resources::{ActorMap, PlayerInfo, PlayerMap},
};

use super::contact_explosions::detonate_actors_touching_players;

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
    mut actor_health: Query<&mut common::protocol::Health, With<ActorMarker>>,
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
    detonate_actors_touching_players(&mut actor_health, &actors, &planned_moves, &server_gameplay_config);
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
    let player_config = &gameplay_config.player;
    let player_physics = player_config.physics();
    for (entity, pos, motion, move_intent, player_id) in query.iter() {
        let is_stunned = players.get(player_id).is_some_and(|info| info.stun_timer > 0.0);
        let has_speed_power_up = players.get(player_id).is_some_and(PlayerInfo::has_speed);
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

        let has_phasing = players.get(player_id).is_some_and(PlayerInfo::has_phasing);
        let has_anti_gravity = players.get(player_id).is_some_and(PlayerInfo::has_anti_gravity);
        let step = step_character_movement(
            pos,
            motion.0,
            collision_world,
            has_phasing,
            has_anti_gravity,
            player_physics,
            target_xz.x,
            target_xz.z,
            delta,
        );

        planned_moves.push(CharacterMovePlan::from_movement_result(
            entity,
            *pos,
            step,
            player_physics,
        ));
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
