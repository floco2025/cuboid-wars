use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::PHYSICS_EPSILON,
    physics::{
        CharacterEnvironment, CharacterMovePlan, CharacterStep, CharacterVerticalVelocity, CollisionWorld,
        KnockbackVelocity, overlapping_character, passable_barrier_kinds, step_character_movement,
    },
    protocol::{ActorMarker, BarrierKindId, MapSettings, PlayerId, PlayerMarker, PlayerMoveIntent, Position},
};

use crate::{
    actors::ActorMap,
    actors::{ActorMovementQuery, apply_actor_moves, plan_actor_moves},
    config::ServerGameplayConfig,
    map::OpenBarrierKinds,
    players::{PlayerInfo, PlayerMap},
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
        Option<&'static KnockbackVelocity>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

// Tick blast shoves down after movement consumed this tick's step.
pub fn knockback_decay_system(
    time: Res<Time>,
    gameplay_config: Res<GameplayConfig>,
    mut knockbacks: Query<&mut KnockbackVelocity>,
) {
    let delta = time.delta_secs();
    for mut knockback in &mut knockbacks {
        knockback.decay(delta, gameplay_config.knockback.deceleration);
    }
}

pub fn characters_movement_system(
    time: Res<Time>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_settings: Res<MapSettings>,
    players: Res<PlayerMap>,
    open_barrier_kinds: Res<OpenBarrierKinds>,
    mut actors: ResMut<ActorMap>,
    mut player_query: PlayerMovementQuery,
    mut actor_health: Query<&mut common::protocol::Health, With<ActorMarker>>,
    mut actor_query: ActorMovementQuery,
) {
    let delta = time.delta_secs();
    let mut planned_moves = Vec::new();
    let actor_starts: Vec<(Entity, Position, CharacterPhysicsConfig)> = actor_query
        .iter()
        .filter_map(|(entity, id, pos, _, _, _, _)| {
            let info = actors.get(id)?;
            Some((entity, *pos, gameplay_config.expect_actor(&info.spawn_kind).physics()))
        })
        .collect();

    plan_player_moves(
        delta,
        &collision_world,
        &gameplay_config,
        &map_settings,
        &players,
        &open_barrier_kinds,
        &player_query,
        &mut planned_moves,
    );
    plan_actor_moves(
        delta,
        &collision_world,
        &gameplay_config,
        &server_gameplay_config,
        &map_settings,
        &players,
        &open_barrier_kinds,
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
        &server_gameplay_config,
        &collision_world,
    );
    apply_actor_moves(&mut actor_query, &planned_moves);
}

fn plan_player_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    map_settings: &MapSettings,
    players: &PlayerMap,
    open_barrier_kinds: &OpenBarrierKinds,
    query: &PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_config = &gameplay_config.player;
    let player_physics = player_config.physics();
    for (entity, pos, motion, move_intent, player_id, knockback) in query.iter() {
        let is_stunned = players.get(player_id).is_some_and(PlayerInfo::is_stunned);
        let has_speed_power_up = players.get(player_id).is_some_and(PlayerInfo::has_speed);
        let velocity = move_intent.to_horizontal_velocity(
            player_config.walk_speed,
            player_config.run_speed,
            has_speed_power_up,
            gameplay_config.power_up_effects.speed_multiplier,
        );
        let velocity_sq = velocity.x.mul_add(velocity.x, velocity.z * velocity.z);
        let is_standing_still = velocity_sq < PHYSICS_EPSILON * PHYSICS_EPSILON;
        let suppress_horizontal = is_stunned || is_standing_still;

        // Blast shove applies on top of intent — and regardless of
        // `suppress_horizontal`: stun or idling doesn't anchor you against
        // an explosion.
        let knockback_step = knockback.map_or(Vec3::ZERO, |k| k.step(delta));
        let target_xz = if suppress_horizontal {
            Position {
                x: pos.x + knockback_step.x,
                y: pos.y,
                z: pos.z + knockback_step.z,
            }
        } else {
            Position {
                x: velocity.x.mul_add(delta, pos.x) + knockback_step.x,
                y: pos.y,
                z: velocity.z.mul_add(delta, pos.z) + knockback_step.z,
            }
        };

        let has_low_gravity = players.get(player_id).is_some_and(PlayerInfo::has_low_gravity);
        let held_keys: &[BarrierKindId] = players.get(player_id).map_or(&[], |info| &info.held_keys);
        // Effective passable kinds = held keys ∪ globally-open kinds (plates).
        // One shared helper between server-authoritative movement and
        // client-side prediction so both decide passability identically.
        let passable_kinds = passable_barrier_kinds(held_keys, &open_barrier_kinds.0);
        let step = step_character_movement(
            CharacterStep {
                start: *pos,
                vertical_velocity: motion.0,
                target_x: target_xz.x,
                target_z: target_xz.z,
                delta,
            },
            &CharacterEnvironment {
                collision_world,
                gravity: map_settings.gravity_for(has_low_gravity),
                passable_kinds: &passable_kinds,
                physics: player_physics,
                ladders: gameplay_config.ladders,
            },
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
        let Ok((_, mut pos, mut motion, _, _, _)) = query.get_mut(planned_move.entity) else {
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
