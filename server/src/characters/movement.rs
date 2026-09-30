use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    map::Carriers,
    physics::{CharacterMovePlan, CollisionWorld},
    protocol::{ActorMarker, PlayerId, PlayerMarker, Position, SwitchState},
};

use crate::{
    actors::{ActorMap, FreeActorQuery, SurfaceActorMoves, apply_flying_moves, plan_flying_moves},
    players::PlayerMap,
};

type PlayerMovementQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static PlayerId, &'static Position), (With<PlayerMarker>, Without<ActorMarker>)>;

// Flying actors sweep against every other body this tick: players where
// their reports put them, the accepted ground moves, and the anchored actors
// already placed.
pub fn flying_actors_movement_system(
    time: Res<Time>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    players: Res<PlayerMap>,
    switch_state: Res<SwitchState>,
    carriers: Res<Carriers>,
    actors: Res<ActorMap>,
    player_query: PlayerMovementQuery,
    ground_moves: Res<SurfaceActorMoves>,
    mut actor_query: FreeActorQuery,
) {
    let delta = time.delta_secs();
    let mut blockers: Vec<CharacterMovePlan> = player_query
        .iter()
        .filter_map(|(entity, id, pos)| {
            let info = players.get(id).filter(|info| !info.is_dead())?;
            Some(CharacterMovePlan::stationary(
                entity,
                *pos,
                info.life.movement.vertical_velocity,
                info.life.movement.stance.physics(&gameplay_config.player),
            ))
        })
        .collect();
    blockers.extend(ground_moves.0.iter().copied());
    let mut flying_starts: Vec<(Entity, Position, CharacterPhysicsConfig)> = Vec::new();
    for actor in actor_query.iter() {
        let Some(info) = actors.get(actor.id) else {
            continue;
        };
        let physics = actor.character.0.physics();
        if info.anchor.is_some() {
            blockers.push(CharacterMovePlan::stationary(
                actor.entity,
                *actor.position,
                0.0,
                physics,
            ));
        } else {
            flying_starts.push((actor.entity, *actor.position, physics));
        }
    }
    let mut planned_moves = blockers.clone();
    plan_flying_moves(
        delta,
        &collision_world,
        &switch_state,
        &carriers,
        &actors,
        &flying_starts,
        &mut actor_query,
        &mut planned_moves,
    );
    apply_flying_moves(
        &mut actor_query,
        &blockers,
        &planned_moves[blockers.len()..],
        &collision_world,
        &switch_state.open_fields,
    );
}
