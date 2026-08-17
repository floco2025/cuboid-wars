use bevy::prelude::*;

// Cross-domain ordering labels for the per-domain `Update` plugins. Each
// plugin keeps its own fine-grained intra-set ordering; only the edges that
// cross plugin boundaries are expressed here, so no plugin has to name
// another plugin's systems.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClientSet {
    // Player input + console keystroke gating.
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
    // Skybox setup/drift and rain.
    Sky,
}

pub fn configure_client_sets(app: &mut App) {
    app.configure_sets(
        Update,
        (
            // Cameras follow the local player after input/prediction has had
            // a chance to update the player state.
            ClientSet::Camera.after(ClientSet::Input),
            // The console claims keystrokes during Input; HUD rendering
            // (including the console itself) must observe this frame's
            // post-input state.
            ClientSet::Hud.after(ClientSet::Input),
            // Laser beams and missile exhaust anchor to this frame's
            // interpolated character/missile transforms, so they must read
            // the freshly-synced values.
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
