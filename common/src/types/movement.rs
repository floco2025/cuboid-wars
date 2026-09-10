use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bincode::{Decode, Encode};

use super::Position;
use crate::physics::CharacterSupport;

#[derive(Debug, Clone, Encode, Decode, Copy, Component, Default, PartialEq)]
pub enum PlayerMoveIntent {
    #[default]
    Idle,
    Walking {
        direction: f32,
    },
    Running {
        direction: f32,
    },
}

impl PlayerMoveIntent {
    #[must_use]
    pub const fn direction(&self) -> Option<f32> {
        match self {
            Self::Idle => None,
            Self::Walking { direction } | Self::Running { direction } => Some(*direction),
        }
    }

    #[must_use]
    pub fn to_horizontal_velocity(
        &self,
        walk_speed: f32,
        run_speed: f32,
        has_speed_power_up: bool,
        speed_multiplier: f32,
    ) -> Vec3 {
        match self {
            Self::Idle => Vec3::ZERO,
            Self::Walking { direction } => {
                let speed = player_speed_with_power_up(walk_speed, has_speed_power_up, speed_multiplier);
                Vec3::new(direction.sin() * speed, 0.0, direction.cos() * speed)
            }
            Self::Running { direction } => {
                let speed = player_speed_with_power_up(run_speed, has_speed_power_up, speed_multiplier);
                Vec3::new(direction.sin() * speed, 0.0, direction.cos() * speed)
            }
        }
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    // Reject directions a malformed/malicious client could send: a NaN/inf
    // direction would turn into a NaN velocity in `to_horizontal_velocity` and
    // corrupt the authoritative position the server broadcasts to everyone.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.direction().is_none_or(f32::is_finite)
    }
}

fn player_speed_with_power_up(speed: f32, has_speed_power_up: bool, speed_multiplier: f32) -> f32 {
    if has_speed_power_up {
        speed * speed_multiplier
    } else {
        speed
    }
}

#[derive(Debug, Clone, Encode, Decode, Copy, Component, Default, PartialEq)]
pub enum ActorMoveIntent {
    #[default]
    Idle,
    Moving {
        direction: f32,
        speed: f32,
    },
    Climbing {
        direction: f32,
        speed: f32,
    },
    ExitingLadder {
        direction: f32,
        speed: f32,
    },
}

impl ActorMoveIntent {
    pub const fn uses_ladders(self) -> bool {
        matches!(self, Self::Climbing { .. } | Self::ExitingLadder { .. })
    }

    pub const fn holding_ladder(self) -> Self {
        match self {
            Self::ExitingLadder { direction, .. } => Self::ExitingLadder { direction, speed: 0.0 },
            Self::Climbing { direction, .. } => Self::Climbing { direction, speed: 0.0 },
            _ => Self::Idle,
        }
    }

    #[must_use]
    pub const fn direction(&self) -> Option<f32> {
        match self {
            Self::Idle => None,
            Self::ExitingLadder { direction, .. }
            | Self::Moving { direction, .. }
            | Self::Climbing { direction, .. } => Some(*direction),
        }
    }

    #[must_use]
    pub const fn speed(&self) -> Option<f32> {
        match self {
            Self::Idle => None,
            Self::ExitingLadder { speed, .. } | Self::Moving { speed, .. } | Self::Climbing { speed, .. } => {
                Some(*speed)
            }
        }
    }

    #[must_use]
    pub fn to_horizontal_velocity(&self) -> Vec3 {
        match self {
            Self::Idle => Vec3::ZERO,
            Self::ExitingLadder { direction, speed }
            | Self::Moving { direction, speed }
            | Self::Climbing { direction, speed } => Vec3::new(direction.sin() * speed, 0.0, direction.cos() * speed),
        }
    }
}

#[derive(Component, Default)]
pub struct FaceYaw(pub f32);

// Everything one movement step leaves behind, as the wire carries it.
// `physics::player_movement_state` builds it from the components and
// `PlayerMotionBundle` turns it back into them.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PlayerMovementState {
    pub pos: Position,
    pub move_intent: PlayerMoveIntent,
    pub vertical_velocity: f32,
    pub face_yaw: f32,
    pub airborne_momentum: [f32; 3],
    pub knockback: [f32; 3],
    pub support: CharacterSupport,
}

impl PlayerMovementState {
    #[must_use]
    pub const fn new(pos: Position, move_intent: PlayerMoveIntent, vertical_velocity: f32, face_yaw: f32) -> Self {
        Self {
            pos,
            move_intent,
            vertical_velocity,
            face_yaw,
            airborne_momentum: [0.0; 3],
            knockback: [0.0; 3],
            support: CharacterSupport::Airborne,
        }
    }

    #[must_use]
    pub fn with_momentum(mut self, airborne: Vec3, knockback: Vec3) -> Self {
        self.airborne_momentum = airborne.to_array();
        self.knockback = knockback.to_array();
        self
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        Vec3::from(self.pos).is_finite()
            && self.move_intent.is_finite()
            && self.vertical_velocity.is_finite()
            && self.face_yaw.is_finite()
            && Vec3::from_array(self.airborne_momentum).is_finite()
            && Vec3::from_array(self.knockback).is_finite()
    }
}

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct ActorMovementState {
    pub pos: Position,
    pub move_intent: ActorMoveIntent,
    pub vertical_velocity: f32,
}

impl ActorMovementState {
    #[must_use]
    pub const fn new(pos: Position, move_intent: ActorMoveIntent, vertical_velocity: f32) -> Self {
        Self {
            pos,
            move_intent,
            vertical_velocity,
        }
    }
}

// Missile flight state on the wire. Unlike `ActorMovementState`, missiles fly:
// the direction needs pitch, which `ActorMoveIntent` structurally cannot carry
// (its velocity is horizontal-only). Decomposed into scalars per wire style.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct MissileMovementState {
    pub pos: Position,
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}

impl MissileMovementState {
    #[must_use]
    pub fn velocity(&self) -> Vec3 {
        crate::math::direction_from_yaw_pitch(self.yaw, self.pitch) * self.speed
    }

    #[must_use]
    pub fn from_velocity(pos: Position, velocity: Vec3) -> Self {
        let speed = velocity.length();
        if speed <= f32::EPSILON {
            return Self {
                pos,
                yaw: 0.0,
                pitch: 0.0,
                speed: 0.0,
            };
        }
        Self {
            pos,
            yaw: velocity.x.atan2(velocity.z),
            pitch: (velocity.y / speed).clamp(-1.0, 1.0).asin(),
            speed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_intents_reject_only_non_finite_directions() {
        assert!(PlayerMoveIntent::Idle.is_finite());
        for (direction, finite) in [
            (0.0, true),
            (-2.5, true),
            (f32::NAN, false),
            (f32::INFINITY, false),
            (f32::NEG_INFINITY, false),
        ] {
            for intent in [
                PlayerMoveIntent::Walking { direction },
                PlayerMoveIntent::Running { direction },
            ] {
                assert_eq!(intent.is_finite(), finite, "{intent:?}");
            }
        }
    }

    #[test]
    fn missile_movement_state_velocity_round_trips() {
        let pos = Position { x: 1.0, y: 2.0, z: 3.0 };
        let velocity = Vec3::new(3.0, -4.0, 12.0);
        let state = MissileMovementState::from_velocity(pos, velocity);
        let recovered = state.velocity();
        assert!((recovered - velocity).length() < 1e-4);
        assert!((state.speed - 13.0).abs() < 1e-4);
    }

    #[test]
    fn missile_movement_state_zero_velocity_is_stationary() {
        let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
        let state = MissileMovementState::from_velocity(pos, Vec3::ZERO);
        assert_eq!(state.speed, 0.0);
        assert_eq!(state.velocity(), Vec3::ZERO);
    }
}
