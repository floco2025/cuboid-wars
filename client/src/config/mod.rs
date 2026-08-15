pub mod assets;
mod audio;
mod camera;
mod hud;
mod rendering;
mod settings;
mod vfx;

pub use assets::{AssetSet, MaterialDef, ModelDef, SkyboxDef};
pub use audio::{AudioConfig, ProjectileImpactAudioConfig};
pub use rendering::OpaqueRenderer;
pub use settings::{ClientSettings, DebugColorMode, GrassConfig};
pub use vfx::{
    ActorBeamInVfxConfig, ExplosionFireballVfxConfig, ExplosionLightVfxConfig, ExplosionScorchesVfxConfig,
    ExplosionShardsVfxConfig, ExplosionShockwaveVfxConfig, ExplosionSmokeVfxConfig, ExplosionVfxConfig,
    ImpactSparksVfxConfig, LaserVfxConfig, ProjectileVfxConfig, VfxConfig,
};
