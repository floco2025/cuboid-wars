use bevy::prelude::*;

#[derive(Component)]
pub struct CharacterVisualTurnState {
    pub start_yaw: f32,
    pub target_yaw: f32,
    pub elapsed: f32,
    pub duration: f32,
}

impl CharacterVisualTurnState {
    pub const fn settled(yaw: f32) -> Self {
        Self {
            start_yaw: yaw,
            target_yaw: yaw,
            elapsed: 0.0,
            duration: 0.0,
        }
    }
}
