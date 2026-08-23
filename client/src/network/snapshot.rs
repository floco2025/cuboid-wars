use bevy::prelude::*;
use common::protocol::*;

use super::{
    actors::{sync_actors, sync_spawning_actors},
    components::{AssetManagers, ClientAssets},
    items::sync_items,
    missiles::sync_missiles,
    players::{PlayerSnapshotAssets, PlayerSnapshotState, sync_players},
};
use crate::{
    actors::ActorMap,
    cameras::MainCameraMarker,
    items::ItemMap,
    network::{LastSnapshotSeq, RoundTripTime},
    players::PlayerMap,
};

pub struct SnapshotState<'a> {
    pub players: &'a mut PlayerMap,
    pub actors: &'a mut ActorMap,
    pub items: &'a mut ItemMap,
    pub rtt: &'a RoundTripTime,
    pub last_snapshot_seq: &'a mut LastSnapshotSeq,
    pub my_player_id: PlayerId,
}

// Handle bulk state synchronization from the `SSnapshot` message.
#[expect(
    clippy::too_many_arguments,
    reason = "queries and system-param bundles stay at this boundary"
)]
pub(super) fn handle_snapshot_message(
    commands: &mut Commands,
    assets: &mut AssetManagers,
    state: &mut SnapshotState,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceYaw), With<PlayerMarker>>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceYaw), With<ActorMarker>>,
    cameras: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    client_assets: &mut ClientAssets,
    msg: SSnapshot,
) {
    if !state.last_snapshot_seq.should_accept(msg.seq) {
        warn!(
            "Ignoring outdated SSnapshot (seq: {}, last: {})",
            msg.seq,
            state
                .last_snapshot_seq
                .last_raw()
                .map_or_else(|| "none".to_string(), |seq| seq.to_string())
        );
        return;
    }

    state.last_snapshot_seq.record(msg.seq);

    let mut player_assets = PlayerSnapshotAssets {
        meshes: &mut assets.meshes,
        materials: &mut assets.materials,
        images: &mut assets.images,
        graphs: &mut assets.graphs,
        asset_server: &client_assets.handles.asset_server,
        asset_set: &client_assets.handles.asset_set,
        client_settings: &client_assets.handles.client_settings,
        gameplay_config: &client_assets.handles.gameplay_config,
    };
    let mut player_state = PlayerSnapshotState {
        players: state.players,
        rtt: state.rtt,
        local_player_info: &mut client_assets.hud.local_player_info,
        feed: &mut client_assets.hud.game_message_feed,
        seen_player_ids: &mut client_assets.hud.seen_player_ids,
        quest_log: &client_assets.hud.quest_log,
        pending_banner: &mut client_assets.hud.pending_banner,
        my_player_id: state.my_player_id,
    };
    sync_players(
        commands,
        &mut player_assets,
        &mut player_state,
        player_data,
        cameras,
        &msg.players,
    );
    sync_actors(
        commands,
        &mut assets.meshes,
        &mut assets.materials,
        &mut assets.graphs,
        state.actors,
        state.rtt,
        actor_data,
        &client_assets.handles.asset_server,
        &client_assets.handles.asset_set,
        &client_assets.handles.client_settings,
        &client_assets.handles.gameplay_config,
        &msg.actors,
    );
    sync_spawning_actors(
        commands,
        &mut client_assets.world_sync.actor_ghosts,
        &client_assets.handles.asset_server,
        &client_assets.handles.asset_set,
        &client_assets.handles.gameplay_config,
        &msg.spawning_actors,
    );
    sync_items(
        commands,
        &client_assets.handles.item_assets,
        &client_assets.handles.barrier_assets,
        &client_assets.handles.missile_assets,
        state.items,
        &msg.items,
    );
    sync_missiles(
        commands,
        &client_assets.handles.missile_assets,
        &mut client_assets.world_sync.missile_map,
        state.rtt,
        &client_assets.world_sync.missile_data,
        &msg.missiles,
    );

    // Snapshot is the system of record for open-by-plate kinds. Server sends
    // these sorted by id so direct Vec equality is stable across ticks.
    if msg.open_barrier_kinds != client_assets.world_sync.open_barrier_kinds.0 {
        client_assets.world_sync.open_barrier_kinds.0 = msg.open_barrier_kinds.clone();
    }

    // Weather and lighting targets; `rain_smoothing_system` and
    // `lighting_blend_system` ease the rendered values.
    client_assets.world_sync.rain_intensity.target = msg.rain_intensity;
    client_assets.world_sync.lighting.target = msg.lighting;
    client_assets.world_sync.lighting.synced = true;
}
