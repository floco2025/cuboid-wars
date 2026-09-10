use bevy::prelude::*;
use std::f32::consts::PI;

use crate::{
    constants::{CAMERA_MAX_PITCH, PORTAL_VIEW_BLEND_SECS},
    players::{LocalPlayerInfo, PortalTransitBlend},
};
use common::{
    constants::PORTAL_STANDABLE_NORMAL_Y,
    physics::{PortalFrame, traverse_vector, traverse_yaw},
};

// Portal-style exit reorientation. Out of a wall, the aim (stored yaw/pitch)
// jumps straight to the mapped upright view — pitch carried through the pair
// — while the camera is seeded with the fully mapped, possibly tilted view
// and `local_player_portal_blend_system` decays the difference over
// `PORTAL_VIEW_BLEND_SECS`. Out of a floor or ceiling the mapped view would
// point at the sky or the ground, so the aim keeps its pitch and turns its
// yaw the way the held input turns, with no transient. The world never
// rotates; only the view transient does.
pub fn apply_portal_view(
    commands: &mut Commands,
    camera: Option<Entity>,
    local_player_info: &mut LocalPlayerInfo,
    eye_pos: Vec3,
    entry: &PortalFrame,
    exit: &PortalFrame,
    fallback_face_yaw: f32,
) {
    let (seeded, target_yaw, target_pitch) = portal_view_transition(
        entry,
        exit,
        local_player_info.stored_yaw,
        local_player_info.stored_pitch,
        fallback_face_yaw,
    );
    local_player_info.stored_yaw = target_yaw;
    local_player_info.stored_pitch = target_pitch;
    let target = Quat::from_euler(EulerRot::YXZ, target_yaw, target_pitch, 0.0);
    if let Some(camera_entity) = camera {
        commands.entity(camera_entity).insert((
            Transform {
                translation: eye_pos,
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
    if exit.normal.y.abs() >= PORTAL_STANDABLE_NORMAL_Y {
        // A camera yaw looks along -Z and a facing yaw along +Z: half a turn apart.
        let yaw = traverse_yaw(entry, exit, camera_yaw + PI) - PI;
        let target = Quat::from_euler(EulerRot::YXZ, yaw, camera_pitch, 0.0);
        return (target, yaw, camera_pitch);
    }
    let rotation = Quat::from_euler(EulerRot::YXZ, camera_yaw, camera_pitch, 0.0);
    let forward = traverse_vector(entry, exit, rotation * Vec3::NEG_Z);
    let up = traverse_vector(entry, exit, rotation * Vec3::Y);
    let seeded = Transform::default().looking_to(forward, up).rotation;
    let target_pitch = forward
        .y
        .clamp(-1.0, 1.0)
        .asin()
        .clamp(-CAMERA_MAX_PITCH, CAMERA_MAX_PITCH);
    let target_yaw = if forward.x * forward.x + forward.z * forward.z > 1e-4 {
        (-forward.x).atan2(-forward.z)
    } else {
        fallback_face_yaw + PI
    };
    (seeded, target_yaw, target_pitch)
}

#[cfg(test)]
#[path = "tests/view.rs"]
mod tests;
