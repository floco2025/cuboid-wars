mod lighting;
mod material;
mod model;
mod pressure_plate;
mod set;
#[cfg(test)]
mod tests;

pub use footsteps::FootstepSounds;
pub use lighting::SkyboxDef;
pub use material::MaterialDef;
pub use model::{AimRigDef, ModelDef, WheelModelDef, gltf_path};
pub use pressure_plate::PressurePlateDef;
pub use set::AssetSet;
mod footsteps;
