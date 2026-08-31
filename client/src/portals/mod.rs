mod prediction;
mod resources;
mod spawn;
mod view;

pub use prediction::local_player_portal_prediction_system;
pub use resources::{PortalInfo, PortalMap};
pub use spawn::{PortalAssets, PortalMarker, spawn_portal};
pub use view::apply_portal_view;
