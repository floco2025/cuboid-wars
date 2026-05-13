use bevy::prelude::*;
use common::{config::GameplayConfig, protocol::*};

use super::{actors::sync_actors, items::sync_items, players::sync_players};
use crate::{
    actors::ActorMap,
    barriers::BarrierAssets,
    cameras::MainCameraMarker,
    config::{AssetSet, RenderSettings},
    items::{ItemAssets, ItemMap},
    network::{LastSnapshotSeq, RoundTripTime},
    players::PlayerMap,
    ui::{GameMessageFeed, SeenPlayerIds},
};

// Handle bulk state synchronization from the `SSnapshot` message.
pub(super) fn handle_snapshot_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    actors: &mut ResMut<ActorMap>,
    items: &mut ResMut<ItemMap>,
    rtt: &ResMut<RoundTripTime>,
    last_snapshot_seq: &mut ResMut<LastSnapshotSeq>,
    local_player_info: &mut crate::players::LocalPlayerInfo,
    feed: &mut GameMessageFeed,
    seen_player_ids: &mut SeenPlayerIds,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    item_assets: &ItemAssets,
    barrier_assets: &BarrierAssets,
    gameplay_config: &GameplayConfig,
    msg: SSnapshot,
) {
    if msg.seq <= last_snapshot_seq.0 {
        warn!(
            "Ignoring outdated SSnapshot (seq: {}, last: {})",
            msg.seq, last_snapshot_seq.0
        );
        return;
    }

    last_snapshot_seq.0 = msg.seq;

    sync_players(
        commands,
        meshes,
        materials,
        images,
        graphs,
        players,
        rtt,
        local_player_info,
        feed,
        seen_player_ids,
        player_data,
        camera_query,
        my_player_id,
        asset_server,
        asset_set,
        render_settings,
        gameplay_config,
        &msg.players,
    );
    sync_actors(
        commands,
        meshes,
        materials,
        images,
        graphs,
        actors,
        rtt,
        actor_data,
        asset_server,
        asset_set,
        render_settings,
        gameplay_config,
        &msg.actors,
    );
    sync_items(commands, item_assets, barrier_assets, items, &msg.items);
}
