use common::config::DeathTrigger;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct RespawnConfig {
    pub players: PlayerRespawnMode,
    pub actors: ActorRespawnConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerRespawnMode {
    #[default]
    Individual,
    Group,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct ActorRespawnConfig {
    pub on_player_death: DeathTrigger,
    pub scope: ActorRespawnScope,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorRespawnScope {
    #[default]
    Dead,
    All,
}
