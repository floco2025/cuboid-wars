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
