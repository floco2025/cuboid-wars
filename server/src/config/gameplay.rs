use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use bevy::prelude::Resource;
use serde::Deserialize;

use super::{
    actors::{ActorKindServerConfig, ActorSettingsConfig},
    combat::CombatConfig,
    cycles::{LightingCycleConfig, WeatherCycleConfig},
    feed::FeedConfig,
    items::{PlacedItemsConfig, PowerUpsConfig},
    maps::{MapServerConfig, validate_maps},
    missiles::MissilesServerConfig,
    scoring::ScoringConfig,
    validation::validate_positive_finite,
};
use common::config::{
    ActorGameplayBootstrap, GameplayBootstrap, GameplayConfig, MissilesGameplayBootstrap, PlayerGameplayBootstrap,
    PlayerGameplayConfig, PortalsConfig, ProjectilesConfig,
};

#[derive(Resource, Debug, Clone)]
pub struct ServerGameplayConfig {
    pub player: PlayerServerConfig,
    pub projectiles: ProjectilesConfig,
    pub portals: PortalsConfig,
    pub maps: HashMap<String, MapServerConfig>,
    pub default_map: String,
    pub weather_cycle: WeatherCycleConfig,
    pub lighting_cycle: LightingCycleConfig,
    pub scoring: ScoringConfig,
    pub feed: FeedConfig,
    pub combat: CombatConfig,
    pub missiles: MissilesServerConfig,
    pub power_ups: PowerUpsConfig,
    pub placed_items: PlacedItemsConfig,
    pub actor_settings: ActorSettingsConfig,
    pub actors: HashMap<String, ActorKindServerConfig>,
}

#[derive(Deserialize)]
struct ServerGameplaySource {
    default_map: String,
    maps: HashMap<String, MapServerConfig>,
    player: PlayerServerConfig,
    actors: ActorsSource,
    weapons: WeaponsSource,
    items: ItemsSource,
    combat: CombatConfig,
    scoring: ScoringConfig,
    cycles: CyclesSource,
    feed: FeedConfig,
}

#[derive(Deserialize)]
struct ActorsSource {
    settings: ActorSettingsConfig,
    kinds: HashMap<String, ActorKindServerConfig>,
}

#[derive(Deserialize)]
struct WeaponsSource {
    projectiles: ProjectilesConfig,
    missiles: MissilesServerConfig,
    portals: PortalsConfig,
}

#[derive(Deserialize)]
struct ItemsSource {
    power_ups: PowerUpsConfig,
    placed: PlacedItemsConfig,
}

#[derive(Deserialize)]
struct CyclesSource {
    weather: WeatherCycleConfig,
    lighting: LightingCycleConfig,
}

impl From<ServerGameplaySource> for ServerGameplayConfig {
    fn from(source: ServerGameplaySource) -> Self {
        Self {
            player: source.player,
            projectiles: source.weapons.projectiles,
            portals: source.weapons.portals,
            maps: source.maps,
            default_map: source.default_map,
            weather_cycle: source.cycles.weather,
            lighting_cycle: source.cycles.lighting,
            scoring: source.scoring,
            feed: source.feed,
            combat: source.combat,
            missiles: source.weapons.missiles,
            power_ups: source.items.power_ups,
            placed_items: source.items.placed,
            actor_settings: source.actors.settings,
            actors: source.actors.kinds,
        }
    }
}

impl ServerGameplayConfig {
    pub fn load_default() -> Result<Self> {
        let config = Self::load_from_path(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../config/server/gameplay.json"
        )))?;
        config.validate()?;
        Ok(config)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
        let source: ServerGameplaySource =
            serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(source.into())
    }

    fn validate(&self) -> Result<()> {
        self.gameplay_config().validate()?;
        validate_positive_finite(self.player.respawn_secs, "player.respawn_secs")?;
        validate_maps(&self.maps, &self.default_map, &self.actors)?;
        self.weather_cycle.validate("cycles.weather")?;
        self.lighting_cycle.validate("cycles.lighting")?;
        self.missiles.validate("weapons.missiles")?;
        self.scoring.validate(&self.actors)?;
        self.feed.validate(&self.actors)?;
        self.combat.validate(&self.actors)?;
        self.gameplay_bootstrap().gameplay_config()?;
        self.power_ups.validate("items.power_ups")?;
        self.placed_items.validate("items.placed")?;
        self.actor_settings.validate("actors.settings")?;
        if self.actors.is_empty() {
            bail!("actors must define at least one kind");
        }
        for (kind, actor) in &self.actors {
            if kind.is_empty() {
                bail!("actor kind must not be empty");
            }
            actor.validate(&format!("actors.kinds.{kind}"))?;
        }
        Ok(())
    }

    #[must_use]
    pub fn actor(&self, kind: &str) -> Option<&ActorKindServerConfig> {
        self.actors.get(kind)
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
            projectiles: self.projectiles.clone(),
            missiles: self.missiles.gameplay,
            portals: self.portals,
            actors: self
                .actors
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
            actor_spawn_warning_secs: self.actor_settings.spawn_warning_secs,
            projectiles: self.projectiles.clone(),
            missiles: MissilesGameplayBootstrap {
                gameplay: self.missiles.gameplay,
                blast_radius: combat.damage.missile_blast.radius,
            },
            portals: self.portals,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerServerConfig {
    #[serde(flatten)]
    pub gameplay: PlayerGameplayConfig,
    pub respawn_secs: f32,
}
