use bevy_math::Vec3;
use rapier3d::{
    parry::{
        bounding_volume::Aabb,
        query::{Ray, RayCast, ShapeCastOptions, cast_shapes, intersection_test},
    },
    prelude::{Cuboid, Pose, Vector},
};

use super::CollisionWorld;
use crate::{
    config::CharacterPhysicsConfig,
    map::{CarrierPose, Carriers},
    physics::characters::{character_movement_pose, character_movement_shape},
    protocol::{CarrierId, Eraser, Position},
};

#[derive(Clone, Copy)]
pub(super) struct EraserVolume {
    min: Vec3,
    max: Vec3,
    carrier: CarrierId,
}

impl EraserVolume {
    pub(super) fn from_eraser(eraser: &Eraser) -> Self {
        let pad = eraser.width / 2.0;
        Self {
            min: Vec3::new(eraser.x1.min(eraser.x2) - pad, eraser.y, eraser.z1.min(eraser.z2) - pad),
            max: Vec3::new(
                eraser.x1.max(eraser.x2) + pad,
                eraser.y + eraser.height,
                eraser.z1.max(eraser.z2) + pad,
            ),
            carrier: eraser.carrier,
        }
    }

    pub(super) fn posed(self, pose: &CarrierPose) -> Self {
        Self {
            min: self.min + pose.translation,
            max: self.max + pose.translation,
            ..self
        }
    }

    pub(super) const fn carrier(self) -> CarrierId {
        self.carrier
    }
}

impl CollisionWorld {
    #[must_use]
    pub fn character_touches_eraser(&self, pos: &Position, physics: CharacterPhysicsConfig) -> bool {
        self.character_crosses_eraser(pos, pos, physics, None)
    }

    #[must_use]
    pub fn character_crosses_eraser(
        &self,
        start: &Position,
        end: &Position,
        physics: CharacterPhysicsConfig,
        carriers: Option<&Carriers>,
    ) -> bool {
        self.character_eraser_contacts(start, end, physics, carriers)
            .next()
            .is_some()
    }

    // Indices name the erasers in the layout for this collision world's lifetime.
    pub fn character_eraser_contacts<'a>(
        &'a self,
        start: &Position,
        end: &Position,
        physics: CharacterPhysicsConfig,
        carriers: Option<&'a Carriers>,
    ) -> impl Iterator<Item = usize> + 'a {
        let shape = character_movement_shape(physics);
        let pose = character_movement_pose(start, physics);
        let translation = Vec3::from(*end) - Vec3::from(*start);
        self.eraser_volumes
            .iter()
            .enumerate()
            .filter_map(move |(index, volume)| {
                // The field is posed at tick end; relative travel catches a moving field sweeping a stationary player.
                let carry = carriers.map_or(Vec3::ZERO, |carriers| carriers.displacement(volume.carrier));
                let field = Cuboid::new(Vector::from_array(((volume.max - volume.min) / 2.0).to_array()));
                let field_pose =
                    Pose::from_translation(Vector::from_array(((volume.min + volume.max) / 2.0).to_array()));
                let mut from = pose;
                from.translation += Vector::from_array(carry.to_array());
                let overlaps = intersection_test(&from, &shape, &field_pose, &field).is_ok_and(|hit| hit);
                (overlaps
                    || cast_shapes(
                        &from,
                        Vector::from_array((translation - carry).to_array()),
                        &shape,
                        &field_pose,
                        Vector::ZERO,
                        &field,
                        ShapeCastOptions {
                            max_time_of_impact: 1.0,
                            ..Default::default()
                        },
                    )
                    .is_ok_and(|hit| hit.is_some()))
                .then_some(index)
            })
    }

    pub(super) fn eraser_blocks_segment(&self, from: Vec3, to: Vec3) -> bool {
        let ray = Ray::new(
            Vector::from_array(from.to_array()),
            Vector::from_array((to - from).to_array()),
        );
        self.eraser_volumes.iter().any(|volume| {
            let bounds = Aabb::new(
                Vector::from_array(volume.min.to_array()),
                Vector::from_array(volume.max.to_array()),
            );
            bounds.cast_local_ray(&ray, 1.0, true).is_some()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::gameplay::load_test_gameplay,
        protocol::{BarrierKindTable, Carrier, MapLayout, Wall},
    };

    fn field() -> Eraser {
        Eraser {
            x1: -2.0,
            z1: 0.0,
            x2: 2.0,
            z2: 0.0,
            width: 0.1,
            y: 0.0,
            height: 4.0,
            level: 0,
            carrier: CarrierId::WORLD,
        }
    }

    fn world(layout: &MapLayout) -> CollisionWorld {
        CollisionWorld::from_map_layout(layout, &BarrierKindTable::default())
    }

    #[test]
    fn portal_segments_include_contacts_but_stop_at_the_endpoint() {
        let world = world(&MapLayout {
            erasers: vec![field()],
            ..Default::default()
        });
        let inside = Vec3::new(0.0, 1.0, 0.0);
        let outside = Vec3::new(0.0, 1.0, -3.0);
        let surface = Vec3::new(0.0, 1.0, -0.05);
        for (from, to, blocked) in [
            (outside, Vec3::new(0.0, 1.0, 3.0), true),
            (outside, Vec3::new(0.0, 1.0, -0.06), false),
            (outside, surface, true),
            (inside, outside, true),
            (inside, inside, true),
            (surface, surface, true),
            (outside, outside, false),
            (surface, surface + Vec3::X, true),
            (outside, outside + Vec3::X, false),
        ] {
            assert_eq!(world.eraser_blocks_segment(from, to), blocked, "{from:?} -> {to:?}");
        }
    }

    #[test]
    fn fast_passes_and_body_overlaps_touch_without_a_solid_collision() {
        let layout = MapLayout {
            erasers: vec![field()],
            ..Default::default()
        };
        let world = world(&layout);
        let physics = load_test_gameplay()
            .expect("test gameplay config rejected")
            .player
            .physics();
        let front = Position::from(Vec3::Z * 10.0);
        let back = Position::from(Vec3::NEG_Z * 10.0);
        assert!(world.character_crosses_eraser(&front, &back, physics, None));
        assert!(world.character_crosses_eraser(&back, &front, physics, None));
        assert!(world.character_touches_eraser(&Position::default(), physics));
        assert!(!world.character_touches_eraser(&Position::from(Vec3::Y * 5.0), physics));
        assert!(!world.character_touches_eraser(&Position::from(Vec3::NEG_Y * 10.0), physics));
        assert!(!world.character_crosses_eraser(
            &Position::from(Vec3::new(5.0, 0.0, 10.0)),
            &Position::from(Vec3::new(5.0, 0.0, -10.0)),
            physics,
            None
        ));
        assert!(world.projectile_path_clear(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z * 20.0, 0.3, &[]));
        assert!(world.colliders.is_empty());
    }

    #[test]
    fn moving_field_sweeps_stationary_player_without_solid_carrier_geometry() {
        let layout = MapLayout {
            erasers: vec![Eraser {
                carrier: CarrierId(1),
                ..field()
            }],
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 1,
                from: Position::from(Vec3::Z * -3.0),
                to: Position::from(Vec3::Z * 3.0),
                travel_ticks: 1,
                pause_ticks: 0,
                phase_ticks: 0,
            }],
            ..Default::default()
        };
        let mut world = world(&layout);
        let mut carriers = Carriers::from_layout(&layout);
        let physics = load_test_gameplay()
            .expect("test gameplay config rejected")
            .player
            .physics();
        let pos = Position::default();
        assert!(!world.character_touches_eraser(&pos, physics));
        carriers.advance(1);
        world.set_carrier_poses(&carriers);
        assert!(!world.character_touches_eraser(&pos, physics));
        assert!(world.character_crosses_eraser(&pos, &pos, physics, Some(&carriers)));
    }

    #[test]
    fn portal_shots_are_blocked_only_before_the_host() {
        let layout = MapLayout {
            erasers: vec![field()],
            walls: vec![Wall {
                x1: -2.0,
                z1: -3.0,
                x2: 2.0,
                z2: -3.0,
                width: 0.3,
                y: 0.0,
                height: 4.0,
                level: 0,
                carrier: CarrierId::WORLD,
            }],
            ..Default::default()
        };
        let world = world(&layout);
        assert!(
            world
                .portal_surface_along_ray(Vec3::new(0.0, 1.0, 3.0), Vec3::NEG_Z, 10.0, &[])
                .is_none()
        );
        assert!(
            world
                .portal_surface_along_ray(Vec3::new(0.0, 1.0, -6.0), Vec3::Z, 10.0, &[])
                .is_some()
        );
    }

    #[test]
    fn erasers_are_transparent_to_attacks_and_projectile_paths() {
        let world = world(&MapLayout {
            erasers: vec![field()],
            ..Default::default()
        });
        let from = Vec3::new(0.0, 1.0, -3.0);
        let to = Vec3::new(0.0, 1.0, 3.0);
        assert!(world.line_of_sight_clear(from, to));
        assert!(world.attack_path_clear(from, to, &[]));
        assert!(world.projectile_path_clear(from, to - from, 0.3, &[]));
        assert!(world.cast_moving_ball(from, to - from, 0.3).is_none());
        assert!(
            world
                .cast_moving_ball_against_fields(from, to - from, 0.3, &[])
                .is_none()
        );
    }
}
