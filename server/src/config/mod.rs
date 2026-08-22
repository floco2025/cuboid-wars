mod actors;
mod gameplay;
mod maps;
mod network;
mod quests;
mod validation;

pub use actors::{
    ActorAttackConfig, ActorBeamAttackConfig, ActorCombatConfig, ActorKindServerConfig, ActorSettingsConfig,
    ExplosionDamageConfig,
};
pub use gameplay::{
    FallDamageConfig, MissilesServerConfig, PlacedItemRespawnSecs, PlacedItemsConfig, PlayerServerConfig,
    PowerUpsConfig, ProjectileConfig, ScoringConfig, ServerGameplayConfig,
};
pub use maps::{MapServerConfig, RainScheduleConfig, RandomItemsConfig};
pub use network::configure_server;
pub use quests::{Quest, QuestKind};
pub(crate) use validation::validate_actor_kinds_consistent;
