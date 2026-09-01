use super::*;
use bevy::prelude::*;

use crate::{cameras::scene_render_target_system, missiles::lock_on_system, schedule::ClientSet};

// Cameras follow the local player; the `Camera` set runs after `Input` so
// this frame's input-driven player state is what the camera reads.
pub fn camera_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            local_player_camera_shake_system,
            local_player_cuboid_shake_system,
            local_player_camera_sync_system.after(local_player_camera_shake_system),
            local_player_portal_blend_system
                .after(local_player_camera_sync_system)
                .before(lock_on_system)
                .before(local_player_rearview_sync_system),
            local_player_rearview_sync_system.after(local_player_camera_sync_system),
            // Lock detection reads this frame's camera ray (shake
            // included) so the lit crosshair matches what's on screen.
            lock_on_system.after(local_player_camera_sync_system),
            local_player_rearview_viewport_system.after(local_player_rearview_sync_system),
            // Resizes the scene image before the rearview viewport is laid
            // out inside it.
            scene_render_target_system.before(local_player_rearview_viewport_system),
            local_player_render_layer_system,
            local_player_light_layer_system,
            local_player_visibility_sync_system,
        )
            .in_set(ClientSet::Camera),
    );
}
