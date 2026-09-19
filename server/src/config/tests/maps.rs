use crate::config::fixtures;

use anyhow::Result;
use serde_json::json;

use super::{
    falling::FallDamageConfig,
    gameplay::ServerGameplayConfig,
    items::{PlacedItemsConfig, PowerUpMode, PowerUpsConfig},
    maps::*,
    respawn::RespawnConfig,
};
use crate::test_geometry::sizes;
use common::{
    celestial::{CelestialMapSettings, LocalTime, Season},
    config::{ActorMovementConfig, KnockbackConfig, MapMovementConfig, PlayerMovementConfig},
    protocol::{MapSettings, PortalMode},
};

fn ok_movement() -> MapMovementConfig {
    MapMovementConfig {
        player: PlayerMovementConfig {
            walk_speed: 6.0,
            run_speed: 9.0,
            speed_power_up: 1.6,
            jump_speed: 12.0,
        },
        actors: [("scuttler", 3.0, 5.0), ("bruiser", 5.0, 8.0), ("zapper", 2.0, 4.0)]
            .into_iter()
            .map(|(kind, roam_speed, active_speed)| {
                (
                    kind.to_owned(),
                    ActorMovementConfig {
                        roam_speed,
                        active_speed,
                    },
                )
            })
            .collect(),
        missile_speed: 16.0,
        projectile_speed: 90.0,
        gravity: 25.0,
        low_gravity: 5.0,
        ladder_climb_ratio: 0.4,
        knockback: KnockbackConfig {
            max_speed: 15.0,
            up_speed: 7.0,
            deceleration: 35.0,
        },
    }
}

fn ok_config() -> ServerGameplayConfig {
    let mut config = fixtures::server_config();
    config.settings = MapSettings {
        grounds: None,
        celestial: CelestialMapSettings {
            latitude_degrees: 40.0,
            season: Season::Summer,
            north_yaw_degrees: 0.0,
            start_local_time: LocalTime::parse("09:00").expect("valid fixture time"),
            start_moon_phase: 0.25,
        },
        textures: Default::default(),
        geometry: sizes(),
        movement: ok_movement(),
        portals: PortalMode::Both,
        switches: Vec::new(),
        field_kinds: Vec::new(),
    };
    config.random_items = None;
    config.player_fall = FallDamageConfig {
        safe_distance: 8.0,
        lethal_distance: 15.0,
    };
    config.actor_fall = FallDamageConfig {
        safe_distance: 8.0,
        lethal_distance: 15.0,
    };
    config.respawn = RespawnConfig::default();
    config.placed_items = Some(ok_placed_items());
    config.power_ups = PowerUpsConfig {
        speed: PowerUpMode::Pickup {
            duration_secs: Some(30.0),
        },
        single_shot: PowerUpMode::Pickup { duration_secs: None },
        multi_shot: PowerUpMode::Pickup {
            duration_secs: Some(25.0),
        },
        low_gravity: PowerUpMode::Pickup {
            duration_secs: Some(20.0),
        },
        portal_gun: PowerUpMode::Pickup { duration_secs: None },
    };
    config.weather = WeatherMode::Clear;
    config.quests = Vec::new();
    config
}

// A map's own settings parsed over the shipped defaults.
fn parse_map(map: serde_json::Value) -> Result<ServerGameplayConfig> {
    ServerGameplayConfig::from_override("hotel", &fixtures::gameplay_defaults(), &map)
}

fn validate_config(config: &ServerGameplayConfig, name: &str) -> Result<()> {
    config.validate(&format!("maps/{name}/settings.json: "))
}

#[test]
fn map_respawn_policy_rejects_unknown_modes() {
    let content: serde_json::Value = serde_json::from_str(fixtures::MAP_JSON).expect("map settings JSON is invalid");
    for invalid in [
        json!({"players": "group_on_respawn", "actors": {"on_player_death": "any", "scope": "all"}}),
        json!({"players": "group", "actors": {"on_player_death": "sometimes", "scope": "all"}}),
        json!({"players": "group", "actors": {"on_player_death": "any", "scope": "some"}}),
        json!({"players": "group", "actors": {"on_player_death": "always", "scope": "all"}}),
        json!({"players": "group", "actors": {"on_player_death": "any", "scope": "all", "extra": 1}}),
    ] {
        let mut map = content.clone();
        map["respawn"] = invalid;
        assert!(parse_map(map).is_err());
    }
    let mut map = content.clone();
    map["respawn"] = json!({"players": "group"});
    assert_eq!(
        parse_map(map).expect("partial respawn policy rejected").respawn,
        RespawnConfig {
            players: crate::config::PlayerRespawnMode::Group,
            ..RespawnConfig::default()
        }
    );
}

fn ok_placed_items() -> PlacedItemsConfig {
    PlacedItemsConfig {
        respawn_secs: crate::config::PlacedItemRespawnSecs {
            gold: Some(60.0),
            ..Default::default()
        },
    }
}

fn ok_random_items(types: &[&str]) -> RandomItemsConfig {
    RandomItemsConfig {
        weights: types.iter().map(|&t| (t.to_owned(), 1.0)).collect(),
        max_number: 30,
        despawn_secs: 60.0,
    }
}

fn with_random_items(random_items: RandomItemsConfig) -> ServerGameplayConfig {
    let mut config = ok_config();
    config.random_items = Some(random_items);
    config
}

fn parse_map_entry(portals: &str, weather: Option<&str>) -> Result<ServerGameplayConfig> {
    let mut map: serde_json::Value = serde_json::from_str(fixtures::MAP_JSON).expect("map settings JSON is invalid");
    map["portals"] = portals.into();
    if let Some(weather) = weather {
        map["weather"] = weather.into();
    }
    parse_map(map)
}

#[test]
fn validate_accepts_a_valid_config() {
    validate_config(&ok_config(), "hotel").expect("valid map config should pass");
}

#[test]
fn map_fall_thresholds_are_validated_with_their_source() {
    let content: serde_json::Value = serde_json::from_str(fixtures::MAP_JSON).expect("map settings JSON is invalid");
    let thresholds: [(&str, fn(&mut ServerGameplayConfig) -> &mut FallDamageConfig); 2] = [
        ("player_fall", |config| &mut config.player_fall),
        ("actor_fall", |config| &mut config.actor_fall),
    ];
    for (key, fall) in thresholds {
        for invalid in [
            json!({"safe_distance": true, "lethal_distance": 12}),
            json!({"safe_distance": 4, "lethal_distance": "12"}),
            json!({"safe_distance": 4, "lethal_distance": 12, "extra": 0}),
        ] {
            let mut map = content.clone();
            map[key] = invalid;
            assert!(parse_map(map).is_err());
        }
        for (safe, lethal, field) in [
            (-1.0, 12.0, "safe_distance"),
            (4.0, -1.0, "lethal_distance"),
            (0.0, 0.0, "safe_distance"),
            (12.0, 12.0, "safe_distance"),
            (13.0, 12.0, "safe_distance"),
            (f32::NAN, 12.0, "safe_distance"),
            (4.0, f32::INFINITY, "lethal_distance"),
        ] {
            let mut config = ok_config();
            *fall(&mut config) = FallDamageConfig {
                safe_distance: safe,
                lethal_distance: lethal,
            };
            let error = validate_config(&config, "example").expect_err("invalid fall thresholds accepted");
            assert!(
                error
                    .to_string()
                    .contains(&format!("maps/example/settings.json: {key}.{field}")),
                "{error}"
            );
        }
        let mut config = ok_config();
        fall(&mut config).safe_distance = 0.0;
        validate_config(&config, "example").expect("zero safe distance rejected");
    }
}

#[test]
fn validate_maps_rejects_empty_registry() {
    let err = validate_map_registry([], "hotel").expect_err("empty registry must be rejected");
    assert!(err.to_string().contains("at least one"));
}

#[test]
fn validate_maps_rejects_unknown_default_map() {
    let err = validate_map_registry(["hotel"], "lobby").expect_err("unknown default must be rejected");
    assert!(err.to_string().contains("default_map"));
}

#[test]
fn validate_maps_rejects_path_unsafe_name() {
    let err = validate_map_registry(["../hotel"], "../hotel").expect_err("path chars must be rejected");
    assert!(err.to_string().contains("ASCII"));
}

#[test]
fn validate_maps_rejects_non_positive_gravity() {
    let mut config = ok_config();
    config.settings.movement.gravity = 0.0;
    let err = validate_config(&config, "hotel").expect_err("zero gravity must be rejected");
    assert!(err.to_string().contains("gravity"));
}

#[test]
fn validate_maps_rejects_non_positive_cell_size() {
    let mut config = ok_config();
    config.settings.geometry.grid_cell_size = 0.0;
    let err = validate_config(&config, "hotel").expect_err("zero cell size must be rejected");
    assert!(err.to_string().contains("settings.json: geometry.grid_cell_size"));
}

#[test]
fn validate_maps_rejects_non_positive_player_speed() {
    let mut config = ok_config();
    config.settings.movement.player.run_speed = 0.0;
    let err = validate_config(&config, "hotel").expect_err("zero run speed must be rejected");
    assert!(err.to_string().contains("movement.player.run_speed"));
}

#[test]
fn validate_maps_rejects_missing_actor_movement() {
    let mut config = ok_config();
    config.settings.movement.actors.remove("scuttler");
    let err = validate_config(&config, "hotel").expect_err("missing actor movement must be rejected");
    assert!(err.to_string().contains("movement.actors"));
    assert!(err.to_string().contains("scuttler"));
}

#[test]
fn validate_maps_rejects_unknown_actor_movement() {
    let mut config = ok_config();
    let movement = config
        .settings
        .movement
        .actors
        .get("zapper")
        .copied()
        .expect("zapper movement missing");
    config.settings.movement.actors.insert("banana".to_owned(), movement);
    let err = validate_config(&config, "hotel").expect_err("unknown actor movement must be rejected");
    assert!(err.to_string().contains("movement.actors"));
    assert!(err.to_string().contains("banana"));
}

#[test]
fn validate_maps_rejects_negative_low_gravity() {
    let mut config = ok_config();
    config.settings.movement.low_gravity = -1.0;
    let err = validate_config(&config, "hotel").expect_err("negative low_gravity must be rejected");
    assert!(err.to_string().contains("low_gravity"));
}

#[test]
fn validate_maps_rejects_invalid_latitude() {
    let mut config = ok_config();
    config.settings.celestial.latitude_degrees = 91.0;
    let err = validate_config(&config, "hotel").expect_err("invalid latitude must be rejected");
    assert!(err.to_string().contains("latitude_degrees"));
}

#[test]
fn map_entry_inherits_the_default_weather() {
    let entry = parse_map_entry("both", None).expect("weather should come from the defaults");
    assert_eq!(entry.weather, WeatherMode::Auto);
}

#[test]
fn textures_require_an_explicit_catalog_with_materials_and_boolean_permissions() {
    let source: serde_json::Value = serde_json::from_str(fixtures::MAP_JSON).expect("map settings JSON is invalid");
    let mut hotel = source.clone();
    hotel
        .as_object_mut()
        .expect("hotel settings is not an object")
        .remove("textures");
    assert!(
        parse_map(hotel)
            .expect_err("missing textures was accepted")
            .to_string()
            .contains("textures")
    );
    for texture in [
        serde_json::json!({"portalable": true}),
        serde_json::json!({"material": "stone"}),
        serde_json::json!({"material": "stone", "portalable": "false"}),
    ] {
        let mut hotel = source.clone();
        hotel["textures"] = serde_json::json!({"stone": texture});
        assert!(parse_map(hotel).is_err());
    }
}

#[test]
fn map_entry_requires_placed_items() {
    let source: serde_json::Value = serde_json::from_str(fixtures::MAP_JSON).expect("map settings JSON is invalid");
    let mut hotel = source.clone();
    hotel
        .as_object_mut()
        .expect("hotel map settings are not an object")
        .remove("placed_items");

    let error = parse_map(hotel).expect_err("placed_items must be defined for every map");
    assert!(error.to_string().contains("placed_items"));
}

#[test]
fn map_entry_accepts_null_placed_items() {
    let mut source: serde_json::Value = serde_json::from_str(fixtures::MAP_JSON).expect("map settings JSON is invalid");
    source["placed_items"] = serde_json::Value::Null;
    let entry = parse_map(source).expect("null placed items rejected");
    assert!(entry.placed_items.is_none());
    validate_config(&entry, "hotel").expect("null placed items failed validation");
}

#[test]
fn validate_maps_rejects_invalid_placed_item_respawn() {
    for seconds in [-1.0, f32::NAN, f32::INFINITY] {
        let mut config = ok_config();
        config
            .placed_items
            .as_mut()
            .expect("placed item settings missing")
            .respawn_secs
            .gold = Some(seconds);

        let error = validate_config(&config, "hotel").expect_err("invalid respawn time accepted");
        assert!(
            error
                .to_string()
                .contains("settings.json: placed_items.respawn_secs.gold")
        );
    }
}

#[test]
fn map_entry_parses_snake_case_weather() {
    let entry = parse_map_entry("both", Some("rain")).expect("map entry should deserialize");
    assert_eq!(entry.weather, WeatherMode::Rain);
}

#[test]
fn map_entry_parses_auto_modes() {
    let entry = parse_map_entry("both", Some("auto")).expect("map entry should deserialize");
    assert_eq!(entry.weather, WeatherMode::Auto);
}

#[test]
fn map_entry_accepts_single_or_both_portal_ownership() {
    let both = parse_map_entry("both", Some("clear")).expect("map entry JSON is invalid");
    assert_eq!(both.settings.portals, PortalMode::Both);

    let single = parse_map_entry("single", Some("clear")).expect("map entry JSON is invalid");
    assert_eq!(single.settings.portals, PortalMode::Single);
}

#[test]
fn validate_maps_accepts_valid_random_items() {
    let config = serde_json::from_str::<RandomItemsConfig>(
        r#"{"weights":{"speed":0.5,"gold":3,"missile_pack":0},"max_number":30,"despawn_secs":60}"#,
    )
    .expect("random item weights JSON is invalid");
    let config = with_random_items(config);
    validate_config(&config, "hotel").expect("valid random item weights rejected");
}

#[test]
fn validate_maps_rejects_key_in_random_pool() {
    let config = with_random_items(ok_random_items(&["speed", "key"]));
    let err = validate_config(&config, "hotel").expect_err("key in random pool must be rejected");
    assert!(err.to_string().contains("field kind"));
}

#[test]
fn validate_maps_rejects_unknown_random_item_type() {
    let config = with_random_items(ok_random_items(&["banana"]));
    let err = validate_config(&config, "hotel").expect_err("unknown type must be rejected");
    assert!(err.to_string().contains("unknown item type"));
}

#[test]
fn validate_maps_rejects_invalid_random_item_weights() {
    for weight in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut config = ok_random_items(&["speed", "gold"]);
        config.weights.insert("speed".to_owned(), weight);
        let config = with_random_items(config);
        let err = validate_config(&config, "hotel").expect_err("invalid random item weight accepted");
        assert!(err.to_string().contains("settings.json: random_items.weights.speed"));
    }
}

#[test]
fn validate_maps_rejects_empty_random_item_weights() {
    let config = with_random_items(ok_random_items(&[]));
    let err = validate_config(&config, "hotel").expect_err("empty pool must be rejected");
    assert!(err.to_string().contains("weights"));
}

#[test]
fn validate_maps_rejects_zero_total_random_item_weight() {
    let mut config = ok_random_items(&["speed", "gold"]);
    config.weights.values_mut().for_each(|weight| *weight = 0.0);
    let config = with_random_items(config);
    let err = validate_config(&config, "hotel").expect_err("zero total random item weight accepted");
    assert!(err.to_string().contains("at least one positive weight"));
}

#[test]
fn validate_maps_rejects_overflowing_total_random_item_weight() {
    let mut config = ok_random_items(&["speed", "gold"]);
    config.weights.values_mut().for_each(|weight| *weight = f64::MAX);
    let config = with_random_items(config);
    let err = validate_config(&config, "hotel").expect_err("overflowing total random item weight accepted");
    assert!(err.to_string().contains("weights total must be finite"));
}

#[test]
fn random_items_requires_weights() {
    let err = serde_json::from_str::<RandomItemsConfig>(r#"{"max_number":30,"despawn_secs":60}"#)
        .expect_err("random item config without weights accepted");
    assert!(err.to_string().contains("missing field `weights`"));
}

#[test]
fn validate_maps_accepts_projectile_pickups_as_the_only_random_items() {
    for item in ["single_shot", "multi_shot"] {
        let config = with_random_items(ok_random_items(&[item]));
        validate_config(&config, "hotel").expect("projectile pickup pool is invalid");
    }
}

#[test]
fn validate_maps_rejects_zero_random_item_max_number() {
    let mut random_items = ok_random_items(&["speed"]);
    random_items.max_number = 0;
    let config = with_random_items(random_items);
    let err = validate_config(&config, "hotel").expect_err("zero max_number must be rejected");
    assert!(err.to_string().contains("max_number"));
}

#[test]
fn optional_feature_blocks_require_explicit_null_and_collections_require_their_type() {
    let source: serde_json::Value = serde_json::from_str(fixtures::MAP_JSON).expect("map fixture is invalid");
    for key in ["grounds", "random_items"] {
        let mut value = source.clone();
        value[key] = serde_json::Value::Null;
        parse_map(value.clone()).expect("disabled feature rejected");
        value.as_object_mut().expect("map fixture is not an object").remove(key);
        assert!(parse_map(value).is_err(), "missing {key} accepted");
    }
    for key in ["quests", "textures"] {
        let mut value = source.clone();
        value[key] = serde_json::Value::Null;
        assert!(parse_map(value).is_err(), "null {key} accepted");
    }
}

#[test]
fn always_active_power_ups_cannot_have_random_pickup_weights() {
    for weight in [0.0, 1.0] {
        let mut config = with_random_items(ok_random_items(&["single_shot", "gold"]));
        config.power_ups.single_shot = PowerUpMode::Always {};
        config
            .random_items
            .as_mut()
            .expect("random items missing")
            .weights
            .insert("single_shot".into(), weight);
        let error = validate_config(&config, "hotel").expect_err("always-active pickup accepted");
        assert!(error.to_string().contains("random_items.weights.single_shot"));
    }
}
