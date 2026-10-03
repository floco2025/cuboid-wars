use serde_json::{Value, json};

use super::{fixtures::scenario, script::Action};

fn events(report: &Value) -> impl Iterator<Item = &Value> {
    report["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .flat_map(|step| step["events"].as_array().expect("events"))
}

fn lowest_health(report: &Value) -> f64 {
    report["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .filter_map(|step| step["state"]["player"]["health"].as_f64())
        .fold(f64::INFINITY, f64::min)
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
const SHIELD_PLATE: usize = 69;

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
    // The turret sees the route only stepping onto and off the shield plate.
    assert!(lowest_health(&report) > 400.0, "{}", lowest_health(&report));
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
fn walking_from_the_shield_to_the_pen_is_lethal() {
    let (_folder, mut script) = scenario("gatehouse");
    script.actions.truncate(SHIELD_PLATE + 1);
    script.actions.extend([walk([1.0, -0.2], 60), advance(20)]);
    let report = script.run().expect("walk the firing line");
    assert!(events(&report).any(|event| event["kind"] == "player_died"));
    assert!(
        report["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .all(|step| !step["state"]["active_switches"].to_string().contains("firing")),
        "the pen's plate stays unpressed"
    );
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
