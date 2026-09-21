use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    map::Carriers,
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{ActorMarker, PlayerId, PlayerMarker, Position, SwitchState},
};

use crate::{
    actors::{ActorCharacter, ActorMap, ActorMovementQuery, SurfaceAgent, apply_actor_moves, plan_actor_moves},
    players::PlayerMap,
};

type PlayerMovementQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static PlayerId, &'static Position), (With<PlayerMarker>, Without<ActorMarker>)>;

pub fn characters_movement_system(
    time: Res<Time>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    players: Res<PlayerMap>,
    switch_state: Res<SwitchState>,
    carriers: Res<Carriers>,
    actors: Res<ActorMap>,
    player_query: PlayerMovementQuery,
    ground_query: Query<(Entity, &Position, &ActorCharacter, &SurfaceAgent)>,
    mut actor_query: ActorMovementQuery,
) {
    let delta = time.delta_secs();
    let mut planned_moves = Vec::new();
    let actor_starts: Vec<(Entity, Position, CharacterPhysicsConfig)> = actor_query
        .iter()
        .filter_map(|(entity, id, _, pos, _, _, _, _, _, _, _, _)| {
            let info = actors.get(id)?;
            Some((entity, *pos, gameplay_config.expect_actor(&info.spawn_kind).physics()))
        })
        .chain(
            ground_query
                .iter()
                .map(|(entity, pos, character, _)| (entity, *pos, character.0.physics())),
        )
        .collect();

    planned_moves.extend(player_query.iter().filter_map(|(entity, id, pos)| {
        let info = players.get(id).filter(|info| !info.is_dead())?;
        Some(CharacterMovePlan::stationary(
            entity,
            *pos,
            info.life.movement.vertical_velocity,
            gameplay_config.player.physics(),
        ))
    }));
    planned_moves.extend(ground_query.iter().map(|(entity, position, character, agent)| {
        let start = agent.executor.as_ref().map_or(*position, |executor| executor.start);
        CharacterMovePlan::from_target(entity, start, *position, 0.0, character.0.physics(), false)
    }));
    plan_actor_moves(
        delta,
        &collision_world,
        &switch_state,
        &carriers,
        &actors,
        &actor_starts,
        &mut actor_query,
        &mut planned_moves,
    );
    apply_actor_moves(
        &mut actor_query,
        &actors,
        &planned_moves,
        &collision_world,
        &switch_state.open_fields,
    );
}
