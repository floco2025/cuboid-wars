mod admin;
mod broadcast;
mod feed;
mod handlers;
mod incoming;
mod links;
mod login;
mod plugin;
mod routing;
mod snapshot;
#[cfg(test)]
#[path = "tests/snapshot.rs"]
mod snapshot_tests;

#[cfg(test)]
pub(crate) use broadcast::collect_player_moves;
pub use broadcast::{broadcast_firework_show, broadcast_player_relocation, broadcast_to_all, broadcast_to_others};
pub use feed::{DeathCause, FeedAudience, FeedEvent, emit_feed};
pub(crate) use handlers::SharedWorld;
pub use incoming::register_local;
pub use links::{ClientLinks, LinkSource, Listener, LocalLink, listen};
pub use plugin::network_plugin;
