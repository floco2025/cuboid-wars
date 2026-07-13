pub mod assets;
mod client;
mod network;

pub use assets::{AssetSet, MaterialDef, ModelDef, SkyboxDef};
pub use client::{
    ActorBeamInVfxConfig, AudioConfig, ClientSettings, DebugColorMode, ExplosionFireballVfxConfig,
    ExplosionLightVfxConfig, ExplosionScorchesVfxConfig, ExplosionShardsVfxConfig, ExplosionShockwaveVfxConfig,
    ExplosionSmokeVfxConfig, ExplosionVfxConfig, GrassConfig, ImpactSparksVfxConfig, OpaqueRenderer,
    ProjectileImpactAudioConfig, ProjectileVfxConfig, VfxConfig,
};
pub use network::configure_client;
