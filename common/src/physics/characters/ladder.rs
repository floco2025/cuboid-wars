use bevy_math::Vec3;

use crate::{
    config::CharacterPhysicsConfig,
    constants::{LADDER_CLIMB_FACING_FRACTION, LADDER_CLIMB_MIN_SPEED, LADDER_FUNNEL_GAIN, LADDER_STANDOFF_CLEARANCE},
    math::PHYSICS_EPSILON,
    physics::world::{CollisionWorld, LadderVolume},
    protocol::{ActorMoveIntent, Position},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LadderMode {
    Disabled,
    Automatic,
    Climb,
    Exit,
}

impl LadderMode {
    #[must_use]
    pub const fn for_actor(can_use_ladders: bool, intent: ActorMoveIntent) -> Self {
        if !can_use_ladders {
            return Self::Disabled;
        }
        match intent {
            ActorMoveIntent::Climbing { .. } => Self::Climb,
            ActorMoveIntent::ExitingLadder { .. } => Self::Exit,
            _ => Self::Disabled,
        }
    }
}

pub(super) enum LadderInteraction<'a> {
    None,
    Holding,
    Ascending { velocity: f32, ladder: &'a LadderVolume },
    Descending { velocity: f32, ladder: &'a LadderVolume },
}

impl LadderInteraction<'_> {
    #[must_use]
    pub(super) const fn vertical_velocity(&self) -> Option<f32> {
        match self {
            Self::None => None,
            Self::Holding => Some(0.0),
            Self::Ascending { velocity, .. } | Self::Descending { velocity, .. } => Some(*velocity),
        }
    }

    #[must_use]
    pub(super) const fn is_ascending(&self) -> bool {
        matches!(self, Self::Ascending { .. })
    }

    #[must_use]
    pub(super) const fn is_supported(&self) -> bool {
        !matches!(self, Self::None)
    }

    #[must_use]
    pub(super) fn funnel_displacement(&self, start: &Position, delta: f32) -> Vec3 {
        let ladder = match self {
            Self::Ascending { ladder, .. } | Self::Descending { ladder, .. } => ladder,
            Self::None | Self::Holding => return Vec3::ZERO,
        };
        let offset = ladder.offset_from_axis(start.x, start.z);
        let Some(direction) = offset.try_normalize() else {
            return Vec3::ZERO;
        };
        let speed = offset.length() * LADDER_FUNNEL_GAIN;
        -direction * speed * delta
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
        let (target_x, target_z) = match self {
            // Descending ignores control's away component but never pulls the
            // body back in, so a shove that moved it out is not undone.
            Self::Descending { ladder, .. } => {
                let held_at = ladder_hold_standoff(physics).max(ladder.offset_from_plane(start.x, start.z));
                ladder.with_plane_offset(target_x, target_z, held_at)
            }
            Self::None | Self::Holding | Self::Ascending { .. } => (target_x, target_z),
        };
        clamp_move_at_ladder_plane(start, target_x, target_z, collision_world, physics)
    }
}

#[must_use]
pub(super) fn evaluate_ladder_interaction<'a>(
    ladder: Option<&'a LadderVolume>,
    mode: LadderMode,
    start: &Position,
    start_vertical_velocity: f32,
    intent_velocity: Vec3,
    delta: f32,
    has_ground_support: bool,
) -> LadderInteraction<'a> {
    if mode == LadderMode::Disabled {
        return LadderInteraction::None;
    }
    let intent_velocity = if mode == LadderMode::Exit {
        Vec3::ZERO
    } else {
        intent_velocity
    };
    // Actors can stop or reverse on a rung immediately; players retain their jump momentum.
    let start_vertical_velocity = if mode == LadderMode::Automatic {
        start_vertical_velocity
    } else {
        0.0
    };
    // No persistent climb state: the front-side volume, support, and
    // control velocity fully determine the interaction on both simulations.
    let ride_velocity = ladder.and_then(|ladder| {
        if has_ground_support && start.y >= ladder.top_landing_y() - PHYSICS_EPSILON {
            return None;
        }

        let toward_plane = -(intent_velocity.x * ladder.normal_x + intent_velocity.z * ladder.normal_z);
        let aligned = toward_plane.abs() >= intent_velocity.x.hypot(intent_velocity.z) * LADDER_CLIMB_FACING_FRACTION;
        (aligned && toward_plane.abs() >= LADDER_CLIMB_MIN_SPEED).then_some(toward_plane)
    });
    let climb_velocity = ride_velocity.filter(|velocity| *velocity > 0.0);
    if let (Some(ladder), Some(vertical_velocity)) = (ladder, climb_velocity) {
        return LadderInteraction::Ascending {
            velocity: vertical_velocity,
            ladder,
        };
    }

    if has_ground_support {
        return LadderInteraction::None;
    }

    let Some(ladder) = ladder.filter(|_| start_vertical_velocity <= 0.0) else {
        return LadderInteraction::None;
    };
    let descend_velocity = ride_velocity.filter(|velocity| *velocity < 0.0);
    if let Some(descend_velocity) = descend_velocity {
        // Never positive: a rounding error below the bottom must hang, not
        // push up and read as rising next tick.
        let stop_velocity = ((ladder.bottom_y() - start.y) / delta).min(0.0);
        return LadderInteraction::Descending {
            velocity: descend_velocity.max(stop_velocity),
            ladder,
        };
    }

    LadderInteraction::Holding
}

fn ladder_hold_standoff(physics: CharacterPhysicsConfig) -> f32 {
    physics.movement_collider.radius() + LADDER_STANDOFF_CLEARANCE
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
    // A body already inside the standoff is not sprung back out, so a shove
    // pressing it toward the rungs keeps the ground it gained.
    let standoff = ladder_hold_standoff(physics).min(ladder.offset_from_plane(start.x, start.z));
    if ladder.offset_from_plane(target_x, target_z) >= standoff {
        return (target_x, target_z);
    }
    ladder.with_plane_offset(target_x, target_z, standoff)
}
