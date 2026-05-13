use bevy::prelude::*;

use super::{
    actors::{handle_actor_death_message, handle_actor_hit_message, handle_actor_move_intent_message},
    components::AssetManagers,
    io::handle_pong_message,
    items::handle_item_collected_message,
    players::{
        handle_player_death_message, handle_player_face_message, handle_player_hit_message, handle_player_jump_message,
        handle_player_move_intent_message, handle_player_shot_message, handle_player_status_message,
    },
    snapshot::handle_snapshot_message,
};
use crate::{
    actors::ActorMap,
    barriers::BarrierAssets,
    cameras::MainCameraMarker,
    config::{AssetSet, ClientSettings},
    items::{ItemAssets, ItemMap},
    network::{LastSnapshotSeq, RoundTripTime},
    players::PlayerMap,
    projectiles::ProjectileAssets,
    ui::{GameMessageFeed, SeenPlayerIds},
};
use common::{config::GameplayConfig, physics::CollisionWorld, protocol::*};

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
    last_snapshot_seq: &mut ResMut<LastSnapshotSeq>,
    assets: &mut AssetManagers,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceDirection), With<ActorMarker>>,
    cameras: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    time: &Res<Time>,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    client_settings: &ClientSettings,
    projectile_assets: &ProjectileAssets,
    item_assets: &ItemAssets,
    barrier_assets: &BarrierAssets,
    gameplay_config: &GameplayConfig,
    collision_world: Option<&CollisionWorld>,
    local_player_info: &mut crate::players::LocalPlayerInfo,
    feed: &mut GameMessageFeed,
    seen_player_ids: &mut SeenPlayerIds,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::PlayerMoveIntent(move_intent_msg) => {
            handle_player_move_intent_message(commands, players, player_data, rtt, gameplay_config, move_intent_msg);
        }
        ServerMessage::ActorMoveIntent(move_intent_msg) => {
            handle_actor_move_intent_message(commands, actors, rtt, actor_data, move_intent_msg);
        }
        ServerMessage::ActorDeath(death_msg) => {
            handle_actor_death_message(
                commands,
                &mut assets.meshes,
                &mut assets.materials,
                asset_server,
                asset_set,
                actors,
                players,
                feed,
                gameplay_config,
                death_msg,
            );
        }
        ServerMessage::PlayerDeath(death_msg) => {
            handle_player_death_message(commands, players, local_player_info, feed, my_player_id, death_msg);
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
        ServerMessage::Snapshot(snapshot_msg) => handle_snapshot_message(
            commands,
            &mut assets.meshes,
            &mut assets.materials,
            &mut assets.images,
            &mut assets.graphs,
            players,
            actors,
            items,
            rtt,
            last_snapshot_seq,
            local_player_info,
            feed,
            seen_player_ids,
            player_data,
            actor_data,
            cameras,
            my_player_id,
            asset_server,
            asset_set,
            client_settings,
            item_assets,
            barrier_assets,
            gameplay_config,
            snapshot_msg,
        ),
        ServerMessage::PlayerHit(hit_msg) => {
            handle_player_hit_message(commands, players, cameras, my_player_id, hit_msg);
        }
        ServerMessage::ActorHit(hit_msg) => handle_actor_hit_message(commands, asset_server, asset_set, hit_msg),
        ServerMessage::PlayerStatus(player_status_msg) => {
            handle_player_status_message(
                commands,
                players,
                feed,
                player_status_msg,
                my_player_id,
                asset_server,
                asset_set,
            );
        }
        ServerMessage::Pong(pong_msg) => handle_pong_message(time, rtt, pong_msg),
        ServerMessage::CookieCollected(cookie_msg) => {
            handle_item_collected_message(commands, cookie_msg, asset_server, asset_set);
        }
    }
}
