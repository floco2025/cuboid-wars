use super::block;
use crate::{
    authoring::{EDGE_LISTS, GLOBAL_LISTS, ZONE_LISTS, empty_level, level_lists},
    geometry,
    values::*,
};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn record_rect(name: &str, v: &Value) -> [i64; 4] {
    if ZONE_LISTS.contains(&name) {
        return geometry::zone_rect(v);
    }
    if EDGE_LISTS.contains(&name) {
        let a = i(v, "c0");
        let b = i(v, "r0");
        let c = i(v, "c1");
        let d = i(v, "r1");
        return [a.min(c), b.min(d), a.max(c), b.max(d)];
    }
    if ["ramps", "nested_maps"].contains(&name) {
        let (a, b) = if name == "ramps" {
            (point(&v["low"]), point(&v["high"]))
        } else {
            (point(&v["from"]), point(&v["to"]))
        };
        let extra = i64::from(name == "nested_maps");
        return [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[0].max(b[0]) + extra,
            a[1].max(b[1]) + extra,
        ];
    }
    [i(v, "col"), i(v, "row"), i(v, "col") + 1, i(v, "row") + 1]
}
pub fn record_levels(v: &Value, level: Option<i64>) -> [i64; 2] {
    if let Some(l) = level {
        return [l, l];
    }
    if v.get("lower_level").is_some() {
        let l = i(v, "lower_level");
        return [l, l + v.get("levels").map(int).unwrap_or(1)];
    }
    let l = i(v, "level");
    if v.get("cols").is_some() && v.get("rows").is_some() {
        let span = v.get("levels").filter(|n| whole(n) && int(n) > 0).map(int).unwrap_or(1);
        return [l, l + span - 1];
    }
    let end = v.get("to_level").map(int).unwrap_or(l);
    [l.min(end), l.max(end)]
}
pub fn records(data: &Value) -> Vec<(Option<usize>, &str, &Value)> {
    let mut out = vec![];
    for (index, level) in list(data, "levels").iter().enumerate() {
        for name in level_lists() {
            for entry in list(level, name) {
                out.push((Some(index), name, entry));
            }
        }
    }
    for name in GLOBAL_LISTS {
        for entry in list(data, name) {
            out.push((None, name, entry));
        }
    }
    out
}
pub fn lists_mut(data: &mut Value, mut f: impl FnMut(&str, &mut Vec<Value>)) {
    if let Some(levels) = data["levels"].as_array_mut() {
        for level in levels {
            for name in level_lists() {
                if let Some(entries) = level[name].as_array_mut() {
                    f(name, entries);
                }
            }
        }
    }
    for name in GLOBAL_LISTS {
        if let Some(entries) = data[name].as_array_mut() {
            f(name, entries);
        }
    }
}
fn translate_entry(name: &str, v: &Value, dc: i64, dr: i64, dl: i64) -> Value {
    let mut moved = v.clone();
    if ZONE_LISTS.contains(&name) {
        for (key, delta) in [("cols", dc), ("rows", dr)] {
            moved[key] = json!(array(&v[key]).iter().map(|n| int(n) + delta).collect::<Vec<_>>());
        }
    } else if EDGE_LISTS.contains(&name) {
        for (key, delta) in [("c0", dc), ("c1", dc), ("r0", dr), ("r1", dr)] {
            moved[key] = json!(i(v, key) + delta);
        }
    } else if ["ramps", "nested_maps"].contains(&name) {
        for key in if name == "ramps" {
            ["low", "high"]
        } else {
            ["from", "to"]
        } {
            let p = point(&v[key]);
            moved[key] = json!([p[0] + dc, p[1] + dr]);
        }
    } else {
        moved["col"] = json!(i(v, "col") + dc);
        moved["row"] = json!(i(v, "row") + dr);
    }
    for key in ["level", "lower_level", "to_level"] {
        if v.get(key).is_some() {
            moved[key] = json!(i(v, key) + dl);
        }
    }
    moved
}
fn translate_map(data: &Value, dc: i64, dr: i64, dl: i64) -> Value {
    let mut moved = data.clone();
    lists_mut(&mut moved, |name, entries| {
        for e in entries {
            *e = translate_entry(name, e, dc, dr, dl);
        }
    });
    moved
}
fn resize_map(data: &Value, cols: i64, rows: i64, dc: i64, dr: i64) -> Value {
    let mut moved = translate_map(data, dc, dr, 0);
    moved["grid_cols"] = json!(cols);
    moved["grid_rows"] = json!(rows);
    lists_mut(&mut moved, |name, entries| {
        entries.retain_mut(|entry| {
            let [mut a, mut b, mut c, mut d] = record_rect(name, entry);
            if ZONE_LISTS.contains(&name) {
                a = a.max(0);
                b = b.max(0);
                c = c.min(cols);
                d = d.min(rows);
                if a >= c || b >= d {
                    return false;
                }
                entry["cols"] = json!([a, c]);
                entry["rows"] = json!([b, d]);
            }
            a >= 0 && b >= 0 && a <= c && c <= cols && b <= d && d <= rows
        });
    });
    moved
}
fn remap_levels(data: &Value, pivot: i64, remove: bool) -> Value {
    let mut moved = data.clone();
    for name in GLOBAL_LISTS {
        let mut kept = vec![];
        for v in list(data, name) {
            let mut e = v.clone();
            let [lower, upper] = record_levels(&e, None);
            if ZONE_LISTS.contains(&name) && lower <= pivot && pivot <= upper {
                let span = e.get("levels").map(int).unwrap_or(1);
                if remove {
                    if span == 1 {
                        continue;
                    }
                    e["levels"] = json!(span - 1);
                    kept.push(e);
                    continue;
                } else if lower < pivot {
                    e["levels"] = json!(span + 1);
                }
            }
            if remove && lower <= pivot && pivot <= upper {
                continue;
            }
            if name == "ladders" && lower < pivot && pivot <= upper {
                e["levels"] = json!(i(&e, "levels") + 1);
            }
            for key in ["level", "lower_level", "to_level"] {
                if let Some(value) = e.get(key)
                    && int(value) >= pivot
                {
                    e[key] = json!(int(value) + if remove { -1 } else { 1 });
                }
            }
            kept.push(e);
        }
        moved[name] = json!(kept);
    }
    moved
}
fn without_level(data: &Value, removed: usize) -> Value {
    let mut moved = remap_levels(data, removed as i64, true);
    if let Some(levels) = moved["levels"].as_array_mut()
        && removed < levels.len()
    {
        levels.remove(removed);
    }
    moved
}
fn edit_levels(data: &Value, levels: &[Value]) -> Result<Value> {
    if levels.is_empty() {
        bail!("A map needs at least one level.");
    }
    let kept: Vec<_> = levels.iter().filter(|v| !v[0].is_null()).map(|v| int(&v[0])).collect();
    let unique: BTreeSet<_> = kept.iter().copied().collect();
    if unique.len() != kept.len() || kept.iter().any(|n| *n < 0 || *n >= list(data, "levels").len() as i64) {
        bail!("Existing levels must be unique and within the map.");
    }
    let mut after = data.clone();
    for idx in (0..list(data, "levels").len()).rev() {
        if !unique.contains(&(idx as i64)) {
            after = without_level(&after, idx);
        }
    }
    let remaining: BTreeMap<_, _> = unique.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let positions: BTreeMap<_, _> = levels
        .iter()
        .enumerate()
        .filter(|(_, v)| !v[0].is_null())
        .map(|(idx, v)| (remaining[&int(&v[0])] as i64, idx as i64))
        .collect();
    let remap = |l: i64| {
        if l >= remaining.len() as i64 {
            l + levels.len() as i64 - remaining.len() as i64
        } else {
            *positions.get(&l).unwrap_or(&l)
        }
    };
    for name in GLOBAL_LISTS {
        let mut entries = vec![];
        for v in list(&after, name) {
            let mut e = v.clone();
            let [lower, upper] = record_levels(&e, None);
            if name == "ramps" && remap(upper) != remap(lower) + 1 {
                continue;
            }
            let span = get(&e, "levels", json!(1));
            if ZONE_LISTS.contains(&name) && whole(&span) && int(&span) > 0 {
                let mut covered = vec![remap(lower), remap(upper)];
                covered.extend(
                    positions
                        .iter()
                        .filter(|(old, _)| lower <= **old && **old <= upper)
                        .map(|(_, new)| *new),
                );
                let min = *covered.iter().min().expect("moved span covers no level");
                let max = *covered.iter().max().expect("moved span covers no level");
                e["level"] = json!(min);
                if e.get("levels").is_some() {
                    e["levels"] = json!(max - min + 1);
                }
            } else if name == "ladders" && i(&e, "levels") > 0 {
                e["lower_level"] = json!(remap(lower).min(remap(upper)));
                e["levels"] = json!((remap(upper) - remap(lower)).abs());
            } else {
                for key in ["level", "lower_level", "to_level"] {
                    if e.get(key).is_some() {
                        e[key] = json!(remap(i(&e, key)));
                    }
                }
            }
            entries.push(e);
        }
        after[name] = json!(entries);
    }
    after["levels"] = json!(
        levels
            .iter()
            .enumerate()
            .map(|(idx, v)| {
                let mut level = if v[0].is_null() {
                    empty_level(idx as i64)
                } else {
                    after["levels"][remaining[&int(&v[0])]].clone()
                };
                let name = v[1].as_str().unwrap_or("").trim();
                level["name"] = json!(if name.is_empty() {
                    format!("Level {idx}")
                } else {
                    name.into()
                });
                level
            })
            .collect::<Vec<_>>()
    );
    Ok(after)
}
fn content_bounds(data: &Value, shapes: &Value, wall: f64, floor: f64) -> Value {
    let mut rectangles = vec![];
    let mut spans = vec![];
    for (level, name, entry) in records(data) {
        rectangles.push(record_rect(name, entry).map(|n| n as f64));
        let span = record_levels(entry, level.map(|n| n as i64));
        spans.push(span.map(|n| n as f64));
        if name == "nested_maps" {
            let shape = &shapes[s(entry, "map")];
            rectangles.extend(geometry::nested_footprints(entry, shape, wall));
            for (end, key) in [("from", "level"), ("to", "to_level")] {
                let nudge = &entry[format!("{end}_nudge")];
                let base = i(entry, key) as f64
                    + if array(nudge).len() == 3 {
                        number(&nudge[1]) * floor
                    } else {
                        0.0
                    };
                let height = shape.get("level_count").map(number).unwrap_or(1.0);
                spans.push([base.floor(), (base + height).ceil() - 1.0]);
            }
        }
    }
    let trim = |low: f64, high: f64, size: i64| {
        let start = (low.floor() as i64).clamp(0, (size - 1).max(0));
        [start, ((high.ceil() as i64).min(size)).max(start + 1)]
    };
    let bounds = |lo: usize, hi: usize, size| {
        trim(
            rectangles.iter().map(|r| r[lo]).reduce(f64::min).unwrap_or(0.0),
            rectangles.iter().map(|r| r[hi]).reduce(f64::max).unwrap_or(1.0),
            size,
        )
    };
    let [left, right] = bounds(0, 2, i(data, "grid_cols"));
    let [top, bottom] = bounds(1, 3, i(data, "grid_rows"));
    let [first, end] = trim(
        spans.iter().map(|s| s[0]).reduce(f64::min).unwrap_or(0.0),
        spans.iter().map(|s| s[1] + 1.0).reduce(f64::max).unwrap_or(1.0),
        list(data, "levels").len() as i64,
    );
    json!({"rect":[left,top,right,bottom],"first_level":first,"last_level":end-1})
}
pub fn dispatch(op: &str, a: &Value) -> Result<Value> {
    Ok(match op {
        "record_rect" => json!(record_rect(a[0].as_str().unwrap_or(""), &a[1])),
        "record_levels" => json!(record_levels(&a[0], a[1].as_i64())),
        "translate_entry" => translate_entry(a[0].as_str().unwrap_or(""), &a[1], int(&a[2]), int(&a[3]), int(&a[4])),
        "translate_map" => translate_map(&a[0], int(&a[1]), int(&a[2]), int(&a[3])),
        "resize_map_offset" => resize_map(&a[0], int(&a[1]), int(&a[2]), int(&a[3]), int(&a[4])),
        "remap_levels" => remap_levels(&a[0], int(&a[1]), truth(&a[2])),
        "map_content_bounds" => content_bounds(&a[0], &a[1], number(&a[2]), number(&a[3])),
        "insert_level_data" => {
            let idx = int(&a[1]);
            if !truth(&a[2]) && list(&a[0], "ramps").iter().any(|v| i(v, "lower_level") + 1 == idx) {
                bail!("The inserted level separates ramp endpoints.");
            }
            let mut after = remap_levels(&a[0], idx, false);
            after["ramps"] = json!(
                list(&after, "ramps")
                    .iter()
                    .filter(|v| i(v, "lower_level") + 1 != idx)
                    .cloned()
                    .collect::<Vec<_>>()
            );
            let levels = after["levels"]
                .as_array_mut()
                .expect("levels missing from the edited map");
            levels.insert((idx.max(0) as usize).min(levels.len()), empty_level(idx));
            after
        }
        "remove_level_data" => {
            if list(&a[0], "levels").len() <= 1 {
                bail!("A map needs at least one level.");
            }
            without_level(&a[0], int(&a[1]) as usize)
        }
        "edit_levels_data" => edit_levels(&a[0], array(&a[1]))?,
        "transform_block" => block::transform(&a[0], a[1].as_str().unwrap_or(""), &a[2])?,
        _ => bail!("Unknown transform: {op}"),
    })
}
