mod equipment;
mod plugin;
mod resources;
mod spawn;
#[cfg(test)]
mod spawn_tests;

pub use plugin::portals_plugin;
pub use resources::{PortalAssignments, PortalMap};
pub(crate) use spawn::handle_portal_shot_message;
