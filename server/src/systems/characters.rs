use bevy::prelude::*;
use rand::{RngExt, rng, rngs::ThreadRng};

use super::network::broadcast_to_all;
use crate::resources::{PlayerInfo, PlayerMap};
use common::{
    constants::{ACTOR_SPEED, PHYSICS_EPSILON},
    markers::{ActorMarker, PlayerMarker},
    physics::{
        CharacterVerticalMotion, CollisionWorld, PlannedCharacterMove, overlapping_character, step_character_movement,
    },
    protocol::{
        ActorId, CharacterMoveIntent, CharacterMovementState, FaceDirection, PlayerId, Position, SActorMoveIntent,
        ServerMessage,
    },
};

type PlayerMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Position,
        &'static mut CharacterVerticalMotion,
        &'static CharacterMoveIntent,
        &'static PlayerId,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

type ActorMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static ActorId,
        &'static mut Position,
        &'static mut CharacterVerticalMotion,
        &'static mut CharacterMoveIntent,
        &'static mut FaceDirection,
    ),
    (With<ActorMarker>, Without<PlayerMarker>),
>;

pub fn characters_movement_system(
    time: Res<Time>,
    collision_world: Res<CollisionWorld>,
    players: Res<PlayerMap>,
    mut player_query: PlayerMovementQuery,
    mut actor_query: ActorMovementQuery,
) {
    let delta = time.delta_secs();
    let mut planned_moves = Vec::new();

    plan_player_moves(delta, &collision_world, &players, &player_query, &mut planned_moves);
    plan_actor_moves(delta, &collision_world, &actor_query, &mut planned_moves);
    apply_player_moves(&mut player_query, &planned_moves);
    apply_actor_moves(&players, &mut actor_query, &planned_moves);
}

fn plan_player_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    players: &PlayerMap,
    query: &PlayerMovementQuery,
    planned_moves: &mut Vec<PlannedCharacterMove>,
) {
    for (entity, pos, motion, move_intent, player_id) in query.iter() {
        let is_stunned = players.0.get(player_id).is_some_and(|info| info.stun_timer > 0.0);
        let has_speed_power_up = players.0.get(player_id).is_some_and(PlayerInfo::has_speed);
        let velocity = move_intent.to_player_horizontal_velocity(has_speed_power_up);
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
            motion,
            collision_world,
            has_phasing,
            target_xz.x,
            target_xz.z,
            delta,
        );

        planned_moves.push(PlannedCharacterMove {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            blocked: step.blocked,
        });
    }
}

fn plan_actor_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    query: &ActorMovementQuery,
    planned_moves: &mut Vec<PlannedCharacterMove>,
) {
    for (entity, _, pos, motion, move_intent, _) in query.iter() {
        let velocity = move_intent.to_horizontal_velocity(ACTOR_SPEED);
        let target_x = velocity.x.mul_add(delta, pos.x);
        let target_z = velocity.z.mul_add(delta, pos.z);
        let step = step_character_movement(pos, motion, collision_world, false, target_x, target_z, delta);

        planned_moves.push(PlannedCharacterMove {
            entity,
            start: *pos,
            target: step.position,
            target_vertical_velocity: step.vertical_velocity,
            blocked: step.blocked,
        });
    }
}

fn apply_player_moves(query: &mut PlayerMovementQuery, planned_moves: &[PlannedCharacterMove]) {
    for planned_move in planned_moves {
        let Ok((_, mut pos, mut motion, _, _)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        if overlapping_character(planned_move, planned_moves).is_some() {
            pos.y = planned_move.target.y;
        } else {
            *pos = planned_move.target;
        }
        motion.vertical_velocity = planned_move.target_vertical_velocity;
    }
}

fn apply_actor_moves(players: &PlayerMap, query: &mut ActorMovementQuery, planned_moves: &[PlannedCharacterMove]) {
    let mut rng = rng();
    for planned_move in planned_moves {
        let Ok((_, id, mut pos, mut motion, mut move_intent, mut face_dir)) = query.get_mut(planned_move.entity) else {
            continue;
        };

        let overlapping_move = overlapping_character(planned_move, planned_moves);
        if overlapping_move.is_some() {
            pos.y = planned_move.target.y;
        } else {
            *pos = planned_move.target;
        }
        motion.vertical_velocity = planned_move.target_vertical_velocity;

        if planned_move.blocked || overlapping_move.is_some() {
            let direction = if let Some(other) = overlapping_move {
                separation_direction(&planned_move.start, &other.start, &mut rng)
            } else {
                rng.random_range(0.0..std::f32::consts::TAU)
            };
            *move_intent = CharacterMoveIntent::Moving { direction };
            face_dir.0 = direction;
            broadcast_actor_move_intent(players, *id, *pos, *move_intent, motion.vertical_velocity);
        }
    }
}

fn separation_direction(pos: &Position, other_pos: &Position, rng: &mut ThreadRng) -> f32 {
    let dx = pos.x - other_pos.x;
    let dz = pos.z - other_pos.z;
    if dx.hypot(dz) <= f32::EPSILON {
        rng.random_range(0.0..std::f32::consts::TAU)
    } else {
        dx.atan2(dz)
    }
}

fn broadcast_actor_move_intent(
    players: &PlayerMap,
    id: ActorId,
    pos: Position,
    move_intent: CharacterMoveIntent,
    vertical_velocity: f32,
) {
    broadcast_to_all(
        players,
        ServerMessage::ActorMoveIntent(SActorMoveIntent {
            id,
            movement: CharacterMovementState::new(pos, move_intent, vertical_velocity),
        }),
    );
}
