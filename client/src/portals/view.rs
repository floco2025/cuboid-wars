use bevy::prelude::*;
use std::f32::consts::PI;

use crate::{
    constants::PORTAL_VIEW_BLEND_SECS,
    input::MAX_PITCH,
    players::{LocalPlayerInfo, PortalTransitBlend},
};
use common::physics::{PortalFrame, traverse_vector};

// Portal-style exit reorientation. The aim (stored yaw/pitch) jumps straight
// to the mapped upright view — pitch carried through the pair — while the
// camera is seeded with the fully mapped, possibly tilted view and
// `local_player_portal_blend_system` decays the difference over
// `PORTAL_VIEW_BLEND_SECS`. The world never rotates; only the view transient
// does.
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
