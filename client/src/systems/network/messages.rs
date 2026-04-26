use bevy::prelude::*;

use super::{
    components::AssetManagers,
    items::handle_item_collected_message,
    login::{handle_player_login_message, handle_player_logoff_message},
    players::{
        handle_player_face_message, handle_player_hit_message, handle_player_move_input_message,
        handle_player_shot_message, handle_player_status_message,
    },
    systems::handle_echo_message,
};
use crate::{
    markers::MainCameraMarker,
    resources::{ItemMap, LastUpdateSeq, PlayerMap, RoundTripTime},
};
use common::{markers::PlayerMarker, protocol::*};

// ============================================================================
// Message Dispatcher
// ============================================================================

// Route logged-in messages to appropriate handlers.
pub fn dispatch_message(
    msg: ServerMessage,
    my_player_id: PlayerId,
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    items: &mut ResMut<ItemMap>,
    rtt: &mut ResMut<RoundTripTime>,
    last_update_seq: &mut ResMut<LastUpdateSeq>,
    assets: &mut AssetManagers,
    player_data: &Query<(&Position, &FaceDirection), With<PlayerMarker>>,
    cameras: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    time: &Res<Time>,
    asset_server: &Res<AssetServer>,
    map_layout: Option<&MapLayout>,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::Login(login) => handle_player_login_message(
            commands,
            &mut assets.meshes,
            &mut assets.materials,
            &mut assets.images,
            &mut assets.graphs,
            players,
            asset_server,
            login,
        ),
        ServerMessage::Logoff(logoff) => handle_player_logoff_message(commands, players, logoff),
        ServerMessage::MoveInput(move_input_msg) => {
            handle_player_move_input_message(commands, players, player_data, rtt, move_input_msg);
        }
        ServerMessage::Face(face_msg) => handle_player_face_message(commands, players, face_msg),
        ServerMessage::Shot(shot_msg) => {
            handle_player_shot_message(
                commands,
                &mut assets.meshes,
                &mut assets.materials,
                players,
                player_data,
                shot_msg,
                map_layout,
            );
        }
        ServerMessage::Update(update_msg) => handle_update_message(
            commands,
            &mut assets.meshes,
            &mut assets.materials,
            &mut assets.images,
            &mut assets.graphs,
            players,
            items,
            rtt,
            last_update_seq,
            player_data,
            cameras,
            my_player_id,
            asset_server,
            update_msg,
        ),
        ServerMessage::Hit(hit_msg) => handle_player_hit_message(commands, players, cameras, my_player_id, hit_msg),
        ServerMessage::PlayerStatus(player_status_msg) => {
            handle_player_status_message(commands, players, player_status_msg, my_player_id, asset_server);
        }
        ServerMessage::Echo(echo_msg) => handle_echo_message(time, rtt, echo_msg),
        ServerMessage::CookieCollected(cookie_msg) => {
            handle_item_collected_message(commands, cookie_msg, asset_server);
        }
    }
}

// ============================================================================
// Update Message Handler
// ============================================================================

// Handle bulk state synchronization from Update message.
pub fn handle_update_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    items: &mut ResMut<ItemMap>,
    rtt: &ResMut<RoundTripTime>,
    last_update_seq: &mut ResMut<LastUpdateSeq>,
    player_data: &Query<(&Position, &FaceDirection), With<PlayerMarker>>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    msg: SUpdate,
) {
    // Ignore outdated updates
    if msg.seq <= last_update_seq.0 {
        warn!(
            "Ignoring outdated SUpdate (seq: {}, last: {})",
            msg.seq, last_update_seq.0
        );
        return;
    }

    // Update the last received sequence number
    last_update_seq.0 = msg.seq;

    super::players::sync_players(
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
        &msg.players,
    );
    super::items::sync_items(commands, meshes, materials, items, asset_server, &msg.items);
}
