use bevy::{audio::SpatialScale, audio::Volume, prelude::*};

use super::{
    actors::{handle_actor_death_message, handle_actor_hit_message, handle_actor_move_intent_message},
    components::{AssetManagers, ClientAssets},
    io::handle_pong_message,
    items::{handle_health_potion_collected_message, handle_item_collected_message},
    players::{
        handle_fall_damage_message, handle_player_death_message, handle_player_face_message, handle_player_hit_message,
        handle_player_jump_message, handle_player_knockback_message, handle_player_move_intent_message,
        handle_player_shot_message, handle_player_status_message, handle_quest_completed_message,
        handle_quest_progress_message, handle_quests_assigned_message,
    },
    snapshot::handle_snapshot_message,
};
use crate::{
    actors::ActorMap,
    cameras::MainCameraMarker,
    constants::{EXPLOSION_SOUND_VOLUME, SPATIAL_SOUND_SCALE},
    items::ItemMap,
    network::{LastSnapshotSeq, RoundTripTime},
    players::PlayerMap,
};
use common::{physics::CollisionWorld, protocol::*};

// ============================================================================
// Message Dispatcher
// ============================================================================

// Route logged-in messages to appropriate handlers. The `ClientAssets`
// bundle is threaded through as a single param — most handlers want some
// subset of its fields, and unpacking once at the SystemParam level keeps
// the dispatch call sites short.
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
    collision_world: Option<&CollisionWorld>,
    client_assets: &mut ClientAssets,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::PlayerMoveIntent(move_intent_msg) => {
            handle_player_move_intent_message(
                commands,
                players,
                player_data,
                rtt,
                &client_assets.gameplay_config,
                move_intent_msg,
            );
        }
        ServerMessage::ActorMoveIntent(move_intent_msg) => {
            handle_actor_move_intent_message(commands, actors, rtt, actor_data, move_intent_msg);
        }
        ServerMessage::ActorDeath(death_msg) => {
            handle_actor_death_message(
                commands,
                &mut assets.materials,
                &client_assets.asset_server,
                &client_assets.asset_set,
                &client_assets.explosion_assets,
                &client_assets.explosion_radii,
                actors,
                players,
                &client_assets.gameplay_config,
                death_msg,
            );
        }
        ServerMessage::PlayerDeath(death_msg) => {
            // Explosion sound here rather than in the handler (same pattern
            // as the pressure-plate sounds below) — the handler stays
            // constructible in unit tests without an `AssetServer`. Spatial:
            // attenuates and pans with distance from the blast.
            commands.spawn((
                AudioPlayer::new(
                    client_assets
                        .asset_server
                        .load(client_assets.asset_set.player_sound("explodes").to_owned()),
                ),
                PlaybackSettings::DESPAWN
                    .with_spatial(true)
                    .with_spatial_scale(SpatialScale::new(SPATIAL_SOUND_SCALE))
                    .with_volume(Volume::Linear(EXPLOSION_SOUND_VOLUME)),
                Transform::from_translation(Vec3::from(death_msg.pos)),
            ));
            handle_player_death_message(
                commands,
                &mut assets.materials,
                &client_assets.explosion_assets,
                &client_assets.explosion_radii,
                players,
                &mut client_assets.local_player_info,
                &mut client_assets.game_message_feed,
                &client_assets.client_settings,
                &mut client_assets.pending_banner,
                &client_assets.gameplay_config,
                my_player_id,
                death_msg,
            );
        }
        ServerMessage::Jump(jump_msg) => {
            handle_player_jump_message(
                commands,
                players,
                player_data,
                rtt,
                &client_assets.gameplay_config,
                jump_msg,
            );
        }
        ServerMessage::Face(face_msg) => handle_player_face_message(commands, players, face_msg),
        ServerMessage::Shot(shot_msg) => {
            handle_player_shot_message(
                commands,
                &client_assets.projectile_assets,
                players,
                player_data,
                shot_msg,
                collision_world,
                &client_assets.gameplay_config,
                &client_assets.open_barrier_kinds,
            );
        }
        ServerMessage::Snapshot(snapshot_msg) => handle_snapshot_message(
            commands,
            assets,
            players,
            actors,
            items,
            rtt,
            last_snapshot_seq,
            player_data,
            actor_data,
            cameras,
            my_player_id,
            client_assets,
            snapshot_msg,
        ),
        ServerMessage::PlayerHit(hit_msg) => {
            handle_player_hit_message(
                commands,
                players,
                cameras,
                &client_assets.client_settings,
                my_player_id,
                hit_msg,
            );
        }
        ServerMessage::ActorHit(hit_msg) => handle_actor_hit_message(
            commands,
            actors,
            actor_data,
            &client_assets.asset_server,
            &client_assets.asset_set,
            hit_msg,
        ),
        ServerMessage::PlayerStatus(player_status_msg) => {
            handle_player_status_message(
                commands,
                players,
                &mut client_assets.game_message_feed,
                player_status_msg,
                my_player_id,
                &client_assets.asset_server,
                &client_assets.asset_set,
            );
        }
        ServerMessage::Pong(pong_msg) => handle_pong_message(time, rtt, pong_msg),
        ServerMessage::CookieCollected(cookie_msg) => {
            handle_item_collected_message(
                commands,
                cookie_msg,
                &client_assets.asset_server,
                &client_assets.asset_set,
                players,
                my_player_id,
            );
        }
        ServerMessage::HealthPotionCollected(potion_msg) => {
            handle_health_potion_collected_message(
                commands,
                potion_msg,
                &client_assets.asset_server,
                &client_assets.asset_set,
                players,
                my_player_id,
            );
        }
        ServerMessage::PlayerKnockback(knockback_msg) => {
            handle_player_knockback_message(commands, players, my_player_id, knockback_msg);
        }
        ServerMessage::PlayerFallDamage(fall_msg) => {
            handle_fall_damage_message(
                commands,
                players,
                cameras,
                &client_assets.client_settings,
                my_player_id,
                &client_assets.asset_server,
                &client_assets.asset_set,
                fall_msg,
            );
        }
        ServerMessage::QuestsAssigned(quest_msg) => {
            handle_quests_assigned_message(
                &mut client_assets.quest_log,
                &client_assets.client_settings,
                &mut client_assets.pending_banner,
                quest_msg,
            );
        }
        ServerMessage::QuestProgress(quest_msg) => {
            handle_quest_progress_message(&mut client_assets.quest_log, quest_msg);
        }
        ServerMessage::QuestCompleted(quest_msg) => {
            handle_quest_completed_message(
                commands,
                &mut client_assets.quest_log,
                &client_assets.client_settings,
                &mut client_assets.pending_banner,
                &client_assets.asset_server,
                &client_assets.asset_set,
                quest_msg,
            );
        }
        ServerMessage::PressurePlatePressed(_) => {
            commands.spawn((
                AudioPlayer::new(
                    client_assets
                        .asset_server
                        .load(client_assets.asset_set.player_sound("plate_press").to_owned()),
                ),
                PlaybackSettings::DESPAWN,
            ));
        }
        ServerMessage::PressurePlateReleased(_) => {
            commands.spawn((
                AudioPlayer::new(
                    client_assets
                        .asset_server
                        .load(client_assets.asset_set.player_sound("plate_release").to_owned()),
                ),
                PlaybackSettings::DESPAWN,
            ));
        }
    }
}
