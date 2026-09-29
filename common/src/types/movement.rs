use bevy_ecs::prelude::*;
use bevy_math::{Vec2, Vec3};
use bincode::{Decode, Encode};

use super::{CarrierId, Position};
use crate::{
    config::{CharacterGameplayConfig, CharacterPhysicsConfig},
    constants::{PLAYER_AIR_STEER_PITCH, PLAYER_CROUCH_EYE_RATIO, PLAYER_CROUCH_HULL_RATIO},
    physics::CharacterSupport,
};

// View-relative movement input. Physics owns velocity and resolved stance;
// releasing buttons never rewrites either. Sideways is positive to the left.
#[derive(Debug, Clone, Encode, Decode, Copy, Component, Default, PartialEq)]
pub struct PlayerMoveIntent {
    pub forward: f32,
    pub sideways: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub crouch: bool,
}

impl PlayerMoveIntent {
    pub const NONE: Self = Self {
        forward: 0.0,
        sideways: 0.0,
        yaw: 0.0,
        pitch: 0.0,
        crouch: false,
    };

    pub const fn moving(direction: f32) -> Self {
        Self {
            forward: 1.0,
            yaw: direction,
            ..Self::NONE
        }
    }

    pub fn direction(self) -> Option<f32> {
        (self.forward != 0.0 || self.sideways != 0.0).then(|| self.yaw + self.sideways.atan2(self.forward))
    }

    // Airborne, forward input follows the view's horizontal component past
    // PLAYER_AIR_STEER_PITCH, scaled to keep full strength up to that angle.
    pub fn wish_velocity(self, speed: f32, airborne: bool) -> Vec3 {
        let input = Vec2::new(self.sideways, self.forward).clamp_length_max(1.0);
        let steer = if airborne {
            (self.pitch.cos() / PLAYER_AIR_STEER_PITCH.cos()).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let forward = input.y * steer;
        let (sin, cos) = self.yaw.sin_cos();
        Vec3::new(sin * forward + cos * input.x, 0.0, cos * forward - sin * input.x) * speed
    }

    pub fn is_finite(self) -> bool {
        self.forward.is_finite() && self.sideways.is_finite() && self.yaw.is_finite() && self.pitch.is_finite()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Component, Encode, Decode)]
pub struct PlayerStance {
    pub crouched: bool,
    // Camera/pose blend; the collision hull changes only at an accepted transition.
    pub fraction: f32,
}

impl PlayerStance {
    pub fn physics(self, config: &CharacterGameplayConfig) -> CharacterPhysicsConfig {
        self.adjust_physics(config.physics())
    }

    pub fn adjust_physics(self, mut physics: CharacterPhysicsConfig) -> CharacterPhysicsConfig {
        if self.crouched {
            physics.movement_collider.height *= PLAYER_CROUCH_HULL_RATIO;
            physics.hitbox.height *= PLAYER_CROUCH_HULL_RATIO;
        }
        physics
    }

    pub fn eye_height(self, config: &CharacterGameplayConfig) -> f32 {
        config.eye_height() * self.blend(PLAYER_CROUCH_EYE_RATIO)
    }

    // Vertical scale of the rendered model, following the hull through the blend.
    pub fn model_height_scale(self) -> f32 {
        self.blend(PLAYER_CROUCH_HULL_RATIO)
    }

    fn blend(self, crouched: f32) -> f32 {
        1.0 - self.fraction * (1.0 - crouched)
    }
}

#[derive(Debug, Clone, Encode, Decode, Copy, Component, Default, PartialEq)]
pub enum ActorMoveIntent {
    #[default]
    Idle,
    Flying {
        velocity: [f32; 3],
    },
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
    pub fn direction(&self) -> Option<f32> {
        match self {
            Self::Idle => None,
            Self::Flying { velocity } => {
                (velocity[0] != 0.0 || velocity[2] != 0.0).then(|| velocity[0].atan2(velocity[2]))
            }
            Self::ExitingLadder { direction, .. }
            | Self::Moving { direction, .. }
            | Self::Climbing { direction, .. } => Some(*direction),
        }
    }

    #[must_use]
    pub fn speed(&self) -> Option<f32> {
        match self {
            Self::Idle => None,
            Self::Flying { velocity } => Some(Vec3::from_array(*velocity).length()),
            Self::ExitingLadder { speed, .. } | Self::Moving { speed, .. } | Self::Climbing { speed, .. } => {
                Some(*speed)
            }
        }
    }

    #[must_use]
    pub fn to_horizontal_velocity(&self) -> Vec3 {
        match self {
            Self::Idle => Vec3::ZERO,
            Self::Flying { velocity } => Vec3::from_array(*velocity).with_y(0.0),
            Self::ExitingLadder { direction, speed }
            | Self::Moving { direction, speed }
            | Self::Climbing { direction, speed } => Vec3::new(direction.sin() * speed, 0.0, direction.cos() * speed),
        }
    }
}

#[derive(Component, Default)]
pub struct FaceYaw(pub f32);

// Everything one movement step leaves behind, as the wire carries it; the
// owning client builds it from its components and turns it back into them.
#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PlayerMovementState {
    // Only position is carrier-local; intent, facing, and velocities stay in world space.
    pub carrier: CarrierId,
    pub pos: Position,
    pub move_intent: PlayerMoveIntent,
    pub vertical_velocity: f32,
    pub face_yaw: f32,
    pub horizontal_velocity: [f32; 3],
    pub stance: PlayerStance,
    pub knockback: [f32; 3],
    pub support: CharacterSupport,
}

impl PlayerMovementState {
    #[must_use]
    pub const fn new(pos: Position, move_intent: PlayerMoveIntent, vertical_velocity: f32, face_yaw: f32) -> Self {
        Self {
            carrier: CarrierId::WORLD,
            pos,
            move_intent,
            vertical_velocity,
            face_yaw,
            horizontal_velocity: [0.0; 3],
            stance: PlayerStance {
                crouched: false,
                fraction: 0.0,
            },
            knockback: [0.0; 3],
            support: CharacterSupport::Airborne,
        }
    }

    #[must_use]
    pub fn with_momentum(mut self, horizontal: Vec3, knockback: Vec3) -> Self {
        self.horizontal_velocity = horizontal.to_array();
        self.knockback = knockback.to_array();
        self
    }

    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.stance.fraction.is_finite()
            && (0.0..=1.0).contains(&self.stance.fraction)
            && self.pos.is_finite()
            && self.move_intent.is_finite()
            && self.vertical_velocity.is_finite()
            && self.face_yaw.is_finite()
            && Vec3::from_array(self.horizontal_velocity).is_finite()
            && Vec3::from_array(self.knockback).is_finite()
    }
}

// Carrier-relative positions keep buffered actor motion aligned with moving geometry.
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct ActorMovementState {
    pub pos: Position,
    pub carrier: CarrierId,
    pub move_intent: ActorMoveIntent,
    pub vertical_velocity: f32,
    pub face_yaw: f32,
    pub support: CharacterSupport,
}

// Missile flight reports its orientation independently of actor facing.
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct MissileMovementState {
    pub pos: Position,
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}

impl MissileMovementState {
    #[must_use]
    pub fn is_finite(&self) -> bool {
        Vec3::from(self.pos).is_finite()
            && self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.speed.is_finite()
            && self.speed >= 0.0
    }

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
#[path = "tests/movement.rs"]
mod tests;
