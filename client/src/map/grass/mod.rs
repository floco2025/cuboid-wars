mod burn;
mod mesh;
mod spawn;

#[cfg(test)]
mod tests;

pub(crate) use burn::GrassBurn;
pub use burn::grass_burn_system;
pub use spawn::{GrassMarker, grass_spawn_system};
