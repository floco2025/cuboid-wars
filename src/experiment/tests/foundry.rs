use serde_json::{Value, json};

use super::{
    fixtures::scenario,
    script::{Action, End},
};

fn events(report: &Value) -> impl Iterator<Item = &Value> {
    report["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .flat_map(|step| step["events"].as_array().expect("events"))
}

fn walk(direction: [f32; 2], ticks: u32, jump: bool) -> Action {
    Action::Move {
        direction,
        ticks,
        crouch: false,
        jump,
    }
}

fn advance(ticks: u32) -> Action {
    Action::Advance { ticks }
}

// The route's step that stands on the shaft's landing, by the hub's door.
const ON_LANDING: usize = 4;
// Where the route shoots the hall's band from: the gallery's east end, through its window.
const GALLERY_EYE: [f32; 3] = [-5.5, 14.42, -2.92];
const BAND: [f32; 3] = [-13.9, 12.6, -10.0];
// The route's step that stands in the loft beside the hatch, both portals open.
const IN_THE_LOFT: usize = 85;
// The route's step that stands in the stack's loft beside its hatch, both portals open.
const IN_THE_STACK_LOFT: usize = 135;
// The route's step that has just caught the stack's low gravity rising out of the corner.
const CAUGHT_LOW_GRAVITY: usize = 138;
// The stack's perch, where its plate stands.
const PERCH_Y: f64 = 24.0;

#[test]
fn foundry_course_crosses_all_three_courses_and_starts_the_fireworks() {
    let (_folder, script) = scenario("foundry");
    let report = script.run().expect("foundry course");
    for step in report["steps"].as_array().expect("steps") {
        match step["action"]["action"].as_str() {
            Some("check") => assert_eq!(
                step["result"]["status"], "passed",
                "step {}: {} {}",
                step["index"], step["result"], step["state"]["player"]
            ),
            Some("portal") => assert_eq!(step["result"]["status"], "submitted", "{step}"),
            _ => {}
        }
    }
    let crossings: Vec<_> = events(&report)
        .filter(|event| event["kind"] == "player_portal_crossing")
        .collect();
    assert_eq!(
        crossings.len(),
        3,
        "the gallery's fall out of the band, the hatch's out of the shallow ramp, the stack's out of the corner"
    );
    // The run off the gallery comes out of the band as lift.
    let velocity = &crossings[0]["velocity_after"];
    assert!(velocity[0].as_f64().expect("fling") > 24.0, "{velocity}");
    assert!(velocity[1].as_f64().expect("lift") > 0.0, "{velocity}");
    // The hatch's fall leaves the shallow ramp along its normal, steeply up toward the ledge.
    let velocity = &crossings[1]["velocity_after"];
    assert!(velocity[1].as_f64().expect("rise") > 20.0, "{velocity}");
    assert!(velocity[0].as_f64().expect("drift") > 5.0, "{velocity}");
    // The stack's fall leaves the corner straight up and catches the low gravity on the way.
    let velocity = &crossings[2]["velocity_after"];
    assert!(velocity[1].as_f64().expect("rise") > 20.0, "{velocity}");
    assert_eq!(
        report["steps"][CAUGHT_LOW_GRAVITY]["state"]["player"]["low_gravity"],
        true
    );
    let last = report["steps"].as_array().expect("steps").len() - 1;
    assert_eq!(
        report["steps"][last]["state"]["active_switches"],
        json!(["gallery", "slopes", "float", "finish"])
    );
    assert_eq!(
        report["steps"][last]["state"]["player"]["low_gravity"], false,
        "the stack's eraser keeps its low gravity in"
    );
    assert!(events(&report).any(|event| event["kind"] == "fireworks_started"));
    assert!(!events(&report).any(|event| event["kind"] == "player_died" || event["kind"] == "player_fall_damage"));
}

#[test]
fn the_landings_fall_throws_the_same_pair_into_the_chasm() {
    let (_folder, mut script) = scenario("foundry");
    script.actions.truncate(ON_LANDING + 1);
    script.actions.extend([
        Action::Aim {
            target: [-14.0, 1.6, 2.0],
        },
        Action::Portal { end: End::A },
        advance(4),
        Action::Place {
            end: End::B,
            eye: GALLERY_EYE,
            target: BAND,
        },
        advance(4),
        walk([1.0, 0.0], 6, false),
        advance(10),
        walk([-1.0, 0.0], 10, false),
        walk([-1.0, 0.0], 8, true),
        advance(90),
    ]);
    let report = script.run().expect("jump off the landing");
    assert!(events(&report).any(|event| event["kind"] == "player_portal_crossing"));
    let last = report["steps"].as_array().expect("steps").len() - 1;
    let player = &report["steps"][last]["state"]["player"];
    let position = &player["position"];
    assert!(position[0].as_f64().expect("x") < 8.0, "short of the ledge: {player}");
    assert!(
        (position[1].as_f64().expect("y") - 3.2).abs() < 0.1,
        "on the chasm's floor: {player}"
    );
    assert_eq!(report["steps"][last]["state"]["active_switches"], json!([]));
}

#[test]
fn the_steep_ramp_throws_the_hatchs_fall_flat_into_the_ledges_face() {
    let (_folder, mut script) = scenario("foundry");
    script.actions.truncate(IN_THE_LOFT + 1);
    script.actions.extend([
        Action::Place {
            end: End::B,
            eye: [19.0, 1.62, 16.5],
            target: [15.0, 1.6, 16.5],
        },
        advance(4),
        walk([1.0, 0.0], 15, false),
        advance(90),
    ]);
    let report = script.run().expect("drop through the hatch onto the steep ramp");
    let crossing = events(&report)
        .find(|event| event["kind"] == "player_portal_crossing")
        .expect("the hatch's fall crosses");
    let velocity = &crossing["velocity_after"];
    assert!(
        velocity[0].as_f64().expect("drift") > velocity[1].as_f64().expect("rise"),
        "flatter than 45°: {velocity}"
    );
    let last = report["steps"].as_array().expect("steps").len() - 1;
    let player = &report["steps"][last]["state"]["player"];
    assert!(
        player["position"][1].as_f64().expect("y").abs() < 0.1,
        "back on the pit floor: {player}"
    );
    assert_eq!(report["steps"][last]["state"]["active_switches"], json!(["gallery"]));
    assert!(!events(&report).any(|event| event["kind"] == "player_died"));
}

fn highest_feet(steps: &[Value]) -> f64 {
    steps
        .iter()
        .flat_map(|step| step["events"].as_array().expect("events"))
        .filter(|event| event["kind"] == "player_step")
        .filter_map(|event| event["position"][1].as_f64())
        .fold(f64::NEG_INFINITY, f64::max)
}

#[test]
fn the_stacks_fall_out_of_the_open_floor_misses_the_low_gravity_and_rises_short_of_the_perch() {
    let (_folder, mut script) = scenario("foundry");
    script.actions.truncate(IN_THE_STACK_LOFT + 1);
    script.actions.extend([
        Action::Place {
            end: End::B,
            eye: [-14.5, 6.42, 12.5],
            target: [-15.5, 4.8, 16.5],
        },
        advance(4),
        walk([-1.0, 0.0], 10, false),
        advance(150),
    ]);
    let report = script
        .run()
        .expect("drop through the stack's hatch, out of the open floor");
    assert!(events(&report).any(|event| event["kind"] == "player_portal_crossing"));
    assert!(!events(&report).any(|event| event["kind"] == "item_collected"));
    let steps = report["steps"].as_array().expect("steps");
    let highest = highest_feet(&steps[IN_THE_STACK_LOFT + 1..]);
    assert!(highest < PERCH_Y - 4.0, "{highest}");
    assert_eq!(
        steps[steps.len() - 1]["state"]["active_switches"],
        json!(["gallery", "slopes"])
    );
}

#[test]
fn a_light_jump_from_the_stacks_floor_rises_short_of_the_perch() {
    let (_folder, mut script) = scenario("foundry");
    script.actions.truncate(CAUGHT_LOW_GRAVITY + 1);
    script.actions.extend([
        advance(150),
        Action::Teleport {
            feet: [-17.5, 4.8, 16.5],
        },
        walk([0.0, -1.0], 30, true),
        advance(150),
    ]);
    let report = script.run().expect("jump light under the perch");
    let steps = report["steps"].as_array().expect("steps");
    let last = &steps[steps.len() - 1]["state"];
    assert_eq!(last["player"]["low_gravity"], true);
    let highest = highest_feet(&steps[CAUGHT_LOW_GRAVITY + 3..]);
    assert!(highest > 15.0 && highest < PERCH_Y - 4.0, "{highest}");
    assert_eq!(last["active_switches"], json!(["gallery", "slopes"]));
}
