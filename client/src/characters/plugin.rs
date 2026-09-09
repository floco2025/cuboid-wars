use super::*;
use bevy::prelude::*;

use common::{
    physics::{carried_portals_refresh_system, carriers_advance_system, knockback_decay_system},
    protocol::server_tick_advance_system,
};

use crate::{
    actors::actors_transform_sync_system,
    carriers::carriers_transform_sync_system,
    input::{capture_player_input_system, commit_player_input_system},
    missiles::missiles_movement_system,
    players::{local_player_cuboid_shake_system, player_animation_update_system, players_transform_sync_system},
    portals::{portal_surfaces_transform_sync_system, portal_transit_system},
    projectiles::projectiles_movement_system,
    schedule::ClientSet,
    ui::floating_labels::{
        floating_health_bar_fill_system, floating_labels_billboard_system, player_name_label_render_system,
    },
};

// Capture the input frame before physics and report its result after portal
// transit, before knockback decay, matching the server comparison phase.
pub fn prediction_plugin(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        (
            server_tick_advance_system,
            capture_player_input_system,
            capture_previous_tick_position_system,
            carriers_advance_system,
            carried_portals_refresh_system,
            characters_movement_system,
            portal_transit_system,
            commit_player_input_system,
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
    app.init_resource::<BoundsMode>();
    app.add_systems(
        Update,
        (
            character_models_attach_system,
            players_transform_sync_system.after(local_player_cuboid_shake_system),
            carriers_transform_sync_system,
            portal_surfaces_transform_sync_system,
            characters_visual_turn_system
                .after(players_transform_sync_system)
                .after(actors_transform_sync_system),
            refresh_grounding_debug_system,
            character_bounds_sync_system
                .after(characters_visual_turn_system)
                .after(refresh_grounding_debug_system),
            grounding_debug_system.after(character_bounds_sync_system),
            player_animation_update_system.after(characters_visual_turn_system),
            floating_labels_billboard_system,
            player_name_label_render_system,
            floating_health_bar_fill_system,
        )
            .in_set(ClientSet::CharacterSync),
    );
}
