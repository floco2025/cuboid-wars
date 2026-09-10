use super::*;
use std::f32::consts::PI;

fn aligned(a: f32, b: f32) -> bool {
    angle_delta_radians(a, b).abs() < 1e-4
}

#[test]
fn snaps_when_within_one_step() {
    assert!(aligned(step_yaw_toward(0.0, 0.1, 0.5), 0.1));
}

#[test]
fn caps_a_large_flip_to_max_step() {
    // Target nearly opposite: a single step advances exactly `max_step`.
    assert!(aligned(step_yaw_toward(0.0, PI - 0.01, 0.2), 0.2));
}

#[test]
fn takes_the_shortest_way_around_the_wrap() {
    // current +3.0, target -3.0: shortest path is ~+0.28 across ±PI, not -6.
    let stepped = step_yaw_toward(3.0, -3.0, 0.1);
    assert!(aligned(stepped, 3.1), "advances 0.1 the short way (wrapping)");
    assert!(
        angle_delta_radians(-3.0, stepped).abs() < angle_delta_radians(-3.0, 3.0).abs(),
        "ended up closer to the target"
    );
}
