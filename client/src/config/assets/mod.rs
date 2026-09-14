mod lighting;
mod material;
mod model;
mod pressure_plate;
mod set;
mod sound;
#[cfg(test)]
mod tests;

pub use footsteps::FootstepSounds;
pub use material::MaterialDef;
pub use model::{AimRigDef, ModelDef, WheelModelDef, gltf_path};
pub use pressure_plate::PressurePlateDef;
pub use set::{AssetSet, MapMaterials};
pub use sound::SoundDef;
mod footsteps;
