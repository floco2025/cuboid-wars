use bevy::prelude::*;
use common::{
    config::CharacterPhysicsConfig,
    constants::CHARACTER_MAX_SLOPE,
    physics::{CollisionWorld, grounding_diagnostics},
    protocol::{ActorMarker, Position},
};

use crate::config::WheelModelDef;

#[derive(Component)]
pub(crate) struct WheelGrounding {
    pub owner: Entity,
    pub physics: CharacterPhysicsConfig,
    pub wheels: WheelModelDef,
    pub rest: Transform,
}

pub(super) fn ground_pose(
    world: &CollisionWorld,
    grounding: &WheelGrounding,
    position: Position,
    origin: Vec3,
    yaw: Quat,
) -> Option<(Quat, f32)> {
    let WheelGrounding {
        physics, wheels, rest, ..
    } = grounding;
    let scale = rest.scale.x;
    let support = grounding_diagnostics(world, &position, *physics, &[], &[]);
    if !support.supported {
        return None;
    }
    let hit = support.hit?;
    let probes = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)].map(|(x, z)| {
        let offset = yaw * Vec3::new(x * (wheels.track * 0.5), 0.0, z * (wheels.wheelbase * 0.5)) * scale;
        world
            .ground_surface_below(
                origin + offset + Vec3::Y * physics.movement_collider.height,
                physics.movement_collider.height + physics.movement_collider.radius(),
            )
            .filter(|sample| sample.normal.y >= CHARACTER_MAX_SLOPE.cos())
            .map(|sample| sample.point)
    });
    let (normal, height) = if let [Some(left_rear), Some(right_rear), Some(left_front), Some(right_front)] = probes {
        let right = right_rear + right_front - left_rear - left_front;
        let forward = left_front + right_front - left_rear - right_rear;
        let normal = forward.cross(right).normalize_or_zero();
        if normal.y >= CHARACTER_MAX_SLOPE.cos() {
            (
                normal,
                (left_rear.y + right_rear.y + left_front.y + right_front.y) * 0.25,
            )
        } else {
            (hit.normal, plane_height(hit.contact, hit.normal, origin))
        }
    } else {
        // Overhanging wheels must not tip the rover toward an unrelated floor below.
        (hit.normal, plane_height(hit.contact, hit.normal, origin))
    };
    Some(slope_pose(normal, height - origin.y, yaw))
}

fn plane_height(point: Vec3, normal: Vec3, origin: Vec3) -> f32 {
    point.y - (normal.x * (origin.x - point.x) + normal.z * (origin.z - point.z)) / normal.y
}

fn slope_pose(normal: Vec3, height: f32, yaw: Quat) -> (Quat, f32) {
    let heading = yaw * Vec3::Z;
    let forward = Vec3::new(heading.x, -normal.dot(heading) / normal.y, heading.z).normalize();
    let right = normal.cross(forward).normalize();
    let rotation = Quat::from_mat3(&Mat3::from_cols(right, normal, forward));
    // Capsule feet rise on slopes; the model's feet origin belongs on the wheel contact plane.
    (rotation, height)
}

pub(crate) fn wheel_grounding_system(
    world: Res<CollisionWorld>,
    owners: Query<(&Position, &Transform), (With<ActorMarker>, Without<WheelGrounding>)>,
    mut models: Query<(&WheelGrounding, &mut Transform)>,
) {
    for (grounding, mut transform) in &mut models {
        let Ok((position, parent)) = owners.get(grounding.owner) else {
            continue;
        };
        *transform = grounding.rest;
        if let Some((rotation, height)) = ground_pose(&world, grounding, *position, parent.translation, parent.rotation)
        {
            transform.rotation = parent.rotation.inverse() * rotation * grounding.rest.rotation;
            transform.translation.y += height;
        }
    }
}
