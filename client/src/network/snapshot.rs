use bevy::prelude::*;
use common::protocol::*;

use super::{
    actors::{sync_actors, sync_spawning_actors},
    components::{AssetManagers, ClientAssets},
    items::sync_items,
    players::sync_players,
};
use crate::{
    actors::ActorMap,
    cameras::MainCameraMarker,
    items::ItemMap,
    network::{LastSnapshotSeq, RoundTripTime},
    players::PlayerMap,
};

// Handle bulk state synchronization from the `SSnapshot` message.
pub(super) fn handle_snapshot_message(
    commands: &mut Commands,
    assets: &mut AssetManagers,
    players: &mut ResMut<PlayerMap>,
    actors: &mut ResMut<ActorMap>,
    items: &mut ResMut<ItemMap>,
    rtt: &ResMut<RoundTripTime>,
    last_snapshot_seq: &mut ResMut<LastSnapshotSeq>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    client_assets: &mut ClientAssets,
    msg: SSnapshot,
) {
    if !last_snapshot_seq.should_accept(msg.seq) {
        warn!(
            "Ignoring outdated SSnapshot (seq: {}, last: {})",
            msg.seq,
            last_snapshot_seq
                .last_raw()
                .map_or_else(|| "none".to_string(), |seq| seq.to_string())
        );
        return;
    }

    last_snapshot_seq.record(msg.seq);

    sync_players(
        commands,
        &mut assets.meshes,
        &mut assets.materials,
        &mut assets.images,
        &mut assets.graphs,
        players,
        rtt,
        &mut client_assets.local_player_info,
        &mut client_assets.game_message_feed,
        &mut client_assets.seen_player_ids,
        &client_assets.quest_log,
        &mut client_assets.pending_banner,
        player_data,
        camera_query,
        my_player_id,
        &client_assets.asset_server,
        &client_assets.asset_set,
        &client_assets.client_settings,
        &client_assets.gameplay_config,
        &msg.players,
    );
    sync_actors(
        commands,
        &mut assets.meshes,
        &mut assets.materials,
        &mut assets.graphs,
        actors,
        rtt,
        actor_data,
        &client_assets.asset_server,
        &client_assets.asset_set,
        &client_assets.client_settings,
        &client_assets.gameplay_config,
        &msg.actors,
    );
    sync_spawning_actors(
        commands,
        &mut client_assets.actor_ghosts,
        &client_assets.asset_server,
        &client_assets.asset_set,
        &client_assets.gameplay_config,
        &msg.spawning_actors,
    );
    sync_items(
        commands,
        &client_assets.item_assets,
        &client_assets.barrier_assets,
        items,
        &msg.items,
    );

    // Snapshot is the system of record for open-by-plate kinds. Server sends
    // these sorted by id so direct Vec equality is stable across ticks.
    if msg.open_barrier_kinds != client_assets.open_barrier_kinds.0 {
        client_assets.open_barrier_kinds.0 = msg.open_barrier_kinds.clone();
    }
}
