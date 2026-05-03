use bevy::prelude::*;

use super::{
    actors::{
        handle_actor_destroyed_message, handle_actor_move_intent_message, handle_actor_teleport_message, sync_actors,
    },
    components::AssetManagers,
    items::{handle_item_collected_message, sync_items},
    login::{handle_player_login_message, handle_player_logoff_message},
    players::{
        handle_player_face_message, handle_player_hit_message, handle_player_jump_message,
        handle_player_move_intent_message, handle_player_shot_message, handle_player_status_message,
        handle_player_teleport_message, sync_players,
    },
    systems::handle_echo_message,
};
use crate::{
    config::{AssetSet, RenderSettings},
    markers::MainCameraMarker,
    resources::{ActorMap, ItemMap, LastUpdateSeq, PlayerMap, RoundTripTime},
    spawning::ProjectileAssets,
};
use common::{
    config::GameplayConfig,
    markers::{ActorMarker, PlayerMarker},
    physics::CollisionWorld,
    protocol::*,
};

// ============================================================================
// Message Dispatcher
// ============================================================================

// Route logged-in messages to appropriate handlers.
pub fn dispatch_message(
    msg: ServerMessage,
    my_player_id: PlayerId,
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    actors: &mut ResMut<ActorMap>,
    items: &mut ResMut<ItemMap>,
    rtt: &mut ResMut<RoundTripTime>,
    last_update_seq: &mut ResMut<LastUpdateSeq>,
    assets: &mut AssetManagers,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    cameras: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    time: &Res<Time>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    render_settings: &RenderSettings,
    projectile_assets: &ProjectileAssets,
    gameplay_config: &GameplayConfig,
    collision_world: Option<&CollisionWorld>,
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
            asset_set,
            render_settings,
            gameplay_config,
            login,
        ),
        ServerMessage::Logoff(logoff) => handle_player_logoff_message(commands, players, logoff),
        ServerMessage::PlayerMoveIntent(move_intent_msg) => {
            handle_player_move_intent_message(commands, players, player_data, rtt, gameplay_config, move_intent_msg);
        }
        ServerMessage::ActorMoveIntent(move_intent_msg) => {
            handle_actor_move_intent_message(commands, actors, rtt, actor_data, move_intent_msg);
        }
        ServerMessage::PlayerTeleport(teleport_msg) => {
            handle_player_teleport_message(commands, players, my_player_id, teleport_msg);
        }
        ServerMessage::ActorTeleport(teleport_msg) => {
            handle_actor_teleport_message(commands, actors, teleport_msg);
        }
        ServerMessage::ActorDestroyed(destroyed_msg) => {
            handle_actor_destroyed_message(
                commands,
                &mut assets.meshes,
                &mut assets.materials,
                actors,
                asset_server,
                asset_set,
                gameplay_config,
                destroyed_msg,
            );
        }
        ServerMessage::Jump(jump_msg) => {
            handle_player_jump_message(commands, players, player_data, rtt, gameplay_config, jump_msg);
        }
        ServerMessage::Face(face_msg) => handle_player_face_message(commands, players, face_msg),
        ServerMessage::Shot(shot_msg) => {
            handle_player_shot_message(
                commands,
                projectile_assets,
                players,
                player_data,
                shot_msg,
                collision_world,
                gameplay_config,
            );
        }
        ServerMessage::Update(update_msg) => handle_update_message(
            commands,
            &mut assets.meshes,
            &mut assets.materials,
            &mut assets.images,
            &mut assets.graphs,
            players,
            actors,
            items,
            rtt,
            last_update_seq,
            player_data,
            actor_data,
            cameras,
            my_player_id,
            asset_server,
            asset_set,
            render_settings,
            gameplay_config,
            update_msg,
        ),
        ServerMessage::Hit(hit_msg) => handle_player_hit_message(commands, players, cameras, my_player_id, hit_msg),
        ServerMessage::PlayerStatus(player_status_msg) => {
            handle_player_status_message(
                commands,
                players,
                player_status_msg,
                my_player_id,
                asset_server,
                asset_set,
            );
        }
        ServerMessage::Echo(echo_msg) => handle_echo_message(time, rtt, echo_msg),
        ServerMessage::CookieCollected(cookie_msg) => {
            handle_item_collected_message(commands, cookie_msg, asset_server, asset_set);
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
