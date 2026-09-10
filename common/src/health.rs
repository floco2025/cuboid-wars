use bevy_ecs::prelude::*;
use bincode::{Decode, Encode};

#[derive(Debug, Clone, Copy, Encode, Decode, Component, PartialEq)]
pub struct Health(pub f32);
