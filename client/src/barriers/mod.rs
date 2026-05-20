mod assets;
mod pressure_plates;
mod pulsate;
mod spawn;

pub use assets::{BarrierAssets, setup_barrier_assets};
pub use pressure_plates::{PressurePlateMarker, pressure_plates_spawn_system};
pub use pulsate::barriers_pulsate_system;
pub use spawn::{
    BarrierKindMarker, BarrierMarker, OpenBarrierKinds, barriers_spawn_system, barriers_visibility_system,
};
