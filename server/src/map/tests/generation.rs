use crate::config::fixtures;
use std::fs;

use rand::random;
use serde_json::{Value, json};

use super::*;

fn settings() -> MapSettings {
    fixtures::server_config().maps["hotel"].settings.clone()
}

struct TestMap(PathBuf);

impl TestMap {
    fn new(edit: impl FnOnce(&mut Value)) -> Self {
        let mut file = json!({"map": {
            "grid_cols": 3, "grid_rows": 2,
            "levels": [{"floors": [
                {"col": 0, "row": 0, "all": "basement-floor"},
                {"col": 1, "row": 0, "all": "basement-floor"}
            ]}],
            "player_spawn_zones": [{"level": 0, "cols": [0, 1], "rows": [0, 1]}],
            "switch_kinds": [
                {"id": "lobby", "activation": "toggle", "reset_on_player_death": "never"},
                {"id": "fireworks", "activation": "momentary", "reset_on_player_death": "never"}
            ],
            "pressure_plates": [{"level": 0, "col": 1, "row": 0, "switch": "fireworks"}],
            "fireworks": {"switch": "fireworks", "cooldown_secs": 2.0}
        }});
        edit(&mut file["map"]);
        let directory = std::env::temp_dir().join(format!("cuboid_generation_{}", random::<u64>()));
        fs::create_dir(&directory).expect("temporary map directory unavailable");
        let path = directory.join("layout.json");
        fs::write(&path, file.to_string()).expect("temporary layout unwritable");
        Self(path)
    }

    fn generate(&self) -> Result<GeneratedMap> {
        generate_map_at(&self.0, "hotel", 30, &settings())
    }

    fn error(&self) -> String {
        format!("{:#}", self.generate().err().expect("invalid layout accepted"))
    }
}

impl Drop for TestMap {
    fn drop(&mut self) {
        fs::remove_dir_all(self.0.parent().expect("temporary layout has no directory"))
            .expect("temporary map directory cleanup failed");
    }
}

#[test]
fn missing_map_returns_contextual_error() {
    let directory = TestMap::new(|_| {});
    let missing = directory.0.with_file_name("missing-layout.json");
    let error = generate_map_at(&missing, "missing", 30, &settings())
        .err()
        .expect("missing map must fail");

    assert!(error.to_string().contains("failed to load map at"));
    assert!(
        error
            .to_string()
            .contains(missing.to_str().expect("test path is not UTF-8"))
    );
}

#[test]
fn a_map_cannot_reference_an_alias_outside_its_host_catalog() {
    let config = fixtures::server_config();
    let mut settings = config.maps["obby"].settings.clone();
    settings.textures.remove("basement-floor");
    let fixture = TestMap::new(|_| {});
    let error = generate_map_at(&fixture.0, "fixture", 30, &settings)
        .err()
        .expect("undeclared map material was accepted");
    assert!(error.to_string().contains("basement-floor"), "{error}");
}

#[test]
fn the_layouts_switch_catalog_and_fireworks_fill_the_generated_map() {
    let map = TestMap::new(|_| {}).generate().expect("test map failed to generate");
    let ids: Vec<&str> = map.settings.switches.iter().map(|def| def.id.as_str()).collect();
    assert_eq!(ids, ["lobby", "fireworks"]);
    assert_eq!(map.switch_table.index_of("fireworks"), map.fireworks_switch);
    assert_eq!(
        map.fireworks.as_ref().map(|fireworks| fireworks.switch.as_str()),
        Some("fireworks")
    );
}

#[test]
fn duplicate_switch_kinds_are_rejected_naming_the_layout() {
    let hotel = TestMap::new(|map| {
        let lobby = map["switch_kinds"][0].clone();
        map["switch_kinds"]
            .as_array_mut()
            .expect("switch_kinds is an array")
            .push(lobby);
    });
    let error = hotel.error();
    assert!(error.contains("switch_kinds") && error.contains("duplicate"), "{error}");
    assert!(error.contains("layout.json"), "{error}");
}

#[test]
fn fireworks_must_name_a_catalogued_switch_with_a_plate_and_a_finite_cooldown() {
    for (edit, expected) in [
        (
            (|map: &mut Value| map["fireworks"]["switch"] = json!("void")) as fn(&mut Value),
            "void",
        ),
        (|map| map["fireworks"]["cooldown_secs"] = json!(-1.0), "cooldown_secs"),
        (
            |map| {
                map["switch_kinds"]
                    .as_array_mut()
                    .expect("switch_kinds is an array")
                    .push(json!({"id": "spare", "activation": "toggle", "reset_on_player_death": "never"}));
                map["fireworks"]["switch"] = json!("spare");
            },
            "operated by no pressure plate",
        ),
    ] {
        let error = TestMap::new(edit).error();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("fireworks"), "{error}");
    }
}

#[test]
fn a_map_without_fireworks_has_no_fireworks_switch() {
    let map = TestMap::new(|map| map["fireworks"] = Value::Null)
        .generate()
        .expect("test map without fireworks failed to generate");
    assert!(map.fireworks.is_none());
    assert_eq!(map.fireworks_switch, None);
}
