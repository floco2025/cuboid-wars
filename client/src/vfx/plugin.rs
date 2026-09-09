use super::*;
use bevy::{app::AnimationSystems, prelude::*, transform::TransformSystems};

use crate::{
    items::{items_animation_system, y_spin_system},
    missiles::missiles_transform_sync_system,
    projectiles::projectiles_transform_sync_system,
    schedule::ClientSet,
};

// Client-side presentation systems that animate non-character entities.
pub fn presentation_plugin(app: &mut App) {
    // Aim after animation and before propagation so the beam and moving muzzle share one pose.
    app.add_systems(
        PostUpdate,
        laser_beam_update_system
            .after(AnimationSystems)
            .before(TransformSystems::Propagate),
    );
    app.init_resource::<FireworkShow>();
    app.init_resource::<PortalFizzleAssets>();
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
            laser_beams_sync_system,
            firework_system,
            portal_fizzle_system,
            items_animation_system,
            y_spin_system,
        )
            .in_set(ClientSet::Presentation),
    );
}
