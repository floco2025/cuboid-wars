use bevy_ecs::prelude::Resource;
use bevy_math::Vec3;

use crate::protocol::{Carrier, CarrierId, MapLayout};

// A carrier's placement in world space. Translation only for now; a
// rotation about the vertical axis joins later, and every consumer goes
// through these methods so that it never touches them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CarrierPose {
    pub translation: Vec3,
}

impl CarrierPose {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
    };

    #[must_use]
    pub const fn from_translation(translation: Vec3) -> Self {
        Self { translation }
    }

    #[must_use]
    pub fn transform_point(&self, point: Vec3) -> Vec3 {
        point + self.translation
    }

    #[must_use]
    pub fn inverse_transform_point(&self, point: Vec3) -> Vec3 {
        point - self.translation
    }

    #[must_use]
    pub const fn transform_vector(&self, vector: Vec3) -> Vec3 {
        vector
    }

    #[must_use]
    pub const fn inverse_transform_vector(&self, vector: Vec3) -> Vec3 {
        vector
    }

    // This pose followed by `child` expressed in it: a child carrier's world
    // pose from its parent's.
    #[must_use]
    pub fn then(&self, child: &Self) -> Self {
        Self {
            translation: self.transform_point(child.translation),
        }
    }

    #[must_use]
    pub fn lerp(&self, other: &Self, alpha: f32) -> Self {
        Self {
            translation: self.translation.lerp(other.translation, alpha),
        }
    }
}

// Where a carrier's origin is in its parent's frame at `tick`: out along the
// path, held, back, held — a pure function of the tick, so both sides place
// every carrier from the shared clock alone.
#[must_use]
pub fn carrier_offset_at(carrier: &Carrier, tick: u32) -> Vec3 {
    let travel = carrier.travel_ticks.max(1);
    let cycle = 2 * (travel + carrier.pause_ticks);
    let phase = tick.wrapping_add(carrier.phase_ticks) % cycle;
    let progress = if phase < travel {
        phase as f32 / travel as f32
    } else if phase < travel + carrier.pause_ticks {
        1.0
    } else if phase < 2 * travel + carrier.pause_ticks {
        1.0 - (phase - travel - carrier.pause_ticks) as f32 / travel as f32
    } else {
        0.0
    };
    Vec3::from(carrier.from).lerp(Vec3::from(carrier.to), progress)
}

// Every carrier with its world pose at the last two ticks, in layout order.
// Built once from the layout on both sides and advanced right before
// character movement (`carriers_advance_system`). The default is the static
// world: no carriers, every id but `WORLD` unknown.
#[derive(Resource, Default)]
pub struct Carriers {
    carried: Vec<CarrierRuntime>,
    max_rise: f32,
    max_drop: f32,
}

struct CarrierRuntime {
    carrier: Carrier,
    previous: CarrierPose,
    current: CarrierPose,
}

impl Carriers {
    #[must_use]
    pub fn from_layout(layout: &MapLayout) -> Self {
        let mut carriers = Self {
            carried: Vec::with_capacity(layout.carriers.len()),
            max_rise: 0.0,
            max_drop: 0.0,
        };
        for (index, carrier) in layout.carriers.iter().enumerate() {
            assert!(
                carrier.parent.carried_index().is_none_or(|parent| parent < index),
                "carrier {} names parent {} but parents must precede their children",
                index + 1,
                carrier.parent.0
            );
            let pose = carriers
                .pose(carrier.parent)
                .then(&CarrierPose::from_translation(carrier_offset_at(carrier, 0)));
            carriers.carried.push(CarrierRuntime {
                carrier: *carrier,
                previous: pose,
                current: pose,
            });
        }
        carriers
    }

    #[must_use]
    pub fn is_static(&self) -> bool {
        self.carried.is_empty()
    }

    #[must_use]
    pub fn carried_count(&self) -> usize {
        self.carried.len()
    }

    // Parents precede children, so each world pose composes from a parent
    // already at this tick.
    pub fn advance(&mut self, tick: u32) {
        self.max_rise = 0.0;
        self.max_drop = 0.0;
        for index in 0..self.carried.len() {
            let carrier = self.carried[index].carrier;
            let pose = self
                .pose(carrier.parent)
                .then(&CarrierPose::from_translation(carrier_offset_at(&carrier, tick)));
            let runtime = &mut self.carried[index];
            runtime.previous = runtime.current;
            runtime.current = pose;
            let rise = pose.translation.y - runtime.previous.translation.y;
            self.max_rise = self.max_rise.max(rise);
            self.max_drop = self.max_drop.max(-rise);
        }
    }

    fn carried(&self, id: CarrierId) -> Option<&CarrierRuntime> {
        let index = id.carried_index()?;
        Some(
            self.carried
                .get(index)
                .expect("carrier named by a record, portal, or collider is not in the map"),
        )
    }

    // The carrier's world pose at this tick; the identity for the world.
    #[must_use]
    pub fn pose(&self, id: CarrierId) -> CarrierPose {
        self.carried(id)
            .map_or(CarrierPose::IDENTITY, |runtime| runtime.current)
    }

    // The pose between the last two ticks, for render-rate interpolation.
    #[must_use]
    pub fn pose_between(&self, id: CarrierId, alpha: f32) -> CarrierPose {
        self.carried(id).map_or(CarrierPose::IDENTITY, |runtime| {
            runtime.previous.lerp(&runtime.current, alpha)
        })
    }

    // How far the carrier moved this tick; zero for the world.
    #[must_use]
    pub fn displacement(&self, id: CarrierId) -> Vec3 {
        self.carried(id).map_or(Vec3::ZERO, |runtime| {
            runtime.current.translation - runtime.previous.translation
        })
    }

    // The largest rise and drop any carrier made this tick, which is how far
    // a floor may have moved through or away from a rider's feet.
    #[must_use]
    pub fn max_rise(&self) -> f32 {
        self.max_rise
    }

    #[must_use]
    pub fn max_drop(&self) -> f32 {
        self.max_drop
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Position;
    use crate::test_geometry::LEVEL_HEIGHT;

    fn slider() -> Carrier {
        Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position::default(),
            to: Position { x: 4.0, y: 0.0, z: 0.0 },
            travel_ticks: 60,
            pause_ticks: 30,
            phase_ticks: 0,
        }
    }

    fn lift() -> Carrier {
        Carrier {
            to: Position {
                x: 0.0,
                y: LEVEL_HEIGHT,
                z: 0.0,
            },
            pause_ticks: 0,
            levels: 1,
            ..slider()
        }
    }

    fn carriers_at(carriers: Vec<Carrier>, tick: u32) -> Carriers {
        let mut runtime = Carriers::from_layout(&MapLayout {
            carriers,
            ..Default::default()
        });
        runtime.advance(tick.wrapping_sub(1));
        runtime.advance(tick);
        runtime
    }

    const SLIDER: CarrierId = CarrierId(1);

    #[test]
    fn carrier_offset_is_from_at_phase_zero() {
        assert_eq!(carrier_offset_at(&slider(), 0), Vec3::ZERO);
        assert_eq!(carrier_offset_at(&slider(), 180), Vec3::ZERO);
    }

    #[test]
    fn carrier_offset_holds_at_to_through_the_pause() {
        let carrier = slider();
        assert_eq!(carrier_offset_at(&carrier, 60), Vec3::new(4.0, 0.0, 0.0));
        assert_eq!(carrier_offset_at(&carrier, 89), Vec3::new(4.0, 0.0, 0.0));
        assert!(carrier_offset_at(&carrier, 91).x < 4.0);
    }

    #[test]
    fn carrier_offset_returns_to_from_after_one_cycle() {
        let carrier = slider();
        let cycle = 2 * (carrier.travel_ticks + carrier.pause_ticks);
        assert_eq!(carrier_offset_at(&carrier, cycle), Vec3::ZERO);
        assert_eq!(carrier_offset_at(&carrier, cycle + 30), carrier_offset_at(&carrier, 30));
    }

    #[test]
    fn world_pose_is_identity_and_displacement_zero() {
        let carriers = carriers_at(vec![slider()], 1);
        assert_eq!(carriers.pose(CarrierId::WORLD), CarrierPose::IDENTITY);
        assert_eq!(carriers.pose_between(CarrierId::WORLD, 0.5), CarrierPose::IDENTITY);
        assert_eq!(carriers.displacement(CarrierId::WORLD), Vec3::ZERO);
        assert!(Carriers::default().is_static());
        assert!(!carriers.is_static());
    }

    #[test]
    fn displacement_is_the_carriers_tick_travel() {
        let carriers = carriers_at(vec![slider()], 1);
        assert!((carriers.displacement(SLIDER) - Vec3::new(4.0 / 60.0, 0.0, 0.0)).length() < 1e-5);
        assert_eq!(carriers.pose(SLIDER).translation, carrier_offset_at(&slider(), 1));
        let halfway = carriers.pose_between(SLIDER, 0.5).translation;
        assert!((halfway.x - 2.0 / 60.0).abs() < 1e-5, "halfway was {halfway}");
        assert_eq!(carriers.max_rise(), 0.0);
        assert_eq!(carriers.max_drop(), 0.0);
    }

    #[test]
    fn a_child_carrier_rides_its_parent() {
        let child = Carrier {
            parent: SLIDER,
            ..lift()
        };
        let carriers = carriers_at(vec![slider(), child], 1);
        let expected = Vec3::new(4.0 / 60.0, LEVEL_HEIGHT / 60.0, 0.0);
        assert!(
            (carriers.pose(CarrierId(2)).translation - expected).length() < 1e-5,
            "child pose {:?}",
            carriers.pose(CarrierId(2))
        );
        assert!((carriers.displacement(CarrierId(2)) - expected).length() < 1e-5);
        assert!((carriers.max_rise() - LEVEL_HEIGHT / 60.0).abs() < 1e-5);
        assert_eq!(carriers.max_drop(), 0.0);
    }

    #[test]
    fn a_sinking_lift_reports_its_drop() {
        let sinking = Carrier {
            phase_ticks: 60,
            ..lift()
        };
        let carriers = carriers_at(vec![sinking], 1);
        assert_eq!(carriers.max_rise(), 0.0);
        assert!((carriers.max_drop() - LEVEL_HEIGHT / 60.0).abs() < 1e-5);
    }
}
