use bevy::{
    ecs::system::SystemParam,
    input::mouse::MouseButton,
    prelude::*,
    window::{CursorGrabMode, CursorOptions},
};

use crate::{
    audio::play_sound,
    cameras::{CameraViewMode, MainCameraMarker},
    config::AssetSet,
    constants::{PORTAL_A_COLOR, PORTAL_B_COLOR, PROJECTILE_SPARK_REFERENCE_SPEED},
    network::{ClientToServer, ClientToServerChannel},
    players::{LocalPlayerInfo, LocalPlayerMarker, MyPlayerId},
    portals::PortalMap,
    vfx::{ImpactKind, ParticleClouds, spawn_impact_sparks},
};
use common::{
    config::GameplayConfig,
    math::direction_from_yaw_pitch,
    physics::{CollisionWorld, compute_portal_placement, portal_placement_overlaps},
    protocol::*,
};

// Which weapon the mouse buttons drive. Client-only presentation state, like
// `CameraViewMode`; the server just receives whichever shot message results.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponMode {
    #[default]
    Gun,
    PortalGun,
}

impl WeaponMode {
    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Gun => Self::PortalGun,
            Self::PortalGun => Self::Gun,
        }
    }
}

#[derive(SystemParam)]
pub struct PortalInputWorld<'w> {
    my_player_id: Option<Res<'w, MyPlayerId>>,
    time: Res<'w, Time>,
    collision_world: Option<Res<'w, CollisionWorld>>,
    map_layout: Option<Res<'w, MapLayout>>,
    portals: Res<'w, PortalMap>,
    gameplay_config: Res<'w, GameplayConfig>,
}

pub fn input_weapon_toggle_system(keyboard: Res<ButtonInput<KeyCode>>, mut mode: ResMut<WeaponMode>) {
    if keyboard.just_pressed(KeyCode::KeyQ) {
        *mode = mode.toggled();
    }
}

// Portal-gun fire: left click places end A (blue), right click end B
// (orange). Placement is predicted with the same shared check the server
// runs on the same static map data, so the feedback is immediate and never
// wrong: a valid aperture plays the fire sound and sends the shot, an
// invalid one (miss, doesn't fit, covers a fixture) dry-fires and sends
// nothing. The portal itself still only appears with `SPortalOpened`.
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
    mut particle_clouds: ResMut<ParticleClouds>,
) {
    if *mode != WeaponMode::PortalGun || local_player_info.is_dead {
        return;
    }
    if cursor_options.grab_mode == CursorGrabMode::None {
        return;
    }
    let end = if mouse.just_pressed(MouseButton::Left) {
        PortalEnd::A
    } else if mouse.just_pressed(MouseButton::Right) {
        PortalEnd::B
    } else {
        return;
    };
    let Some(my_player_id) = world.my_player_id.as_deref() else {
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
    let (Some(collision_world), Some(map_layout)) = (world.collision_world.as_deref(), world.map_layout.as_deref())
    else {
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
        collision_world,
        map_layout,
    );
    let existing = world.portals.wire_portals();
    if placement.is_none_or(|placement| portal_placement_overlaps(&placement, my_player_id.0, end, &existing)) {
        // Portal-style fizzle: dry-fire plus a spark burst in the end's
        // color at the impact point, so a failed placement is visibly
        // rejected where it landed. Nothing is sent — the server would
        // reach the same verdict.
        if let Some(hit) =
            collision_world.world_surface_along_ray(origin, direction, world.gameplay_config.portals.range)
        {
            let color = match end {
                PortalEnd::A => PORTAL_A_COLOR,
                PortalEnd::B => PORTAL_B_COLOR,
            };
            spawn_impact_sparks(
                &mut particle_clouds.sparks,
                hit.point,
                hit.normal,
                hit.normal,
                PROJECTILE_SPARK_REFERENCE_SPEED,
                ImpactKind::Barrier(color),
            );
        }
        play_sound(&mut commands, &asset_server, asset_set.player_sound("dry_fire"));
        return;
    }

    local_player_info.last_shot_time = now;
    play_sound(&mut commands, &asset_server, asset_set.player_sound("fire"));
    let _ = to_server.send(ClientToServer::Send(ClientMessage::PortalShot(CPortalShot {
        end,
        face_yaw: face_yaw.0,
        face_pitch: pitch,
    })));
}
