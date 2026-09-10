use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindId, Health, ItemType, MapItems, PlayerId, PowerUpKind},
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use super::{
    components::PlayerListMarker,
    entry::{PlayerEntryStyle, player_health, spawn_player_entry},
    shapes::HudShapeAssets,
};
use crate::{
    barriers::{BarrierAssets, KeyKinds},
    characters::MaxHealth,
    config::ClientSettings,
    players::{MyPlayerId, PlayerMap},
};

pub fn ui_player_list_rebuild_system(
    mut commands: Commands,
    players: Res<PlayerMap>,
    my_player_id: Res<MyPlayerId>,
    gameplay_config: Res<GameplayConfig>,
    map_items: Res<MapItems>,
    max_health: Res<MaxHealth>,
    client_settings: Res<ClientSettings>,
    key_kinds: Res<KeyKinds>,
    barrier_assets: Res<BarrierAssets>,
    shapes: Res<HudShapeAssets>,
    health_query: Query<&Health>,
    player_list_ui: Single<Entity, With<PlayerListMarker>>,
    children_query: Query<&Children>,
    mut last_content: Local<Option<u64>>,
) {
    if !players.is_changed() {
        return;
    }

    let local_player_id = Some(my_player_id.0);

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

    let style = PlayerEntryStyle {
        name_font_size: client_settings.hud.font_sizes.player_list,
        score_font_size: client_settings.hud.font_sizes.score,
        min_entry_width: client_settings.hud.health_bars.player_list_width,
        health_bar_height: client_settings.hud.health_bars.player_list_height,
        max_missiles: gameplay_config.missiles.max_missiles,
        power_up_kinds: PowerUpKind::ALL
            .into_iter()
            .filter(|kind| map_items.contains(kind.to_item_type()))
            .collect(),
        show_missiles: map_items.contains(ItemType::MissilePack),
    };
    rebuild_player_list(
        &mut commands,
        *player_list_ui,
        &players,
        local_player_id,
        max_health.player,
        &style,
        &key_kinds.0,
        Some(&barrier_assets),
        &shapes,
        &health_query,
        &children_query,
    );
}

fn rebuild_player_list(
    commands: &mut Commands,
    player_list_entity: Entity,
    players: &PlayerMap,
    local_player_id: Option<PlayerId>,
    max_health: f32,
    style: &PlayerEntryStyle,
    key_kinds: &[BarrierKindId],
    barrier_assets: Option<&BarrierAssets>,
    shapes: &HudShapeAssets,
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
    for (player_id, player_info) in sorted_players {
        let current_health = player_health(player_info, health_query, max_health);
        let entity = spawn_player_entry(
            commands,
            player_info,
            *player_id,
            local_player_id == Some(*player_id),
            max_health,
            current_health,
            key_kinds,
            barrier_assets,
            shapes,
            style,
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
        info.missiles.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
#[path = "tests/rebuild.rs"]
mod tests;
