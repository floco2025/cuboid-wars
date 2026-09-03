use super::*;
use bevy::prelude::*;

use crate::{
    actors::actors_transform_sync_system,
    input::{commit_player_input_system, send_client_snapshot_system},
    missiles::missiles_movement_system,
    players::players_transform_sync_system,
    projectiles::projectiles_movement_system,
    schedule::ClientSet,
    ui::floating_labels::{
        floating_health_bar_fill_system, floating_labels_billboard_system, player_name_label_render_system,
    },
};

// Character prediction runs at the shared `TICK_HZ` tick. The
// commit system sends the player's input to the server before
// physics consumes it (so what physics simulates is what was sent).
// The capture system stamps `PreviousTickPosition` before movement
// so the render-rate transform sync can interpolate.
pub fn prediction_plugin(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        (
            commit_player_input_system,
            send_client_snapshot_system,
            capture_previous_tick_position_system,
            characters_movement_system,
            knockback_decay_system,
            // Projectiles step at the same fixed tick as the server so
            // the step-size-dependent integration doesn't diverge from
            // the authoritative trajectories.
            projectiles_movement_system,
            missiles_movement_system,
        )
            .chain(),
    );
}

// Transform sync runs every render frame and lerps between the last
// two ticks' positions so motion looks smooth above 30 Hz.
pub fn character_sync_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            players_transform_sync_system,
            actors_transform_sync_system,
            characters_visual_turn_system
                .after(players_transform_sync_system)
                .after(actors_transform_sync_system),
            floating_labels_billboard_system,
            player_name_label_render_system,
            floating_health_bar_fill_system,
        )
            .in_set(ClientSet::CharacterSync),
    );
}
