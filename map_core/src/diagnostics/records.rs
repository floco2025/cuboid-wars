use super::validation::*;
use crate::values::*;
use crate::{authoring::cell_error, geometry};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub(super) fn zone_rect(zone: &Value, label: &str, data: &Value, errors: &mut Errors) {
    let level = i(zone, "level");
    let count = list(data, "levels").len() as i64;
    if level < 0 || level >= count {
        errors.add(format!("{label} has an invalid level {level}"));
    }
    let span = get(zone, "levels", json!(1));
    if !whole(&span) || int(&span) < 1 || level + int(&span) > count {
        errors.add(format!("{label} has an invalid level span {}", repr(&span)));
    }
    let [c0, r0, c1, r1] = geometry::zone_rect(zone);
    if c1 <= c0 || r1 <= r0 {
        errors.add(format!(
            "{label} has an empty range cols={} rows={}",
            repr(&zone["cols"]),
            repr(&zone["rows"])
        ));
    }
    if c0 < 0 || c1 > i(data, "grid_cols") || r0 < 0 || r1 > i(data, "grid_rows") {
        errors.add(format!(
            "{label} is outside the grid: cols={} rows={}",
            repr(&zone["cols"]),
            repr(&zone["rows"])
        ));
    }
}
pub(super) fn zone_course(zone: &Value, label: &str, numbers: Option<&BTreeSet<i64>>, errors: &mut Errors) {
    let until = &zone["until_checkpoint"];
    if !until.is_null() {
        if !whole(until) || int(until) < 1 {
            errors.add(format!("{label} until_checkpoint must be a positive whole number"));
        } else if numbers.is_some_and(|numbers| !numbers.contains(&int(until))) {
            errors.add(format!("{label} until_checkpoint {} names no checkpoint", int(until)));
        }
    }
    if zone["on_checkpoint"].is_null() {
        return;
    }
    if !["stop", "destroy"].contains(&s(zone, "on_checkpoint")) {
        errors.add(format!("{label} on_checkpoint must be one of stop, destroy"));
    } else if until.is_null() {
        errors.add(format!("{label} on_checkpoint needs an until_checkpoint"));
    }
}
pub(super) fn checkpoints(data: &Value, errors: &mut Errors) {
    for (idx, zone) in list(data, "checkpoints").iter().enumerate() {
        let label = format!("checkpoints[{idx}]");
        errors.locate("checkpoints", zone, None);
        zone_rect(zone, &label, data, errors);
        if !["individual", "group_any", "group_all"].contains(&s(zone, "type")) {
            errors.add(format!(
                "{label} has an unknown checkpoint type {}",
                repr(&zone["type"])
            ));
        }
        if !whole(&zone["number"]) || int(&zone["number"]) < 0 {
            errors.add(format!("{label} needs a whole `number` of at least 0"));
        }
        let level = i(zone, "level");
        if level < 0 || level >= list(data, "levels").len() as i64 {
            continue;
        }
        let floors: BTreeSet<_> = list(&data["levels"][level as usize], "floors")
            .iter()
            .chain(list(&data["levels"][level as usize], "terrain"))
            .map(|f| [i(f, "col") as i32, i(f, "row") as i32])
            .collect();
        let ramps = geometry::cells_on_level(list(data, "ramps"), level);
        let [c0, r0, c1, r1] = geometry::zone_rect(zone);
        if (c0..c1).any(|c| {
            (r0..r1).any(|r| {
                let p = [c as i32, r as i32];
                !floors.contains(&p) || ramps.contains(&p)
            })
        }) {
            errors.add(format!("{label} requires flat accessible floor throughout"));
        }
        if list(data, "checkpoints")[..idx]
            .iter()
            .any(|o| i(o, "level") == level && geometry::overlap(geometry::zone_rect(zone), geometry::zone_rect(o)))
        {
            errors.add(format!("{label} overlaps another checkpoint"));
        }
    }
}
pub(super) fn pressure_plates(data: &Value, context: &Value, errors: &mut Errors) {
    let mut seen = BTreeSet::new();
    for (idx, plate) in list(data, "pressure_plates").iter().enumerate() {
        let label = format!("pressure_plates[{idx}]");
        errors.locate("pressure_plates", plate, None);
        let level = i(plate, "level");
        let col = i(plate, "col");
        let row = i(plate, "row");
        if level < 0 || level >= list(data, "levels").len() as i64 {
            errors.add(format!("{label} has an invalid level {level}"));
            continue;
        }
        if col < 0 || col >= i(data, "grid_cols") || row < 0 || row >= i(data, "grid_rows") {
            errors.add(format!("{label} [{col}, {row}] is outside the grid"));
            continue;
        }
        if !truth(&plate["switch"]) {
            errors.add(format!("{label} has no switch"));
        } else if check_kind(&plate["switch"], &context["switches"]) {
            errors.add(format!(
                "{label} has unknown switch {}; known: [{}]",
                repr(&plate["switch"]),
                known(&context["switches"])
            ));
        }
        if list(&data["levels"][level as usize], "light_bridges")
            .iter()
            .any(|b| i(b, "col") == col && i(b, "row") == row)
        {
            errors.add(format!("{label} [{col}, {row}] sits on a light bridge"));
        }
        if let Some(error) = cell_error(data, level, col, row, true) {
            errors.add(format!("{label} {error}"));
        }
        if !seen.insert((level, col, row)) {
            errors.add(format!("{label} duplicates a plate at level {level} [{col}, {row}]"));
        }
    }
}
pub(super) fn items(data: &Value, context: &Value, errors: &mut Errors) {
    let mut seen = BTreeSet::new();
    for (idx, item) in list(data, "items").iter().enumerate() {
        let label = format!("items[{idx}]");
        errors.locate("items", item, None);
        let level = i(item, "level");
        let col = i(item, "col");
        let row = i(item, "row");
        if level < 0 || level >= list(data, "levels").len() as i64 {
            errors.add(format!("{label} has an invalid level {level}"));
            continue;
        }
        if col < 0 || col >= i(data, "grid_cols") || row < 0 || row >= i(data, "grid_rows") {
            errors.add(format!("{label} [{col}, {row}] is outside the grid"));
            continue;
        }
        let kind = s(item, "type");
        if kind == "key" {
            if !truth(&item["kind"]) || check_kind(&item["kind"], &context["barrier_kinds"]) {
                errors.add(format!(
                    "{label} has unknown key kind {}; known: [{}]",
                    repr(&item["kind"]),
                    known(&context["barrier_kinds"])
                ));
            }
        } else if !item_type(kind) {
            errors.add(format!("{label} has unknown type {}; known: [single_shot, multi_shot, missile_pack, portal_gun, health_potion, speed, low_gravity, gold, key]",repr(&item["type"])));
        } else if !item["kind"].is_null() {
            errors.add(format!(
                "{label} ({kind}) must not have `kind` — only key items take one"
            ));
        }
        if let Some(error) = cell_error(data, level, col, row, false) {
            errors.add(format!("{label} {error}"));
        }
        if !seen.insert((level, col, row)) {
            errors.add(format!("{label} duplicates an item at level {level} [{col}, {row}]"));
        }
    }
}
