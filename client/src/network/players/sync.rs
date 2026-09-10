use bevy::prelude::*;
use std::{collections::HashSet, f32::consts::PI};

use super::super::context::ServerMessageContext;
use crate::{
    characters::PreviousTickPosition,
    network::ServerReconciliation,
    players::{LocalPlayerInfo, PlayerInfo, PlayerSpawnContext, spawn_player},
    ui::BannerMessage,
};
use common::{
    physics::{AirborneMomentum, CharacterVerticalVelocity, KnockbackVelocity},
    protocol::{FaceYaw, Player, PlayerId},
};

pub(in crate::network) fn sync_players(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
    tick: u32,
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

        spawn_snapshot_player(commands, context, my_player_id, tick, *id, player);
    }

    // Absence from the snapshot despawns a remote player; the local player's
    // entity is hidden and its `PlayerInfo` kept, and `LocalPlayerInfo.is_dead`
    // is set.
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
        context.local_player_info.portal_crossings.clear();
    }

    // Local-player respawn: our id reappears while dead, so hard-teleport the
    // kept entity to the spawn and restore it; the loop below applies the
    // snapshot fields as for everyone else.
    if context.local_player_info.is_dead
        && let Some((_, server_player)) = server_players.iter().find(|(id, _)| *id == my_player_id)
        && let Some(info) = context.players.get_mut(&my_player_id)
    {
        // Adopt the server-assigned respawn facing. Without this the next
        // input frame keeps the pre-death aim and overwrites `FaceYaw` with it.
        apply_local_spawn_facing(&mut context.local_player_info, server_player.movement.face_yaw);
        // A move message received before this snapshot measured the respawned
        // body against the corpse; applying that gap after the teleport would
        // drag the body back toward it.
        commands
            .entity(info.entity)
            .insert((
                server_player.movement.pos,
                // Reset the previous-tick anchor so render interpolation doesn't
                // smear the respawn teleport across one render frame.
                PreviousTickPosition(server_player.movement.pos),
                FaceYaw(server_player.movement.face_yaw),
                CharacterVerticalVelocity(server_player.movement.vertical_velocity),
                AirborneMomentum(Vec3::from_array(server_player.movement.airborne_momentum)),
                KnockbackVelocity(Vec3::from_array(server_player.movement.knockback)),
                Visibility::Visible,
            ))
            .remove::<ServerReconciliation>();
        info.last_movement_tick = tick;
        context.local_player_info.committed_positions.clear();
        context.local_player_info.last_comparison_seq = Some(context.local_player_info.move_seq);
        context.local_player_info.portal_crossings.clear();
        context.local_player_info.is_dead = false;

        if let Some(reminder) = context.quest_log.reminder() {
            context.banner.push(BannerMessage::QuestAnnouncement(reminder));
        }
    }

    for (id, server_player) in server_players {
        update_snapshot_player(commands, context, *id, server_player);
    }
}

fn spawn_snapshot_player(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
    tick: u32,
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
        PlayerSpawnContext {
            asset_server: &context.assets.asset_server,
            meshes: &mut context.meshes,
            materials: &mut context.materials,
            images: &mut context.images,
            asset_set: &context.assets.asset_set,
            client_settings: &context.client_settings,
            gameplay_config: &context.gameplay_config,
            max_health: context.assets.max_health.player,
        },
        id,
        player,
        is_local,
    );

    if is_local {
        apply_local_spawn_facing(&mut context.local_player_info, player.movement.face_yaw);
    }

    context
        .players
        .insert(id, PlayerInfo::from_snapshot(entity, player, tick));
}

// Point the stored mouse look at the server-assigned spawn facing; the follow
// camera places itself from it every frame.
fn apply_local_spawn_facing(local_player_info: &mut LocalPlayerInfo, face_yaw: f32) {
    local_player_info.stored_yaw = face_yaw + PI;
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
