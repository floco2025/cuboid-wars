mod animation;
mod model;
mod spawn;
#[cfg(test)]
mod tests;
mod visibility;

pub(crate) use animation::pressure_plates_animation_system;
pub(crate) use model::{PressurePlateModel, pressure_plates_attach_system};
pub use spawn::{PlateSwitchMarker, PressurePlateMarker, pressure_plates_spawn_system};
pub use visibility::{LockedSwitches, pressure_plates_visibility_system};
