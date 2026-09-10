use super::*;
use serde_json::{Value, json};

fn geometry(names: &[&str]) -> Value {
    json!({
        "grid_cols": 4, "grid_rows": 4, "levels": [{}],
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
    prepare_source(serde_json::from_value(value).expect("test map source is invalid"))
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
fn invalid_named_geometry_is_rejected() {
    let mut value = geometry(&["room"]);
    value["nested_geometry"] = json!({"room": geometry(&[])});
    value["nested_geometry"]["room"]["grid_cols"] = json!(0);
    let error = prepare_source(serde_json::from_value(value).expect("test source is invalid"))
        .expect_err("invalid nested geometry accepted");
    assert!(format!("{error:#}").contains("room"));
}
