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
fn slope_is_judged_against_the_character_motor() {
    let steep = ramp_slope(3.4, 4.4);
    assert!(!steep.climbable && (steep.degrees - 52.3).abs() < 0.1, "{steep:?}");
    let walkable = ramp_slope(6.8, 4.4);
    assert!(
        walkable.climbable && (walkable.degrees - 32.9).abs() < 0.1,
        "{walkable:?}"
    );
    assert!((walkable.limit_degrees - 45.0).abs() < 0.001);
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
