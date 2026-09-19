//! Grid geometry, independent of the renderer and editor widgets.
use crate::values::*;
use anyhow::{Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn normalized_wall([a, b, c, d]: [i32; 4]) -> [i32; 4] {
    if (c, d) < (a, b) { [c, d, a, b] } else { [a, b, c, d] }
}
pub fn ramp_rect(low: [i32; 2], high: [i32; 2]) -> [i32; 4] {
    [
        low[0].min(high[0]),
        low[1].min(high[1]),
        low[0].max(high[0]),
        low[1].max(high[1]),
    ]
}
pub fn ramp_cells(low: [i32; 2], high: [i32; 2]) -> Vec<[i32; 2]> {
    let [a, b, c, d] = ramp_rect(low, high);
    (a..c).flat_map(|col| (b..d).map(move |row| [col, row])).collect()
}
pub fn wall_endpoints(col: i32, row: i32, side: &str) -> Result<[i32; 4]> {
    Ok(match side {
        "N" => [col, row, col + 1, row],
        "S" => [col, row + 1, col + 1, row + 1],
        "W" => [col, row, col, row + 1],
        "E" => [col + 1, row, col + 1, row + 1],
        _ => bail!("unknown side {side:?}"),
    })
}
pub fn edge(entry: &Value) -> [i32; 4] {
    normalized_wall([
        i(entry, "c0") as i32,
        i(entry, "r0") as i32,
        i(entry, "c1") as i32,
        i(entry, "r1") as i32,
    ])
}
pub fn cells(ramp: &Value) -> Vec<[i32; 2]> {
    ramp_cells(
        point(&ramp["low"]).map(|n| n as i32),
        point(&ramp["high"]).map(|n| n as i32),
    )
}
pub fn cells_on_level(ramps: &[Value], level: i64) -> BTreeSet<[i32; 2]> {
    ramps
        .iter()
        .filter(|r| i(r, "lower_level") == level || i(r, "lower_level") + 1 == level)
        .flat_map(cells)
        .collect()
}
pub fn ramp_error(low: [i32; 2], high: [i32; 2], lower: i64, cols: i64, rows: i64, levels: i64) -> Option<String> {
    let within = |p: [i32; 2]| p[0] >= 0 && i64::from(p[0]) <= cols && p[1] >= 0 && i64::from(p[1]) <= rows;
    let message = if lower < 0 || lower + 1 >= levels {
        "lower_level must have an upper level"
    } else if !within(low) {
        "low point is outside the grid-line bounds"
    } else if !within(high) {
        "high point is outside the grid-line bounds"
    } else if low[0] == high[0] || low[1] == high[1] {
        "ramp must span a non-empty rectangular footprint"
    } else if (high[0] - low[0]).abs() == (high[1] - low[1]).abs() {
        "ramp needs one clear longer axis"
    } else {
        return None;
    };
    Some(message.into())
}
pub fn ramp_axis(ramp: &Value) -> &'static str {
    let a = point(&ramp["low"]);
    let b = point(&ramp["high"]);
    let dx = b[0] - a[0];
    let dz = b[1] - a[1];
    if dx.abs() > dz.abs() {
        if dx > 0 { "east" } else { "west" }
    } else if dz > 0 {
        "south"
    } else {
        "north"
    }
}
pub fn landing_edges(low: [i32; 2], high: [i32; 2]) -> Vec<(char, i32, i32)> {
    let [c0, r0, c1, r1] = ramp_rect(low, high);
    if (high[0] - low[0]).abs() > (high[1] - low[1]).abs() {
        let col = if high[0] > low[0] { c1 } else { c0 };
        (r0..r1).map(|row| ('v', row, col)).collect()
    } else {
        let row = if high[1] > low[1] { r1 } else { r0 };
        (c0..c1).map(|col| ('h', row, col)).collect()
    }
}
pub fn overlap(a: [i64; 4], b: [i64; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}
pub fn rect(value: &Value) -> [i64; 4] {
    [int(&value[0]), int(&value[1]), int(&value[2]), int(&value[3])]
}
pub fn zone_rect(v: &Value) -> [i64; 4] {
    [
        int(&v["cols"][0]),
        int(&v["rows"][0]),
        int(&v["cols"][1]),
        int(&v["rows"][1]),
    ]
}
pub fn nested_rest(entry: &Value, width: f64) -> [[f64; 2]; 2] {
    let mut out = [[0.0; 2]; 2];
    for (index, end) in ["from", "to"].iter().enumerate() {
        let anchor = &entry[*end];
        let nudge = &entry[format!("{end}_nudge")];
        out[index] = [number(&anchor[0]), number(&anchor[1])];
        if array(nudge).len() == 3 {
            out[index][0] += number(&nudge[0]) * width;
            out[index][1] += number(&nudge[2]) * width;
        }
    }
    out
}
pub fn nested_footprints(entry: &Value, shape: &Value, width: f64) -> [[f64; 4]; 2] {
    nested_rest(entry, width).map(|[x, y]| {
        [
            x,
            y,
            x + shape.get("grid_cols").map(number).unwrap_or(1.0),
            y + shape.get("grid_rows").map(number).unwrap_or(1.0),
        ]
    })
}
#[derive(Clone, Copy, Deserialize)]
pub struct FloorNeighbors {
    pub n: bool,
    pub s: bool,
    pub e: bool,
    pub w: bool,
    pub nw: bool,
    pub ne: bool,
    pub sw: bool,
    pub se: bool,
}

#[derive(Clone, Copy, Deserialize)]
pub struct RampLandings {
    pub n: bool,
    pub s: bool,
    pub e: bool,
    pub w: bool,
}

// Diagonal slabs already extend east/west. Suppress the overlapping north/south
// strip and fill its remaining gap; ramp landings must stay flush with the slope.
pub fn floor_rectangles(bounds: [f64; 4], pad: f64, n: FloorNeighbors, landing: RampLandings) -> Vec<[f64; 4]> {
    let [x0, z0, x_end, z_end] = bounds;
    let x1 = if n.w || landing.w { x0 } else { x0 - pad };
    let x2 = if n.e || landing.e { x_end } else { x_end + pad };
    let z1 = if n.n || n.nw || n.ne || landing.n { z0 } else { z0 - pad };
    let z2 = if n.s || n.sw || n.se || landing.s {
        z_end
    } else {
        z_end + pad
    };
    let mut out = vec![[x1, z1, x2, z2]];
    if pad <= 0.0 {
        return out;
    }
    for (neighbor, west, east, blocked, start, end) in [
        (n.n, n.nw, n.ne, landing.n, z0 - pad, z0),
        (n.s, n.sw, n.se, landing.s, z_end, z_end + pad),
    ] {
        if !neighbor && (west || east) && !blocked {
            let left = if west { x0 + pad } else { x1 };
            let right = if east { x_end - pad } else { x2 };
            if right > left {
                out.push([left, start, right, end]);
            }
        }
    }
    out
}

pub fn dispatch(op: &str, a: &Value) -> Result<Value> {
    let p = |idx| point(&a[idx]).map(|n| n as i32);
    Ok(match op {
        "normalized_wall" => json!(normalized_wall(rect(&a[0]).map(|n| n as i32))),
        "wall_endpoints_for_cell_side" => json!(wall_endpoints(
            int(&a[0]) as i32,
            int(&a[1]) as i32,
            a[2].as_str().unwrap_or("")
        )?),
        "ramp_rect" => json!(ramp_rect(
            point(&a[0]["low"]).map(|n| n as i32),
            point(&a[0]["high"]).map(|n| n as i32)
        )),
        "ramp_cells" => json!(cells(&a[0])),
        "ramp_cells_on_level" => json!(cells_on_level(array(&a[0]), int(&a[1]))),
        "ramp_axis" => json!(ramp_axis(&a[0])),
        "ramp_error" => json!(ramp_error(p(0), p(1), int(&a[2]), int(&a[3]), int(&a[4]), int(&a[5]))),
        "zone_rect" => json!(zone_rect(&a[0])),
        "rects_overlap" => json!(overlap(rect(&a[0]), rect(&a[1]))),
        "grid_point_in_bounds" => {
            json!(int(&a[0]) >= 0 && int(&a[0]) <= int(&a[2]) && int(&a[1]) >= 0 && int(&a[1]) <= int(&a[3]))
        }
        "nested_map_rest_points" => json!(nested_rest(&a[0], number(&a[1]))),
        "nested_map_footprints" => json!(nested_footprints(&a[0], &a[1], number(&a[2]))),
        "nested_map_starts_at_end_2" => json!(s(&a[0], "motion") == "follow_switch" && truth(&a[0]["switch_inverted"])),
        "ramp_landing_edges" => json!(
            list(&a[0], "levels")
                .iter()
                .enumerate()
                .map(|(level, _)| list(&a[0], "ramps")
                    .iter()
                    .filter(|r| i(r, "lower_level") + 1 == level as i64)
                    .flat_map(|r| landing_edges(
                        point(&r["low"]).map(|n| n as i32),
                        point(&r["high"]).map(|n| n as i32)
                    ))
                    .collect::<BTreeSet<_>>())
                .collect::<Vec<_>>()
        ),
        _ => bail!("Unknown map geometry operation: {op}"),
    })
}
