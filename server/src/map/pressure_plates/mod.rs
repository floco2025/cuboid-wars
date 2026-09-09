mod switches;
mod system;

#[cfg(test)]
mod tests;

pub(super) use switches::{PressureSwitches, plate_state_sync_system};
pub(super) use system::{pressure_plates_system, pressure_switch_reset_system};
