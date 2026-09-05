use bevy_ecs::prelude::Resource;
use bevy_math::Vec3;

use crate::{
    config::CharacterPhysicsConfig,
    constants::MOVING_FLOOR_RIDE_TOLERANCE,
    protocol::{MapLayout, MovingFloor},
};

// Where a tile's standing surface is centered at `tick`: out along the path,
// held, back, held — a pure function of the tick, so both sides place every
// tile from the shared clock alone.
#[must_use]
pub fn surface_center_at(floor: &MovingFloor, tick: u32) -> Vec3 {
    let travel = floor.travel_ticks.max(1);
    let cycle = 2 * (travel + floor.pause_ticks);
    let phase = tick.wrapping_add(floor.phase_ticks) % cycle;
    let progress = if phase < travel {
        phase as f32 / travel as f32
    } else if phase < travel + floor.pause_ticks {
        1.0
    } else if phase < 2 * travel + floor.pause_ticks {
        1.0 - (phase - travel - floor.pause_ticks) as f32 / travel as f32
    } else {
        0.0
    };
    floor.end1().lerp(floor.end2(), progress)
}

// Every moving floor with its surface center at the last two ticks, in
// layout order. Built once from the layout on both sides and advanced right
// before character movement (`moving_floors_advance_system`).
#[derive(Resource, Default)]
pub struct MovingFloors {
    floors: Vec<MovingFloorRuntime>,
}

struct MovingFloorRuntime {
    floor: MovingFloor,
    previous: Vec3,
    current: Vec3,
}

impl MovingFloors {
    #[must_use]
    pub fn from_layout(layout: &MapLayout) -> Self {
        Self {
            floors: layout
                .moving_floors
                .iter()
                .map(|floor| {
                    let center = surface_center_at(floor, 0);
                    MovingFloorRuntime {
                        floor: *floor,
                        previous: center,
                        current: center,
                    }
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.floors.is_empty()
    }

    pub fn advance(&mut self, tick: u32) {
        for runtime in &mut self.floors {
            runtime.previous = runtime.current;
            runtime.current = surface_center_at(&runtime.floor, tick);
        }
    }

    // Collider centers at the current pose, in layout order.
    #[must_use]
    pub fn collider_centers(&self) -> Vec<Vec3> {
        self.floors
            .iter()
            .map(|runtime| runtime.current - Vec3::Y * (runtime.floor.thickness / 2.0))
            .collect()
    }

    // The surface center between the last two ticks, for render-rate
    // interpolation.
    #[must_use]
    pub fn interpolated_surface_center(&self, index: usize, alpha: f32) -> Option<Vec3> {
        let runtime = self.floors.get(index)?;
        Some(runtime.previous.lerp(runtime.current, alpha))
    }

    // How far the tile under a body moved this tick. A body rides the tile
    // whose surface its feet rest on — within the ride tolerance of the top
    // at the tile's *previous* pose, which is where the body was left
    // standing — and whose top the support probe's footprint overlaps, so
    // what carries a body is exactly what the ground probe finds under it.
    // No vertical-velocity condition: the tick a jump leaves the tile the
    // feet are still on it, and that tick's carry is what hands the jumper
    // the tile's velocity.
    #[must_use]
    pub fn carry_at(&self, feet: Vec3, physics: CharacterPhysicsConfig) -> Vec3 {
        let probe_half_x = physics.support_probe.width / 2.0;
        let probe_half_z = physics.support_probe.depth / 2.0;
        self.floors
            .iter()
            .find(|runtime| {
                let top = runtime.previous;
                (feet.y - top.y).abs() <= MOVING_FLOOR_RIDE_TOLERANCE
                    && (feet.x - top.x).abs() < runtime.floor.half_x + probe_half_x
                    && (feet.z - top.z).abs() < runtime.floor.half_z + probe_half_z
            })
            .map_or(Vec3::ZERO, |runtime| runtime.current - runtime.previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CharacterColliderAnchor, CharacterColliderConfig, CharacterSupportProbeConfig};
    use crate::test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT};

    fn slider() -> MovingFloor {
        MovingFloor {
            x1: 0.0,
            y1: 0.0,
            z1: 0.0,
            x2: 4.0,
            y2: 0.0,
            z2: 0.0,
            half_x: 1.5,
            half_z: 1.5,
            thickness: FLOOR_THICKNESS,
            travel_ticks: 60,
            pause_ticks: 30,
            phase_ticks: 0,
            level: 0,
            levels: 0,
        }
    }

    fn lift() -> MovingFloor {
        MovingFloor {
            x2: 0.0,
            y2: LEVEL_HEIGHT,
            pause_ticks: 0,
            levels: 1,
            ..slider()
        }
    }

    fn physics() -> CharacterPhysicsConfig {
        CharacterPhysicsConfig {
            collider: CharacterColliderConfig {
                width: 1.0,
                height: 1.8,
                depth: 0.6,
                y_offset: 0.5,
                y_offset_anchor: CharacterColliderAnchor::Bottom,
            },
            support_probe: CharacterSupportProbeConfig { width: 0.2, depth: 0.2 },
        }
    }

    fn floors_at(floor: MovingFloor, tick: u32) -> MovingFloors {
        let mut floors = MovingFloors::from_layout(&MapLayout {
            moving_floors: vec![floor],
            ..Default::default()
        });
        floors.advance(tick.wrapping_sub(1));
        floors.advance(tick);
        floors
    }

    #[test]
    fn surface_center_is_end1_at_phase_zero() {
        assert_eq!(surface_center_at(&slider(), 0), Vec3::ZERO);
        assert_eq!(surface_center_at(&slider(), 180), Vec3::ZERO);
    }

    #[test]
    fn surface_center_holds_at_end2_through_the_pause() {
        let floor = slider();
        assert_eq!(surface_center_at(&floor, 60), Vec3::new(4.0, 0.0, 0.0));
        assert_eq!(surface_center_at(&floor, 89), Vec3::new(4.0, 0.0, 0.0));
        assert!(surface_center_at(&floor, 91).x < 4.0);
    }

    #[test]
    fn surface_center_returns_to_end1_after_one_cycle() {
        let floor = slider();
        let cycle = 2 * (floor.travel_ticks + floor.pause_ticks);
        assert_eq!(surface_center_at(&floor, cycle), Vec3::ZERO);
        assert_eq!(surface_center_at(&floor, cycle + 30), surface_center_at(&floor, 30));
    }

    #[test]
    fn carry_is_the_tile_displacement_for_a_standing_rider() {
        let floors = floors_at(slider(), 1);
        let carry = floors.carry_at(Vec3::new(1.0, 0.0, 0.5), physics());
        assert!(
            (carry - Vec3::new(4.0 / 60.0, 0.0, 0.0)).length() < 1e-5,
            "carry was {carry}"
        );
        let lift = floors_at(lift(), 1);
        let carry = lift.carry_at(Vec3::ZERO, physics());
        assert!((carry.y - LEVEL_HEIGHT / 60.0).abs() < 1e-5, "carry was {carry}");
    }

    #[test]
    fn carry_is_zero_beside_the_tile() {
        let floors = floors_at(slider(), 1);
        assert_eq!(floors.carry_at(Vec3::new(1.7, 0.0, 0.0), physics()), Vec3::ZERO);
        assert_eq!(floors.carry_at(Vec3::new(0.0, 0.0, -1.7), physics()), Vec3::ZERO);
    }

    #[test]
    fn carry_is_zero_above_the_ride_tolerance() {
        let floors = floors_at(slider(), 1);
        assert_eq!(
            floors.carry_at(Vec3::new(0.0, MOVING_FLOOR_RIDE_TOLERANCE * 2.0, 0.0), physics()),
            Vec3::ZERO
        );
    }
}
