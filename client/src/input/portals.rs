use bevy::{
    ecs::system::SystemParam,
    input::mouse::MouseButton,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use super::WeaponMode;
use crate::{
    audio::play_sound,
    cameras::{CameraViewMode, MainCameraMarker},
    config::AssetSet,
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker},
    portals::PortalMap,
};
use common::{
    config::GameplayConfig,
    map::Carriers,
    math::direction_from_yaw_pitch,
    physics::{CollisionWorld, compute_portal_placement, portal_placement_overlaps},
    protocol::*,
};

#[derive(SystemParam)]
pub struct PortalInputWorld<'w> {
    time: Res<'w, Time>,
    collision_world: Res<'w, CollisionWorld>,
    carriers: Res<'w, Carriers>,
    map_layout: Res<'w, MapLayout>,
    map_settings: Res<'w, MapSettings>,
    plates: Res<'w, PlateState>,
    portals: Res<'w, PortalMap>,
    gameplay_config: Res<'w, GameplayConfig>,
}

// Portal-gun fire: both-access uses left=A and right=B; single-access uses
// left for its assigned end. Placement is predicted with the same shared check the server
// runs on the same map data and tile poses: a valid aperture sends the shot and its
// opening sound arrives with `SPortalOpened`; an invalid one (miss, doesn't
// fit, covers a fixture) dry-fires immediately and sends nothing.
pub fn input_portal_system(
    mut commands: Commands,
    mode: Res<WeaponMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    cursor_options: Single<&CursorOptions>,
    camera_query: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>)>,
    local_player_query: Query<(&Position, &FaceYaw), With<LocalPlayerMarker>>,
    to_server: Res<ClientToServerChannel>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    view_mode: Res<CameraViewMode>,
    mut local_player_info: ResMut<LocalPlayerInfo>,
    world: PortalInputWorld,
    portal_access: Res<PortalAccess>,
) {
    if *mode != WeaponMode::Portal || local_player_info.is_dead {
        return;
    }
    if cursor_options.grab_mode == CursorGrabMode::None {
        return;
    }
    let access = *portal_access;
    let end = match access {
        PortalAccess::None => return,
        PortalAccess::Single { end, .. } if mouse.just_pressed(MouseButton::Left) => end,
        PortalAccess::Single { .. } => return,
        PortalAccess::Both { .. } if mouse.just_pressed(MouseButton::Left) => PortalEnd::A,
        PortalAccess::Both { .. } if mouse.just_pressed(MouseButton::Right) => PortalEnd::B,
        PortalAccess::Both { .. } => return,
    };
    let Some(pair) = access.pair() else {
        return;
    };
    let now = world.time.elapsed_secs();
    if now - local_player_info.last_shot_time < world.gameplay_config.projectiles.cooldown_secs {
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    }
    let Some((pos, face_yaw)) = local_player_query.iter().next() else {
        return;
    };
    let pitch = if view_mode.is_first_person() {
        camera_query
            .iter()
            .next()
            .map_or(0.0, |transform| transform.rotation.to_euler(EulerRot::YXZ).1)
    } else {
        0.0
    };

    let origin = Vec3::new(pos.x, pos.y + world.gameplay_config.player.eye_height(), pos.z);
    let direction = direction_from_yaw_pitch(face_yaw.0, pitch);
    let placement = compute_portal_placement(
        origin,
        direction,
        face_yaw.0,
        world.gameplay_config.portals.range,
        &world.collision_world,
        &world.map_layout,
        &world.carriers,
        world.map_settings.portal_shots,
        &world.plates.open_barrier_kinds,
    );
    let existing = world.portals.wire_portals();
    if placement.is_none_or(|placement| portal_placement_overlaps(&placement, pair, end, &existing, &world.carriers)) {
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    }

    local_player_info.last_shot_time = now;
    let _ = to_server.send(ClientToServer::Send(ClientMessage::PortalShot(CPortalShot {
        end,
        face_yaw: face_yaw.0,
        face_pitch: pitch,
    })));
}
