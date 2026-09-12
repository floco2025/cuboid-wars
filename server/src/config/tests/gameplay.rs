use super::*;
use crate::config::fixtures;
use rand::random;
use serde_json::{Value, json};
use std::path::PathBuf;

struct TestConfigDir(PathBuf);

impl TestConfigDir {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("cuboid_map_settings_{}", random::<u64>()));
        fs::create_dir(&root).expect("temporary config directory unavailable");
        let config = Self(root);
        config.write_settings("hotel", fixtures::MAP_JSON);
        config.write_registry(json!(["hotel"]), "hotel");
        config
    }

    fn write_registry(&self, names: Value, default_map: &str) {
        let mut global: Value = serde_json::from_str(fixtures::GAMEPLAY_JSON).expect("global settings JSON invalid");
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
fn settings_and_controls_resolve_beside_global_config_without_loading_unregistered_folders() {
    let directory = TestConfigDir::new();
    let mut settings: Value = serde_json::from_str(fixtures::MAP_JSON).expect("hotel settings JSON invalid");
    settings["skybox"] = json!("custom-sky");
    directory.write_settings("hotel", &settings.to_string());
    directory.write_settings("unregistered", "invalid JSON");
    let loaded = directory.load().expect("valid split config rejected");
    assert_eq!(loaded.default_map, "hotel");
    assert_eq!(loaded.maps.len(), 1);
    assert_eq!(loaded.maps["hotel"].settings.skybox, "custom-sky");
}

#[test]
fn a_registered_map_loads_without_a_layout() {
    let directory = TestConfigDir::new();
    directory.write_settings("fresh", fixtures::MAP_JSON);
    directory.write_registry(json!(["hotel", "fresh"]), "hotel");
    assert!(!directory.0.join("maps").join("fresh").join("layout.json").exists());
    let loaded = directory.load().expect("a map without a layout blocked the registry");
    assert!(loaded.maps.contains_key("fresh"));
    assert!(loaded.maps["fresh"].settings.switches.is_empty());
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
    let mut settings: Value = serde_json::from_str(fixtures::MAP_JSON).expect("map settings JSON invalid");
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
    let mut settings: Value = serde_json::from_str(fixtures::MAP_JSON).expect("hotel settings JSON invalid");
    settings["geometry"]["grid_cell_size"] = json!(0);
    directory.write_settings("hotel", &settings.to_string());
    let error = format!("{:#}", directory.load().expect_err("invalid map geometry accepted"));
    assert!(error.contains("settings.json: geometry.grid_cell_size"), "{error}");
}

#[test]
fn immovable_actor_rejects_unused_speed_settings() {
    let mut config = fixtures::server_config();
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
    let mut config = fixtures::server_config();
    let actor = config.actors.kinds.get_mut("turret").expect("turret config missing");
    actor.character.immovable = false;

    let error = config
        .validate(Path::new("."))
        .expect_err("movable actor accepted missing speeds");
    assert!(error.to_string().contains("missing actor kind \"turret\""));
}
