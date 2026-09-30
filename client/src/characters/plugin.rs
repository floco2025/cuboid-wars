use super::*;
use bevy::prelude::*;

use common::{physics::carriers_advance_system, protocol::server_tick_advance_system};

use crate::{
    actors::actors_transform_sync_system,
    carriers::carriers_transform_sync_system,
    missiles::{interpolate_remote_missiles_system, missiles_movement_system, remote_missile_impacts_system},
    players::{
        crouch_blend_system, footsteps_plugin, interpolate_remote_players_system, local_player_cuboid_shake_system,
        local_player_feedback_system, local_player_movement_system, player_animation_update_system,
        players_transform_sync_system,
    },
    portals::{carried_portals_refresh_system, portal_body_clipping_system, portal_surfaces_transform_sync_system},
    projectiles::projectiles_movement_system,
    schedule::ClientSet,
    ui::floating_labels::{
        floating_health_bar_fill_system, floating_labels_billboard_system, player_name_label_render_system,
    },
};

// The tick advances first so everything the step records carries the tick it
// belongs to; the owner's tick then moves, crosses, reports, and decays in one.
pub fn local_simulation_plugin(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        (
            server_tick_advance_system,
            carriers_advance_system,
            carried_portals_refresh_system,
            local_player_movement_system,
            local_player_feedback_system,
            projectiles_movement_system,
            missiles_movement_system,
        )
            .chain()
            .run_if(crate::network::live_gameplay),
    );
}

// Character presentation follows local fixed ticks or buffered remote samples each render frame.
pub fn character_sync_plugin(app: &mut App) {
    app.add_plugins(footsteps_plugin);
    app.init_resource::<BoundsMode>();
    app.add_systems(
        Update,
        (
            character_models_attach_system,
            interpolate_remote_players_system.run_if(crate::network::live_gameplay),
            crouch_blend_system.after(interpolate_remote_players_system),
            (interpolate_remote_missiles_system, remote_missile_impacts_system)
                .chain()
                .run_if(crate::network::live_gameplay),
            players_transform_sync_system
                .after(local_player_cuboid_shake_system)
                .after(crouch_blend_system),
            carriers_transform_sync_system,
            portal_surfaces_transform_sync_system,
            characters_visual_turn_system
                .after(players_transform_sync_system)
                .after(actors_transform_sync_system),
            refresh_grounding_debug_system
                .after(interpolate_remote_players_system)
                .after(actors_transform_sync_system),
            character_bounds_sync_system
                .after(characters_visual_turn_system)
                .after(refresh_grounding_debug_system),
            grounding_debug_system.after(character_bounds_sync_system),
            player_animation_update_system.after(characters_visual_turn_system),
            portal_body_clipping_system
                .after(player_animation_update_system)
                .after(crouch_blend_system),
            floating_labels_billboard_system,
            player_name_label_render_system,
            floating_health_bar_fill_system,
        )
            .in_set(ClientSet::CharacterSync),
    );
}
