use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindTable, Health, PlayerId},
};

use super::{
    components::PlayerListMarker,
    entry::{player_health, spawn_player_entry},
};
use crate::{
    barriers::BarrierAssets,
    players::{MyPlayerId, PlayerMap},
};

pub fn ui_player_list_rebuild_system(
    mut commands: Commands,
    players: Res<PlayerMap>,
    my_player_id: Option<Res<MyPlayerId>>,
    gameplay_config: Res<GameplayConfig>,
    kind_table: Res<BarrierKindTable>,
    barrier_assets: Option<Res<BarrierAssets>>,
    health_query: Query<&Health>,
    player_list_ui: Single<Entity, With<PlayerListMarker>>,
    children_query: Query<&Children>,
) {
    if !players.is_changed() {
        return;
    }

    let local_player_id = my_player_id.as_ref().map(|id| id.0);

    rebuild_player_list(
        &mut commands,
        *player_list_ui,
        &players,
        local_player_id,
        &gameplay_config,
        &kind_table,
        barrier_assets.as_deref(),
        &health_query,
        &children_query,
    );
}

#[allow(clippy::too_many_arguments)]
fn rebuild_player_list(
    commands: &mut Commands,
    player_list_entity: Entity,
    players: &PlayerMap,
    local_player_id: Option<PlayerId>,
    gameplay_config: &GameplayConfig,
    kind_table: &BarrierKindTable,
    barrier_assets: Option<&BarrierAssets>,
    health_query: &Query<&Health>,
    children_query: &Query<&Children>,
) {
    if let Ok(children) = children_query.get(player_list_entity) {
        for &child in children {
            commands.entity(child).despawn();
        }
    }

    let mut sorted_players: Vec<_> = players.iter().collect();
    sorted_players.sort_by_key(|(player_id, _)| player_id.0);

    let mut ordered_children = Vec::with_capacity(sorted_players.len());
    let max_health = gameplay_config.player.health().max;
    for (player_id, player_info) in sorted_players {
        let current_health = player_health(player_info, health_query, max_health);
        let entity = spawn_player_entry(
            commands,
            player_info,
            *player_id,
            local_player_id == Some(*player_id),
            max_health,
            current_health,
            kind_table,
            barrier_assets,
        );
        ordered_children.push(entity);
    }

    commands.entity(player_list_entity).replace_children(&ordered_children);
}
