use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    protocol::{BarrierKindId, Health, MapSettings, PlayerId},
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
    map_settings: Res<MapSettings>,
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
        show_multi_shot: map_settings.weapons.projectiles,
        show_missiles: map_settings.weapons.missiles,
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
mod tests {
    use super::*;
    use crate::players::PlayerInfo;
    use common::protocol::{BarrierKindId, PowerUpKind};

    fn player(name: &str, score: i32) -> PlayerInfo {
        PlayerInfo {
            entity: Entity::PLACEHOLDER,
            score,
            name: name.to_owned(),
            power_ups: [false; PowerUpKind::COUNT],
            stunned: false,
            held_keys: Vec::new(),
            missiles: 0,
            snap_speed: 0.0,
            hops: 0,
            hop_tick: 0,
            disputed_since: None,
        }
    }

    fn map(entries: Vec<(u32, PlayerInfo)>) -> PlayerMap {
        let mut map = PlayerMap::default();
        for (id, info) in entries {
            map.insert(PlayerId(id), info);
        }
        map
    }

    #[test]
    fn content_hash_is_independent_of_insertion_order() {
        let forward = map(vec![(1, player("alice", 3)), (2, player("bob", 0))]);
        let reverse = map(vec![(2, player("bob", 0)), (1, player("alice", 3))]);

        assert_eq!(
            player_list_content_hash(&forward, Some(PlayerId(1))),
            player_list_content_hash(&reverse, Some(PlayerId(1)))
        );
    }

    #[test]
    fn content_hash_changes_when_rendered_fields_change() {
        let base = map(vec![(1, player("alice", 3))]);
        let base_hash = player_list_content_hash(&base, Some(PlayerId(1)));

        let scored = map(vec![(1, player("alice", 4))]);
        assert_ne!(player_list_content_hash(&scored, Some(PlayerId(1))), base_hash);

        let renamed = map(vec![(1, player("alicia", 3))]);
        assert_ne!(player_list_content_hash(&renamed, Some(PlayerId(1))), base_hash);

        let mut with_key = player("alice", 3);
        with_key.held_keys.push(BarrierKindId(0));
        let keyed = map(vec![(1, with_key)]);
        assert_ne!(player_list_content_hash(&keyed, Some(PlayerId(1))), base_hash);

        let mut with_power_up = player("alice", 3);
        with_power_up.power_ups[PowerUpKind::Speed.index()] = true;
        let powered = map(vec![(1, with_power_up)]);
        assert_ne!(player_list_content_hash(&powered, Some(PlayerId(1))), base_hash);

        let mut with_missiles = player("alice", 3);
        with_missiles.missiles = 2;
        let armed = map(vec![(1, with_missiles)]);
        assert_ne!(player_list_content_hash(&armed, Some(PlayerId(1))), base_hash);

        let joined = map(vec![(1, player("alice", 3)), (2, player("bob", 0))]);
        assert_ne!(player_list_content_hash(&joined, Some(PlayerId(1))), base_hash);

        assert_ne!(player_list_content_hash(&base, Some(PlayerId(2))), base_hash);
    }

    #[test]
    fn content_hash_ignores_fields_rendered_in_place() {
        let base = map(vec![(1, player("alice", 3))]);
        let base_hash = player_list_content_hash(&base, Some(PlayerId(1)));

        // `stunned` is painted by `ui_stunned_blink_system` and `snap_speed`
        // is reconciliation-internal — neither may force a list rebuild.
        let mut in_place = player("alice", 3);
        in_place.stunned = true;
        in_place.snap_speed = 5.0;
        let updated = map(vec![(1, in_place)]);

        assert_eq!(player_list_content_hash(&updated, Some(PlayerId(1))), base_hash);
    }
}
