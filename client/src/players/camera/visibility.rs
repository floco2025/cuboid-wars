use bevy::prelude::*;

use crate::{cameras::CameraViewMode, players::LocalPlayerMarker};

// Update local player visibility based on camera view mode.
pub fn local_player_visibility_sync_system(
    view_mode: Res<CameraViewMode>,
    mut local_player_query: Query<&mut Visibility, With<LocalPlayerMarker>>,
) {
    // Always check and update, not just when changed, to ensure it's correct.
    for mut visibility in &mut local_player_query {
        let desired_visibility = match *view_mode {
            CameraViewMode::FirstPerson => Visibility::Hidden,
            CameraViewMode::TopDown => Visibility::Visible,
        };

        if *visibility != desired_visibility {
            *visibility = desired_visibility;
        }
    }
}
