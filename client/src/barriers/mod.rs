mod keys;
mod pressure_plates;
mod spawn;

pub use common::protocol::SwitchState;
pub use keys::KeyFields;
pub use pressure_plates::{
    LockedSwitches, PlateSwitchMarker, PressurePlateMarker, pressure_plates_spawn_system,
    pressure_plates_visibility_system,
};
pub(crate) use pressure_plates::{PressurePlateModel, pressure_plates_animation_system, pressure_plates_attach_system};
pub use spawn::{BarrierMarker, barriers_spawn_system};
