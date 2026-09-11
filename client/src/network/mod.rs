mod actors;
mod bootstrap;
mod context;
mod incoming;
mod items;
mod link;
mod missiles;
mod ping;
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

pub(crate) use bootstrap::install_bootstrap;
pub use bootstrap::login;
pub use link::{ClientToServerChannel, Impairment, ServerLink, connect};
pub use plugin::network_plugin;
pub(crate) use resources::accept_newer_tick;
pub use resources::{LastPlayerMovesTick, LastSnapshotTick, RoundTripTime};
pub(crate) use sample_buffer::{SampleBuffer, SampleTiming};
pub use tick::TickSync;

#[cfg(test)]
#[path = "tests/timing.rs"]
mod timing_tests;
