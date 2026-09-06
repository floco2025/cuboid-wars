use bevy_math::Vec3;

use super::CollisionWorld;
use crate::{
    config::CharacterPhysicsConfig,
    map::{CarrierPose, Carriers},
    physics::characters::{character_center, character_shape},
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
        let shape = character_shape(physics);
        let center = character_center(*start, physics);
        let feet = start.y.min(center.y - shape.half_extents.y);
        let head = center.y + shape.half_extents.y;
        let half = Vec3::new(shape.half_extents.x, (head - feet) / 2.0, shape.half_extents.z);
        let from = Vec3::new(center.x, (head + feet) / 2.0, center.z);
        let to = from + Vec3::from(*end) - Vec3::from(*start);
        self.eraser_volumes
            .iter()
            .enumerate()
            .filter_map(move |(index, volume)| {
                // The field is posed at tick end; relative travel catches a moving field sweeping a stationary player.
                let carry = carriers.map_or(Vec3::ZERO, |carriers| carriers.displacement(volume.carrier));
                segment_intersects_box(from + carry, to, volume.min - half, volume.max + half).then_some(index)
            })
    }

    pub(super) fn eraser_blocks_segment(&self, from: Vec3, to: Vec3) -> bool {
        self.eraser_volumes
            .iter()
            .any(|volume| segment_intersects_box(from, to, volume.min, volume.max))
    }
}

fn segment_intersects_box(from: Vec3, to: Vec3, min: Vec3, max: Vec3) -> bool {
    let delta = to - from;
    let mut enter = 0.0_f32;
    let mut leave = 1.0_f32;
    for axis in 0..3 {
        if delta[axis] == 0.0 {
            if from[axis] < min[axis] || from[axis] > max[axis] {
                return false;
            }
        } else {
            let a = (min[axis] - from[axis]) / delta[axis];
            let b = (max[axis] - from[axis]) / delta[axis];
            enter = enter.max(a.min(b));
            leave = leave.min(a.max(b));
            if enter > leave {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{PortalShotSettings, gameplay::load_test_gameplay},
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
    fn portal_shot_blocking_is_optional_and_only_checks_before_the_host() {
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
        for erasers_block in [false, true] {
            let settings = PortalShotSettings {
                erasers_block,
                ..Default::default()
            };
            let hit = world.portal_surface_along_ray(Vec3::new(0.0, 1.0, 3.0), Vec3::NEG_Z, 10.0, settings, &[]);
            assert_eq!(hit.is_some(), !erasers_block);
            assert!(
                world
                    .portal_surface_along_ray(Vec3::new(0.0, 1.0, -6.0), Vec3::Z, 10.0, settings, &[])
                    .is_some()
            );
        }
    }
}
