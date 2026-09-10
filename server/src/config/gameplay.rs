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
            let settings =
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
mod tests {
    use super::*;
    use rand::random;
    use serde_json::{Value, json};
    use std::path::PathBuf;

    struct TestConfigDir(PathBuf);

    impl TestConfigDir {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("cuboid_map_settings_{}", random::<u64>()));
            fs::create_dir(&root).expect("temporary config directory unavailable");
            let config = Self(root);
            config.write_settings("hotel", include_str!("../../../config/server/maps/hotel/settings.json"));
            config.write_registry(json!(["hotel"]), "hotel");
            config
        }

        fn write_registry(&self, names: Value, default_map: &str) {
            let mut global: Value = serde_json::from_str(include_str!("../../../config/server/gameplay.json"))
                .expect("global settings JSON invalid");
            global["maps"] = names;
            global["default_map"] = json!(default_map);
            fs::write(self.0.join("gameplay.json"), global.to_string()).expect("temporary global settings unwritable");
        }

        fn write_settings(&self, name: &str, text: &str) {
            let directory = self.0.join("maps").join(name);
            fs::create_dir_all(&directory).expect("temporary map directory unavailable");
            fs::write(directory.join("settings.json"), text).expect("temporary map settings unwritable");
        }

        fn load(&self) -> Result<ServerGameplayConfig> {
            ServerGameplayConfig::load_from_path(&self.0.join("gameplay.json"))
        }
    }

    impl Drop for TestConfigDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("temporary config directory cleanup failed");
        }
    }

    #[test]
    fn settings_resolve_beside_global_config_without_loading_layouts_or_unregistered_folders() {
        let directory = TestConfigDir::new();
        let mut settings: Value = serde_json::from_str(include_str!("../../../config/server/maps/hotel/settings.json"))
            .expect("hotel settings JSON invalid");
        settings["skybox"] = json!("custom-sky");
        directory.write_settings("hotel", &settings.to_string());
        directory.write_settings("unregistered", "invalid JSON");
        let loaded = directory.load().expect("valid split config rejected");
        assert_eq!(loaded.default_map, "hotel");
        assert_eq!(loaded.maps.len(), 1);
        assert_eq!(loaded.maps["hotel"].settings.skybox, "custom-sky");
        assert!(!directory.0.join("maps/hotel/layout.json").exists());
    }

    #[test]
    fn registry_errors_are_rejected_before_map_files_are_read() {
        let directory = TestConfigDir::new();
        for (names, default_map, expected) in [
            (json!([]), "hotel", "at least one"),
            (json!([""]), "", "must not be empty"),
            (json!(["missing", "missing"]), "missing", "duplicate"),
            (json!(["../hotel"]), "../hotel", "ASCII"),
            (json!(["hotel"]), "missing", "default_map"),
            (json!({"hotel": {}}), "hotel", "expected a sequence"),
        ] {
            directory.write_registry(names, default_map);
            let error = format!("{:#}", directory.load().expect_err("invalid registry accepted"));
            assert!(error.contains("gameplay.json"), "{error}");
            assert!(error.contains(expected), "{error}");
        }
    }

    #[test]
    fn maps_load_independent_fall_thresholds() {
        let directory = TestConfigDir::new();
        let mut settings: Value = serde_json::from_str(include_str!("../../../config/server/maps/hotel/settings.json"))
            .expect("map settings JSON invalid");
        settings["player_fall"] = json!({"safe_distance": 2.0, "lethal_distance": 6.0});
        directory.write_settings("first", &settings.to_string());
        settings["player_fall"] = json!({"safe_distance": 12.0, "lethal_distance": 30.0});
        directory.write_settings("second", &settings.to_string());
        directory.write_registry(json!(["first", "second"]), "second");
        let loaded = directory.load().expect("valid fall thresholds rejected");
        assert_eq!(loaded.maps["first"].player_fall.safe_distance, 2.0);
        assert_eq!(loaded.maps["first"].player_fall.lethal_distance, 6.0);
        assert_eq!(loaded.maps["second"].player_fall.safe_distance, 12.0);
        assert_eq!(loaded.maps["second"].player_fall.lethal_distance, 30.0);
    }

    #[test]
    fn every_registered_settings_file_is_required_and_errors_name_its_source() {
        let directory = TestConfigDir::new();
        directory.write_registry(json!(["hotel", "obby"]), "hotel");
        let error = format!(
            "{:#}",
            directory.load().expect_err("missing non-default map settings accepted")
        );
        assert!(
            error.contains(
                directory
                    .0
                    .join("maps/obby/settings.json")
                    .to_str()
                    .expect("test path is not UTF-8")
            ),
            "{error}"
        );
        assert!(error.contains("failed to read"), "{error}");
        directory.write_settings("obby", "{");
        let error = format!("{:#}", directory.load().expect_err("malformed map settings accepted"));
        assert!(
            error.contains(
                directory
                    .0
                    .join("maps/obby/settings.json")
                    .to_str()
                    .expect("test path is not UTF-8")
            ),
            "{error}"
        );
        assert!(error.contains("failed to parse"), "{error}");
        directory.write_settings("obby", "{}");
        let error = format!(
            "{:#}",
            directory.load().expect_err("missing map settings fields accepted")
        );
        assert!(
            error.contains(
                directory
                    .0
                    .join("maps/obby/settings.json")
                    .to_str()
                    .expect("test path is not UTF-8")
            ),
            "{error}"
        );
        assert!(error.contains("missing field"), "{error}");
    }

    #[test]
    fn invalid_map_values_name_the_settings_file_and_field() {
        let directory = TestConfigDir::new();
        let mut settings: Value = serde_json::from_str(include_str!("../../../config/server/maps/hotel/settings.json"))
            .expect("hotel settings JSON invalid");
        settings["geometry"]["grid_cell_size"] = json!(0);
        directory.write_settings("hotel", &settings.to_string());
        let error = format!("{:#}", directory.load().expect_err("invalid map geometry accepted"));
        assert!(error.contains("settings.json: geometry.grid_cell_size"), "{error}");
    }

    #[test]
    fn mobile_actor_requires_positive_roam_steps() {
        let mut config = ServerGameplayConfig::load_default().expect("gameplay config rejected");
        config
            .actors
            .kinds
            .get_mut("scuttler")
            .expect("scuttler config missing")
            .roam_steps = 0;
        let error = config
            .validate(Path::new("."))
            .expect_err("mobile actor accepted zero roam steps");
        assert!(error.to_string().contains("actors.kinds.scuttler.roam_steps"));
    }
    #[test]
    fn immovable_actor_rejects_unused_speed_settings() {
        let mut config = ServerGameplayConfig::load_default().expect("gameplay config rejected");
        let map = config.maps.get_mut("obby").expect("Obby settings missing");
        let speeds = *map.settings.movement.expect_actor("zapper");
        map.settings.movement.actors.insert("turret".into(), speeds);
        let error = config
            .validate(Path::new("."))
            .expect_err("immovable actor accepted speed settings");
        assert!(
            error
                .to_string()
                .contains("settings.json: movement.actors.turret must be omitted")
        );
    }

    #[test]
    fn movable_actor_requires_speed_settings() {
        let mut config = ServerGameplayConfig::load_default().expect("gameplay config rejected");
        let actor = config.actors.kinds.get_mut("turret").expect("turret config missing");
        actor.character.immovable = false;
        actor.roam_steps = 1;
        let error = config
            .validate(Path::new("."))
            .expect_err("movable actor accepted missing speeds");
        assert!(error.to_string().contains("missing actor kind \"turret\""));
    }
}
