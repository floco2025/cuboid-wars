use crate::{geometry, values::*};
use serde_json::Value;
pub fn ladders_overlap(a: &Value, b: &Value) -> bool {
    let a_edge = geometry::wall_endpoints(i(a, "col") as i32, i(a, "row") as i32, s(a, "side"));
    let b_edge = geometry::wall_endpoints(i(b, "col") as i32, i(b, "row") as i32, s(b, "side"));
    matches!((a_edge,b_edge),(Ok(a),Ok(b)) if a==b)
        && i(a, "lower_level") < i(b, "lower_level") + i(b, "levels")
        && i(b, "lower_level") < i(a, "lower_level") + i(a, "levels")
}
pub fn cell_error(data: &Value, level: i64, col: i64, row: i64, plate: bool) -> Option<String> {
    let Some(level_data) = list(data, "levels").get(level as usize) else {
        return Some(format!("invalid level {level}"));
    };
    if plate && geometry::cells_on_level(list(data, "ramps"), level).contains(&[col as i32, row as i32]) {
        return Some(format!("[{col}, {row}] is inside a ramp footprint"));
    }
    let names = if plate {
        &["floors", "inaccessible_floors", "terrain"][..]
    } else {
        &["floors", "terrain"][..]
    };
    if !names
        .iter()
        .flat_map(|n| list(level_data, n))
        .any(|v| i(v, "col") == col && i(v, "row") == row)
    {
        return Some(format!(
            "[{col}, {row}] has no {}floor",
            if plate { "" } else { "regular " }
        ));
    }
    geometry::cells_on_level(list(data, "ramps"), level)
        .contains(&[col as i32, row as i32])
        .then(|| format!("[{col}, {row}] is inside a ramp footprint"))
}
pub fn light_error(data: &Value, level: i64, col: i64, row: i64, side: &str) -> Option<String> {
    let level_data = &data["levels"][level as usize];
    if !geometry::wall_endpoints(col as i32, row as i32, side)
        .is_ok_and(|edge| list(level_data, "walls").iter().any(|w| geometry::edge(w) == edge))
    {
        return Some(format!("No wall on the {side} side of cell [{col}, {row}]."));
    }
    if geometry::cells_on_level(list(data, "ramps"), level).contains(&[col as i32, row as i32]) {
        return Some(format!(
            "Cannot place a light inside a ramp footprint ([{col}, {row}])."
        ));
    }
    list(level_data, "lights")
        .iter()
        .any(|v| i(v, "col") == col && i(v, "row") == row && s(v, "side") == side)
        .then(|| {
            format!("There is already a light on the {side} side of cell [{col}, {row}]; right-click it to erase.")
        })
}
