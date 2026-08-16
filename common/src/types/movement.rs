use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bincode::{Decode, Encode};

use super::Position;

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
    pub fn to_horizontal_velocity(&self, walk_speed: f32, run_speed: f32, has_speed_power_up: bool) -> Vec3 {
        match self {
            Self::Idle => Vec3::ZERO,
            Self::Walking { direction } => {
                let speed = player_speed_with_power_up(walk_speed, has_speed_power_up);
                Vec3::new(direction.sin() * speed, 0.0, direction.cos() * speed)
            }
            Self::Running { direction } => {
                let speed = player_speed_with_power_up(run_speed, has_speed_power_up);
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

fn player_speed_with_power_up(speed: f32, has_speed_power_up: bool) -> f32 {
    if has_speed_power_up {
        speed * crate::constants::POWER_UP_SPEED_MULTIPLIER
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
}

impl ActorMoveIntent {
    #[must_use]
    pub const fn direction(&self) -> Option<f32> {
        match self {
            Self::Idle => None,
            Self::Moving { direction, .. } => Some(*direction),
        }
    }

    #[must_use]
    pub const fn speed(&self) -> Option<f32> {
        match self {
            Self::Idle => None,
            Self::Moving { speed, .. } => Some(*speed),
        }
    }

    #[must_use]
    pub fn to_horizontal_velocity(&self) -> Vec3 {
        match self {
            Self::Idle => Vec3::ZERO,
            Self::Moving { direction, speed } => Vec3::new(direction.sin() * speed, 0.0, direction.cos() * speed),
        }
    }
}

#[derive(Component, Default)]
pub struct FaceDirection(pub f32);

#[derive(Debug, Clone, Copy, Encode, Decode)]
pub struct PlayerMovementState {
    pub pos: Position,
    pub move_intent: PlayerMoveIntent,
    pub vertical_velocity: f32,
}

impl PlayerMovementState {
    #[must_use]
    pub const fn new(pos: Position, move_intent: PlayerMoveIntent, vertical_velocity: f32) -> Self {
        Self {
            pos,
            move_intent,
            vertical_velocity,
        }
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
    fn idle_intent_is_finite() {
        assert!(PlayerMoveIntent::Idle.is_finite());
    }

    #[test]
    fn finite_directions_are_accepted() {
        assert!(PlayerMoveIntent::Walking { direction: 0.0 }.is_finite());
        assert!(PlayerMoveIntent::Running { direction: -2.5 }.is_finite());
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

    #[test]
    fn non_finite_directions_are_rejected() {
        assert!(!PlayerMoveIntent::Walking { direction: f32::NAN }.is_finite());
        assert!(
            !PlayerMoveIntent::Running {
                direction: f32::INFINITY
            }
            .is_finite()
        );
        assert!(
            !PlayerMoveIntent::Walking {
                direction: f32::NEG_INFINITY
            }
            .is_finite()
        );
    }
}
