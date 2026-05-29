use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindTable, Health, PlayerId},
};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::{
    components::PlayerListMarker,
    entry::{player_health, spawn_player_entry},
};
use crate::{
    barriers::BarrierAssets,
    config::ClientSettings,
    players::{MyPlayerId, PlayerMap},
};

pub fn ui_player_list_rebuild_system(
    mut commands: Commands,
    players: Res<PlayerMap>,
    my_player_id: Option<Res<MyPlayerId>>,
    gameplay_config: Res<GameplayConfig>,
    client_settings: Res<ClientSettings>,
    kind_table: Res<BarrierKindTable>,
    barrier_assets: Option<Res<BarrierAssets>>,
    health_query: Query<&Health>,
    player_list_ui: Single<Entity, With<PlayerListMarker>>,
    children_query: Query<&Children>,
    mut last_content: Local<Option<u64>>,
) {
    if !players.is_changed() {
        return;
    }

    let local_player_id = my_player_id.as_ref().map(|id| id.0);

    // `PlayerMap` is mutated every snapshot, so `is_changed()` alone fires
    // constantly. Only rebuild when something the entries actually render
    // changed — roster, name, score, power-ups, keys, or which row is local.
    // Health is excluded: its bar fill is animated in place by
    // `ui_health_bar_fill_system`, not by a rebuild.
    let content = player_list_content_hash(&players, local_player_id);
    if *last_content == Some(content) {
        return;
    }
    *last_content = Some(content);

    rebuild_player_list(
        &mut commands,
        *player_list_ui,
        &players,
        local_player_id,
        &gameplay_config,
        client_settings.hud.font_sizes.player_list,
        client_settings.hud.font_sizes.score,
        client_settings.hud.health_bars.player_list_width,
        client_settings.hud.health_bars.player_list_height,
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
    hud_font_size: f32,
    score_font_size: f32,
    health_bar_width: f32,
    health_bar_height: f32,
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
            hud_font_size,
            score_font_size,
            health_bar_width,
            health_bar_height,
        );
        ordered_children.push(entity);
    }

    commands.entity(player_list_entity).replace_children(&ordered_children);
}

// Hash of everything the player-list entries render (excluding health, which is
// animated in place). Sorted by id so the result is order-independent.
fn player_list_content_hash(players: &PlayerMap, local_player_id: Option<PlayerId>) -> u64 {
    let mut entries: Vec<_> = players.iter().collect();
    entries.sort_by_key(|(id, _)| id.0);

    let mut hasher = DefaultHasher::new();
    local_player_id.map(|id| id.0).hash(&mut hasher);
    for (id, info) in entries {
        id.0.hash(&mut hasher);
        info.name.hash(&mut hasher);
        info.score.hash(&mut hasher);
        info.power_ups.hash(&mut hasher);
        info.held_keys.hash(&mut hasher);
    }
    hasher.finish()
}
