use bevy::prelude::*;
use std::collections::HashSet;
use std::f32::consts::PI;

use super::{super::context::ServerMessageContext, handlers::player_movement_velocity};
use crate::{
    cameras::MainCameraMarker,
    characters::PreviousTickPosition,
    constants::PORTAL_VIEW_BLEND_SECS,
    input::MAX_PITCH,
    network::ServerReconciliation,
    players::{LocalPlayerInfo, PlayerInfo, PortalTransitBlend, spawn_player},
    ui::BannerMessage,
};
use common::{
    physics::{CharacterVerticalVelocity, PortalFrame, traverse_vector},
    protocol::{FaceYaw, Player, PlayerId, Position, PowerUpKind, SPlayerTeleport},
};

pub(in crate::network) fn sync_players(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
    server_players: &[(PlayerId, Player)],
) {
    let update_ids: HashSet<PlayerId> = server_players.iter().map(|(id, _)| *id).collect();

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
        commands.entity(entity).remove::<ServerReconciliation>();
        info.apply_snapshot(server_player);
        context.local_player_info.is_dead = false;
        local_just_respawned = true;

        if let Some(reminder) = context.quest_log.reminder() {
            context.banner.push(BannerMessage::QuestAnnouncement(reminder));
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
        update_snapshot_player(commands, context, my_player_id, *id, server_player);
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

// Portal-style exit reorientation. The aim (stored yaw/pitch) jumps straight
// to the mapped upright view — pitch carried through the pair — while the
// camera is seeded with the fully mapped, possibly tilted view and
// `local_player_portal_blend_system` decays the difference over
// `PORTAL_VIEW_BLEND_SECS`. The world never rotates; only the view transient
// does. Falls back to the respawn-style snap when the gate can't be
// recovered from the teleport endpoints.
pub(super) fn apply_local_portal_facing(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    message: &SPlayerTeleport,
) {
    let Some((entry, exit)) = context
        .portal_set
        .traversal_frames(Vec3::from(message.from_pos), Vec3::from(message.pos))
    else {
        apply_local_spawn_facing(
            commands,
            &context.cameras,
            &mut context.local_player_info,
            &message.pos,
            message.face_yaw,
        );
        return;
    };
    let (seeded, target_yaw, target_pitch) = portal_view_transition(
        &entry,
        &exit,
        context.local_player_info.stored_yaw,
        context.local_player_info.stored_pitch,
        message.face_yaw,
    );
    context.local_player_info.stored_yaw = target_yaw;
    context.local_player_info.stored_pitch = target_pitch;
    let target = Quat::from_euler(EulerRot::YXZ, target_yaw, target_pitch, 0.0);
    if let Ok(camera_entity) = context.cameras.single() {
        commands.entity(camera_entity).insert((
            Transform {
                translation: Vec3::new(
                    message.pos.x,
                    message.pos.y + context.gameplay_config.player.eye_height(),
                    message.pos.z,
                ),
                rotation: seeded,
                ..default()
            },
            PortalTransitBlend {
                delta: seeded * target.inverse(),
                timer: Timer::from_seconds(PORTAL_VIEW_BLEND_SECS, TimerMode::Once),
            },
        ));
    }
}

// Maps the current camera view through the pair and splits it into the
// upright target aim (yaw, pitch clamped to the mouse-look limits) plus the
// seeded full rotation whose leftover tilt the blend decays. Camera forward
// is `rotation * -Z`; a vertically mapped forward has no yaw, so the
// server's mapped facing breaks the tie.
fn portal_view_transition(
    entry: &PortalFrame,
    exit: &PortalFrame,
    camera_yaw: f32,
    camera_pitch: f32,
    fallback_face_yaw: f32,
) -> (Quat, f32, f32) {
    let rotation = Quat::from_euler(EulerRot::YXZ, camera_yaw, camera_pitch, 0.0);
    let forward = traverse_vector(entry, exit, rotation * Vec3::NEG_Z);
    let up = traverse_vector(entry, exit, rotation * Vec3::Y);
    let seeded = Transform::default().looking_to(forward, up).rotation;
    let target_pitch = forward.y.clamp(-1.0, 1.0).asin().clamp(-MAX_PITCH, MAX_PITCH);
    let target_yaw = if forward.x * forward.x + forward.z * forward.z > 1e-4 {
        (-forward.x).atan2(-forward.z)
    } else {
        fallback_face_yaw + PI
    };
    (seeded, target_yaw, target_pitch)
}

fn update_snapshot_player(
    commands: &mut Commands,
    context: &mut ServerMessageContext,
    my_player_id: PlayerId,
    id: PlayerId,
    server_player: &Player,
) {
    if let Some(client_player) = context.players.get_mut(&id) {
        if let Ok((client_pos, _, _)) = context.player_data.get(client_player.entity) {
            let server_velocity = player_movement_velocity(
                server_player.movement,
                &context.gameplay_config,
                server_player.power_up(PowerUpKind::Speed),
                server_player.stunned,
            );

            if id != my_player_id {
                commands.entity(client_player.entity).insert((
                    server_player.movement.move_intent,
                    FaceYaw(server_player.movement.face_yaw),
                ));
            }
            commands.entity(client_player.entity).insert(ServerReconciliation::new(
                *client_pos,
                server_player.movement.pos,
                server_velocity,
                &context.rtt,
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

#[cfg(test)]
mod tests {
    use super::*;
    use common::math::angle_delta_radians;

    #[test]
    fn view_through_a_facing_pair_is_preserved_without_tilt() {
        let entry = PortalFrame::from_surface(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0);
        let exit = PortalFrame::from_surface(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z, 0.0);
        let (seeded, yaw, pitch) = portal_view_transition(&entry, &exit, 0.0, -0.3, 0.0);
        assert!(yaw.abs() < 1e-4);
        assert!((pitch + 0.3).abs() < 1e-4);
        let target = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
        assert!(seeded.angle_between(target) < 1e-3);
    }

    #[test]
    fn view_through_a_same_wall_pair_turns_around_without_tilt() {
        let entry = PortalFrame::from_surface(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0);
        let exit = PortalFrame::from_surface(Vec3::new(5.0, 1.0, 0.0), Vec3::Z, 0.0);
        let (seeded, yaw, pitch) = portal_view_transition(&entry, &exit, 0.0, 0.2, 0.0);
        assert!(angle_delta_radians(yaw, PI).abs() < 1e-4);
        assert!((pitch - 0.2).abs() < 1e-4);
        let target = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
        assert!(seeded.angle_between(target) < 1e-3);
    }
}
