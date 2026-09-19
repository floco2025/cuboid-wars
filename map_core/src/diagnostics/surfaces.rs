use super::validation::*;
use crate::{
    authoring::{FACES, TERRAIN_FACES, level_label},
    geometry,
    values::*,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub(super) fn validate(data: &Value, context: &Value, errors: &mut Errors) {
    let cols = i(data, "grid_cols");
    let rows = i(data, "grid_rows");
    let inside = |c: i64, r: i64| c >= 0 && c < cols && r >= 0 && r < rows;
    let point_inside = |c: i64, r: i64| c >= 0 && c <= cols && r >= 0 && r <= rows;
    for (level_idx, level) in list(data, "levels").iter().enumerate() {
        let prefix = level_label(level, level_idx as i64);
        let mut floors = BTreeSet::new();
        let mut blocked = BTreeSet::new();
        let mut terrain = BTreeSet::new();
        for (key, label) in [
            ("floors", "floor"),
            ("inaccessible_floors", "inaccessible floor"),
            ("terrain", "terrain"),
        ] {
            for tile in list(level, key) {
                errors.locate(key, tile, Some(level_idx as i64));
                let c = i(tile, "col");
                let r = i(tile, "row");
                let p = [c as i32, r as i32];
                if !inside(c, r) {
                    errors.add(format!("{prefix}: {label} [{c}, {r}] is outside the grid"));
                }
                if key == "inaccessible_floors" && floors.contains(&p)
                    || key == "terrain" && (floors.contains(&p) || blocked.contains(&p))
                {
                    errors.add(format!("{prefix}: {label} [{c}, {r}] overlaps a floor"));
                }
                let cells = match key {
                    "floors" => &mut floors,
                    "inaccessible_floors" => &mut blocked,
                    _ => &mut terrain,
                };
                if !cells.insert(p) {
                    errors.add(format!(
                        "{prefix}: {label} [{c}, {r}] duplicates another {label}{}",
                        if key == "terrain" { " floor" } else { "" }
                    ));
                }
            }
        }
        let walls: BTreeSet<_> = list(level, "walls").iter().map(geometry::edge).collect();
        for (key, singular) in [("walls", "wall"), ("barriers", "barrier"), ("erasers", "eraser")] {
            let mut seen = BTreeSet::new();
            for (idx, entry) in list(level, key).iter().enumerate() {
                errors.locate(key, entry, Some(level_idx as i64));
                let label = if key == "walls" {
                    format!("{prefix}: wall")
                } else {
                    format!("{prefix}: {singular}[{idx}]")
                };
                let [a, b, c, d] = geometry::edge(entry);
                let coordinates = format!("[{a}, {b}, {c}, {d}]");
                if !point_inside(i64::from(a), i64::from(b)) || !point_inside(i64::from(c), i64::from(d)) {
                    errors.add(format!("{label} {coordinates} is outside the grid-line bounds"));
                }
                if (c - a).abs() + (d - b).abs() != 1 {
                    errors.add(format!("{label} {coordinates} is not one grid edge"));
                }
                if key == "barriers" {
                    if !truth(&entry["field"]) || check_kind(&entry["field"], &context["fields"]) {
                        errors.add(format!(
                            "{label} has unknown field {}; known: [{}]",
                            repr(&entry["field"]),
                            known(&context["fields"])
                        ));
                    }
                    if walls.contains(&[a, b, c, d]) {
                        errors.add(format!("{label} {coordinates} overlaps a wall"));
                    }
                }
                if !seen.insert([a, b, c, d]) {
                    errors.add(format!("{label} {coordinates} duplicates another {singular}"));
                }
            }
        }
        let ramps = geometry::terrain_excluded_cells(list(data, "ramps"), level_idx as i64);
        for [c, r] in terrain.intersection(&ramps) {
            errors.locate("terrain", &json!({"col":c,"row":r}), Some(level_idx as i64));
            errors.add(format!("{prefix}: terrain [{c}, {r}] sits on a ramp"));
        }
        let slab: BTreeSet<_> = floors.union(&blocked).copied().chain(terrain).collect();
        let mut bridges = BTreeSet::new();
        for (idx, bridge) in list(level, "light_bridges").iter().enumerate() {
            errors.locate("light_bridges", bridge, Some(level_idx as i64));
            let label = format!("{prefix}: light_bridge[{idx}]");
            let c = i(bridge, "col");
            let r = i(bridge, "row");
            let p = [c as i32, r as i32];
            if !inside(c, r) {
                errors.add(format!("{label} [{c}, {r}] is outside the grid"));
            }
            if !truth(&bridge["field"]) || check_kind(&bridge["field"], &context["fields"]) {
                errors.add(format!(
                    "{label} has unknown field {}; known: [{}]",
                    repr(&bridge["field"]),
                    known(&context["fields"])
                ));
            }
            if slab.contains(&p) {
                errors.add(format!("{label} [{c}, {r}] sits on a floor"));
            }
            if ramps.contains(&p) {
                errors.add(format!("{label} [{c}, {r}] sits on a ramp"));
            }
            if !bridges.insert(p) {
                errors.add(format!("{label} [{c}, {r}] duplicates another light bridge"));
            }
        }
        for light in list(level, "lights") {
            errors.locate("lights", light, Some(level_idx as i64));
            let c = i(light, "col");
            let r = i(light, "row");
            let side = s(light, "side");
            if s(light, "kind").trim().is_empty() {
                errors.add(format!("{prefix}: light kind must not be empty"));
            }
            if check_kind(&light["kind"], &context["wall_light_kinds"]) {
                errors.add(format!(
                    "{prefix}: light has unknown kind {}; known: [{}]",
                    repr(&light["kind"]),
                    known(&context["wall_light_kinds"])
                ));
            }
            if !inside(c, r) {
                errors.add(format!("{prefix}: light [{c}, {r}, {side}] is outside the grid"));
                continue;
            }
            let Ok(edge) = geometry::wall_endpoints(c as i32, r as i32, side) else {
                errors.add(format!("{prefix}: light [{c}, {r}, {side}] has invalid side"));
                continue;
            };
            if !walls.contains(&edge) {
                errors.add(format!("{prefix}: light [{c}, {r}, {side}] has no wall on that side"));
            }
        }
    }
}
fn face_aliases(entry: &Value, label: &str, faces: &[&str], aliases: &Value, errors: &mut Errors) {
    for face in faces {
        let value = &entry[face];
        if !value.is_null() && !contains(aliases, value) {
            errors.add(format!("{label}: face {} value {} is not an alias; add it to the host map’s textures in its settings.json or choose an available alias",repr(&json!(face)),repr(value)));
        }
    }
}
pub(super) fn materials(data: &Value, context: &Value, errors: &mut Errors) {
    let aliases = &context["material_aliases"];
    if aliases.is_null() {
        return;
    }
    for (level_idx, level) in list(data, "levels").iter().enumerate() {
        let prefix = level_label(level, level_idx as i64);
        for (name, label) in [
            ("floors", "floor"),
            ("inaccessible_floors", "inaccessible_floor"),
            ("terrain", "terrain"),
            ("walls", "wall"),
        ] {
            for entry in list(level, name) {
                errors.locate(name, entry, Some(level_idx as i64));
                let coords = if name == "walls" {
                    repr(&json!(geometry::edge(entry)))
                } else {
                    format!("[{}, {}]", i(entry, "col"), i(entry, "row"))
                };
                face_aliases(
                    entry,
                    &format!("{prefix}: {label} {coords}"),
                    if name == "terrain" { &TERRAIN_FACES } else { &FACES },
                    aliases,
                    errors,
                );
            }
        }
    }
    for ramp in list(data, "ramps") {
        errors.locate("ramps", ramp, None);
        face_aliases(
            ramp,
            &format!(
                "ramp cols={} rows={} (level {})",
                repr(&ramp["cols"]),
                repr(&ramp["rows"]),
                i(ramp, "lower_level")
            ),
            &FACES,
            aliases,
            errors,
        );
    }
}
