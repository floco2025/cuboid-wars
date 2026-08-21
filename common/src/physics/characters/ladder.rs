use bevy_math::Vec3;

use crate::{
    config::{CharacterPhysicsConfig, LaddersConfig},
    constants::{LADDER_CLIMB_FACING_FRACTION, LADDER_CLIMB_MIN_SPEED, LADDER_STANDOFF_CLEARANCE, PHYSICS_EPSILON},
    physics::world::{CollisionWorld, LadderVolume},
    protocol::Position,
};

pub(super) struct LadderInteraction<'a> {
    vertical_velocity: Option<f32>,
    descend_hold: Option<&'a LadderVolume>,
    climbing: bool,
}

impl LadderInteraction<'_> {
    #[must_use]
    pub(super) const fn vertical_velocity(&self) -> Option<f32> {
        self.vertical_velocity
    }

    #[must_use]
    pub(super) const fn climbing(&self) -> bool {
        self.climbing
    }

    #[must_use]
    pub(super) fn constrain_target(
        &self,
        start: &Position,
        target_x: f32,
        target_z: f32,
        collision_world: &CollisionWorld,
        physics: CharacterPhysicsConfig,
    ) -> (f32, f32) {
        let (target_x, target_z) = match self.descend_hold {
            Some(ladder) => ladder.with_plane_offset(target_x, target_z, ladder_hold_standoff(ladder, physics)),
            None => (target_x, target_z),
        };
        clamp_move_at_ladder_plane(start, target_x, target_z, collision_world, physics)
    }
}

#[must_use]
pub(super) fn evaluate_ladder_interaction<'a>(
    collision_world: &'a CollisionWorld,
    start: &Position,
    start_vertical_velocity: f32,
    control_velocity: Vec3,
    delta: f32,
    has_ground_support: bool,
    config: LaddersConfig,
) -> LadderInteraction<'a> {
    // No persistent climb state: the current front-side volume, support, and
    // control velocity fully determine the interaction on both simulations.
    let ladder = collision_world.ladder_volume_at(start);
    let ride_velocity = ladder.and_then(|ladder| {
        if has_ground_support && start.y >= ladder.top_landing_y() - PHYSICS_EPSILON {
            return None;
        }

        let toward_plane = -(control_velocity.x * ladder.normal_x + control_velocity.z * ladder.normal_z);
        let aligned = toward_plane.abs() >= control_velocity.x.hypot(control_velocity.z) * LADDER_CLIMB_FACING_FRACTION;
        (aligned && toward_plane.abs() >= LADDER_CLIMB_MIN_SPEED).then_some(toward_plane * config.climb_speed_ratio)
    });
    let climb_velocity = ride_velocity.filter(|velocity| *velocity > 0.0);
    if let Some(vertical_velocity) = climb_velocity {
        return LadderInteraction {
            vertical_velocity: Some(vertical_velocity),
            descend_hold: None,
            climbing: true,
        };
    }

    if has_ground_support {
        return LadderInteraction {
            vertical_velocity: None,
            descend_hold: None,
            climbing: false,
        };
    }

    let Some(ladder) = ladder.filter(|_| start_vertical_velocity <= 0.0) else {
        return LadderInteraction {
            vertical_velocity: None,
            descend_hold: None,
            climbing: false,
        };
    };
    let descend_velocity = ride_velocity.filter(|velocity| *velocity < 0.0);
    if let Some(descend_velocity) = descend_velocity {
        return LadderInteraction {
            vertical_velocity: Some(descend_velocity.max((ladder.bottom_y() - start.y) / delta)),
            descend_hold: Some(ladder),
            climbing: false,
        };
    }

    LadderInteraction {
        vertical_velocity: Some(0.0),
        descend_hold: None,
        climbing: false,
    }
}

fn ladder_hold_standoff(ladder: &LadderVolume, physics: CharacterPhysicsConfig) -> f32 {
    let half_extent_toward_plane = if ladder.normal_x != 0.0 {
        physics.collider.width / 2.0
    } else {
        physics.collider.depth / 2.0
    };
    half_extent_toward_plane + LADDER_STANDOFF_CLEARANCE
}

fn clamp_move_at_ladder_plane(
    start: &Position,
    target_x: f32,
    target_z: f32,
    collision_world: &CollisionWorld,
    physics: CharacterPhysicsConfig,
) -> (f32, f32) {
    // The target query catches crossings while the start-side check keeps the
    // back permeable. The band ends at the landing so a crest can cross.
    let Some(ladder) = collision_world.ladder_band_at(target_x, target_z, start.y) else {
        return (target_x, target_z);
    };
    if ladder.offset_from_plane(start.x, start.z) <= 0.0 {
        return (target_x, target_z);
    }
    let standoff = ladder_hold_standoff(ladder, physics);
    if ladder.offset_from_plane(target_x, target_z) >= standoff {
        return (target_x, target_z);
    }
    ladder.with_plane_offset(target_x, target_z, standoff)
}
