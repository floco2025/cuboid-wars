mod inputs;
mod system;

pub(super) use inputs::PressurePlateInputs;
pub(super) use system::{pressure_plates_collision_system, pressure_plates_system, switch_reset_system};
