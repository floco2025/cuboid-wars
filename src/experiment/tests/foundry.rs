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

#[test]
fn foundry_course_crosses_the_gallery_and_starts_the_fireworks() {
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
    assert_eq!(crossings.len(), 1, "the gallery's fall out of the band");
    // The run off the gallery comes out of the band as lift.
    let velocity = &crossings[0]["velocity_after"];
    assert!(velocity[0].as_f64().expect("fling") > 24.0, "{velocity}");
    assert!(velocity[1].as_f64().expect("lift") > 0.0, "{velocity}");
    let last = report["steps"].as_array().expect("steps").len() - 1;
    assert_eq!(
        report["steps"][last]["state"]["active_switches"],
        json!(["gallery", "finish"])
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
