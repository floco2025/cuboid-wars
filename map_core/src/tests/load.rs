use super::prepare_source;
use crate::schema::*;
use anyhow::Result;
use serde_json::{Value, json};

// A floored corner with the start on it, placing `names` along the top row.
fn geometry(names: &[&str]) -> Value {
    json!({
        "grid_cols": 4, "grid_rows": 4, "levels": [{"floors": [{"col": 0, "row": 0, "all": "test"}]}],
        "checkpoints": [{"level": 0, "cols": [0, 1], "rows": [0, 1], "type": "individual", "number": 0}],
        "nested_maps": names.iter().enumerate().map(|(index, name)| json!({
            "map": name, "level": 0, "from": [index, 0], "to": [index, 0], "travel_secs": 1.0,
        })).collect::<Vec<_>>()
    })
}

fn source(root: &[&str], definitions: &[(&str, &[&str])]) -> Result<MapSource> {
    let mut value = geometry(root);
    value["nested_geometry"] = definitions
        .iter()
        .map(|(name, children)| (name.to_string(), geometry(children)))
        .collect();
    prepare_source(serde_json::from_value::<MapDef>(value).expect("test map source is invalid"))
}

#[test]
fn nested_cycle_is_rejected_naming_the_chain() {
    let error = source(&["a"], &[("a", &["b"]), ("b", &["a"])]).expect_err("cycle accepted");
    assert!(error.to_string().contains("a -> b -> a"), "{error}");
}

#[test]
fn references_resolve_only_to_the_parents_named_geometry() {
    let error = source(&["hotel"], &[]).expect_err("missing embedded geometry accepted");
    assert!(error.to_string().contains("hotel"), "{error}");
    assert!(error.to_string().contains("nested_geometry"), "{error}");
}

#[test]
fn repeated_placements_share_one_definition() {
    let loaded = source(&["a", "b"], &[("a", &["c"]), ("b", &["c"]), ("c", &[])]).expect("shared definition rejected");
    assert_eq!(loaded.nested_geometry.len(), 3);
}

#[test]
fn unused_definitions_are_checked_but_do_not_compile() {
    let loaded = source(&["used"], &[("used", &[]), ("unused", &[])]).expect("unused geometry rejected");
    assert_eq!(loaded.nested_geometry.len(), 1);
    assert!(loaded.nested_geometry.contains_key("used"));
    assert!(source(&[], &[("unused", &["missing"])]).is_err());
    assert!(source(&[], &[("a", &["b"]), ("b", &["a"])]).is_err());
}

#[test]
fn the_placed_tree_starts_at_checkpoint_zero_and_zones_end_at_a_placed_number() {
    let floored = |mut value: Value| {
        value["levels"] =
            json!([{"floors": [{"col": 0, "row": 0, "all": "test"}, {"col": 1, "row": 0, "all": "test"}]}]);
        value
    };
    let mut value = floored(geometry(&["room"]));
    value["checkpoints"] = json!([
        {"level": 0, "cols": [0, 1], "rows": [0, 1], "type": "individual", "number": 0},
        {"level": 0, "cols": [1, 2], "rows": [0, 1], "type": "individual", "number": 1}
    ]);
    let mut room = floored(geometry(&[]));
    room["checkpoints"] = json!([
        {"level": 0, "cols": [0, 1], "rows": [0, 1], "type": "individual", "number": 2},
        {"level": 0, "cols": [1, 2], "rows": [0, 1], "type": "group_any", "number": 1}
    ]);
    room["actor_spawn_zones"] = json!([{
        "level": 0, "cols": [1, 2], "rows": [1, 2], "kind": "actor", "count": [1], "respawn_secs": null,
        "until_checkpoint": 1, "on_checkpoint": "destroy",
    }]);
    value["nested_geometry"] = json!({ "room": room });
    let parse = |value: &Value| serde_json::from_value::<MapDef>(value.clone()).expect("test source is invalid");
    prepare_source(parse(&value)).expect("numbered document rejected");

    let mut startless = value.clone();
    startless["checkpoints"][0]["number"] = json!(3);
    let error = prepare_source(parse(&startless))
        .expect_err("a course without a start accepted")
        .to_string();
    assert!(error.contains("checkpoint 0"), "{error}");
    startless["nested_geometry"]["room"]["checkpoints"][0]["number"] = json!(0);
    prepare_source(parse(&startless)).expect("a nested start rejected");

    let mut dangling = value.clone();
    dangling["nested_geometry"]["room"]["actor_spawn_zones"][0]["until_checkpoint"] = json!(9);
    let error = prepare_source(parse(&dangling))
        .expect_err("a reference to no checkpoint accepted")
        .to_string();
    assert!(error.contains("names no checkpoint"), "{error}");

    let mut orphan = value.clone();
    orphan["nested_geometry"]["room"]["actor_spawn_zones"][0]
        .as_object_mut()
        .expect("zone missing")
        .remove("until_checkpoint");
    let error = format!(
        "{:#}",
        prepare_source(parse(&orphan)).expect_err("a response without a checkpoint accepted")
    );
    assert!(error.contains("needs an until_checkpoint"), "{error}");

    // An unplaced definition is scratch geometry: its numbers are free, and nothing can end at them.
    let mut spare = value;
    spare["nested_geometry"]["spare"] = spare["nested_geometry"]["room"].clone();
    spare["nested_geometry"]["spare"]["actor_spawn_zones"] = json!([]);
    prepare_source(parse(&spare)).expect("an unplaced definition rejected");
    spare["nested_geometry"]["spare"]["checkpoints"][0]["number"] = json!(7);
    spare["actor_spawn_zones"] = json!([{
        "level": 0, "cols": [1, 2], "rows": [1, 2], "kind": "actor", "count": [1], "respawn_secs": null,
        "until_checkpoint": 7,
    }]);
    let error = prepare_source(parse(&spare))
        .expect_err("a reference into unplaced geometry accepted")
        .to_string();
    assert!(error.contains("names no checkpoint"), "{error}");
}

#[test]
fn invalid_named_geometry_is_rejected() {
    let mut value = geometry(&["room"]);
    value["nested_geometry"] = json!({"room": geometry(&[])});
    value["nested_geometry"]["room"]["grid_cols"] = json!(0);
    let error = prepare_source(serde_json::from_value::<MapDef>(value).expect("test source is invalid"))
        .expect_err("invalid nested geometry accepted");
    assert!(format!("{error:#}").contains("room"));
}

#[test]
fn root_catalogs_move_off_the_geometry_and_nested_geometry_may_not_define_them() {
    let mut value = geometry(&["room"]);
    value["switches"] = json!([{"id": "door", "activation": "toggle", "reset_on_player_death": "never"}]);
    value["barrier_kinds"] = json!([{"id": "red", "color": "#ff0000"}]);
    value["bridge_kinds"] = json!([{"id": "skyway", "color": "#30d8ff"}]);
    value["fireworks"] = json!({"switch": "door", "cooldown_secs": 3.0});
    value["pressure_plates"] = json!([{"level": 0, "col": 0, "row": 0, "switch": "door"}]);
    value["nested_geometry"] = json!({"room": geometry(&[])});
    let loaded = prepare_source(serde_json::from_value::<MapDef>(value.clone()).expect("test source is invalid"))
        .expect("root catalogs rejected");
    assert_eq!(loaded.switches[0].id, "door");
    assert_eq!(loaded.barrier_kinds[0].id, "red");
    assert_eq!(loaded.bridge_kinds[0].id, "skyway");
    assert_eq!(
        loaded.fireworks.map(|fireworks| fireworks.switch).as_deref(),
        Some("door")
    );
    let root = &loaded.geometry;
    assert!(root.switches.is_empty() && root.fireworks.is_none());
    assert!(root.barrier_kinds.is_empty() && root.bridge_kinds.is_empty());
    for (key, nested) in [
        (
            "switches",
            json!([{"id": "door", "activation": "toggle", "reset_on_player_death": "never"}]),
        ),
        ("barrier_kinds", json!([{"id": "red", "color": "#ff0000"}])),
        ("bridge_kinds", json!([{"id": "skyway", "color": "#30d8ff"}])),
        ("fireworks", json!({"switch": "door", "cooldown_secs": 3.0})),
        ("nested_geometry", json!({"inner": geometry(&[])})),
    ] {
        let mut value = value.clone();
        value["nested_geometry"]["room"][key] = nested;
        let error = prepare_source(serde_json::from_value::<MapDef>(value).expect("test source is invalid"))
            .expect_err("nested root catalog accepted");
        assert!(error.to_string().contains("room"), "{error}");
        assert!(error.to_string().contains(key), "{error}");
    }
}

#[test]
fn unknown_root_keys_and_a_fireworks_response_are_rejected() {
    let mut value = geometry(&[]);
    value["firework"] = json!({"switch": "door", "cooldown_secs": 3.0});
    let error = serde_json::from_value::<MapDef>(value).expect_err("misspelled root key accepted");
    assert!(error.to_string().contains("firework"), "{error}");
    let mut value = geometry(&[]);
    value["fireworks"] = json!({"switch": "door", "cooldown_secs": 3.0, "switch_inverted": true});
    let error = serde_json::from_value::<MapDef>(value).expect_err("fireworks response accepted");
    assert!(error.to_string().contains("switch_inverted"), "{error}");
}

#[test]
fn root_fireworks_is_required_and_nullable_without_requiring_it_on_nested_geometry() {
    let mut root = geometry(&["room"]);
    root["nested_geometry"] = json!({"room": geometry(&[])});
    assert!(serde_json::from_value::<MapFile>(json!({"map": root.clone()})).is_err());
    root["fireworks"] = Value::Null;
    let parsed = serde_json::from_value::<MapFile>(json!({"map": root.clone()})).expect("disabled fireworks rejected");
    prepare_source(parsed.map).expect("nested geometry without fireworks rejected");
    for invalid in [json!([]), json!({})] {
        root["fireworks"] = invalid;
        assert!(serde_json::from_value::<MapFile>(json!({"map": root.clone()})).is_err());
    }
}

#[test]
fn unplaced_geometry_may_target_its_own_plates_and_checkpoints() {
    let mut value = geometry(&[]);
    value["switches"] = json!([{"id": "door", "activation": "toggle", "reset_on_player_death": "never"}]);
    let mut scratch = geometry(&[]);
    scratch["pressure_plates"] = json!([{"level": 0, "col": 0, "row": 0, "switch": "door"}]);
    scratch["checkpoints"][0]["number"] = json!(4);
    scratch["actor_spawn_zones"] = json!([{
        "level": 0, "cols": [1, 2], "rows": [1, 2], "kind": "actor", "count": [1], "respawn_secs": null,
        "switch": "door", "until_checkpoint": 4,
    }]);
    value["nested_geometry"] = json!({ "scratch": scratch });
    let parse = |value: &Value| serde_json::from_value::<MapDef>(value.clone()).expect("test source is invalid");
    let loaded = prepare_source(parse(&value)).expect("self-contained scratch geometry rejected");
    assert!(loaded.nested_geometry.is_empty());

    value["nested_geometry"]["scratch"]["pressure_plates"] = json!([]);
    let error = prepare_source(parse(&value))
        .expect_err("a switch no plate operates accepted")
        .to_string();
    assert!(error.contains("no pressure plate operates"), "{error}");
}

#[test]
fn duplicate_ramps_are_rejected() {
    let mut value = geometry(&[]);
    value["levels"] = json!([
        {"floors": [{"col": 0, "row": 0, "all": "test"}]},
        {"floors": []}
    ]);
    let ramp = json!({"lower_level": 0, "low": [1, 1], "high": [3, 2], "all": "test"});
    value["ramps"] = json!([ramp]);
    let parse = |value: &Value| serde_json::from_value::<MapDef>(value.clone()).expect("test source is invalid");
    prepare_source(parse(&value)).expect("single ramp rejected");
    value["ramps"] = json!([ramp, ramp]);
    let error = prepare_source(parse(&value))
        .expect_err("duplicate ramp accepted")
        .to_string();
    assert!(error.contains("duplicates another ramp"), "{error}");
}
