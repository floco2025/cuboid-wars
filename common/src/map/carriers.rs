use bevy_ecs::prelude::Resource;
use bevy_math::Vec3;
use bincode::{Decode, Encode};

use crate::{
    math::sequence_is_newer,
    protocol::{Carrier, CarrierId, MapLayout, PlateState, Position},
};

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
    pub fn transform_position(&self, pos: &Position) -> Position {
        Position::from(self.transform_point(Vec3::from(*pos)))
    }

    #[must_use]
    pub fn inverse_transform_position(&self, pos: &Position) -> Position {
        Position::from(self.inverse_transform_point(Vec3::from(*pos)))
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

// How far a switched carrier has run: the run ticks it had at `since_tick`
// and whether it has been running since. A pure function of the tick once
// replicated, so both sides place the carrier from the shared clock and
// this small value; a flip changes only the value, never the pose it was
// at. Absent from `PlateState`, a switched carrier rests at run 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct CarrierRun {
    pub running: bool,
    pub run_ticks: u32,
    pub since_tick: u32,
}

impl CarrierRun {
    pub const STOPPED: Self = Self {
        running: false,
        run_ticks: 0,
        since_tick: 0,
    };

    // A stamp newer than the tick (a client whose clock trails the server's)
    // adds nothing yet rather than wrapping.
    #[must_use]
    pub fn run_ticks_at(&self, tick: u32) -> u32 {
        if !self.running || sequence_is_newer(self.since_tick, tick) {
            self.run_ticks
        } else {
            self.run_ticks.wrapping_add(tick.wrapping_sub(self.since_tick))
        }
    }

    // The run after its switch flips at `tick`; unchanged when it did not,
    // since re-stamping would park a trailing client's carrier again.
    #[must_use]
    pub fn set_running(self, running: bool, tick: u32) -> Self {
        if running == self.running {
            return self;
        }
        Self {
            running,
            run_ticks: self.run_ticks_at(tick),
            since_tick: tick,
        }
    }
}

// Where a carrier's origin is in its parent's frame after `run_ticks` of
// motion: out along the path, held, back, held. A free carrier's run ticks
// are the shared tick; a switched one's come from its `CarrierRun`.
#[must_use]
pub fn carrier_offset_at(carrier: &Carrier, run_ticks: u32) -> Vec3 {
    let travel = carrier.travel_ticks;
    let cycle = 2 * (travel + carrier.pause_ticks);
    let phase = run_ticks.wrapping_add(carrier.phase_ticks) % cycle;
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
// character movement (`carriers_advance_system`), switched carriers from
// the runs the plate state carries. The default is the static world: no
// carriers, every id but `WORLD` unknown.
#[derive(Resource, Default)]
pub struct Carriers {
    carried: Vec<CarrierRuntime>,
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
            // Run 0 on both a free and a switched carrier: the tick-0 pose and the
            // stopped pose a client holds until its first snapshot coincide.
            carriers.carried.push(CarrierRuntime {
                carrier: *carrier,
                previous: pose,
                current: pose,
            });
        }
        carriers
    }

    #[must_use]
    // This fast path means no carried geometry; nested maps can also be stationary.
    pub fn is_static(&self) -> bool {
        self.carried.is_empty()
    }

    #[must_use]
    pub fn carried_count(&self) -> usize {
        self.carried.len()
    }

    // Wire-supplied carrier ids must be checked here before any pose lookup.
    #[must_use]
    pub fn contains(&self, id: CarrierId) -> bool {
        id.carried_index().is_none_or(|index| index < self.carried.len())
    }

    // Every carrier but the world, in layout order.
    pub fn carried_ids(&self) -> impl Iterator<Item = CarrierId> {
        (0..self.carried.len()).map(CarrierId::from_carried_index)
    }

    // Parents precede children, so each world pose composes from a parent
    // already at this tick.
    pub fn advance(&mut self, tick: u32, plates: &PlateState) {
        for index in 0..self.carried.len() {
            let carrier = self.carried[index].carrier;
            let id = CarrierId::from_carried_index(index);
            let run_ticks = if carrier.switch.is_some() {
                plates.carrier_run(id).unwrap_or(CarrierRun::STOPPED).run_ticks_at(tick)
            } else {
                tick
            };
            let pose = self
                .pose(carrier.parent)
                .then(&CarrierPose::from_translation(carrier_offset_at(&carrier, run_ticks)));
            let runtime = &mut self.carried[index];
            runtime.previous = runtime.current;
            runtime.current = pose;
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

    // The carrier's world pose at the last tick, where a body that rode it
    // was left standing; the identity for the world.
    #[must_use]
    pub fn previous_pose(&self, id: CarrierId) -> CarrierPose {
        self.carried(id)
            .map_or(CarrierPose::IDENTITY, |runtime| runtime.previous)
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
}

#[cfg(test)]
#[path = "tests/carriers.rs"]
mod tests;
