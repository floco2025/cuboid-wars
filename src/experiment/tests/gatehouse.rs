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

// The route's step that has just opened the portal west of the Cistern's tank.
const HALL_PORTAL: usize = 41;
// The route's step that stands on the Firing Line's shield plate.
const SHIELD_PLATE: usize = 81;
// The route's step that has just opened the portal in the Vat's ceiling.
const VAT_CEILING_PORTAL: usize = 125;

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
        5,
        "the Drop, the Cistern, the Firing Line in and out, and the Vat"
    );
    // The Drop leaves the pit floor upward; the Cistern takes the same fall up out of the hall
    // floor and toward the tank.
    assert!(crossings[0]["velocity_after"][1].as_f64().expect("drop fling") > 12.0);
    assert!(crossings[1]["velocity_after"][1].as_f64().expect("cistern fling") > 15.0);
    assert!(crossings[1]["velocity_after"][0].as_f64().expect("cistern drift") > 0.0);
    // The Vat's angled run leaves the ceiling falling and drifting toward the vat.
    assert!(crossings[4]["velocity_after"][1].as_f64().expect("vat fall") < 0.0);
    assert!(crossings[4]["velocity_after"][0].as_f64().expect("vat drift") > 3.0);
    let last = report["steps"].as_array().expect("steps").len() - 1;
    assert_eq!(
        report["steps"][last]["state"]["active_switches"],
        json!(["drop", "cistern", "firing", "vat", "finish"])
    );
    assert!(events(&report).any(|event| event["kind"] == "fireworks_started"));
    assert!(!events(&report).any(|event| event["kind"] == "player_died" || event["kind"] == "player_fall_damage"));
    // The turret sees the route only stepping onto and off the shield plate.
    assert!(lowest_health(&report) > 350.0, "{}", lowest_health(&report));
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
fn the_hall_takes_portals_only_on_its_floor_and_lowest_walls() {
    let (_folder, mut script) = scenario("gatehouse");
    script.actions.truncate(36);
    script.actions.push(Action::Probe {
        eye: None,
        targets: vec![
            [0.0, 12.6, -10.5],
            [-5.9, 10.0, -10.5],
            [-5.9, 6.2, -10.5],
            [-1.75, 4.8, -13.5],
        ],
    });
    let report = script.run().expect("probe the hall");
    let shots = &report["steps"][36]["result"]["shots"];
    let statuses: Vec<_> = (0..4).map(|index| shots[index]["status"].clone()).collect();
    assert_eq!(
        statuses,
        ["incompatible_material", "incompatible_material", "placed", "placed"],
        "ceiling, upper wall, lower wall, floor: {shots}"
    );
}

#[test]
fn a_hop_inside_the_hall_rises_short_of_the_rim() {
    let (_folder, mut script) = scenario("gatehouse");
    script.actions.truncate(HALL_PORTAL + 1);
    script.actions.extend([
        Action::Aim {
            target: [-4.85, 4.8, -5.0],
        },
        Action::Portal { end: End::A },
        advance(4),
        walk([0.0, 1.0], 6),
        Action::Move {
            direction: [0.0, 1.0],
            ticks: 12,
            crouch: false,
            jump: true,
        },
        advance(80),
    ]);
    let report = script.run().expect("hop into the hall's own pair");
    assert!(events(&report).any(|event| event["kind"] == "player_portal_crossing"));
    let last = report["steps"].as_array().expect("steps").len() - 1;
    let player = &report["steps"][last]["state"]["player"];
    assert!(
        player["position"][0].as_f64().expect("x") < 2.0,
        "outside the tank: {player}"
    );
    assert_eq!(report["steps"][last]["state"]["active_switches"], json!(["drop"]));
}

#[test]
fn a_straight_run_out_of_the_vats_ceiling_drops_beside_the_vat() {
    let (_folder, mut script) = scenario("gatehouse");
    script.actions.truncate(VAT_CEILING_PORTAL + 1);
    script
        .actions
        .extend([walk([0.0, -1.0], 16), advance(10), walk([-1.0, 0.0], 30), advance(40)]);
    let report = script.run().expect("run straight at the wall portal");
    assert!(events(&report).any(|event| event["kind"] == "player_portal_crossing"));
    let last = report["steps"].as_array().expect("steps").len() - 1;
    let player = &report["steps"][last]["state"]["player"];
    assert!(
        player["position"][0].as_f64().expect("x") < 11.9,
        "west of the vat: {player}"
    );
    assert!(
        !report["steps"][last]["state"]["active_switches"]
            .to_string()
            .contains("vat")
    );
}
