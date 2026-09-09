use crate::{cameras::FollowCamera, config::FollowCameraConfig};
use bevy::prelude::*;
use common::physics::CollisionWorld;

pub(super) fn third_person_transform(
    world: &CollisionWorld,
    pivot: Vec3,
    rotation: Quat,
    config: FollowCameraConfig,
    radius: f32,
    dt: f32,
    state: &mut FollowCamera,
) -> Transform {
    let distance = state.distance.clamp(0.0, config.max_distance);
    let offset = rotation * Vec3::new(config.shoulder_offset, 0.0, distance);
    let allowed = world.camera_arm_distance(pivot, offset, radius);
    let reset = state
        .previous_pivot
        .is_none_or(|previous| previous.distance(pivot) > config.max_distance);
    state.arm_distance = if reset || allowed < state.arm_distance {
        allowed
    } else {
        state.arm_distance + (allowed - state.arm_distance) * (1.0 - (-config.obstruction_return_rate * dt).exp())
    };
    state.previous_pivot = Some(pivot);
    Transform {
        translation: pivot + offset.normalize_or_zero() * state.arm_distance,
        rotation,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cameras::CameraViewMode, config::ClientSettings, constants::INPUT_ZOOM_SENSITIVITY_BASE};
    use common::protocol::{BarrierKindTable, CarrierId, MapLayout, Wall};
    fn world(wall: bool) -> CollisionWorld {
        CollisionWorld::from_map_layout(
            &MapLayout {
                walls: if wall {
                    vec![Wall {
                        x1: -5.0,
                        z1: 2.0,
                        x2: 5.0,
                        z2: 2.0,
                        width: 0.2,
                        y: 0.0,
                        height: 4.0,
                        level: 0,
                        carrier: CarrierId::WORLD,
                    }]
                } else {
                    vec![]
                },
                ..default()
            },
            &BarrierKindTable::default(),
        )
    }
    #[test]
    fn arm_retracts_before_wall_and_eases_back_into_clear_space() {
        let mut state = FollowCamera {
            distance: 4.0,
            ..Default::default()
        };
        let config = ClientSettings::load_default()
            .expect("client settings are invalid")
            .camera
            .follow;
        let pivot = Vec3::Y * config.pivot_height;
        let clear = world(false);
        let blocked = world(true);
        let far = third_person_transform(&clear, pivot, Quat::IDENTITY, config, 0.2, 1.0 / 60.0, &mut state);
        let close = third_person_transform(&blocked, pivot, Quat::IDENTITY, config, 0.2, 1.0 / 60.0, &mut state);
        assert!(close.translation.z < 1.71);
        let released = third_person_transform(&clear, pivot, Quat::IDENTITY, config, 0.2, 1.0 / 60.0, &mut state);
        assert!(released.translation.z > close.translation.z && released.translation.z < far.translation.z);
    }
    #[test]
    fn camera_pivot_inside_wall_collapses_arm() {
        assert_eq!(
            world(true).camera_arm_distance(Vec3::new(0.0, 1.4, 2.0), Vec3::Z * 4.0, 0.2),
            0.0
        );
    }

    #[test]
    fn inward_scroll_starts_at_obstructed_camera_and_keeps_the_new_distance() {
        for shoulder_offset in [0.0, 0.65] {
            let mut config = ClientSettings::load_default()
                .expect("client settings are invalid")
                .camera
                .follow;
            config.shoulder_offset = shoulder_offset;
            let mut state = FollowCamera {
                distance: 4.0,
                ..Default::default()
            };
            let pivot = Vec3::Y * config.pivot_height;
            let clear = world(false);
            let blocked = world(true);
            third_person_transform(&clear, pivot, Quat::IDENTITY, config, 0.2, 1.0 / 60.0, &mut state);
            let close = third_person_transform(&blocked, pivot, Quat::IDENTITY, config, 0.2, 1.0 / 60.0, &mut state);
            let expected_distance = close.translation.z - 0.2;

            assert_eq!(
                state.zoom(
                    CameraViewMode::ThirdPerson,
                    1.0,
                    0.2 / INPUT_ZOOM_SENSITIVITY_BASE,
                    config,
                ),
                CameraViewMode::ThirdPerson
            );
            let zoomed = third_person_transform(&blocked, pivot, Quat::IDENTITY, config, 0.2, 1.0 / 60.0, &mut state);
            assert!((zoomed.translation.z - expected_distance).abs() < 1e-5);
            let released = third_person_transform(&clear, pivot, Quat::IDENTITY, config, 0.2, 1.0 / 60.0, &mut state);
            assert!((released.translation.z - expected_distance).abs() < 1e-5);
            assert!((state.distance - expected_distance).abs() < 1e-5);
        }
    }
}
