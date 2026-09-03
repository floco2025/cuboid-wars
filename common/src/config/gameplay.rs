use std::collections::HashMap;

use anyhow::{Result, bail};
use bevy_ecs::prelude::Resource;
use bincode::{Decode, Encode};
use serde::Deserialize;

use super::{
    CharacterGameplayConfig, MissilesConfig, PlayerGameplayConfig, PortalsConfig, ProjectilesConfig,
    validation::{validate_non_negative_finite, validate_positive_finite},
};

#[derive(Resource, Debug, Clone, Deserialize)]
pub struct GameplayConfig {
    pub player: PlayerGameplayConfig,
    pub projectiles: ProjectilesConfig,
    pub missiles: MissilesConfig,
    pub portals: PortalsConfig,
    pub actors: HashMap<String, CharacterGameplayConfig>,
}

impl GameplayConfig {
    pub fn validate(&self) -> Result<()> {
        self.player.validate("player")?;
        self.projectiles.validate("projectiles")?;
        self.missiles.validate("missiles")?;
        self.portals.validate("portals")?;
        if self.actors.is_empty() {
            bail!("actors must define at least one kind");
        }
        for (kind, actor) in &self.actors {
            if kind.is_empty() {
                bail!("actor kind must not be empty");
            }
            actor.validate(&format!("actors.{kind}"))?;
        }
        Ok(())
    }

    #[must_use]
    pub fn actor(&self, kind: &str) -> Option<&CharacterGameplayConfig> {
        self.actors.get(kind)
    }

    #[must_use]
    pub fn expect_actor(&self, kind: &str) -> &CharacterGameplayConfig {
        self.actor(kind).expect("actor kind missing from gameplay config")
    }
}

// Test-only view of `config/server/gameplay.json`: the source structs mirror
// the server's `ServerGameplayConfig` layout, so the two change together.
#[cfg(test)]
pub(crate) fn load_test_gameplay() -> Result<GameplayConfig> {
    use anyhow::Context;

    #[derive(Deserialize)]
    struct TestGameplaySource {
        player: PlayerGameplayConfig,
        actors: TestActorsSource,
        weapons: TestWeaponsSource,
    }

    #[derive(Deserialize)]
    struct TestActorsSource {
        kinds: HashMap<String, CharacterGameplayConfig>,
    }

    #[derive(Deserialize)]
    struct TestWeaponsSource {
        projectiles: ProjectilesConfig,
        missiles: MissilesConfig,
        portals: PortalsConfig,
    }

    let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../config/server/gameplay.json"));
    let text = std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let source: TestGameplaySource =
        serde_json::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
    let config = GameplayConfig {
        player: source.player,
        projectiles: source.weapons.projectiles,
        missiles: source.weapons.missiles,
        portals: source.weapons.portals,
        actors: source.actors.kinds,
    };
    config.validate()?;
    Ok(config)
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct GameplayBootstrap {
    pub player: PlayerGameplayBootstrap,
    pub actors: Vec<(String, ActorGameplayBootstrap)>,
    pub actor_spawn_warning_secs: f32,
    pub projectiles: ProjectilesConfig,
    pub missiles: MissilesGameplayBootstrap,
    pub portals: PortalsConfig,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct PlayerGameplayBootstrap {
    pub gameplay: PlayerGameplayConfig,
    pub max_health: f32,
    pub death_blast_radius: f32,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct ActorGameplayBootstrap {
    pub gameplay: CharacterGameplayConfig,
    pub max_health: f32,
    pub death_blast_radius: f32,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct MissilesGameplayBootstrap {
    pub gameplay: MissilesConfig,
    pub blast_radius: f32,
}

impl GameplayBootstrap {
    pub fn gameplay_config(&self) -> Result<GameplayConfig> {
        validate_positive_finite(self.player.max_health, "gameplay.player.max_health")?;
        validate_positive_finite(self.player.death_blast_radius, "gameplay.player.death_blast_radius")?;
        validate_positive_finite(self.missiles.blast_radius, "gameplay.missiles.blast_radius")?;
        validate_non_negative_finite(self.actor_spawn_warning_secs, "gameplay.actor_spawn_warning_secs")?;

        let mut actors = HashMap::with_capacity(self.actors.len());
        for (kind, actor) in &self.actors {
            validate_positive_finite(actor.max_health, &format!("gameplay.actors.{kind}.max_health"))?;
            validate_positive_finite(
                actor.death_blast_radius,
                &format!("gameplay.actors.{kind}.death_blast_radius"),
            )?;
            if actors.insert(kind.clone(), actor.gameplay.clone()).is_some() {
                bail!("gameplay.actors contains duplicate actor kind {kind:?}");
            }
        }

        let config = GameplayConfig {
            player: self.player.gameplay.clone(),
            projectiles: self.projectiles.clone(),
            missiles: self.missiles.gameplay,
            portals: self.portals,
            actors,
        };
        config.validate()?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gameplay_bootstrap() -> GameplayBootstrap {
        let gameplay = load_test_gameplay().expect("server gameplay projection should load");
        let mut actors: Vec<_> = gameplay
            .actors
            .iter()
            .map(|(kind, actor)| {
                (
                    kind.clone(),
                    ActorGameplayBootstrap {
                        gameplay: actor.clone(),
                        max_health: 100.0,
                        death_blast_radius: 5.0,
                    },
                )
            })
            .collect();
        actors.sort_by(|a, b| a.0.cmp(&b.0));
        GameplayBootstrap {
            player: PlayerGameplayBootstrap {
                gameplay: gameplay.player,
                max_health: 100.0,
                death_blast_radius: 5.0,
            },
            actors,
            actor_spawn_warning_secs: 2.0,
            projectiles: gameplay.projectiles,
            missiles: MissilesGameplayBootstrap {
                gameplay: gameplay.missiles,
                blast_radius: 5.0,
            },
            portals: gameplay.portals,
        }
    }

    #[test]
    fn gameplay_bootstrap_builds_hash_indexed_runtime_config() {
        let bootstrap = gameplay_bootstrap();
        let gameplay = bootstrap.gameplay_config().expect("bootstrap should validate");
        assert_eq!(gameplay.actors.len(), bootstrap.actors.len());
        assert!(gameplay.actor("zapper").is_some());
    }

    #[test]
    fn gameplay_bootstrap_rejects_duplicate_actor_kinds() {
        let mut bootstrap = gameplay_bootstrap();
        bootstrap.actors.push(bootstrap.actors[0].clone());
        let error = bootstrap.gameplay_config().expect_err("duplicate actor kind accepted");
        assert!(error.to_string().contains("duplicate actor kind"));
    }
}
