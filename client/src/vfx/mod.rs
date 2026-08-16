mod beam;
mod cube;
mod exhaust;
mod explosion;
mod laser;
mod particles;
mod rain;
mod spark;

pub use beam::{
    BeamEmitter, BeamInGhost, beam_ghost_fade_system, beam_ghost_removed_system, beam_ghost_sparkle_system,
    ghost_fade_setup_system,
};
pub use exhaust::missile_exhaust_system;
pub use explosion::{
    ExplosionAssets, ExplosionRadii, ExplosionSpawnCtx, ExplosionVfxBudget, explosion_lights_system,
    explosion_particles_system, explosion_pulse_system, explosion_sound_speed, scorch_marks_system,
    spawn_actor_explosion, spawn_missile_explosion, spawn_player_explosion,
};
pub(crate) use explosion::{ScorchOutline, with_white_vertex_colors};
pub use laser::{LaserBeam, laser_beam_update_system, spawn_laser_beam};
pub use particles::{ParticleCloud, ParticleClouds, particle_clouds_system};
pub use rain::{RainIntensity, rain_audio_system, rain_particles_system, rain_smoothing_system};
pub use spark::{ImpactKind, spawn_impact_sparks};
