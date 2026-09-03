mod actors;
mod combat;
mod cycles;
mod feed;
mod gameplay;
mod items;
mod maps;
mod missiles;
mod network;
mod quests;
mod scoring;
mod validation;

pub use actors::{
    ActorAttackConfig, ActorBeamAttackConfig, ActorKindServerConfig, ActorSettingsConfig, ContactAttackConfig,
    ContactBeamAttackConfig,
};
pub use combat::{
    ActorDamageConfig, ActorHealthConfig, BlastConfig, CombatConfig, DamageConfig, FallDamageConfig, HealthConfig,
    PlayerHealthConfig,
};
pub use cycles::{LightingCycleConfig, WeatherCycleConfig};
pub use feed::FeedConfig;
pub use gameplay::{PlayerServerConfig, ServerGameplayConfig};
pub use items::{PlacedItemRespawnSecs, PlacedItemsConfig, PowerUpDurationSecs, PowerUpsConfig};
pub use maps::{LightingMode, MapServerConfig, RandomItemsConfig, WeatherMode};
pub use missiles::MissilesServerConfig;
pub use network::configure_server;
pub use quests::{Quest, QuestKind};
pub use scoring::ScoringConfig;
pub(crate) use validation::{validate_map_actor_kinds, validate_map_quests};
