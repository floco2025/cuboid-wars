mod actors;
mod combat;
mod cycles;
mod falling;
mod feed;
mod gameplay;
mod items;
mod maps;
mod missiles;
mod quests;
mod respawn;
mod scoring;
mod validation;
mod weapons;

#[cfg(test)]
#[path = "tests/maps.rs"]
mod maps_tests;

pub use actors::{
    ActorAttackConfig, ActorBeamAttackConfig, ActorKindServerConfig, ActorSettingsConfig, ActorsConfig,
    ContactAttackConfig, ContactBeamAttackConfig,
};
pub use combat::{
    ActorDamageConfig, ActorHealthConfig, BlastConfig, CombatConfig, DamageConfig, HealthConfig, PlayerHealthConfig,
};
pub use cycles::{CyclesConfig, LightingCycleConfig, WeatherCycleConfig};
pub use falling::FallDamageConfig;
pub use feed::FeedConfig;
pub use gameplay::{PlayerServerConfig, ServerGameplayConfig};
pub use items::{PlacedItemRespawnSecs, PlacedItemsConfig, PowerUpDurationSecs, PowerUpsConfig};
pub(crate) use maps::is_valid_map_name;
pub use maps::{LightingMode, MapServerConfig, RandomItemsConfig, WeatherMode};
pub use missiles::MissilesServerConfig;
pub use quests::{Quest, QuestKind};
pub use respawn::{ActorRespawnConfig, ActorRespawnScope, PlayerRespawnMode, RespawnConfig};
pub use scoring::ScoringConfig;
pub(crate) use validation::{deserialize_required_option, validate_map_actor_kinds, validate_map_quests};
pub use weapons::WeaponsConfig;

#[cfg(test)]
pub(crate) use gameplay::fixtures;
