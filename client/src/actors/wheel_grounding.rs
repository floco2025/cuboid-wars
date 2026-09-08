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

fn ground_pose(
    world: &CollisionWorld,
    position: Position,
    origin: Vec3,
    yaw: Quat,
    physics: CharacterPhysicsConfig,
    wheels: WheelModelDef,
    scale: f32,
) -> Option<(Quat, f32)> {
    let support = grounding_diagnostics(world, &position, physics, &[], &[]);
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
        if let Some((rotation, height)) = ground_pose(
            &world,
            *position,
            parent.translation,
            parent.rotation,
            grounding.physics,
            grounding.wheels,
            grounding.rest.scale.x,
        ) {
            transform.rotation = parent.rotation.inverse() * rotation * grounding.rest.rotation;
            transform.translation.y += height;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::{
        config::{HitboxConfig, MovementColliderConfig},
        protocol::{BarrierKindTable, CarrierId, Floor, MapLayout, Ramp},
    };

    fn wheels() -> WheelModelDef {
        WheelModelDef {
            radius: 0.215,
            track: 0.87,
            wheelbase: 0.58,
            idle_animation: 0,
            drive_animation: 1,
            drive_cycle_secs: 1.0,
        }
    }

    fn physics() -> CharacterPhysicsConfig {
        CharacterPhysicsConfig {
            movement_collider: MovementColliderConfig {
                diameter: 1.05,
                height: 1.1,
            },
            hitbox: HitboxConfig {
                width: 1.1,
                height: 0.88,
                depth: 1.02,
                bottom_offset: 0.0,
            },
        }
    }

    fn ramp_world() -> CollisionWorld {
        CollisionWorld::from_map_layout(
            &MapLayout {
                ramps: vec![Ramp {
                    x1: -4.0,
                    x2: 4.0,
                    z1: 0.0,
                    z2: 12.0,
                    y1: 0.0,
                    y2: 6.0,
                    carrier: CarrierId::WORLD,
                }],
                floors: vec![Floor {
                    x1: -4.0,
                    x2: 4.0,
                    z1: -4.0,
                    z2: 0.0,
                    y: 0.0,
                    thickness: 0.2,
                    level: 0,
                    carrier: CarrierId::WORLD,
                }],
                ..default()
            },
            &BarrierKindTable::default(),
        )
    }

    #[test]
    fn tyres_rest_on_ramp_when_driving_up_down_or_across_it() {
        let world = ramp_world();
        let normal = Vec3::new(0.0, 1.0, -0.5).normalize();
        let position = Position {
            x: 0.0,
            y: 3.0 + physics().movement_collider.radius() * (normal.y.recip() - 1.0),
            z: 6.0,
        };
        for angle in [0.0, 1.0, std::f32::consts::FRAC_PI_2, std::f32::consts::PI] {
            let yaw = Quat::from_rotation_y(angle);
            let (rotation, offset) = ground_pose(&world, position, position.into(), yaw, physics(), wheels(), 1.0)
                .expect("ramp pose missing for a supported mine");
            assert!((rotation * Vec3::Y).abs_diff_eq(normal, 1e-4));
            assert!(
                (rotation * Vec3::Z)
                    .xz()
                    .normalize()
                    .abs_diff_eq((yaw * Vec3::Z).xz(), 1e-4)
            );
            for x in [-0.435, 0.435] {
                for z in [-0.29, 0.29] {
                    let centre = Vec3::from(position) + Vec3::Y * offset + rotation * Vec3::new(x, wheels().radius, z);
                    assert!(
                        (normal.dot(centre) - wheels().radius).abs() < 1e-4,
                        "tyre does not touch the ramp"
                    );
                }
            }
        }
    }

    #[test]
    fn flat_floor_is_level_and_airborne_mines_are_not_pulled_down() {
        let world = ramp_world();
        let yaw = Quat::from_rotation_y(0.7);
        let position = Position {
            x: 0.0,
            y: 0.0,
            z: -2.0,
        };
        let (rotation, offset) = ground_pose(&world, position, position.into(), yaw, physics(), wheels(), 1.0)
            .expect("flat ground pose missing");
        assert!(rotation.abs_diff_eq(yaw, 1e-5));
        assert!(offset.abs() < 1e-5);
        let airborne = Position { y: 1.0, ..position };
        assert!(ground_pose(&world, airborne, airborne.into(), yaw, physics(), wheels(), 1.0).is_none());
    }

    #[test]
    fn visual_tilt_preserves_the_actor_and_its_collider_frame() {
        let mut app = App::new();
        app.insert_resource(ramp_world());
        app.add_systems(Update, wheel_grounding_system);
        let position = Position {
            x: 0.0,
            y: 3.062,
            z: 6.0,
        };
        let parent = Transform::from_translation(position.into()).with_rotation(Quat::from_rotation_y(0.7));
        let owner = app.world_mut().spawn((ActorMarker, position, parent)).id();
        let model = app
            .world_mut()
            .spawn((
                WheelGrounding {
                    owner,
                    physics: physics(),
                    wheels: wheels(),
                    rest: Transform::IDENTITY,
                },
                Transform::IDENTITY,
            ))
            .id();
        app.update();
        assert_eq!(
            *app.world().get::<Transform>(owner).expect("actor transform missing"),
            parent
        );
        let model = app.world().get::<Transform>(model).expect("mine transform missing");
        assert!((parent.rotation * model.rotation * Vec3::Y).abs_diff_eq(Vec3::new(0.0, 1.0, -0.5).normalize(), 1e-4));
    }

    #[test]
    fn wheel_footprint_blends_the_tilt_at_the_foot_of_a_ramp() {
        let world = ramp_world();
        let normal = Vec3::new(0.0, 1.0, -0.5).normalize();
        let rise = physics().movement_collider.radius() * (normal.y.recip() - 1.0);
        let mut previous_angle = 0.0;
        for step in -40..=40 {
            let z = step as f32 * 0.01;
            let position = Position {
                x: 0.0,
                y: (z * 0.5 + rise).max(0.0),
                z,
            };
            let (rotation, _) = ground_pose(
                &world,
                position,
                position.into(),
                Quat::IDENTITY,
                physics(),
                wheels(),
                1.0,
            )
            .expect("ground pose missing at the ramp transition");
            let angle = (rotation * Vec3::Y).angle_between(Vec3::Y);
            assert!(angle + 1e-4 >= previous_angle);
            assert!(angle - previous_angle < 0.012, "tilt snapped at the ramp transition");
            previous_angle = angle;
        }
        assert!((previous_angle - normal.angle_between(Vec3::Y)).abs() < 1e-4);
    }
}
