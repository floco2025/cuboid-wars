use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail, ensure};

use super::{
    schema::{MapDef, MapFile},
    validation::{canonicalize, validate_map},
};
use common::config::MapGeometryConfig;

pub(crate) fn load_map(path: &Path) -> Result<MapDef> {
    let text = fs::read_to_string(path).with_context(|| format!("reading map at {}", path.display()))?;
    let mut file: MapFile =
        serde_json::from_str(&text).with_context(|| format!("parsing map JSON at {}", path.display()))?;
    validate_map(&file.map).with_context(|| format!("validating map at {}", path.display()))?;
    canonicalize(&mut file.map);
    Ok(file.map)
}

// Every map the root nests, transitively, by name; each loaded once.
pub(crate) type LoadedMaps = HashMap<String, MapDef>;

// Loads the root's nested maps depth first. A nested map takes every setting
// from the root; `geometry_of` is the registry lookup for the maps that are
// also playable on their own, whose geometry block must then equal the
// root's, so its grid aligns with the parent's cells and storeys. A name on
// the current chain is a cycle; a name already loaded is the same map nested
// twice, which is fine.
pub(crate) fn load_map_tree(
    root_name: &str,
    root: &MapDef,
    sizes: MapGeometryConfig,
    geometry_of: &dyn Fn(&str) -> Option<MapGeometryConfig>,
    load: &mut dyn FnMut(&str) -> Result<MapDef>,
) -> Result<LoadedMaps> {
    let mut loaded = LoadedMaps::new();
    let mut chain = vec![root_name.to_owned()];
    load_nested(root, sizes, geometry_of, load, &mut chain, &mut loaded)?;
    Ok(loaded)
}

fn load_nested(
    def: &MapDef,
    sizes: MapGeometryConfig,
    geometry_of: &dyn Fn(&str) -> Option<MapGeometryConfig>,
    load: &mut dyn FnMut(&str) -> Result<MapDef>,
    chain: &mut Vec<String>,
    loaded: &mut LoadedMaps,
) -> Result<()> {
    let parent = chain.last().cloned().expect("nesting chain lost its root");
    for entry in &def.nested_maps {
        let name = &entry.map;
        if chain.iter().any(|link| link == name) {
            let cycle: Vec<&str> = chain.iter().map(String::as_str).chain([name.as_str()]).collect();
            bail!("map {name:?} nests itself: {}", cycle.join(" -> "));
        }
        if loaded.contains_key(name) {
            continue;
        }
        if let Some(geometry) = geometry_of(name) {
            ensure!(
                geometry == sizes,
                "map {name:?} nested in {parent:?} has a different geometry block; a nested map shares the root's"
            );
        }
        let nested = load(name).with_context(|| format!("loading map {name:?} nested in {parent:?}"))?;
        chain.push(name.clone());
        load_nested(&nested, sizes, geometry_of, load, chain, loaded)?;
        chain.pop();
        loaded.insert(name.clone(), nested);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_geometry::sizes;

    fn map_nesting(names: &[&str]) -> MapDef {
        let text = format!(
            r#"{{"grid_cols": 2, "grid_rows": 2, "levels": [{{}}], "nested_maps": [{}]}}"#,
            names
                .iter()
                .map(|name| format!(r#"{{"map": "{name}", "level": 0, "from": [0, 0], "to": [0, 0], "speed": 1.0}}"#))
                .collect::<Vec<_>>()
                .join(", ")
        );
        serde_json::from_str(&text).expect("test map JSON is malformed")
    }

    fn tree(
        root: &MapDef,
        files: &[(&str, &[&str])],
        geometry_of: &dyn Fn(&str) -> Option<MapGeometryConfig>,
    ) -> Result<LoadedMaps> {
        let mut load = |name: &str| {
            files
                .iter()
                .find(|(file, _)| *file == name)
                .map(|(_, nested)| map_nesting(nested))
                .ok_or_else(|| anyhow::anyhow!("no file for {name:?}"))
        };
        load_map_tree("root", root, sizes(), geometry_of, &mut load)
    }

    #[test]
    fn nested_cycle_is_rejected_naming_the_chain() {
        let root = map_nesting(&["a"]);
        let error = tree(&root, &[("a", &["b"]), ("b", &["a"])], &|_| Some(sizes())).expect_err("cycle accepted");
        assert!(error.to_string().contains("root -> a -> b -> a"), "{error}");
    }

    #[test]
    fn a_nested_map_without_a_registry_entry_inherits_the_roots_geometry() {
        let root = map_nesting(&["a"]);
        let loaded = tree(&root, &[("a", &[])], &|_| None).expect("unregistered nested map rejected");
        assert!(loaded.contains_key("a"));
    }

    #[test]
    fn nested_geometry_mismatch_is_rejected() {
        let root = map_nesting(&["a"]);
        let other = MapGeometryConfig {
            grid_cell_size: sizes().grid_cell_size + 1.0,
            ..sizes()
        };
        let error = tree(&root, &[("a", &[])], &|_| Some(other)).expect_err("mismatch accepted");
        assert!(error.to_string().contains("different geometry block"), "{error}");
    }

    #[test]
    fn a_map_nested_twice_loads_once() {
        let root = map_nesting(&["a", "b"]);
        let loaded =
            tree(&root, &[("a", &["c"]), ("b", &["c"]), ("c", &[])], &|_| Some(sizes())).expect("diamond rejected");
        let mut names: Vec<&String> = loaded.keys().collect();
        names.sort();
        assert_eq!(names, ["a", "b", "c"]);
    }
}
