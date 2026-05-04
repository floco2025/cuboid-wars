use bevy::prelude::*;
use common::{config::GameplayConfig, protocol::*};

use super::{actors::sync_actors, items::sync_items, players::sync_players};
use crate::{
    actors::ActorMap,
    cameras::MainCameraMarker,
    config::{AssetSet, RenderSettings},
    items::ItemMap,
    network::{LastUpdateSeq, RoundTripTime},
    players::PlayerMap,
};

// Handle bulk state synchronization from Update message.
pub(super) fn handle_update_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    actors: &mut ResMut<ActorMap>,
    items: &mut ResMut<ItemMap>,
    rtt: &ResMut<RoundTripTime>,
    last_update_seq: &mut ResMut<LastUpdateSeq>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    gameplay_config: &GameplayConfig,
    msg: SUpdate,
) {
    if msg.seq <= last_update_seq.0 {
        warn!(
            "Ignoring outdated SUpdate (seq: {}, last: {})",
            msg.seq, last_update_seq.0
        );
        return;
    }

    last_update_seq.0 = msg.seq;

    sync_players(
        commands,
        meshes,
        materials,
        images,
        graphs,
        players,
        rtt,
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
    sync_items(
        commands,
        meshes,
        materials,
        items,
        asset_server,
        asset_set,
        render_settings,
        &msg.items,
    );
}
