use super::validation::*;
use crate::{authoring::ladders_overlap, values::*};
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub fn placed_definitions(root: &Value, definitions: &Value) -> Value {
    let mut placed = json!({});
    let mut pending: Vec<_> = list(root, "nested_maps")
        .iter()
        .filter_map(|v| v["map"].as_str())
        .collect();
    while let Some(name) = pending.pop() {
        if placed.get(name).is_some() {
            continue;
        }
        if let Some(data) = definitions.get(name) {
            placed[name] = data.clone();
            pending.extend(list(data, "nested_maps").iter().filter_map(|v| v["map"].as_str()));
        }
    }
    placed
}
pub fn nested_cycle(edited: Option<&str>, entries: &[Value], shapes: &Value) -> Option<Vec<String>> {
    fn visit(
        name: &str,
        shapes: &Value,
        chain: &mut Vec<String>,
        checked: &mut BTreeSet<String>,
    ) -> Option<Vec<String>> {
        if let Some(idx) = chain.iter().position(|v| v == name) {
            return Some(chain[idx..].iter().cloned().chain([name.into()]).collect());
        }
        if checked.contains(name) {
            return None;
        }
        let shape = shapes.get(name)?;
        chain.push(name.into());
        for child in list(shape, "nested_names").iter().filter_map(Value::as_str) {
            if let Some(cycle) = visit(child, shapes, chain, checked) {
                return Some(cycle);
            }
        }
        chain.pop();
        checked.insert(name.into());
        None
    }
    let mut checked = BTreeSet::new();
    let mut chain = vec![edited.unwrap_or("(this map)").into()];
    for entry in entries {
        if let Some(cycle) = visit(s(entry, "map"), shapes, &mut chain, &mut checked) {
            return Some(cycle);
        }
    }
    None
}
pub(super) fn ladders(data: &Value, errors: &mut Errors) {
    let count = list(data, "levels").len() as i64;
    for (idx, ladder) in list(data, "ladders").iter().enumerate() {
        errors.locate("ladders", ladder, None);
        let label = format!("ladders[{idx}]");
        let col = i(ladder, "col");
        let row = i(ladder, "row");
        let lower = i(ladder, "lower_level");
        let span = i(ladder, "levels");
        if col < 0 || col >= i(data, "grid_cols") || row < 0 || row >= i(data, "grid_rows") {
            errors.add(format!("{label} [{col}, {row}] is outside the grid"));
        }
        if !["N", "S", "E", "W"].contains(&s(ladder, "side")) {
            errors.add(format!("{label} has invalid side {}", repr(&ladder["side"])));
        }
        if span < 1 {
            errors.add(format!("{label} must span at least 1 storey"));
        }
        if lower < 0 || lower + span >= count {
            errors.add(format!(
                "{label} spans levels {lower}..{} but the map has {count} level(s)",
                lower + span
            ));
        }
        for (other_idx, other) in list(data, "ladders")[..idx].iter().enumerate() {
            if ladders_overlap(ladder, other) {
                errors.add(format!("{label} overlaps ladders[{other_idx}] on the same edge"));
            }
        }
    }
}
pub(super) fn validate(data: &Value, context: &Value, errors: &mut Errors) {
    let count = list(data, "levels").len() as i64;
    let mut seen = BTreeSet::new();
    for (idx, entry) in list(data, "nested_maps").iter().enumerate() {
        errors.locate("nested_maps", entry, None);
        let label = format!("nested_maps[{idx}]");
        let name = s(entry, "map");
        if !crate::is_valid_geometry_name(name) {
            errors.add(format!(
                "{label} map name {} must be nonempty with no surrounding spaces",
                repr(&entry["map"])
            ));
        } else if !context["map_name"].is_null() && context["map_name"] == entry["map"] {
            errors.add(format!("{label} nests the edited map itself"));
        } else if !context["nested_shapes"].is_null() && context["nested_shapes"].get(name).is_none_or(Value::is_null) {
            errors.add(format!(
                "{label} names {}, but its named geometry is missing from this parent map’s nested_geometry",
                repr(&entry["map"])
            ));
        }
        let level = i(entry, "level");
        let to = i(entry, "to_level");
        let start = point(&entry["from"]);
        let end = point(&entry["to"]);
        if level < 0 || level >= count || to < 0 || to >= count {
            errors.add(format!(
                "{label} spans levels {level}..{to} but the map has {count} level(s)"
            ));
        }
        if [start, end]
            .iter()
            .any(|[c, r]| *c < 0 || *c >= i(data, "grid_cols") || *r < 0 || *r >= i(data, "grid_rows"))
        {
            errors.add(format!(
                "{label} {}->{} is outside the grid",
                repr(&entry["from"]),
                repr(&entry["to"])
            ));
        }
        if !number(&entry["travel_secs"]).is_finite() || number(&entry["travel_secs"]) <= 0.0 {
            errors.add(format!("{label} needs a positive travel time"));
        }
        if !nonnegative(&entry["pause_secs"]) || !nonnegative(&entry["phase_secs"]) {
            errors.add(format!("{label} has a negative pause or phase, or a non-finite value"));
        }
        if !["cycle", "follow_switch"].contains(&s(entry, "motion")) {
            errors.add(format!("{label} motion must be cycle or follow_switch"));
        } else if entry["motion"] == "follow_switch" && !truth(&entry["switch"]) {
            errors.warn(format!("{label} Follow switch motion has no switch, so it never moves"));
        }
        for key in ["from_nudge", "to_nudge"] {
            let nudge = &entry[key];
            if array(nudge).len() != 3 || array(nudge).iter().any(|v| !v.is_number() || !number(v).is_finite()) {
                errors.add(format!("{label} {key} is not three numbers"));
            }
        }
        switch_target(entry, &label, context, errors);
        if !seen.insert((level, start)) {
            errors.add(format!(
                "{label} duplicates a nested map starting at level {level} {}",
                repr(&entry["from"])
            ));
        }
    }
    if !context["nested_shapes"].is_null()
        && let Some(cycle) = nested_cycle(
            context["map_name"].as_str(),
            list(data, "nested_maps"),
            &context["nested_shapes"],
        )
    {
        errors.add(format!("nested maps loop: {}", cycle.join(" -> ")));
    }
}
