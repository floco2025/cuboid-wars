use bevy::prelude::*;
use common::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    map::Carriers,
    physics::{CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld},
    protocol::{ActorMarker, MapSettings, PlateState, PlayerId, PlayerMarker, Position},
};

use crate::{
    actors::{ActorMap, ActorMovementQuery, apply_actor_moves, plan_actor_moves},
    players::PlayerMap,
};

type PlayerMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        &'static Position,
        &'static CharacterVerticalVelocity,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

pub fn characters_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    collision_world: Res<CollisionWorld>,
    gameplay_config: Res<GameplayConfig>,
    map_settings: Res<MapSettings>,
    players: Res<PlayerMap>,
    plates: Res<PlateState>,
    carriers: Res<Carriers>,
    actors: Res<ActorMap>,
    player_query: PlayerMovementQuery,
    mut actor_query: ActorMovementQuery,
) {
    let delta = time.delta_secs();
    let mut planned_moves = Vec::new();
    let actor_starts: Vec<(Entity, Position, CharacterPhysicsConfig)> = actor_query
        .iter()
        .filter_map(|(entity, id, _, pos, _, _, _, _, _, _)| {
            let info = actors.get(id)?;
            Some((entity, *pos, gameplay_config.expect_actor(&info.spawn_kind).physics()))
        })
        .collect();

    planned_moves.extend(player_query.iter().filter_map(|(entity, id, pos, vertical)| {
        players.get(id).filter(|info| !info.is_dead())?;
        Some(CharacterMovePlan::stationary(
            entity,
            *pos,
            vertical.0,
            gameplay_config.player.physics(),
        ))
    }));
    plan_actor_moves(
        delta,
        &collision_world,
        &map_settings,
        &mut commands,
        &plates,
        &carriers,
        &actors,
        &actor_starts,
        &mut actor_query,
        &mut planned_moves,
    );
    apply_actor_moves(&mut actor_query, &actors, &planned_moves);
}
