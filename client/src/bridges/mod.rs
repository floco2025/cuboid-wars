mod assets;
mod fade;
mod spawn;

pub use assets::{BridgeAssets, build_bridge_assets};
pub use fade::bridges_fade_system;
pub use spawn::{BridgeKindMarker, LightBridgeMarker, bridges_spawn_system};
