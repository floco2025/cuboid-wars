use super::merge_map_settings;
use serde_json::{Value, json};

fn defaults() -> Value {
    json!({
        "network": {"server_hz": 30},
        "movement": {"gravity": 25.0, "player": {"walk_speed": 6.0, "run_speed": 9.0}},
        "power_ups": {
            "single_shot": {"mode": "always"},
            "speed": {"mode": "pickup", "duration_secs": 30.0}
        },
        "actors": {"zapper": {"vision_range": 40.0, "attack": {"type": "beam", "range": 15.0}}},
        "weapons": {"allowed_patterns": ["triple", "spread"]}
    })
}

fn merge(map: Value) -> Result<Value, String> {
    merge_map_settings(&defaults(), &map).map_err(|error| format!("{error:#}"))
}

#[test]
fn nested_overrides_replace_leaves_and_keep_their_siblings() {
    let merged = merge(json!({"textures": {}, "movement": {"player": {"run_speed": 5.0}}})).expect("valid override");
    assert_eq!(
        merged["movement"],
        json!({"gravity": 25.0, "player": {"walk_speed": 6.0, "run_speed": 5.0}})
    );
    assert_eq!(merged["textures"], json!({}));
    assert_eq!(merged["network"], json!({"server_hz": 30}));
}

#[test]
fn null_and_arrays_replace_the_default() {
    let merged =
        merge(json!({"weapons": {"allowed_patterns": ["spread"]}, "power_ups": {"speed": {"duration_secs": null}}}))
            .expect("valid override");
    assert_eq!(merged["weapons"]["allowed_patterns"], json!(["spread"]));
    assert_eq!(
        merged["power_ups"]["speed"],
        json!({"mode": "pickup", "duration_secs": null})
    );
}

#[test]
fn an_unknown_key_names_its_path() {
    let error = merge(json!({"movement": {"player": {"jump_speeed": 1.0}}})).expect_err("typo accepted");
    assert!(
        error.contains("movement.player.jump_speeed is not a key in the defaults"),
        "{error}"
    );
    let error = merge(json!({"actors": {"bruiser": {"vision_range": 1.0}}})).expect_err("new kind accepted");
    assert!(error.contains("actors.bruiser is not a key in the defaults"), "{error}");
    let error = merge(json!({"movment": {}})).expect_err("top-level typo accepted");
    assert!(error.contains("movment is not a key in the defaults"), "{error}");
}

#[test]
fn a_tagged_object_replaces_the_default_only_when_its_tag_changes() {
    let merged = merge(json!({"power_ups": {"single_shot": {"mode": "pickup", "duration_secs": null}}}))
        .expect("variant change accepted");
    assert_eq!(
        merged["power_ups"]["single_shot"],
        json!({"mode": "pickup", "duration_secs": null})
    );
    let merged = merge(json!({"actors": {"zapper": {"attack": {"range": 20.0}}}})).expect("partial variant accepted");
    assert_eq!(
        merged["actors"]["zapper"]["attack"],
        json!({"type": "beam", "range": 20.0})
    );
    let error =
        merge(json!({"actors": {"zapper": {"attack": {"type": "beam", "rnage": 20.0}}}})).expect_err("typo accepted");
    assert!(error.contains("actors.zapper.attack.rnage is not a key"), "{error}");
    let error = merge(json!({"movement": {"type": "fast"}})).expect_err("tag on an untagged object accepted");
    assert!(error.contains("movement.type is not a key"), "{error}");
}

#[test]
fn content_stays_with_the_map_and_registry_keys_with_the_defaults() {
    let merged = merge(json!({"quests": [{"id": "gold"}], "grounds": null})).expect("content accepted");
    assert_eq!(merged["quests"], json!([{"id": "gold"}]));
    assert_eq!(merged["grounds"], Value::Null);
    let error = merge(json!({"network": {"server_hz": 60}})).expect_err("global key accepted");
    assert!(error.contains("network is global"), "{error}");
    let mut defaults = defaults();
    defaults["quests"] = json!([]);
    let error = merge_map_settings(&defaults, &json!({})).expect_err("content default accepted");
    assert!(format!("{error:#}").contains("quests is map content"), "{error:#}");
}
