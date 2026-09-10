mod actors;
mod characters;
mod death;
pub mod gameplay;
mod geometry;
mod missiles;
mod movement;
pub mod network;
mod portals;
mod pressure_switch;
mod projectiles;
mod replication;
mod validation;

pub use actors::ActorGameplayConfig;
pub use characters::{CharacterGameplayConfig, CharacterPhysicsConfig, HitboxConfig, MovementColliderConfig};
pub use death::DeathTrigger;
pub use gameplay::{
    ActorGameplayBootstrap, GameplayBootstrap, GameplayConfig, MissilesGameplayBootstrap, PlayerGameplayBootstrap,
};
pub use geometry::MapGeometryConfig;
pub use missiles::MissilesConfig;
pub use movement::{ActorMovementConfig, KnockbackConfig, MapMovementConfig, PlayerMovementConfig};
pub use network::{create_quinn_client_config, create_quinn_server_config, load_certs, load_private_key};
pub use portals::PortalsConfig;
pub use pressure_switch::{PressureSwitchActivation, PressureSwitchConfig};
pub use projectiles::{MultiShotConfig, MultiShotPatternConfig, ProjectilesConfig};
pub use validation::{validate_non_negative_finite, validate_positive_finite};

pub use replication::NetworkConfig;
