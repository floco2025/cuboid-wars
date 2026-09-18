use bevy::{input::InputSystems, prelude::*};

// Cross-domain ordering labels for the per-domain plugins. Each
// plugin keeps its own fine-grained intra-set ordering; only the edges that
// cross plugin boundaries are expressed here, so no plugin has to name
// another plugin's systems.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientSet {
    // Console keystrokes: opening, typing, submitting.
    Console,
    // Movement/view input in PreUpdate; weapon selection and display toggles in Update.
    Input,
    // Consume server messages, send pings.
    Network,
    // Render-frame interpolation of character transforms + floating labels.
    CharacterSync,
    // Camera follow/shake and crosshair lock detection.
    Camera,
    // Animation of non-character entities (projectiles, missiles, vfx, items).
    Presentation,
    // Map geometry spawning and visibility/material maintenance.
    MapMaintenance,
    // Screen-space HUD.
    Hud,
    // Procedural celestial sky, scene lighting, and rain.
    Sky,
}

pub fn configure_client_sets(app: &mut App) {
    app.configure_sets(
        PreUpdate,
        (ClientSet::Console, ClientSet::Input).chain().after(InputSystems),
    );
    app.configure_sets(
        Update,
        (
            ClientSet::Input,
            // Cameras follow the local player after the fixed simulation and
            // this frame's remaining input and network state.
            ClientSet::Camera.after(ClientSet::Input).after(ClientSet::Network),
            // HUD rendering observes this frame's keystrokes and the
            // feed/banner lines Network pushed.
            ClientSet::Hud
                .after(ClientSet::Input)
                .after(ClientSet::Network)
                .after(ClientSet::Camera),
            // Laser beams and missile exhaust anchor to this frame's
            // interpolated character/missile transforms, so they must read
            // the freshly-synced values.
            ClientSet::CharacterSync.after(ClientSet::Network),
            ClientSet::Presentation.after(ClientSet::CharacterSync),
            // Rain intensity is smoothed in Sky before the shared particle
            // clouds in Presentation consume the spawned drops.
            ClientSet::Sky.before(ClientSet::Presentation),
            // Grass burn reacts to this frame's scorch marks (Presentation)
            // and to explosions delivered by this frame's server messages.
            ClientSet::MapMaintenance
                .after(ClientSet::Presentation)
                .after(ClientSet::Network),
        ),
    );
}

#[cfg(test)]
#[path = "tests/schedule.rs"]
mod tests;
