use bevy::prelude::*;

use super::{
    actors::{
        handle_actor_beam_message, handle_actor_death_message, handle_actor_hit_message,
        handle_actor_move_intent_message,
    },
    components::{AssetManagers, ClientAssets},
    io::handle_pong_message,
    items::{handle_health_potion_collected_message, handle_item_collected_message},
    missiles::{
        handle_missile_death_message, handle_missile_launch_message, handle_missile_move_intent_message,
        handle_missiles_collected_message,
    },
    players::{
        handle_fall_damage_message, handle_player_blast_message, handle_player_death_message,
        handle_player_hit_message, handle_player_jump_message, handle_player_move_message, handle_player_shot_message,
        handle_player_status_message,
    },
    quests::handle_quest_message,
    snapshot::{SnapshotState, handle_snapshot_message},
};
use crate::{
    actors::ActorMap,
    audio::{play_explosion_sound, play_sound},
    cameras::MainCameraMarker,
    items::ItemMap,
    network::{LastSnapshotSeq, RoundTripTime},
    players::PlayerMap,
    vfx::ExplosionSpawnCtx,
};
use common::{physics::CollisionWorld, protocol::*};

// ============================================================================
// Message Dispatcher
// ============================================================================

// Route logged-in messages to appropriate handlers. The `ClientAssets`
// bundle is threaded through as a single param so the enclosing system
// stays under Bevy's parameter limit; each arm unpacks the fields its
// handler needs.
pub fn dispatch_message(
    msg: ServerMessage,
    my_player_id: PlayerId,
    commands: &mut Commands,
    players: &mut PlayerMap,
    actors: &mut ActorMap,
    items: &mut ItemMap,
    rtt: &mut RoundTripTime,
    last_snapshot_seq: &mut LastSnapshotSeq,
    assets: &mut AssetManagers,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceYaw), With<PlayerMarker>>,
    actor_data: &Query<(&Position, &ActorMoveIntent, &FaceYaw), With<ActorMarker>>,
    cameras: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    time: &Time,
    collision_world: Option<&CollisionWorld>,
    client_assets: &mut ClientAssets,
) {
    match msg {
        ServerMessage::Init(_) => {
            error!("received Init more than once");
        }
        ServerMessage::PlayerMove(move_msg) => {
            handle_player_move_message(
                commands,
                players,
                player_data,
                rtt,
                &client_assets.handles.gameplay_config,
                move_msg,
            );
        }
        ServerMessage::ActorMove(move_intent_msg) => {
            handle_actor_move_intent_message(commands, actors, rtt, actor_data, move_intent_msg);
        }
        ServerMessage::ActorDeath(death_msg) => {
            let mut ctx = ExplosionSpawnCtx {
                meshes: &mut assets.meshes,
                materials: &mut assets.materials,
                budget: &mut client_assets.world_sync.explosion_vfx_budget,
                explosion_assets: &client_assets.handles.explosion_assets,
                gameplay_config: &client_assets.handles.gameplay_config,
                collision_world,
                map_layout: client_assets.world_sync.map_layout.as_deref(),
            };
            handle_actor_death_message(
                commands,
                &mut ctx,
                &client_assets.handles.asset_server,
                &client_assets.handles.asset_set,
                &client_assets.handles.client_settings.audio,
                &client_assets.handles.explosion_radii,
                actors,
                players,
                death_msg,
            );
        }
        ServerMessage::PlayerDeath(death_msg) => {
            // Explosion sound here rather than in the handler (same pattern
            // as the pressure-plate sounds below) — the handler stays
            // constructible in unit tests without an `AssetServer`. Spatial:
            // attenuates and pans with distance from the blast.
            play_explosion_sound(
                commands,
                &client_assets.handles.asset_server,
                client_assets.handles.asset_set.player_sound("explodes"),
                &client_assets.handles.client_settings.audio,
                Vec3::from(death_msg.pos),
                Some(client_assets.handles.explosion_radii.player),
            );
            let mut ctx = ExplosionSpawnCtx {
                meshes: &mut assets.meshes,
                materials: &mut assets.materials,
                budget: &mut client_assets.world_sync.explosion_vfx_budget,
                explosion_assets: &client_assets.handles.explosion_assets,
                gameplay_config: &client_assets.handles.gameplay_config,
                collision_world,
                map_layout: client_assets.world_sync.map_layout.as_deref(),
            };
            handle_player_death_message(
                commands,
                &mut ctx,
                &client_assets.handles.explosion_radii,
                players,
                &mut client_assets.hud.local_player_info,
                &mut client_assets.hud.banner,
                my_player_id,
                death_msg,
            );
        }
        ServerMessage::PlayerJump(jump_msg) => {
            handle_player_jump_message(
                commands,
                players,
                player_data,
                rtt,
                &client_assets.handles.gameplay_config,
                jump_msg,
            );
        }
        ServerMessage::PlayerShot(shot_msg) => {
            handle_player_shot_message(
                commands,
                &client_assets.handles.projectile_assets,
                players,
                player_data,
                shot_msg,
                collision_world,
                &client_assets.handles.gameplay_config,
                &client_assets.world_sync.open_barrier_kinds,
            );
        }
        ServerMessage::Snapshot(snapshot_msg) => {
            let mut state = SnapshotState {
                players,
                actors,
                items,
                rtt,
                last_snapshot_seq,
                my_player_id,
            };
            handle_snapshot_message(
                commands,
                assets,
                &mut state,
                player_data,
                actor_data,
                cameras,
                client_assets,
                snapshot_msg,
            );
        }
        ServerMessage::PlayerHit(hit_msg) => {
            handle_player_hit_message(
                commands,
                players,
                cameras,
                &client_assets.handles.client_settings,
                my_player_id,
                hit_msg,
            );
        }
        ServerMessage::ActorHit(hit_msg) => handle_actor_hit_message(
            commands,
            actors,
            actor_data,
            &client_assets.handles.asset_server,
            &client_assets.handles.asset_set,
            &client_assets.handles.client_settings.audio,
            hit_msg,
        ),
        ServerMessage::ActorBeam(beam_msg) => handle_actor_beam_message(
            commands,
            &mut assets.meshes,
            &mut assets.materials,
            actors,
            actor_data,
            &client_assets.handles.asset_server,
            &client_assets.handles.asset_set,
            &client_assets.handles.client_settings.audio,
            beam_msg,
        ),
        ServerMessage::PlayerStatus(player_status_msg) => {
            handle_player_status_message(
                commands,
                players,
                player_status_msg,
                my_player_id,
                &client_assets.handles.asset_server,
                &client_assets.handles.asset_set,
            );
        }
        ServerMessage::MissileLaunch(launch_msg) => {
            handle_missile_launch_message(
                commands,
                &client_assets.handles.missile_assets,
                &mut client_assets.world_sync.missile_map,
                &client_assets.handles.asset_server,
                &client_assets.handles.asset_set,
                &client_assets.handles.client_settings.audio,
                my_player_id,
                launch_msg,
            );
        }
        ServerMessage::MissileMove(intent_msg) => {
            handle_missile_move_intent_message(
                commands,
                &client_assets.world_sync.missile_map,
                rtt,
                &client_assets.world_sync.missile_data,
                intent_msg,
            );
        }
        ServerMessage::MissileDeath(death_msg) => {
            let mut ctx = ExplosionSpawnCtx {
                meshes: &mut assets.meshes,
                materials: &mut assets.materials,
                budget: &mut client_assets.world_sync.explosion_vfx_budget,
                explosion_assets: &client_assets.handles.explosion_assets,
                gameplay_config: &client_assets.handles.gameplay_config,
                collision_world,
                map_layout: client_assets.world_sync.map_layout.as_deref(),
            };
            handle_missile_death_message(
                commands,
                &mut ctx,
                &client_assets.handles.asset_server,
                &client_assets.handles.asset_set,
                &client_assets.handles.client_settings.audio,
                &mut client_assets.world_sync.missile_map,
                death_msg,
            );
        }
        ServerMessage::MissilesCollected(collected_msg) => {
            handle_missiles_collected_message(
                commands,
                collected_msg,
                &client_assets.handles.asset_server,
                &client_assets.handles.asset_set,
                players,
                my_player_id,
            );
        }
        ServerMessage::Firework(firework_msg) => {
            // Presentation only: derive the whole show from the seed so all
            // clients play the identical choreography.
            client_assets
                .world_sync
                .firework_show
                .start(firework_msg.seed, client_assets.world_sync.map_layout.as_deref());
        }
        ServerMessage::Pong(pong_msg) => handle_pong_message(time, rtt, pong_msg),
        ServerMessage::CookieCollected(cookie_msg) => {
            handle_item_collected_message(
                commands,
                cookie_msg,
                &client_assets.handles.asset_server,
                &client_assets.handles.asset_set,
                players,
                my_player_id,
            );
        }
        ServerMessage::HealthPotionCollected(potion_msg) => {
            handle_health_potion_collected_message(
                commands,
                potion_msg,
                &client_assets.handles.asset_server,
                &client_assets.handles.asset_set,
                players,
                my_player_id,
            );
        }
        ServerMessage::PlayerBlast(blast_msg) => {
            handle_player_blast_message(commands, players, my_player_id, blast_msg);
        }
        ServerMessage::PlayerFallDamage(fall_msg) => {
            handle_fall_damage_message(
                commands,
                players,
                cameras,
                &client_assets.handles.client_settings,
                my_player_id,
                &client_assets.handles.asset_server,
                &client_assets.handles.asset_set,
                fall_msg,
            );
        }
        msg @ (ServerMessage::QuestsAssigned(_)
        | ServerMessage::QuestProgress(_)
        | ServerMessage::QuestCompleted(_)) => {
            if matches!(msg, ServerMessage::QuestCompleted(_)) {
                play_sound(
                    commands,
                    &client_assets.handles.asset_server,
                    client_assets.handles.asset_set.player_sound("quest_completed"),
                );
            }
            handle_quest_message(&mut client_assets.hud.quest_log, &mut client_assets.hud.banner, msg);
        }
        ServerMessage::PressurePlate(plate_msg) => {
            let sound = if plate_msg.pressed {
                "plate_press"
            } else {
                "plate_release"
            };
            play_sound(
                commands,
                &client_assets.handles.asset_server,
                client_assets.handles.asset_set.player_sound(sound),
            );
        }
        ServerMessage::Feed(SFeed { event }) => client_assets.hud.feed.push(event),
    }
}
