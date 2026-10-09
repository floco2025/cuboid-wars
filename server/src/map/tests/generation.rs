use std::fs;

use rand::random;
use serde_json::{Value, json};

use super::*;
use crate::config::fixtures;

fn settings() -> MapSettings {
    fixtures::server_config().settings.clone()
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
            "checkpoints": [{"level": 0, "cols": [0, 1], "rows": [0, 1], "type": "individual", "number": 0}],
            "switches": [
                {"id": "lobby", "activation": "toggle", "reset_on_player_death": "never"},
                {"id": "fireworks", "activation": "momentary", "reset_on_player_death": "never"}
            ],
            "fields": [
                {"id": "lobby", "color": "#22cc33", "switch": "lobby"},
                {"id": "skyway", "color": "#30d8ff", "initially_on": false}
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
fn maximum_level_count_compiles_for_root_and_nested_grids() {
    let fixture = TestMap::new(|map| {
        let level = map["levels"][0].clone();
        map["levels"] = json!(vec![level.clone(); 255]);
        map["checkpoints"][0]["level"] = json!(254);
        map["nested_geometry"] = json!({"room": {
            "grid_cols": 3, "grid_rows": 2, "levels": vec![level; 255]
        }});
        map["nested_maps"] = json!([{
            "map": "room", "level": 0, "from": [0, 0], "to": [0, 0], "travel_secs": 1.0
        }]);
    });
    let generated = fixture.generate().expect("maximum level count rejected");
    assert_eq!(generated.config.grids.len(), 2);
    for grid in &generated.config.grids {
        assert_eq!(
            u8::try_from(grid.levels.len()).expect("bootstrap level count overflow"),
            255
        );
    }
    assert_eq!(generated.layout.checkpoints[0].level, 254);
}

#[test]
fn excessive_level_counts_fail_loading_root_placed_and_unplaced_geometry() {
    for level_count in [256, 257] {
        for nested_placement in [None, Some(false), Some(true)] {
            let fixture = TestMap::new(|map| {
                let levels = json!(vec![map["levels"][0].clone(); level_count]);
                if let Some(placed) = nested_placement {
                    map["nested_geometry"] = json!({"room": {
                        "grid_cols": 3, "grid_rows": 2, "levels": levels
                    }});
                    if placed {
                        map["nested_maps"] = json!([{
                            "map": "room", "level": 0, "from": [0, 0], "to": [0, 0], "travel_secs": 1.0
                        }]);
                    }
                } else {
                    map["levels"] = levels;
                }
            });
            let error = fixture.error();
            assert!(error.contains("validating map"), "{error}");
            assert!(
                error.contains(&format!("at most 255 levels are supported (found {level_count})")),
                "{error}"
            );
            if nested_placement.is_some() {
                assert!(error.contains("Nested room:"), "{error}");
            }
        }
    }
}

#[test]
fn a_map_cannot_reference_an_alias_outside_its_host_catalog() {
    let mut settings = settings();
    settings.textures.remove("basement-floor");
    let fixture = TestMap::new(|_| {});
    let error = generate_map_at(&fixture.0, "fixture", 30, &settings)
        .err()
        .expect("undeclared map material was accepted");
    assert!(error.to_string().contains("basement-floor"), "{error}");
}

#[test]
fn fireworks_must_name_a_catalogued_switch_and_a_finite_cooldown() {
    for (edit, expected) in [
        (
            (|map: &mut Value| map["fireworks"]["switch"] = json!("void")) as fn(&mut Value),
            "void",
        ),
        (|map| map["fireworks"]["cooldown_secs"] = json!(-1.0), "cooldown_secs"),
    ] {
        let error = TestMap::new(edit).error();
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("fireworks"), "{error}");
    }
}
