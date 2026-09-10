mod projection;
mod render;
mod resources;
mod spawn;
mod transform_sync;
mod transit;
mod view;

pub use render::portal_render_plugin;
pub use resources::{PortalInfo, PortalMap};
pub use spawn::{PortalAssets, spawn_portal};
pub(crate) use transform_sync::portal_surfaces_transform_sync_system;
pub use transit::portal_transit_system;
pub use view::apply_portal_view;
