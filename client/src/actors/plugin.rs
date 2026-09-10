use super::*;
use bevy::prelude::*;

use crate::{characters::characters_visual_turn_system, schedule::ClientSet};

// Actor visuals follow the interpolated character transforms every render
// frame: the wheels read the turned body, so both run after the visual turn.
pub fn actor_visuals_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            interpolate_remote_actors_system,
            actors_transform_sync_system.after(interpolate_remote_actors_system),
            wheel_animation_update_system.after(characters_visual_turn_system),
            wheel_grounding_system.after(characters_visual_turn_system),
        )
            .in_set(ClientSet::CharacterSync),
    );
}
