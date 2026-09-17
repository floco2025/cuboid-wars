use std::{
    collections::{HashMap, HashSet},
    fs, mem,
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};

use super::{
    schema::{MapDef, MapFile, MapSource},
    validation::{canonicalize, validate_checkpoint_course, validate_map},
};
use crate::config::is_valid_map_name;

pub(crate) type LoadedMaps = HashMap<String, MapDef>;

pub(crate) fn load_map(path: &Path) -> Result<MapSource> {
    let text = fs::read_to_string(path).with_context(|| format!("reading map at {}", path.display()))?;
    let file: MapFile =
        serde_json::from_str(&text).with_context(|| format!("parsing map JSON at {}", path.display()))?;
    prepare_source(file.map).with_context(|| format!("validating map at {}", path.display()))
}

fn prepare_source(mut root: MapDef) -> Result<MapSource> {
    let switches = mem::take(&mut root.switches);
    let barrier_kinds = mem::take(&mut root.barrier_kinds);
    let bridge_kinds = mem::take(&mut root.bridge_kinds);
    let fireworks = root.fireworks.take();
    let mut nested_geometry = mem::take(&mut root.nested_geometry);
    validate_map(&root)?;
    canonicalize(&mut root);
    for (name, geometry) in &mut nested_geometry {
        ensure!(is_valid_map_name(name), "invalid nested_geometry name {name:?}");
        ensure!(
            geometry.switches.is_empty()
                && geometry.barrier_kinds.is_empty()
                && geometry.bridge_kinds.is_empty()
                && geometry.fireworks.is_none()
                && geometry.nested_geometry.is_empty(),
            "nested geometry {name:?} defines switches, barrier_kinds, bridge_kinds, fireworks, or nested_geometry, which only the root map defines"
        );
        validate_map(geometry).with_context(|| format!("nested geometry {name:?}"))?;
        canonicalize(geometry);
    }
    let mut checked = HashSet::new();
    visit(&root, &nested_geometry, &mut Vec::new(), &mut checked)?;
    let used = checked.clone();
    for name in nested_geometry.keys() {
        visit_named(name, &nested_geometry, &mut Vec::new(), &mut checked)?;
    }
    // Unplaced definitions must not contribute pressure plates or checkpoints to compilation.
    nested_geometry.retain(|name, _| used.contains(name));
    validate_checkpoint_course(&root, &nested_geometry)?;
    Ok(MapSource {
        geometry: root,
        nested_geometry,
        switches,
        barrier_kinds,
        bridge_kinds,
        fireworks,
    })
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
#[path = "tests/load.rs"]
mod tests;
