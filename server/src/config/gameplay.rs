use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
use bevy::prelude::Resource;
use serde::Deserialize;

use super::{
    actors::{ActorKindServerConfig, ActorsConfig},
    combat::CombatConfig,
    cycles::CyclesConfig,
    feed::FeedConfig,
    maps::{MapServerConfig, validate_map_registry, validate_maps},
    scoring::ScoringConfig,
    validation::validate_positive_finite,
    weapons::WeaponsConfig,
};
use common::config::{
    ActorGameplayBootstrap, CharacterGameplayConfig, GameplayBootstrap, GameplayConfig, MissilesGameplayBootstrap,
    NetworkConfig, PlayerGameplayBootstrap,
};

#[derive(Resource, Debug, Clone)]
pub struct ServerGameplayConfig {
    pub network: NetworkConfig,
    pub default_map: String,
    pub maps: HashMap<String, MapServerConfig>,
    pub player: PlayerServerConfig,
    pub actors: ActorsConfig,
    pub weapons: WeaponsConfig,
    pub combat: CombatConfig,
    pub scoring: ScoringConfig,
    pub cycles: CyclesConfig,
    pub feed: FeedConfig,
}

#[derive(Deserialize)]
struct GameplayFile {
    #[serde(default)]
    network: NetworkConfig,
    default_map: String,
    maps: Vec<String>,
    player: PlayerServerConfig,
    actors: ActorsConfig,
    weapons: WeaponsConfig,
    combat: CombatConfig,
    scoring: ScoringConfig,
    cycles: CyclesConfig,
    feed: FeedConfig,
}

impl ServerGameplayConfig {
    pub fn load_default() -> Result<Self> {
        Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/server/gameplay.json"
        )))
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let source: GameplayFile =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        validate_map_registry(source.maps.iter().map(String::as_str), &source.default_map)
            .with_context(|| format!("invalid map registry in {}", path.display()))?;
        let directory = path.parent().context("gameplay configuration directory missing")?;
        let mut maps = HashMap::new();
        for name in source.maps {
            let settings_path = directory.join("maps").join(&name).join("settings.json");
            let text = fs::read_to_string(&settings_path)
                .with_context(|| format!("failed to read {}", settings_path.display()))?;
            let settings: MapServerConfig =
                serde_json::from_str(&text).with_context(|| format!("failed to parse {}", settings_path.display()))?;
            maps.insert(name, settings);
        }
        let config = Self {
            network: source.network,
            default_map: source.default_map,
            maps,
            player: source.player,
            actors: source.actors,
            weapons: source.weapons,
            combat: source.combat,
            scoring: source.scoring,
            cycles: source.cycles,
            feed: source.feed,
        };
        config
            .validate(directory)
            .with_context(|| format!("invalid configuration loaded from {}", path.display()))?;
        Ok(config)
    }

    fn validate(&self, directory: &Path) -> Result<()> {
        self.network.validate()?;
        self.player.validate("player")?;
        self.actors.validate("actors")?;
        self.weapons.validate("weapons")?;
        self.combat.validate(&self.actors.kinds)?;
        self.scoring.validate(&self.actors.kinds)?;
        self.cycles.validate("cycles")?;
        self.feed.validate(&self.actors.kinds)?;
        validate_maps(
            &self.maps,
            &self.default_map,
            &self.actors.kinds,
            &directory.join("maps"),
        )
    }

    #[must_use]
    pub fn actor(&self, kind: &str) -> Option<&ActorKindServerConfig> {
        self.actors.kinds.get(kind)
    }

    #[must_use]
    pub fn expect_actor(&self, kind: &str) -> &ActorKindServerConfig {
        self.actor(kind)
            .expect("actor kind missing from server gameplay config")
    }

    #[must_use]
    pub fn gameplay_config(&self) -> GameplayConfig {
        GameplayConfig {
            player: self.player.gameplay.clone(),
            projectiles: self.weapons.projectiles.clone(),
            missiles: self.weapons.missiles.gameplay,
            portals: self.weapons.portals,
            actors: self
                .actors
                .kinds
                .iter()
                .map(|(kind, actor)| (kind.clone(), actor.character.clone()))
                .collect(),
        }
    }

    #[must_use]
    pub fn gameplay_bootstrap(&self) -> GameplayBootstrap {
        let combat = &self.combat;
        let mut actors: Vec<_> = self
            .actors
            .kinds
            .iter()
            .map(|(kind, actor)| {
                let health = combat
                    .health
                    .actors
                    .get(kind)
                    .expect("actor health missing after server config validation");
                let damage = combat
                    .damage
                    .actors
                    .get(kind)
                    .expect("actor damage missing after server config validation");
                (
                    kind.clone(),
                    ActorGameplayBootstrap {
                        gameplay: actor.character.clone(),
                        max_health: health.max,
                        death_blast_radius: damage.death_blast.radius,
                    },
                )
            })
            .collect();
        actors.sort_by(|a, b| a.0.cmp(&b.0));

        GameplayBootstrap {
            player: PlayerGameplayBootstrap {
                gameplay: self.player.gameplay.clone(),
                max_health: combat.health.player.max,
                death_blast_radius: combat.damage.player_blast.radius,
            },
            actors,
            projectiles: self.weapons.projectiles.clone(),
            missiles: MissilesGameplayBootstrap {
                gameplay: self.weapons.missiles.gameplay,
                blast_radius: combat.damage.missile_blast.radius,
            },
            portals: self.weapons.portals,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerServerConfig {
    #[serde(flatten)]
    pub gameplay: CharacterGameplayConfig,
    pub respawn_secs: f32,
}

impl PlayerServerConfig {
    fn validate(&self, path: &str) -> Result<()> {
        self.gameplay.validate(path)?;
        validate_positive_finite(self.respawn_secs, &format!("{path}.respawn_secs"))
    }
}

#[cfg(test)]
#[path = "tests/gameplay.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/fixtures.rs"]
pub(crate) mod fixtures;
