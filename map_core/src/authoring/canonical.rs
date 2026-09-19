use super::{FACES, cell_error, ladders_overlap, normalize_map, record_key};
use crate::{geometry, values::*};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn dedupe_by_key(values: &[Value], kind: &str) -> Vec<Value> {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| cmp(&record_key(kind, a), &record_key(kind, b)));
    sorted.dedup_by(|a, b| cmp(&record_key(kind, a), &record_key(kind, b)).is_eq());
    sorted
}
fn dedupe_cells(values: &[Value]) -> Vec<Value> {
    let mut by_key = BTreeMap::new();
    for value in values {
        by_key.insert((i(value, "row"), i(value, "col")), value.clone());
    }
    by_key.into_values().collect()
}
fn dedupe_edges(values: &[Value]) -> Vec<Value> {
    let mut by_key = BTreeMap::new();
    for value in values {
        let key = geometry::edge(value);
        let mut v = value.clone();
        for (name, n) in ["c0", "r0", "c1", "r1"].into_iter().zip(key) {
            v[name] = json!(n);
        }
        by_key.insert(key, v);
    }
    by_key.into_values().collect()
}
fn positions(values: &[Value]) -> BTreeSet<[i32; 2]> {
    values
        .iter()
        .map(|v| [i(v, "col") as i32, i(v, "row") as i32])
        .collect()
}
pub fn enforce_ramp_floor_rules(data: &mut Value) {
    let ramps = list(data, "ramps").to_vec();
    for ramp in ramps {
        let lower = i(&ramp, "lower_level");
        let upper = lower + 1;
        if lower < 0 || upper >= list(data, "levels").len() as i64 {
            continue;
        }
        let cells: BTreeSet<_> = geometry::cells(&ramp).into_iter().collect();
        if cells.is_empty() {
            continue;
        }
        let existing = positions(list(&data["levels"][lower as usize], "floors"));
        let mut floors = list(&data["levels"][lower as usize], "floors").to_vec();
        for [col, row] in &cells {
            if !existing.contains(&[*col, *row]) {
                let mut floor = json!({"col":col,"row":row});
                for face in FACES {
                    floor[face] = get(&ramp, face, json!(""));
                }
                floors.push(floor);
            }
        }
        data["levels"][lower as usize]["floors"] = json!(floors);
        for (level, key) in [
            (lower, "inaccessible_floors"),
            (upper, "floors"),
            (upper, "inaccessible_floors"),
        ] {
            data["levels"][level as usize][key] = json!(
                list(&data["levels"][level as usize], key)
                    .iter()
                    .filter(|v| !cells.contains(&[i(v, "col") as i32, i(v, "row") as i32]))
                    .cloned()
                    .collect::<Vec<_>>()
            );
        }
    }
}
pub fn canonicalize_map(v: &Value) -> Value {
    let mut b = normalize_map(v);
    if let Some(f) = b.get_mut("fireworks").and_then(Value::as_object_mut) {
        f.remove("switch_inverted");
    }
    let mut ramps = list(&b, "ramps").to_vec();
    ramps.sort_by(|a, b| {
        cmp(
            &json!([a["lower_level"], a["low"], a["high"]]),
            &json!([b["lower_level"], b["low"], b["high"]]),
        )
    });
    b["ramps"] = json!(ramps);
    enforce_ramp_floor_rules(&mut b);
    for (key, kind) in [
        ("actor_spawn_zones", "actor_zone"),
        ("checkpoints", "checkpoint"),
        ("pressure_plates", "pressure_plate"),
    ] {
        if key == "checkpoints" {
            for v in b[key]
                .as_array_mut()
                .expect("checkpoints missing from the normalized map")
            {
                if whole(&v["number"]) && int(&v["number"]) == 0 {
                    v["type"] = json!("individual");
                }
            }
        }
        b[key] = json!(dedupe_by_key(list(&b, key), kind));
    }
    let cols = i(&b, "grid_cols");
    let rows = i(&b, "grid_rows");
    let count = list(&b, "levels").len();
    for index in 0..count {
        let ramp_cells = geometry::cells_on_level(list(&b, "ramps"), index as i64);
        let level = &mut b["levels"][index];
        let terrain: Vec<_> = dedupe_cells(list(level, "terrain"))
            .into_iter()
            .filter(|v| !ramp_cells.contains(&[i(v, "col") as i32, i(v, "row") as i32]))
            .collect();
        let terrain_keys = positions(&terrain);
        let floors: Vec<_> = dedupe_cells(list(level, "floors"))
            .into_iter()
            .filter(|v| !terrain_keys.contains(&[i(v, "col") as i32, i(v, "row") as i32]))
            .collect();
        let floor_keys = positions(&floors);
        let blocked: Vec<_> = dedupe_cells(list(level, "inaccessible_floors"))
            .into_iter()
            .filter(|v| {
                let p = [i(v, "col") as i32, i(v, "row") as i32];
                !floor_keys.contains(&p) && !terrain_keys.contains(&p)
            })
            .collect();
        level["floors"] = json!(floors);
        level["terrain"] = json!(terrain);
        level["inaccessible_floors"] = json!(blocked);
        for key in ["walls", "erasers"] {
            level[key] = json!(dedupe_edges(list(level, key)));
        }
        let walls: BTreeSet<_> = list(level, "walls").iter().map(geometry::edge).collect();
        level["barriers"] = json!(
            dedupe_edges(list(level, "barriers"))
                .into_iter()
                .filter(|v| !walls.contains(&geometry::edge(v)))
                .collect::<Vec<_>>()
        );
        level["light_bridges"] = json!(dedupe_cells(list(level, "light_bridges")));
        let mut lights = BTreeMap::new();
        for light in list(level, "lights") {
            let col = i(light, "col");
            let row = i(light, "row");
            if col >= 0
                && col < cols
                && row >= 0
                && row < rows
                && !ramp_cells.contains(&[col as i32, row as i32])
                && geometry::wall_endpoints(col as i32, row as i32, s(light, "side"))
                    .is_ok_and(|edge| walls.contains(&edge))
            {
                lights.insert((row, col, s(light, "side").to_owned()), light.clone());
            }
        }
        level["lights"] = json!(lights.into_values().collect::<Vec<_>>());
    }
    let mut items = BTreeMap::new();
    for item in list(&b, "items") {
        let level = i(item, "level");
        let col = i(item, "col");
        let row = i(item, "row");
        if level >= 0 && (level as usize) < count && cell_error(&b, level, col, row, false).is_none() {
            items.insert((level, row, col), item.clone());
        }
    }
    b["items"] = json!(items.into_values().collect::<Vec<_>>());
    let mut ladders: Vec<_> = list(&b, "ladders")
        .iter()
        .filter(|v| {
            let c = i(v, "col");
            let r = i(v, "row");
            let l = i(v, "lower_level");
            let span = i(v, "levels");
            c >= 0
                && c < cols
                && r >= 0
                && r < rows
                && l >= 0
                && span >= 1
                && l + span < count as i64
                && ["N", "S", "E", "W"].contains(&s(v, "side"))
        })
        .cloned()
        .collect();
    ladders.sort_by(|a, b| cmp(&record_key("ladder", a), &record_key("ladder", b)));
    let mut kept = vec![];
    for ladder in ladders {
        if !kept.iter().any(|other| ladders_overlap(&ladder, other)) {
            kept.push(ladder);
        }
    }
    b["ladders"] = json!(kept);
    let mut nested = BTreeMap::new();
    for entry in list(&b, "nested_maps") {
        let ends = [point(&entry["from"]), point(&entry["to"])];
        if ends.iter().all(|[c, r]| *c >= 0 && *c < cols && *r >= 0 && *r < rows)
            && [i(entry, "level"), i(entry, "to_level")]
                .iter()
                .all(|n| *n >= 0 && *n < count as i64)
            && crate::is_valid_map_name(s(entry, "map"))
        {
            nested.insert((i(entry, "level"), point(&entry["from"])), entry.clone());
        }
    }
    let mut nested: Vec<_> = nested.into_values().collect();
    nested.sort_by(|a, b| cmp(&record_key("nested_map", a), &record_key("nested_map", b)));
    b["nested_maps"] = json!(nested);
    b
}
