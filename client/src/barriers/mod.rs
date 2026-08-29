mod assets;
mod pressure_plates;
mod pulsate;
mod spawn;

pub use assets::{BarrierAssets, setup_barrier_assets};
pub use common::physics::OpenBarrierKinds;
pub use pressure_plates::{
    FireworkPlatesActive, PlatePurposeMarker, PressurePlateMarker, pressure_plates_spawn_system,
    pressure_plates_visibility_system,
};
pub use pulsate::barriers_pulsate_system;
pub use spawn::{BarrierKindMarker, BarrierMarker, barriers_spawn_system, barriers_visibility_system};
