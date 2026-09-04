use bevy::prelude::*;

use super::PreviousTickPosition;
use crate::{
    actors::{ActorMap, ActorMovementQuery, actor_start_positions, apply_actor_moves, plan_actor_moves},
    config::{AssetSet, ClientSettings},
    players::{PlayerMap, PlayerMovementQuery, apply_player_moves, plan_player_moves},
};
use common::{
    config::GameplayConfig,
    physics::{CollisionWorld, PortalSet},
    protocol::{ActorMarker, MapSettings, PlateState, PlayerMarker, Position},
};

// Run at the start of each fixed tick, before `characters_movement_system`,
// so `PreviousTickPosition` captures the value `Position` had at the end of
// the previous tick. The render-rate transform sync then lerps between
// these two values for smooth motion above 30 Hz.
pub fn capture_previous_tick_position_system(
    mut query: Query<(&Position, &mut PreviousTickPosition), Or<(With<PlayerMarker>, With<ActorMarker>)>>,
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
    mut players: ResMut<PlayerMap>,
    actors: Res<ActorMap>,
    plates: Res<PlateState>,
    portal_set: Res<PortalSet>,
    mut players_query: PlayerMovementQuery,
    mut actors_query: ActorMovementQuery,
) {
    let delta = time.delta_secs();
    let mut planned_moves = Vec::new();
    let actor_starts = actor_start_positions(&actors_query, &actors, &gameplay_config);

    plan_player_moves(
        &mut commands,
        delta,
        &collision_world,
        &map_settings,
        &gameplay_config,
        &mut players,
        &plates,
        &portal_set,
        &mut players_query,
        &mut planned_moves,
    );
    plan_actor_moves(
        &mut commands,
        delta,
        &collision_world,
        &map_settings,
        &gameplay_config,
        &actors,
        &plates,
        &actor_starts,
        &mut actors_query,
        &mut planned_moves,
    );
    apply_player_moves(
        &mut commands,
        delta,
        &asset_server,
        &asset_set,
        &client_settings.audio,
        &mut players_query,
        &planned_moves,
    );
    apply_actor_moves(&mut actors_query, &planned_moves);
}
