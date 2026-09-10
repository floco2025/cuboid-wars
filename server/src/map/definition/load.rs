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
    // Unplaced definitions must not contribute pressure plates to compilation.
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
#[path = "tests/load.rs"]
mod tests;
