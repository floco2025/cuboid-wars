mod actors;
mod bootstrap;
mod context;
mod impairment;
mod io;
mod items;
mod missiles;
mod players;
mod plugin;
mod portals;
mod presentation;
mod projectiles;
mod quests;
mod resources;
mod routing;
mod sample_buffer;
mod snapshot;
mod tick;
mod transport;

pub(crate) use bootstrap::install_bootstrap;
pub use bootstrap::login;
pub use impairment::Impairment;
pub use plugin::network_plugin;
pub(crate) use resources::accept_newer_tick;
pub use resources::{ClientToServerChannel, LastPlayerMovesTick, LastSnapshotTick, RoundTripTime, ServerLink};
pub(crate) use sample_buffer::{SampleBuffer, SampleTiming};
pub use tick::TickSync;
pub use transport::connect;

#[cfg(test)]
#[path = "tests/timing.rs"]
mod timing_tests;
