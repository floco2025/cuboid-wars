use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};

use super::{
    schema::{MapDef, MapFile, MapSource},
    validation::{canonicalize, validate_map},
};
use crate::config::is_valid_map_name;

pub(crate) type LoadedMaps = HashMap<String, MapDef>;

pub(crate) fn load_map(path: &Path) -> Result<MapSource> {
    let text = fs::read_to_string(path).with_context(|| format!("reading map at {}", path.display()))?;
    let file: MapFile =
        serde_json::from_str(&text).with_context(|| format!("parsing map JSON at {}", path.display()))?;
    prepare_source(file.map).with_context(|| format!("validating map at {}", path.display()))
}

fn prepare_source(mut source: MapSource) -> Result<MapSource> {
    validate_map(&source.geometry)?;
    canonicalize(&mut source.geometry);
    for (name, geometry) in &mut source.nested_geometry {
        ensure!(is_valid_map_name(name), "invalid nested_geometry name {name:?}");
        validate_map(geometry).with_context(|| format!("nested geometry {name:?}"))?;
        canonicalize(geometry);
    }
    let mut checked = HashSet::new();
    visit(&source.geometry, &source.nested_geometry, &mut Vec::new(), &mut checked)?;
    let used = checked.clone();
    for name in source.nested_geometry.keys() {
        visit_named(name, &source.nested_geometry, &mut Vec::new(), &mut checked)?;
    }
    // Unplaced definitions must not contribute pressure-plate purposes to compilation.
    source.nested_geometry.retain(|name, _| used.contains(name));
    Ok(source)
}

fn visit_named(
    name: &str,
    definitions: &LoadedMaps,
    chain: &mut Vec<String>,
    checked: &mut HashSet<String>,
) -> Result<()> {
    if chain.iter().any(|link| link == name) {
        let cycle: Vec<&str> = chain.iter().map(String::as_str).chain([name]).collect();
        bail!("nested maps loop: {}", cycle.join(" -> "));
    }
    if checked.contains(name) {
        return Ok(());
    }
    let geometry = definitions
        .get(name)
        .with_context(|| format!("nested map {name:?} has no named geometry in this parent's nested_geometry"))?;
    chain.push(name.to_owned());
    visit(geometry, definitions, chain, checked)?;
    chain.pop();
    checked.insert(name.to_owned());
    Ok(())
}

fn visit(
    geometry: &MapDef,
    definitions: &LoadedMaps,
    chain: &mut Vec<String>,
    checked: &mut HashSet<String>,
) -> Result<()> {
    for entry in &geometry.nested_maps {
        visit_named(&entry.map, definitions, chain, checked)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
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
        let loaded =
            source(&["a", "b"], &[("a", &["c"]), ("b", &["c"]), ("c", &[])]).expect("shared definition rejected");
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
}
