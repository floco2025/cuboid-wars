use bevy::prelude::*;
use std::collections::HashSet;
use std::f32::consts::PI;

use super::super::context::ServerMessageContext;
use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    network::ServerReconciliation,
    players::{LocalPlayerInfo, PlayerInfo, spawn_player},
    ui::BannerMessage,
};
use common::{
    physics::{CharacterVerticalVelocity, PortalMomentum},
    protocol::{FaceYaw, Player, PlayerId, Position},
};

pub(in crate::network) fn sync_players(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
    server_players: &[(PlayerId, Player)],
) {
    let update_ids: HashSet<PlayerId> = server_players.iter().map(|(id, _)| *id).collect();

    if let Some((_, me)) = server_players.iter().find(|(id, _)| *id == my_player_id) {
        *context.portal_access = me.portal_access;
    }

    // Spawn newly-appeared players. Skip the local player if it's already in
    // the map (e.g., we kept its entity through death — see respawn handling
    // further down).
    for (id, player) in server_players {
        if context.players.contains_key(id) {
            continue;
        }

        spawn_snapshot_player(commands, context, my_player_id, *id, player);
    }

    // Handle players no longer in the snapshot:
    //   * remote players → despawn entity, drop PlayerInfo (logoff / death of
    //     another player both look identical from this side).
    //   * the local player → keep entity (camera/mouse-look need it), hide
    //     visibility, keep PlayerInfo (preserves score), insert
    //     LocalPlayerDead. The next snapshot that re-includes our id will
    //     teleport us to the respawn position.
    let mut local_just_died = false;
    context.players.retain(|id, player| {
        if update_ids.contains(id) {
            return true;
        }
        if *id == my_player_id {
            commands.entity(player.entity).insert(Visibility::Hidden);
            local_just_died = true;
            true
        } else {
            commands.entity(player.entity).despawn();
            false
        }
    });
    if local_just_died {
        context.local_player_info.is_dead = true;
    }

    // Handle local-player respawn: if we were dead and our id reappeared in
    // this snapshot, hard-teleport our existing entity to the new spawn
    // position, restore visibility, and clear the death state.
    let mut local_just_respawned = false;
    if context.local_player_info.is_dead
        && let Some((_, server_player)) = server_players.iter().find(|(id, _)| *id == my_player_id)
        && let Some(info) = context.players.get_mut(&my_player_id)
    {
        let entity = info.entity;
        // Adopt the server-assigned respawn facing. Without this the next
        // input frame recomputes yaw from the unchanged camera transform and
        // overwrites `FaceYaw` with the pre-death facing.
        apply_local_spawn_facing(
            commands,
            &context.cameras,
            &mut context.local_player_info,
            &server_player.movement.pos,
            server_player.movement.face_yaw,
        );
        commands.entity(info.entity).insert((
            server_player.movement.pos,
            // Reset the previous-tick anchor so render interpolation doesn't
            // smear the respawn teleport across one render frame.
            PreviousTickPosition(server_player.movement.pos),
            FaceYaw(server_player.movement.face_yaw),
            CharacterVerticalVelocity(server_player.movement.vertical_velocity),
            server_player.health,
            Visibility::Visible,
        ));
        // Clear any stale `ServerReconciliation` that survived the death
        // window — the next-snapshot `update_snapshot_player` call below is
        // skipped for the local player this frame (see below), so without
        // this remove() the recon component would linger pointing at the
        // pre-death position.
        commands
            .entity(entity)
            .remove::<(ServerReconciliation, PortalMomentum)>();
        info.apply_snapshot(server_player);
        // The pre-death records and any pending crossing dispute describe a
        // player that no longer exists.
        info.hops = server_player.hops;
        info.disputed_echoes = 0;
        context.local_player_info.committed_positions.clear();
        context.local_player_info.is_dead = false;
        local_just_respawned = true;

        if let Some(reminder) = context.quest_log.reminder() {
            context.banner.push(BannerMessage::QuestAnnouncement(reminder));
        }
    }

    for (id, server_player) in server_players {
        // The respawn block above already synced the local player.
        if local_just_respawned && *id == my_player_id {
            continue;
        }
        update_snapshot_player(commands, context, *id, server_player);
    }
}

fn spawn_snapshot_player(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
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
        &context.asset_server,
        &mut context.meshes,
        &mut context.materials,
        &mut context.images,
        &mut context.graphs,
        &context.asset_set,
        &context.client_settings,
        &context.gameplay_config,
        context.max_health.player,
        id.0,
        &player.name,
        &player.movement.pos,
        player.movement.move_intent,
        player.health,
        player.movement.face_yaw,
        is_local,
    );
    commands
        .entity(entity)
        .insert(CharacterVerticalVelocity(player.movement.vertical_velocity));

    if is_local {
        apply_local_spawn_facing(
            commands,
            &context.cameras,
            &mut context.local_player_info,
            &player.movement.pos,
            player.movement.face_yaw,
        );
    }

    context.players.insert(id, PlayerInfo::from_snapshot(entity, player));
}

// Point the main camera (and the stored mouse-look fallback state) at the
// server-assigned spawn facing.
pub(super) fn apply_local_spawn_facing(
    commands: &mut Commands,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    local_player_info: &mut LocalPlayerInfo,
    pos: &Position,
    face_yaw: f32,
) {
    let camera_rotation = face_yaw + PI;
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
    context: &mut ServerMessageContext,
    id: PlayerId,
    server_player: &Player,
) {
    if let Some(client_player) = context.players.get_mut(&id) {
        client_player.apply_snapshot(server_player);
        commands.entity(client_player.entity).insert(server_player.health);
    }
}
