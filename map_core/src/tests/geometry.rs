use super::*;
use serde_json::json;

#[test]
fn landings_sit_at_the_storey_a_ramp_arrives_at_and_follow_its_direction() {
    let one_cell = json!({"lower_level": 0, "levels": 2, "cols": [2, 3], "rows": [4, 5], "direction": "W"});
    let wide = json!({"lower_level": 0, "cols": [0, 3], "rows": [1, 2], "direction": "S"});
    let ramps = [one_cell, wide];

    assert_eq!(
        landing_edges(&ramps, 2),
        BTreeSet::from([('v', 4, 2)]),
        "a one-cell ramp rising west arrives along its west side, two storeys up"
    );
    assert_eq!(
        landing_edges(&ramps, 1),
        BTreeSet::from([('h', 2, 0), ('h', 2, 1), ('h', 2, 2)]),
        "a ramp wider than long still arrives along the side it rises toward"
    );
    assert_eq!(
        landing_edges(&ramps, 0),
        BTreeSet::from([('v', 4, 3), ('h', 1, 0), ('h', 1, 1), ('h', 1, 2)]),
        "low edges on the ramps' own level"
    );
}

#[test]
fn a_strip_beside_a_ramp_opening_runs_to_the_lip_the_arrival_slab_stays_flush_with() {
    // The cell north of an opening, with the arrival slab to its south-west.
    let neighbors = FloorNeighbors {
        n: true,
        s: false,
        e: true,
        w: true,
        nw: true,
        ne: true,
        sw: true,
        se: false,
    };
    let flush = |sw| RampLandings {
        n: false,
        s: false,
        e: false,
        w: false,
        nw: false,
        ne: false,
        sw,
        se: false,
    };
    let strip = |landing| floor_rectangles([10.0, 20.0, 14.0, 24.0], 0.2, neighbors, landing)[1];

    assert_eq!(
        strip(flush(false)),
        [10.2, 24.0, 14.0, 24.2],
        "an overhanging slab covers the corner"
    );
    assert_eq!(
        strip(flush(true)),
        [10.0, 24.0, 14.0, 24.2],
        "a flush one leaves it to the strip"
    );
}

// A 7 by 7 floor on level 0 with plates on the given cells, nothing else.
fn plated_room(plates: &[[i32; 2]]) -> Value {
    let floors: Vec<_> = (0..7)
        .flat_map(|col| (0..7).map(move |row| json!({"col": col, "row": row})))
        .collect();
    json!({
        "grid_cols": 7,
        "grid_rows": 7,
        "levels": [{"floors": floors, "walls": [], "barriers": [], "erasers": []}],
        "pressure_plates": plates
            .iter()
            .map(|[col, row]| json!({"level": 0, "col": col, "row": row, "switch": "door"}))
            .collect::<Vec<_>>(),
        "ramps": [],
        "ladders": [],
    })
}

#[test]
fn a_plate_takes_its_full_size_where_its_floor_has_room_and_shrinks_only_for_what_stands_within_it() {
    const CELL: f64 = 1.0;
    const WALL: f64 = 0.2;
    let full = f64::from(PRESSURE_PLATE_SIDE);
    let cramped = CELL - WALL - 2.0 * f64::from(PRESSURE_PLATE_GAP);
    let side = |data: &Value| pressure_plate_sides(data, CELL, WALL);
    let assert_sides = |data: &Value, expected: &[f64], what: &str| {
        let sides = side(data);
        assert_eq!(sides.len(), expected.len(), "{what}");
        for (actual, expected) in sides.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6, "{what}: {sides:?}");
        }
    };
    let open = plated_room(&[[3, 3]]);
    assert_sides(&open, &[full], "open floor");

    let mut walled = open.clone();
    walled["levels"][0]["walls"] = json!([{"c0": 3, "r0": 3, "c1": 3, "r1": 4}]);
    assert_sides(&walled, &[cramped], "a wall on its own cell's edge");
    let mut fenced = open.clone();
    fenced["levels"][0]["barriers"] = json!([{"c0": 4, "r0": 2, "c1": 4, "r1": 3, "field": "door"}]);
    assert_sides(&fenced, &[cramped], "a field between two neighbours its corner reaches");
    let mut holed = open.clone();
    holed["levels"][0]["floors"]
        .as_array_mut()
        .expect("floors missing from the room")
        .retain(|floor| floor != &json!({"col": 2, "row": 2}));
    assert_sides(&holed, &[cramped], "a diagonal neighbour without a slab");
    let mut ramped = open.clone();
    ramped["ramps"] = json!([{"lower_level": 0, "levels": 1, "cols": [4, 6], "rows": [3, 4], "direction": "E"}]);
    assert_sides(&ramped, &[cramped], "a ramp rising beside it");
    let mut laddered = open.clone();
    laddered["ladders"] = json!([{"lower_level": 0, "col": 3, "row": 3, "side": "N", "levels": 1}]);
    assert_sides(&laddered, &[cramped], "a ladder on its edge");

    assert_sides(
        &plated_room(&[[3, 3], [4, 3]]),
        &[cramped, cramped],
        "adjacent plates share the edge",
    );
    assert_sides(&plated_room(&[[3, 3], [5, 3]]), &[full, full], "a cell apart, both fit");
    assert_eq!(
        pressure_plate_sides(&walled, 4.0, WALL),
        [full],
        "a coarse cell holds the plate inside its walls"
    );
    assert_eq!(
        pressure_plate_sides(&walled, CELL, 0.9),
        [CELL / 2.0],
        "never below half a cell"
    );
}
