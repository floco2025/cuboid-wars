use crate::{
    geometry::normalized_wall,
    schema::{MapDef, MapFile, MapSource},
};
use anyhow::{Context, Result, anyhow};
use serde_json::json;
use std::{collections::HashMap, fs, mem, path::Path};

pub type LoadedMaps = HashMap<String, MapDef>;

pub fn load_map(path: &Path) -> Result<MapSource> {
    let text = fs::read_to_string(path).with_context(|| format!("reading map at {}", path.display()))?;
    let file: MapFile =
        serde_json::from_str(&text).with_context(|| format!("parsing map JSON at {}", path.display()))?;
    prepare_source(file.map).with_context(|| format!("validating map at {}", path.display()))
}

pub fn prepare_source(mut root: MapDef) -> Result<MapSource> {
    let source = source_value(&root)?;
    let context = json!({
        "switches": root.switches.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
        "barrier_kinds": root.barrier_kinds.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
        "bridge_kinds": root.bridge_kinds.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
    });
    let issues = crate::diagnostics::validate_document(&source, &context);
    if !issues.is_empty() {
        return Err(anyhow!(
            issues
                .into_iter()
                .map(|issue| issue.message)
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    let used = crate::diagnostics::placed_definitions(&source, &source["nested_geometry"]);
    let switches = mem::take(&mut root.switches);
    let barrier_kinds = mem::take(&mut root.barrier_kinds);
    let bridge_kinds = mem::take(&mut root.bridge_kinds);
    let fireworks = root.fireworks.take();
    let mut nested_geometry = mem::take(&mut root.nested_geometry);
    canonicalize(&mut root);
    // Unplaced definitions are validated but cannot supply runtime plates or checkpoints.
    nested_geometry.retain(|name, _| used.get(name).is_some());
    for geometry in nested_geometry.values_mut() {
        canonicalize(geometry);
    }
    Ok(MapSource {
        geometry: root,
        nested_geometry,
        switches,
        barrier_kinds,
        bridge_kinds,
        fireworks,
    })
}

pub fn validate_map(map_def: &MapDef) -> Result<()> {
    let source = source_value(map_def)?;
    let context = json!({"defer_course_references": true});
    if let Some(issue) = crate::diagnostics::validate_map(&source, &context).into_iter().next() {
        return Err(anyhow!(issue.message));
    }
    Ok(())
}

pub fn canonicalize(map_def: &mut MapDef) {
    map_def.actor_spawn_zones.sort_by_cached_key(|zone| {
        crate::values::SortKey(crate::authoring::record_key(
            "actor_zone",
            &serde_json::to_value(zone).expect("source serializes"),
        ))
    });
    map_def.actor_spawn_zones.dedup();

    map_def.checkpoints.sort_by_key(|checkpoint| checkpoint.number);

    for level in &mut map_def.levels {
        level.floors.sort_by_key(|f| (f.row, f.col));
        level.floors.dedup_by_key(|f| (f.row, f.col));
        level.inaccessible_floors.sort_by_key(|f| (f.row, f.col));
        level.inaccessible_floors.dedup_by_key(|f| (f.row, f.col));
        level.terrain.sort_by_key(|f| (f.row, f.col));
        level.terrain.dedup_by_key(|f| (f.row, f.col));

        for wall in &mut level.walls {
            let [c0, r0, c1, r1] = normalized_wall([wall.c0, wall.r0, wall.c1, wall.r1]);
            wall.c0 = c0;
            wall.r0 = r0;
            wall.c1 = c1;
            wall.r1 = r1;
        }
        level.walls.sort_by_key(|w| (w.c0, w.r0, w.c1, w.r1));
        level.walls.dedup_by_key(|w| (w.c0, w.r0, w.c1, w.r1));
    }

    map_def.ramps.sort_by_key(|r| (r.lower_level, r.low, r.high));
    map_def.ramps.dedup_by_key(|r| (r.lower_level, r.low, r.high));

    map_def
        .ladders
        .sort_by_key(|l| (l.lower_level, l.row, l.col, l.side as u8, l.levels));
    map_def.ladders.dedup();

    map_def.nested_maps.sort_by(|a, b| {
        (a.motion.level, a.motion.from, a.motion.to_level(), a.motion.to, &a.map).cmp(&(
            b.motion.level,
            b.motion.from,
            b.motion.to_level(),
            b.motion.to,
            &b.map,
        ))
    });
}

fn source_value(map_def: &MapDef) -> Result<serde_json::Value> {
    fn resolve(data: &mut serde_json::Value) {
        for entry in data["nested_maps"].as_array_mut().expect("serialized map list") {
            if entry["to_level"].is_null() {
                entry["to_level"] = entry["level"].clone();
            }
        }
        if let Some(definitions) = data
            .get_mut("nested_geometry")
            .and_then(serde_json::Value::as_object_mut)
        {
            for geometry in definitions.values_mut() {
                resolve(geometry);
            }
        }
    }
    let mut value = serde_json::to_value(map_def)?;
    resolve(&mut value);
    Ok(value)
}

#[cfg(test)]
#[path = "tests/load.rs"]
mod tests;
