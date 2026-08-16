use bevy::prelude::*;
use std::collections::HashSet;

use super::handlers::player_movement_velocity;
use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    config::{AssetSet, ClientSettings},
    network::{RoundTripTime, ServerReconciliation},
    players::{LocalPlayerInfo, PlayerInfo, PlayerMap, spawn_player},
    ui::{GameMessage, GameMessageFeed, PendingBanner, QuestLog, SeenPlayerIds},
};
use common::{
    config::GameplayConfig,
    physics::CharacterVerticalVelocity,
    protocol::{FaceDirection, Player, PlayerId, PlayerMarker, PlayerMoveIntent, Position, PowerUpKind},
};

#[expect(
    clippy::too_many_arguments,
    reason = "snapshot reconciliation needs the asset stack at this entry point"
)]
pub fn sync_players(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    local_player_info: &mut LocalPlayerInfo,
    feed: &mut GameMessageFeed,
    seen_player_ids: &mut SeenPlayerIds,
    quest_log: &QuestLog,
    pending_banner: &mut PendingBanner,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    client_settings: &ClientSettings,
    gameplay_config: &GameplayConfig,
    server_players: &[(PlayerId, Player)],
) {
    let update_ids: HashSet<PlayerId> = server_players.iter().map(|(id, _)| *id).collect();

    // Spawn newly-appeared players. Skip the local player if it's already in
    // the map (e.g., we kept its entity through death — see respawn handling
    // further down).
    for (id, player) in server_players {
        if players.contains_key(id) {
            continue;
        }

        spawn_snapshot_player(
            commands,
            meshes,
            materials,
            images,
            graphs,
            players,
            local_player_info,
            camera_query,
            my_player_id,
            asset_server,
            asset_set,
            client_settings,
            gameplay_config,
            *id,
            player,
        );

        // Skip the local player's own "joined" line — they know they
        // joined. Also suppress duplicate "joined" lines on respawn —
        // a respawning player looks identical to a fresh join from the
        // snapshot diff's perspective, so we gate on first-ever-seen.
        let is_first_seen = seen_player_ids.insert_if_new(*id);
        if *id != my_player_id && is_first_seen {
            feed.push(GameMessage::PlayerJoined {
                name: player.name.clone(),
            });
        }
    }

    // Handle players no longer in the snapshot:
    //   * remote players → despawn entity, drop PlayerInfo (logoff / death of
    //     another player both look identical from this side).
    //   * the local player → keep entity (camera/mouse-look need it), hide
    //     visibility, keep PlayerInfo (preserves score), insert
    //     LocalPlayerDead. The next snapshot that re-includes our id will
    //     teleport us to the respawn position.
    let mut local_just_died = false;
    players.retain(|id, player| {
        if update_ids.contains(id) {
            return true;
        }
        if *id == my_player_id {
            commands.entity(player.entity).insert(Visibility::Hidden);
            local_just_died = true;
            true
        } else {
            commands.entity(player.entity).despawn();
            feed.push(GameMessage::PlayerLeft {
                name: player.name.clone(),
            });
            false
        }
    });
    if local_just_died {
        local_player_info.is_dead = true;
    }

    // Handle local-player respawn: if we were dead and our id reappeared in
    // this snapshot, hard-teleport our existing entity to the new spawn
    // position, restore visibility, and clear the death state.
    let mut local_just_respawned = false;
    if local_player_info.is_dead
        && let Some((_, server_player)) = server_players.iter().find(|(id, _)| *id == my_player_id)
        && let Some(info) = players.get_mut(&my_player_id)
    {
        let entity = info.entity;
        // Adopt the server-assigned respawn facing. Without this the next
        // input frame recomputes yaw from the unchanged camera transform and
        // overwrites `FaceDirection` with the pre-death facing.
        apply_local_spawn_facing(
            commands,
            camera_query,
            local_player_info,
            &server_player.movement.pos,
            server_player.face_dir,
        );
        commands.entity(info.entity).insert((
            server_player.movement.pos,
            // Reset the previous-tick anchor so render interpolation doesn't
            // smear the respawn teleport across one render frame.
            PreviousTickPosition(server_player.movement.pos),
            FaceDirection(server_player.face_dir),
            CharacterVerticalVelocity(server_player.movement.vertical_velocity),
            server_player.health,
            Visibility::Visible,
        ));
        // Clear any stale `ServerReconciliation` that survived the death
        // window — the next-snapshot `update_snapshot_player` call below is
        // skipped for the local player this frame (see below), so without
        // this remove() the recon component would linger pointing at the
        // pre-death position.
        commands.entity(entity).remove::<ServerReconciliation>();
        info.apply_snapshot(server_player);
        local_player_info.is_dead = false;
        local_just_respawned = true;

        // Re-show the announcement (title + description) for still-active
        // quests, so a respawning player is reminded of their objectives.
        // ONE combined banner in display order — the pending slot holds a
        // single request, so per-quest sets would keep only the last quest.
        let text = quest_log
            .sorted()
            .into_iter()
            .filter(|(_, entry)| !entry.completed)
            .map(|(_, entry)| format!("{}: {}", entry.title, entry.description))
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            pending_banner.set(text, client_settings.hud.banner.quest_announcement_duration_secs);
        }
    }

    for (id, server_player) in server_players {
        // On the respawn frame the local player was just hard-teleported by
        // the block above; the Query still sees the pre-respawn position,
        // so handing it to `update_snapshot_player` would produce a huge
        // bogus reconciliation delta. Their `PlayerInfo` was already synced.
        if local_just_respawned && *id == my_player_id {
            continue;
        }
        update_snapshot_player(
            commands,
            players,
            rtt,
            player_data,
            my_player_id,
            gameplay_config,
            *id,
            server_player,
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy spawn path needs asset resources at the call site"
)]
fn spawn_snapshot_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    local_player_info: &mut LocalPlayerInfo,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    client_settings: &ClientSettings,
    gameplay_config: &GameplayConfig,
    id: PlayerId,
    player: &Player,
) {
    let is_local = id == my_player_id;
    debug!(
        "spawning {}#{} from Snapshot (is_local: {})",
        player.name, id.0, is_local
    );
    let entity = spawn_player(
        commands,
        asset_server,
        meshes,
        materials,
        images,
        graphs,
        asset_set,
        client_settings,
        gameplay_config,
        id.0,
        &player.name,
        &player.movement.pos,
        player.movement.move_intent,
        player.health,
        player.face_dir,
        is_local,
    );
    commands
        .entity(entity)
        .insert(CharacterVerticalVelocity(player.movement.vertical_velocity));

    if is_local {
        apply_local_spawn_facing(
            commands,
            camera_query,
            local_player_info,
            &player.movement.pos,
            player.face_dir,
        );
    }

    players.insert(id, PlayerInfo::from_snapshot(entity, player));
}

// Point the main camera (and the stored mouse-look fallback state) at the
// server-assigned spawn facing.
fn apply_local_spawn_facing(
    commands: &mut Commands,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    local_player_info: &mut LocalPlayerInfo,
    pos: &Position,
    face_dir: f32,
) {
    let camera_rotation = face_dir + std::f32::consts::PI;
    if let Ok(camera_entity) = camera_query.single() {
        commands
            .entity(camera_entity)
            .insert(Transform::from_xyz(pos.x, 2.5, pos.z + 3.0).with_rotation(Quat::from_rotation_y(camera_rotation)));
    }
    local_player_info.stored_yaw = camera_rotation;
    local_player_info.stored_pitch = 0.0;
}

fn update_snapshot_player(
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    my_player_id: PlayerId,
    gameplay_config: &GameplayConfig,
    id: PlayerId,
    server_player: &Player,
) {
    if let Some(client_player) = players.get_mut(&id) {
        if let Ok((client_pos, _, _)) = player_data.get(client_player.entity) {
            let server_velocity = player_movement_velocity(
                server_player.movement,
                gameplay_config.player.walk_speed,
                gameplay_config.player.run_speed,
                server_player.power_up(PowerUpKind::Speed),
            );

            if id != my_player_id {
                commands
                    .entity(client_player.entity)
                    .insert(server_player.movement.move_intent);
            }
            commands.entity(client_player.entity).insert(ServerReconciliation::new(
                *client_pos,
                server_player.movement.pos,
                server_velocity,
                rtt,
            ));
            if id != my_player_id {
                commands
                    .entity(client_player.entity)
                    .insert(CharacterVerticalVelocity(server_player.movement.vertical_velocity));
            }
        }

        client_player.apply_snapshot(server_player);
        commands.entity(client_player.entity).insert(server_player.health);
    }
}
