use std::path::PathBuf;

use rand::random;
use serde_json::{Value, json};

use super::*;
use crate::config::{PowerUpMode, fixtures};

#[test]
fn shipped_server_settings_load_and_validate() {
    GameplayCatalog::load_default().expect("shipped server settings invalid");
}

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

    fn shipped_gameplay() -> Value {
        serde_json::from_str(fixtures::GAMEPLAY_JSON).expect("global settings JSON invalid")
    }

    fn write_registry(&self, names: Value, default_map: &str) {
        let mut global = Self::shipped_gameplay();
        global["maps"] = names;
        global["default_map"] = json!(default_map);
        self.write_gameplay(&global);
    }

    fn write_gameplay(&self, global: &Value) {
        fs::write(self.0.join("gameplay.json"), global.to_string()).expect("temporary global settings unwritable");
    }

    fn write_settings(&self, name: &str, text: &str) {
        let directory = self.0.join("maps").join(name);
        fs::create_dir_all(&directory).expect("temporary map directory unavailable");
        fs::write(directory.join("settings.json"), text).expect("temporary map settings unwritable");
    }

    // The test map with one override written over its file.
    fn write_hotel_override(&self, edit: impl FnOnce(&mut Value)) {
        let mut settings: Value = serde_json::from_str(fixtures::MAP_JSON).expect("hotel settings JSON invalid");
        edit(&mut settings);
        self.write_settings("hotel", &settings.to_string());
    }

    fn load(&self) -> Result<GameplayCatalog> {
        GameplayCatalog::load_from_path(&self.0.join("gameplay.json"))
    }

    fn load_error(&self, accepted: &str) -> String {
        format!("{:#}", self.load().expect_err(accepted))
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
    directory.write_hotel_override(|settings| settings["celestial"]["north_yaw_degrees"] = json!(37.0));
    directory.write_settings("unregistered", "invalid JSON");
    let loaded = directory.load().expect("valid split config rejected");
    assert_eq!(loaded.default_map, "hotel");
    assert_eq!(loaded.maps.len(), 1);
    assert_eq!(loaded.maps["hotel"].settings.celestial.north_yaw_degrees, 37.0);
    assert_eq!(loaded.maps["hotel"].map_name, "hotel");
}

#[test]
fn a_map_override_replaces_one_leaf_and_inherits_the_rest() {
    let directory = TestConfigDir::new();
    directory.write_hotel_override(|settings| settings["movement"]["gravity"] = json!(24.0));
    let loaded = directory.load().expect("valid override rejected");
    let movement = &loaded.maps["hotel"].settings.movement;
    let defaults = TestConfigDir::shipped_gameplay();
    assert_eq!(movement.gravity, 24.0);
    assert_eq!(
        movement.low_gravity,
        defaults["movement"]["low_gravity"].as_f64().expect("default") as f32
    );
    assert_eq!(
        movement.player.move_speed,
        defaults["movement"]["player"]["move_speed"].as_f64().expect("default") as f32
    );
}

#[test]
fn a_tag_change_replaces_the_variant_and_a_matching_tag_merges() {
    let directory = TestConfigDir::new();
    directory.write_hotel_override(|settings| {
        settings["power_ups"]["single_shot"] = json!({"mode": "always"});
        settings["power_ups"]["speed"] = json!({"duration_secs": null});
    });
    let loaded = directory.load().expect("variant overrides rejected");
    let power_ups = &loaded.maps["hotel"].power_ups;
    assert_eq!(power_ups.single_shot, PowerUpMode::Always {});
    assert_eq!(power_ups.speed, PowerUpMode::Pickup { duration_secs: None });
}

#[test]
fn an_unknown_override_key_names_the_map_file_and_path() {
    let directory = TestConfigDir::new();
    directory.write_hotel_override(|settings| settings["movement"]["playr"] = json!({"move_speed": 1.0}));
    let error = directory.load_error("typo accepted");
    assert!(error.contains("maps/hotel/settings.json"), "{error}");
    assert!(error.contains("movement.playr is not a key in the defaults"), "{error}");
}

#[test]
fn a_map_cannot_add_an_actor_kind_or_change_a_kinds_body_class() {
    let directory = TestConfigDir::new();
    directory.write_hotel_override(|settings| settings["actors"]["banana"] = json!({"vision_range": 1.0}));
    let error = directory.load_error("new kind accepted");
    assert!(error.contains("actors.banana is not a key in the defaults"), "{error}");
    directory.write_hotel_override(|settings| settings["actors"]["turret"] = json!({"immovable": false}));
    let error = directory.load_error("immovable override accepted");
    assert!(
        error.contains("actors.turret.immovable cannot be overridden per map"),
        "{error}"
    );
    directory.write_hotel_override(|settings| settings["actors"]["turret"] = json!({"vision_range": 12.0}));
    let loaded = directory.load().expect("vision override rejected");
    assert_eq!(loaded.maps["hotel"].expect_actor("turret").vision_range, 12.0);
}

#[test]
fn a_global_key_in_a_map_file_is_rejected() {
    let directory = TestConfigDir::new();
    directory.write_hotel_override(|settings| settings["network"] = json!({"server_hz": 60}));
    let error = directory.load_error("per-map network accepted");
    assert!(error.contains("network is global"), "{error}");
}

#[test]
fn a_bad_or_unknown_default_names_the_gameplay_file() {
    let directory = TestConfigDir::new();
    let mut global = TestConfigDir::shipped_gameplay();
    global["combat"]["damage"]["projectile"] = json!(-1.0);
    directory.write_gameplay(&global);
    let error = directory.load_error("negative default damage accepted");
    assert!(error.contains("gameplay.json: combat.damage.projectile"), "{error}");
    let mut global = TestConfigDir::shipped_gameplay();
    global["scorring"] = json!({});
    directory.write_gameplay(&global);
    let error = directory.load_error("unknown default section accepted");
    assert!(error.contains("gameplay.json"), "{error}");
    assert!(error.contains("unknown field `scorring`"), "{error}");
    let mut global = TestConfigDir::shipped_gameplay();
    global["quests"] = json!([]);
    directory.write_gameplay(&global);
    let error = directory.load_error("content default accepted");
    assert!(error.contains("unknown field `quests`"), "{error}");
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
        let error = directory.load_error("invalid registry accepted");
        assert!(error.contains("gameplay.json"), "{error}");
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn every_registered_settings_file_is_required_and_errors_name_its_source() {
    let directory = TestConfigDir::new();
    directory.write_registry(json!(["hotel", "obby"]), "hotel");
    let obby = directory.0.join("maps/obby/settings.json");
    let obby = obby.to_str().expect("test path is not UTF-8");
    let error = directory.load_error("missing non-default map settings accepted");
    assert!(error.contains(obby), "{error}");
    assert!(error.contains("failed to read"), "{error}");
    directory.write_settings("obby", "{");
    let error = directory.load_error("malformed map settings accepted");
    assert!(error.contains(obby), "{error}");
    assert!(error.contains("failed to parse"), "{error}");
    directory.write_settings("obby", "{}");
    let error = directory.load_error("missing map settings fields accepted");
    assert!(error.contains(obby), "{error}");
    assert!(error.contains("missing field"), "{error}");
}

#[test]
fn invalid_map_values_name_the_settings_file_and_field() {
    let directory = TestConfigDir::new();
    directory.write_hotel_override(|settings| settings["geometry"]["grid_cell_size"] = json!(0));
    let error = directory.load_error("invalid map geometry accepted");
    assert!(error.contains("settings.json: geometry.grid_cell_size"), "{error}");
    directory.write_hotel_override(|settings| settings["combat"]["damage"]["projectile"] = json!(-1.0));
    let error = directory.load_error("negative projectile damage accepted");
    assert!(error.contains("settings.json: combat.damage.projectile"), "{error}");
}

#[test]
fn immovable_actor_rejects_unused_speed_settings() {
    let mut config = fixtures::server_config();
    let speeds = *config.settings.movement.expect_actor("zapper");
    config.settings.movement.actors.insert("turret".into(), speeds);
    let error = config
        .validate("settings.json: ")
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
    let actor = config.actors.get_mut("turret").expect("turret config missing");
    actor.character.immovable = false;

    let error = config
        .validate("settings.json: ")
        .expect_err("movable actor accepted missing speeds");
    assert!(error.to_string().contains("missing actor kind \"turret\""));
}

#[test]
fn map_fall_thresholds_are_validated_with_their_source() {
    let thresholds: [(&str, fn(&mut ServerGameplayConfig) -> &mut FallDamageConfig); 2] = [
        ("player_fall", |config| &mut config.player_fall),
        ("actor_fall", |config| &mut config.actor_fall),
    ];
    for (key, fall) in thresholds {
        for (safe, lethal, field) in [
            (-1.0, 12.0, "safe_distance"),
            (4.0, -1.0, "lethal_distance"),
            (0.0, 0.0, "safe_distance"),
            (12.0, 12.0, "safe_distance"),
            (13.0, 12.0, "safe_distance"),
            (f32::NAN, 12.0, "safe_distance"),
            (4.0, f32::INFINITY, "lethal_distance"),
        ] {
            let mut config = fixtures::server_config();
            *fall(&mut config) = FallDamageConfig {
                safe_distance: safe,
                lethal_distance: lethal,
            };
            let error = config
                .validate("maps/example/settings.json: ")
                .expect_err("invalid fall thresholds accepted");
            assert!(
                error
                    .to_string()
                    .contains(&format!("maps/example/settings.json: {key}.{field}")),
                "{error}"
            );
        }
        let mut config = fixtures::server_config();
        *fall(&mut config) = FallDamageConfig {
            safe_distance: 0.0,
            lethal_distance: 15.0,
        };
        config
            .validate("maps/example/settings.json: ")
            .expect("zero safe distance rejected");
    }
}

#[test]
fn optional_feature_blocks_require_explicit_null_and_collections_require_their_type() {
    let source: Value = serde_json::from_str(fixtures::MAP_JSON).expect("map fixture is invalid");
    let parse = |map: Value| ServerGameplayConfig::from_override("hotel", &fixtures::gameplay_defaults(), &map);
    for key in ["grounds", "random_items", "placed_items"] {
        let mut value = source.clone();
        value[key] = Value::Null;
        parse(value.clone()).expect("disabled feature rejected");
        value.as_object_mut().expect("map fixture is not an object").remove(key);
        assert!(parse(value).is_err(), "missing {key} accepted");
    }
    for key in ["quests", "textures"] {
        let mut value = source.clone();
        value[key] = Value::Null;
        assert!(parse(value).is_err(), "null {key} accepted");
    }
}

#[test]
fn always_active_power_ups_cannot_have_random_pickup_weights() {
    for weight in [0.0, 1.0] {
        let mut config = fixtures::server_config();
        config.power_ups.single_shot = PowerUpMode::Always {};
        config.random_items = Some(RandomItemsConfig {
            weights: [("gold".to_owned(), 1.0), ("single_shot".to_owned(), weight)].into(),
            max_number: 30,
            despawn_secs: 60.0,
        });
        let error = config
            .validate("settings.json: ")
            .expect_err("always-active pickup accepted");
        assert!(
            error.to_string().contains("random_items.weights.single_shot"),
            "{error}"
        );
    }
}
