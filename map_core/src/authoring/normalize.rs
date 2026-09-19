use super::{FACES, TERRAIN_FACES, level_lists};
use crate::values::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn select(v: &Value, keys: &[&str]) -> Value {
    let mut out = json!({});
    for k in keys {
        if let Some(value) = v.get(k) {
            out[k] = value.clone();
        }
    }
    out
}
fn coords(v: &Value, keys: &[&str]) -> Value {
    let mut out = json!({});
    for k in keys {
        out[k] = v.get(k).cloned().unwrap_or(json!(0));
    }
    out
}
fn merge(mut a: Value, b: Value) -> Value {
    if let (Some(a), Some(b)) = (a.as_object_mut(), b.as_object()) {
        a.extend(b.clone());
    }
    a
}
pub fn empty_level(index: i64) -> Value {
    let mut v = json!({"name":format!("Level {index}")});
    for key in level_lists() {
        v[key] = json!([]);
    }
    v
}
pub fn level_label(v: &Value, index: i64) -> String {
    if truth(&v["name"]) {
        format!("Level {index} ({})", s(v, "name"))
    } else {
        format!("Level {index}")
    }
}
pub fn empty_map(cols: i64, rows: i64) -> Value {
    json!({"grid_cols":cols,"grid_rows":rows,"actor_spawn_zones":[],"checkpoints":[{"level":0,"cols":[0,cols.min(2)],"rows":[0,rows.min(2)],"type":"individual","number":0}],"items":[],"pressure_plates":[],"levels":[empty_level(0)],"ramps":[],"ladders":[],"nested_maps":[]})
}
pub fn expand_materials(v: &Value, faces: &[&str]) -> Value {
    let fallback = v
        .get("all")
        .filter(|v| !v.is_null())
        .or_else(|| faces.iter().find_map(|f| v.get(f)))
        .cloned()
        .unwrap_or(json!(""));
    let mut out = json!({});
    for face in faces {
        out[face] = get(v, face, fallback.clone());
    }
    out
}
pub fn compact_materials(v: &Value, faces: &[&str]) -> Value {
    let mut counts = BTreeMap::<String, usize>::new();
    for face in faces {
        if let Some(value) = v.get(face) {
            *counts.entry(value.to_string()).or_default() += 1;
        }
    }
    let Some((best, count)) = counts.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0))) else {
        return json!({});
    };
    if *count <= 1 {
        return select(v, faces);
    }
    let value: Value = serde_json::from_str(best).expect("counted material value is not JSON");
    let mut out = json!({"all":value});
    for face in faces {
        if let Some(v) = v.get(face)
            && v != &value
        {
            out[face] = v.clone();
        }
    }
    out
}
// Only an absent side takes the default; any other authored value survives for validation to report.
fn side(v: &Value) -> Value {
    match v.get("side") {
        None => json!("N"),
        Some(Value::String(side)) => json!(side.to_uppercase()),
        Some(other) => other.clone(),
    }
}
pub fn normalize_record(kind: &str, v: &Value) -> Value {
    let controls = select(v, &["switch", "switch_inverted"]);
    match kind {
        "floor" | "terrain" => merge(
            coords(v, &["col", "row"]),
            expand_materials(v, if kind == "terrain" { &TERRAIN_FACES } else { &FACES }),
        ),
        "wall" => merge(coords(v, &["c0", "r0", "c1", "r1"]), expand_materials(v, &FACES)),
        "eraser" => coords(v, &["c0", "r0", "c1", "r1"]),
        "barrier" | "light_bridge" => {
            let mut out = coords(
                v,
                if kind == "barrier" {
                    &["c0", "r0", "c1", "r1"]
                } else {
                    &["col", "row"]
                },
            );
            out["kind"] = get(v, "kind", json!(""));
            merge(out, controls)
        }
        "ramp" => merge(select(v, &["low", "high", "lower_level"]), expand_materials(v, &FACES)),
        "ladder" => {
            let mut out = coords(v, &["lower_level", "col", "row"]);
            out["levels"] = get(v, "levels", json!(1));
            out["side"] = side(v);
            out
        }
        "light" => {
            let mut out = coords(v, &["col", "row"]);
            out["side"] = side(v);
            out["kind"] = get(v, "kind", json!(""));
            out
        }
        "nested_map" => {
            let mut out = coords(v, &["level"]);
            out["map"] = get(v, "map", json!(""));
            for k in ["from", "to"] {
                out[k] = get(v, k, json!([0, 0]));
            }
            out["to_level"] = v
                .get("to_level")
                .filter(|v| !v.is_null())
                .cloned()
                .unwrap_or(out["level"].clone());
            for (key, default) in [("travel_secs", 2.0), ("pause_secs", 0.0), ("phase_secs", 0.0)] {
                out[key] = get(v, key, json!(default));
            }
            for key in ["from_nudge", "to_nudge"] {
                out[key] = get(v, key, json!([0.0, 0.0, 0.0]));
            }
            out["motion"] = get(v, "motion", json!("cycle"));
            merge(out, controls)
        }
        "actor_spawn_zone" | "checkpoint" => {
            let mut out = coords(v, &["level"]);
            for k in ["cols", "rows"] {
                out[k] = v.get(k).filter(|v| truth(v)).cloned().unwrap_or(json!([0, 0]));
            }
            if kind == "checkpoint" {
                out["type"] = get(v, "type", json!(""));
                if let Some(n) = v.get("number") {
                    out["number"] = n.clone();
                }
            } else {
                out["kind"] = get(v, "kind", json!(""));
                out["count"] = get(v, "count", json!([]));
                for (key, default) in [("levels", 1.0), ("roam_distance", 0.0)] {
                    if let Some(value) = v.get(key)
                        && (number(value) != default || !value.is_number())
                    {
                        out[key] = value.clone();
                    }
                }
                if let Some(value) = v.get("respawn_secs") {
                    out["respawn_secs"] = value.clone();
                }
                out = merge(out, controls);
                if let Some(value) = v.get("until_checkpoint").filter(|v| !v.is_null()) {
                    out["until_checkpoint"] = value.clone();
                    out["on_checkpoint"] = get(v, "on_checkpoint", json!("stop"));
                }
            }
            out
        }
        "item" => {
            let mut out = coords(v, &["level", "col", "row"]);
            out["type"] = get(v, "type", json!(""));
            if v["type"] == "key" || v.get("kind").is_some() {
                out["kind"] = get(v, "kind", json!(""));
            }
            out
        }
        "pressure_plate" => merge(coords(v, &["level", "col", "row"]), select(v, &["switch"])),
        _ => v.clone(),
    }
}
pub fn normalize_map(v: &Value) -> Value {
    let mut out = select(v, &["switches", "barrier_kinds", "bridge_kinds", "fireworks"]);
    out["grid_cols"] = get(v, "grid_cols", json!(20));
    out["grid_rows"] = get(v, "grid_rows", json!(20));
    for (key, kind) in [
        ("actor_spawn_zones", "actor_spawn_zone"),
        ("checkpoints", "checkpoint"),
        ("items", "item"),
        ("pressure_plates", "pressure_plate"),
        ("ramps", "ramp"),
        ("ladders", "ladder"),
        ("nested_maps", "nested_map"),
    ] {
        out[key] = json!(
            list(v, key)
                .iter()
                .map(|r| normalize_record(kind, r))
                .collect::<Vec<_>>()
        );
    }
    let mut levels = vec![];
    for (index, level) in list(v, "levels").iter().enumerate() {
        let mut next = empty_level(index as i64);
        if truth(&level["name"]) {
            next["name"] = level["name"].clone();
        }
        for key in level_lists() {
            let kind = match key {
                "inaccessible_floors" => "floor",
                "terrain" => "terrain",
                _ => key.trim_end_matches('s'),
            };
            next[key] = json!(
                list(level, key)
                    .iter()
                    .map(|v| normalize_record(kind, v))
                    .collect::<Vec<_>>()
            );
        }
        levels.push(next);
    }
    if levels.is_empty() {
        levels.push(empty_level(0));
    }
    out["levels"] = json!(levels);
    if let Some(definitions) = v.get("nested_geometry") {
        out["nested_geometry"] = json!({});
        if let Some(defs) = definitions.as_object() {
            for (name, data) in defs {
                out["nested_geometry"][name] = normalize_map(data);
            }
        }
    }
    out
}
pub fn actor_count_error(count: &Value) -> Option<String> {
    let Some(values) = count.as_array() else {
        return Some("Count must be a list, for example [3] or [2, 3, 4].".into());
    };
    if values.is_empty() {
        return Some("Count needs at least one entry.".into());
    }
    if values
        .iter()
        .any(|v| v.as_u64().is_none_or(|n| n > u64::from(u32::MAX)))
    {
        return Some(format!("Count entries must be whole numbers from 0 to {}.", u32::MAX));
    }
    if values.windows(2).any(|w| int(&w[0]) > int(&w[1])) {
        return Some("Counts must stay the same or increase as players join.".into());
    }
    None
}
pub fn actor_count_key(v: &Value) -> Value {
    if actor_count_error(v).is_some() {
        json!([2, py_type(v), repr(v)])
    } else {
        json!([0, v])
    }
}
fn numeric_key(v: &Value) -> Value {
    if v.is_number() && number(v).is_finite() {
        json!([0, v])
    } else {
        json!([1, py_type(v), repr(v)])
    }
}
fn control_key(v: &Value) -> Value {
    json!([
        py_type(v),
        if v.is_string() || v.is_boolean() {
            v.clone()
        } else {
            json!(repr(v))
        }
    ])
}
pub fn record_key(kind: &str, v: &Value) -> Value {
    match kind {
        "ladder" => json!([v["lower_level"], v["row"], v["col"], v["side"], v["levels"]]),
        "nested_map" => json!([v["level"], v["from"], v["to_level"], v["to"], v["map"]]),
        "light" => json!([v["row"], v["col"], v["side"]]),
        "pressure_plate" => json!([v["level"], v["row"], v["col"], get(v, "switch", json!(""))]),
        "checkpoint" => json!([
            numeric_key(&v["number"]),
            v["level"],
            v["rows"][0],
            v["cols"][0],
            v["rows"][1],
            v["cols"][1],
            v["type"]
        ]),
        "actor_zone" => json!([
            v["level"],
            numeric_key(&get(v, "levels", json!(1))),
            v["rows"][0],
            v["cols"][0],
            v["rows"][1],
            v["cols"][1],
            v["kind"],
            actor_count_key(&v["count"]),
            control_key(&v["switch"]),
            control_key(&get(v, "switch_inverted", json!(false))),
            numeric_key(&get(v, "roam_distance", json!(0.0))),
            if v["respawn_secs"].is_null() {
                json!([0])
            } else {
                json!([1, numeric_key(&v["respawn_secs"])])
            },
            if v["until_checkpoint"].is_null() {
                json!([0])
            } else {
                json!([1, numeric_key(&v["until_checkpoint"])])
            },
            control_key(&get(v, "on_checkpoint", json!("stop")))
        ]),
        _ => v.clone(),
    }
}
