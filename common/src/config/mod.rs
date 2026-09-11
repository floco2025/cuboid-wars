mod actors;
mod characters;
mod death;
pub mod gameplay;
mod geometry;
mod missiles;
mod movement;
mod network;
mod portals;
mod pressure_switch;
mod projectiles;
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
pub use network::{NetworkConfig, UpdateCadence};
pub use portals::PortalsConfig;
pub use pressure_switch::{PressureSwitchActivation, PressureSwitchConfig, SwitchHold};
pub use projectiles::{MultiShotConfig, MultiShotPatternConfig, ProjectilesConfig};
pub use validation::{validate_non_negative_finite, validate_positive_finite};
