//! Grid geometry, independent of the renderer and editor widgets.
use crate::values::*;
use anyhow::{Result, bail};
use common::{constants::CHARACTER_MAX_SLOPE, protocol::RampDirection};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn normalized_wall([a, b, c, d]: [i32; 4]) -> [i32; 4] {
    if (c, d) < (a, b) { [c, d, a, b] } else { [a, b, c, d] }
}
pub fn rect_cells([c0, r0, c1, r1]: [i32; 4]) -> Vec<[i32; 2]> {
    (c0..c1).flat_map(|col| (r0..r1).map(move |row| [col, row])).collect()
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
pub fn ramp_rect(ramp: &Value) -> [i32; 4] {
    zone_rect(ramp).map(|n| n as i32)
}
pub fn cells(ramp: &Value) -> Vec<[i32; 2]> {
    rect_cells(ramp_rect(ramp))
}
// The storeys a ramp rises; a record without the key rises one.
pub fn ramp_levels(ramp: &Value) -> i64 {
    ramp.get("levels").map_or(1, int)
}
// A ramp's footprint on every level it touches: its own, the ones it passes,
// and the one it arrives at.
pub fn cells_on_level(ramps: &[Value], level: i64) -> BTreeSet<[i32; 2]> {
    ramps
        .iter()
        .filter(|r| (i(r, "lower_level")..=i(r, "lower_level") + ramp_levels(r)).contains(&level))
        .flat_map(cells)
        .collect()
}
// The footprints opened in a level's floors: those of ramps passing or arriving.
pub fn opening_cells(ramps: &[Value], level: i64) -> BTreeSet<[i32; 2]> {
    ramps
        .iter()
        .filter(|r| i(r, "lower_level") < level && level <= i(r, "lower_level") + ramp_levels(r))
        .flat_map(cells)
        .collect()
}
// Terrain never lies in a ramp's opening, nor under a solid ramp, whose wedge
// its grass would grow through; a plank leaves the ground under it.
pub fn terrain_excluded_cells(ramps: &[Value], level: i64) -> BTreeSet<[i32; 2]> {
    let mut excluded = opening_cells(ramps, level);
    excluded.extend(
        ramps
            .iter()
            .filter(|r| i(r, "lower_level") == level && s(r, "shape") != "plank")
            .flat_map(cells),
    );
    excluded
}
pub fn ramp_direction(ramp: &Value) -> Option<RampDirection> {
    serde_json::from_value(ramp["direction"].clone()).ok()
}
// Slope is never checked: an over-steep ramp is valid and merely unclimbable.
pub fn ramp_error(ramp: &Value, cols: i64, rows: i64, level_count: i64) -> Option<String> {
    let lower = i(ramp, "lower_level");
    let levels = ramp_levels(ramp);
    let [c0, r0, c1, r1] = zone_rect(ramp);
    let message = if ramp.get("levels").is_some_and(|v| !whole(v)) || levels < 1 {
        "levels must be a whole number of at least 1".into()
    } else if lower < 0 || lower + levels >= level_count {
        format!("needs level {} to arrive at", lower + levels)
    } else if c0 < 0 || r0 < 0 || c1 > cols || r1 > rows {
        "footprint is outside the grid".into()
    } else if c1 <= c0 || r1 <= r0 {
        "ramp must span a non-empty rectangular footprint".into()
    } else if ramp_direction(ramp).is_none() {
        "direction must be N, S, E, or W".into()
    } else if !matches!(
        ramp.get("shape").map_or("solid", |v| v.as_str().unwrap_or("")),
        "solid" | "plank"
    ) {
        "shape must be solid or plank".into()
    } else {
        return None;
    };
    Some(message)
}
// Cells along the direction a ramp rises.
pub fn ramp_run_cells([c0, r0, c1, r1]: [i32; 4], direction: RampDirection) -> i32 {
    match direction {
        RampDirection::East | RampDirection::West => c1 - c0,
        RampDirection::North | RampDirection::South => r1 - r0,
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RampSlope {
    pub degrees: f32,
    pub limit_degrees: f32,
    pub climbable: bool,
}
// Whether the character motor climbs a slope, judged like its ground support:
// by the surface normal against `CHARACTER_MAX_SLOPE`.
pub fn ramp_slope(run: f32, rise: f32) -> RampSlope {
    RampSlope {
        degrees: rise.atan2(run).to_degrees(),
        limit_degrees: CHARACTER_MAX_SLOPE.to_degrees(),
        climbable: run / run.hypot(rise) >= CHARACTER_MAX_SLOPE.cos(),
    }
}
// The grid edges along the side of a footprint that `side` names.
pub fn side_edges([c0, r0, c1, r1]: [i32; 4], side: RampDirection) -> Vec<(char, i32, i32)> {
    match side {
        RampDirection::North => (c0..c1).map(|col| ('h', r0, col)).collect(),
        RampDirection::South => (c0..c1).map(|col| ('h', r1, col)).collect(),
        RampDirection::West => (r0..r1).map(|row| ('v', row, c0)).collect(),
        RampDirection::East => (r0..r1).map(|row| ('v', row, c1)).collect(),
    }
}
// Where floor slabs end flush instead of overhanging: a ramp's high edge on
// the level it arrives at and its low edge on its own.
pub fn landing_edges(ramps: &[Value], level: i64) -> BTreeSet<(char, i32, i32)> {
    let mut edges = BTreeSet::new();
    for ramp in ramps {
        let Some(direction) = ramp_direction(ramp) else {
            continue;
        };
        let lower = i(ramp, "lower_level");
        if lower + ramp_levels(ramp) == level {
            edges.extend(side_edges(ramp_rect(ramp), direction));
        }
        if lower == level {
            edges.extend(side_edges(ramp_rect(ramp), direction.opposite()));
        }
    }
    edges
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

// Where a slab ends flush with a ramp: this cell's own four sides, and for
// each diagonal neighbour the side it turns toward this cell's north or south
// strip.
#[derive(Clone, Copy, Deserialize)]
pub struct RampLandings {
    pub n: bool,
    pub s: bool,
    pub e: bool,
    pub w: bool,
    pub nw: bool,
    pub ne: bool,
    pub sw: bool,
    pub se: bool,
}

// Diagonal slabs already extend east/west. Suppress the overlapping north/south
// strip and fill its remaining gap; ramp landings must stay flush with the slope.
// A diagonal slab that is itself flush there extends nothing, so the strip
// runs on to the corner instead of stopping a pad short of the ramp's lip.
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
    for (neighbor, west, east, west_flush, east_flush, blocked, start, end) in [
        (n.n, n.nw, n.ne, landing.nw, landing.ne, landing.n, z0 - pad, z0),
        (n.s, n.sw, n.se, landing.sw, landing.se, landing.s, z_end, z_end + pad),
    ] {
        if !neighbor && (west || east) && !blocked {
            let left = if west && !west_flush { x0 + pad } else { x1 };
            let right = if east && !east_flush { x_end - pad } else { x2 };
            if right > left {
                out.push([left, start, right, end]);
            }
        }
    }
    out
}

pub fn dispatch(op: &str, a: &Value) -> Result<Value> {
    Ok(match op {
        "normalized_wall" => json!(normalized_wall(rect(&a[0]).map(|n| n as i32))),
        "wall_endpoints_for_cell_side" => json!(wall_endpoints(
            int(&a[0]) as i32,
            int(&a[1]) as i32,
            a[2].as_str().unwrap_or("")
        )?),
        "ramp_cells_on_level" => json!(cells_on_level(array(&a[0]), int(&a[1]))),
        "ramp_error" => json!(ramp_error(&a[0], int(&a[1]), int(&a[2]), int(&a[3]))),
        "ramp_slope" => json!(
            ramp_direction(&a[0])
                .filter(|_| ramp_levels(&a[0]) >= 1)
                .map(|direction| {
                    ramp_slope(
                        ramp_run_cells(ramp_rect(&a[0]), direction) as f32 * number(&a[1]) as f32,
                        ramp_levels(&a[0]) as f32 * number(&a[2]) as f32,
                    )
                })
        ),
        "zone_rect" => json!(zone_rect(&a[0])),
        "rects_overlap" => json!(overlap(rect(&a[0]), rect(&a[1]))),
        "grid_point_in_bounds" => {
            json!(int(&a[0]) >= 0 && int(&a[0]) <= int(&a[2]) && int(&a[1]) >= 0 && int(&a[1]) <= int(&a[3]))
        }
        "nested_map_rest_points" => json!(nested_rest(&a[0], number(&a[1]))),
        "nested_map_footprints" => json!(nested_footprints(&a[0], &a[1], number(&a[2]))),
        "nested_map_starts_at_end_2" => json!(s(&a[0], "motion") == "follow_switch" && truth(&a[0]["switch_inverted"])),
        "ramp_landing_edges" => json!(
            (0..list(&a[0], "levels").len())
                .map(|level| landing_edges(list(&a[0], "ramps"), level as i64))
                .collect::<Vec<_>>()
        ),
        _ => bail!("Unknown map geometry operation: {op}"),
    })
}

#[cfg(test)]
#[path = "tests/geometry.rs"]
mod tests;
