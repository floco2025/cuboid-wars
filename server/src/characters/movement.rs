use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    physics::{
        CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity, PlayerMovementStep,
        PortalMomentum, PortalSet, overlapping_character, player_control_velocity, step_player_movement,
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
        Option<&'static mut PortalMomentum>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

pub fn characters_movement_system(
    time: Res<Time>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    map_settings: Res<MapSettings>,
    mut players: ResMut<PlayerMap>,
    open_barrier_kinds: Res<OpenBarrierKinds>,
    portal_set: Res<PortalSet>,
    actors: Res<ActorMap>,
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
        &mut players,
        &open_barrier_kinds,
        &portal_set,
        &mut player_query,
        &mut planned_moves,
    );
    plan_actor_moves(
        delta,
        &collision_world,
        &gameplay_config,
        &map_settings,
        &players,
        &open_barrier_kinds,
        &actors,
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
    players: &mut PlayerMap,
    open_barrier_kinds: &OpenBarrierKinds,
    portal_set: &PortalSet,
    query: &mut PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_config = &gameplay_config.player;
    let player_physics = player_config.physics();
    for (entity, pos, motion, move_intent, player_id, knockback, mut momentum) in query.iter_mut() {
        let info = players.get(player_id);
        let control_velocity = player_control_velocity(
            *move_intent,
            gameplay_config,
            info.is_some_and(PlayerInfo::has_speed),
            info.is_some_and(PlayerInfo::is_stunned),
        );

        let has_low_gravity = info.is_some_and(PlayerInfo::has_low_gravity);
        let held_keys: &[BarrierKindId] = info.map_or(&[], |info| &info.life.held_keys);
        let step = step_player_movement(PlayerMovementStep {
            start: *pos,
            vertical_velocity: motion.0,
            control_velocity,
            additional_displacement: Vec3::ZERO,
            delta,
            has_low_gravity,
            held_keys,
            open_barrier_kinds: &open_barrier_kinds.0,
            knockback,
            portal_momentum: momentum.as_deref_mut(),
            collision_world,
            map_settings,
            gameplay_config,
            portal_set,
        });
        if let Some(info) = players.get_mut(player_id) {
            info.life.fall_state.set_support(step.support);
        }

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
        let Ok((_, mut pos, mut motion, _, _, _, _)) = query.get_mut(planned_move.entity) else {
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
