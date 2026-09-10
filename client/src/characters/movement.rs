use bevy::prelude::*;

use super::PreviousTickPosition;
use crate::{
    actors::ActorMap,
    config::{AssetSet, ClientSettings},
    players::{LocalPlayerMarker, PlayerMap, PlayerMovementQuery, apply_player_moves, plan_player_moves},
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{CharacterMovePlan, CollisionWorld, PortalSet},
    protocol::{ActorId, ActorMarker, MapSettings, PlateState, PlayerMarker, Position},
};

// Run at the start of each fixed tick, before `characters_movement_system`,
// so `PreviousTickPosition` captures the value `Position` had at the end of
// the previous tick. The render-rate transform sync then lerps between
// these two values for smooth motion above 30 Hz.
pub fn capture_previous_tick_position_system(
    mut query: Query<(&Position, &mut PreviousTickPosition), With<LocalPlayerMarker>>,
) {
    for (pos, mut prev) in &mut query {
        prev.0 = *pos;
    }
}

pub fn characters_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    client_settings: Res<ClientSettings>,
    gameplay_config: Res<GameplayConfig>,
    collision_world: Res<CollisionWorld>,
    map_settings: Res<MapSettings>,
    players: Res<PlayerMap>,
    actors: Res<ActorMap>,
    plates: Res<PlateState>,
    portal_set: Res<PortalSet>,
    carriers: Res<Carriers>,
    mut players_query: PlayerMovementQuery,
    actors_query: Query<(Entity, &ActorId, &Position), (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let delta = time.delta_secs();
    let mut planned_moves = Vec::new();

    plan_player_moves(
        &mut commands,
        delta,
        &collision_world,
        &map_settings,
        &gameplay_config,
        &players,
        &plates,
        &portal_set,
        &carriers,
        &mut players_query,
        &mut planned_moves,
    );
    planned_moves.extend(actors_query.iter().filter_map(|(entity, id, pos)| {
        let info = actors.get(id)?;
        Some(CharacterMovePlan::stationary(
            entity,
            *pos,
            0.0,
            gameplay_config.expect_actor(&info.kind).physics(),
        ))
    }));
    apply_player_moves(
        &mut commands,
        delta,
        &asset_server,
        &asset_set,
        &client_settings.audio,
        &mut players_query,
        &planned_moves,
    );
}
