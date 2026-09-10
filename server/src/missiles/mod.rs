mod expiry;
mod plugin;
mod reports;
mod resources;
mod spawn;

#[cfg(test)]
#[path = "tests/expiry.rs"]
mod expiry_tests;
#[cfg(test)]
#[path = "tests/reports.rs"]
mod reports_tests;

pub use plugin::missiles_plugin;
pub(crate) use reports::{handle_missile_detonated, handle_missile_moves};
pub use resources::MissileMap;
pub(crate) use spawn::handle_missile_shot_message;
