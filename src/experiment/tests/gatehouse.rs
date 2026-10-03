use serde_json::{Value, json};

use super::{fixtures::scenario, script::Action};

fn events(report: &Value) -> impl Iterator<Item = &Value> {
    report["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .flat_map(|step| step["events"].as_array().expect("events"))
}

fn health(report: &Value, step: usize) -> f64 {
    report["steps"][step]["state"]["player"]["health"]
        .as_f64()
        .expect("player health")
}

fn walk(direction: [f32; 2], ticks: u32) -> Action {
    Action::Move {
        direction,
        ticks,
        crouch: false,
        jump: false,
    }
}

fn advance(ticks: u32) -> Action {
    Action::Advance { ticks }
}

// The route's step that stands on the Firing Line's shield plate.
const SHIELD_PLATE: usize = 71;

#[test]
fn gatehouse_course_lowers_every_gate_and_starts_the_fireworks() {
    let (_folder, script) = scenario("gatehouse");
    let report = script.run().expect("gatehouse course");
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
        4,
        "the Drop, the Cistern, and the Firing Line in and out"
    );
    // The Drop leaves the pit floor upward; the Cistern leaves the wall above the tank westward.
    assert!(crossings[0]["velocity_after"][1].as_f64().expect("drop fling") > 12.0);
    assert!(crossings[1]["velocity_after"][0].as_f64().expect("cistern fling") < -12.0);
    let last = report["steps"].as_array().expect("steps").len() - 1;
    assert_eq!(
        report["steps"][last]["state"]["active_switches"],
        json!(["drop", "cistern", "firing", "finish"])
    );
    assert!(events(&report).any(|event| event["kind"] == "fireworks_started"));
    assert!(!events(&report).any(|event| event["kind"] == "player_died" || event["kind"] == "player_fall_damage"));
    // The turret costs the route some health and no life.
    assert!(health(&report, last) < 500.0);
}

#[test]
fn the_shield_plate_stops_the_turret_while_it_is_held() {
    let (_folder, mut script) = scenario("gatehouse");
    script.actions.truncate(SHIELD_PLATE + 1);
    script.actions.extend([Action::Inspect, advance(60)]);
    let report = script.run().expect("stand on the shield plate");
    assert_eq!(
        report["steps"][SHIELD_PLATE + 1]["state"]["active_switches"],
        json!(["drop", "cistern", "shield"])
    );
    let hits = report["steps"][SHIELD_PLATE + 2]["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter(|event| event["kind"] == "player_hit")
        .count();
    assert_eq!(hits, 0, "no damage on the plate");
}

#[test]
fn walking_to_the_pen_and_back_is_lethal() {
    let (_folder, mut script) = scenario("gatehouse");
    script.actions.truncate(SHIELD_PLATE + 1);
    script.actions.extend([
        walk([0.0, -1.0], 12),
        walk([1.0, 0.0], 60),
        advance(20),
        Action::Inspect,
        walk([-1.0, 0.0], 60),
        advance(20),
    ]);
    let report = script.run().expect("walk the firing line");
    let at_the_pen = &report["steps"][SHIELD_PLATE + 4]["state"]["player"];
    assert!(at_the_pen["position"][0].as_f64().expect("x") > 24.0, "{at_the_pen}");
    assert!(at_the_pen["health"].as_f64().expect("health") < 300.0, "{at_the_pen}");
    assert!(events(&report).any(|event| event["kind"] == "player_died"));
}

#[test]
fn the_ceiling_over_the_cistern_takes_no_portal() {
    let (_folder, mut script) = scenario("gatehouse");
    script.actions.truncate(36);
    script.actions.push(Action::Probe {
        eye: None,
        targets: vec![[5.0, 12.6, -10.5], [0.0, 12.6, -10.5]],
    });
    let report = script.run().expect("probe the hall ceiling");
    let shots = &report["steps"][36]["result"]["shots"];
    assert_eq!(shots[0]["status"], "incompatible_material", "over the tank: {shots}");
    assert_eq!(shots[1]["status"], "placed", "beside it: {shots}");
}
