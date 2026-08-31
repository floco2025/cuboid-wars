mod actors;
mod combat;
mod cycles;
mod feed;
mod gameplay;
mod maps;
mod network;
mod quests;
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
pub use gameplay::{
    MissilesServerConfig, PlacedItemRespawnSecs, PlacedItemsConfig, PowerUpDurationSecs, PowerUpsConfig, ScoringConfig,
    ServerGameplayConfig,
};
pub use maps::{LightingMode, MapServerConfig, RandomItemsConfig, WeatherMode};
pub use network::configure_server;
pub use quests::{Quest, QuestKind};
pub(crate) use validation::validate_actor_kinds_consistent;
