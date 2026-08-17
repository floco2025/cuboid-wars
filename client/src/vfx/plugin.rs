use super::*;
use bevy::prelude::*;

use crate::{
    items::{items_animation_system, y_spin_system},
    missiles::missiles_transform_sync_system,
    projectiles::projectiles_transform_sync_system,
    schedule::ClientSet,
};

// Client-side presentation systems that animate non-character entities.
// Laser beams anchor to both endpoints' interpolated transforms — the
// `Presentation` set runs after `CharacterSync` so they read this frame's
// synced values.
pub fn presentation_plugin(app: &mut App) {
    app.add_observer(beam_ghost_removed_system);
    app.add_systems(
        Update,
        (
            projectiles_transform_sync_system,
            missiles_transform_sync_system,
            // Reads the freshly-synced missile transforms so the trail
            // starts at this frame's nozzle position.
            missile_exhaust_system.after(missiles_transform_sync_system),
            explosion_pulse_system,
            explosion_particles_system,
            explosion_lights_system,
            scorch_marks_system,
            beam_ghost_fade_system,
            beam_ghost_sparkle_system.after(beam_ghost_fade_system),
            particle_clouds_system.after(beam_ghost_sparkle_system),
            laser_beam_update_system,
            items_animation_system,
            y_spin_system,
        )
            .in_set(ClientSet::Presentation),
    );
}
