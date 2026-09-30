use bevy::prelude::*;

use crate::{
    actors::ActorMap,
    config::{AssetSet, ClientSettings},
    players::{LocalPlayerInfo, PlayerMap, PlayerMovementQuery, apply_player_moves, plan_player_moves},
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{CharacterMovePlan, CollisionWorld, PortalSet},
    protocol::{ActorId, ActorMarker, MapSettings, PlayerMarker, Position, SwitchState},
};

pub(crate) fn characters_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    client_settings: Res<ClientSettings>,
    gameplay_config: Res<GameplayConfig>,
    collision_world: Res<CollisionWorld>,
    map_settings: Res<MapSettings>,
    players: Res<PlayerMap>,
    local: Res<LocalPlayerInfo>,
    actors: Res<ActorMap>,
    switch_state: Res<SwitchState>,
    portal_set: Res<PortalSet>,
    carriers: Res<Carriers>,
    mut players_query: PlayerMovementQuery,
    actors_query: Query<(Entity, &ActorId, &Position), (With<ActorMarker>, Without<PlayerMarker>)>,
) {
    let delta = time.delta_secs();
    let blockers: Vec<_> = actors_query
        .iter()
        .filter_map(|(entity, id, pos)| {
            let info = actors.get(id)?;
            Some(CharacterMovePlan::stationary(
                entity,
                *pos,
                0.0,
                gameplay_config.expect_actor(&info.kind).physics(),
            ))
        })
        .collect();
    let planned_moves = plan_player_moves(
        delta,
        &collision_world,
        &map_settings,
        &gameplay_config,
        &players,
        &switch_state,
        &portal_set,
        &carriers,
        local.is_dead,
        &mut players_query,
        &blockers,
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
}
