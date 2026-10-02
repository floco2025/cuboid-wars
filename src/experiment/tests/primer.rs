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

fn assert_passed(step: &Value) {
    assert_eq!(
        step["result"]["status"], "passed",
        "step {}: {} {}",
        step["index"], step["result"], step["state"]["player"]
    );
}

fn walk(direction: [f32; 2], ticks: u32) -> Action {
    Action::Move {
        direction,
        ticks,
        crouch: false,
        jump: false,
    }
}

fn jump(direction: [f32; 2], ticks: u32) -> Action {
    Action::Move {
        direction,
        ticks,
        crouch: false,
        jump: true,
    }
}

fn advance(ticks: u32) -> Action {
    Action::Advance { ticks }
}

fn check(min: [f32; 3], max: [f32; 3]) -> Action {
    Action::Check {
        min,
        max,
        grounded: true,
    }
}

const DECK3: ([f32; 3], [f32; 3]) = ([-20.0, 4.3, -18.0], [-16.0, 4.5, -14.0]);
const REFILL: ([f32; 3], [f32; 3]) = ([0.0, 2.1, -20.0], [32.0, 2.3, -10.0]);
const LANDING4: ([f32; 3], [f32; 3]) = ([20.0, 6.5, -20.0], [30.0, 6.7, -12.0]);
const SUMMIT: ([f32; 3], [f32; 3]) = ([40.0, 15.3, -22.0], [52.0, 15.5, -10.0]);
const FINISH: ([f32; 3], [f32; 3]) = ([54.0, -0.1, -12.0], [66.0, 0.1, 16.0]);

#[test]
fn primer_course_connects_every_chamber_and_starts_the_fireworks() {
    let (_folder, script) = scenario("portal_primer");
    let report = script.run().expect("primer course");
    for step in report["steps"].as_array().expect("steps") {
        match step["action"]["action"].as_str() {
            Some("check") => assert_passed(step),
            Some("portal") => assert_eq!(step["result"]["status"], "submitted", "{step}"),
            _ => {}
        }
    }
    let checkpoints: Vec<_> = events(&report)
        .filter(|event| event["kind"] == "checkpoint_reached")
        .map(|event| event["checkpoint"].clone())
        .collect();
    assert_eq!(checkpoints, [json!(1), json!(2), json!(3), json!(4), json!(5)]);
    let crossings: Vec<_> = events(&report)
        .filter(|event| event["kind"] == "player_portal_crossing")
        .collect();
    assert_eq!(crossings.len(), 4);
    // The pit drop leaves the wall sideways; the ledge drop leaves it at the speed of a 15 m fall.
    assert!(crossings[0]["velocity_after"][2].as_f64().expect("lobby fling") < -15.0);
    assert!(crossings[3]["velocity_after"][2].as_f64().expect("finale fling") > 25.0);
    assert_eq!(report["steps"][41]["state"]["active_switches"], json!(["island"]));
    let collected: Vec<_> = events(&report)
        .filter(|event| event["kind"] == "item_collected")
        .map(|event| event["item"].clone())
        .collect();
    assert_eq!(collected, [json!("key"), json!("speed"), json!("low_gravity")]);
    assert!(events(&report).any(|event| event["kind"] == "equipment_erased"));
    let after_erasure = &report["steps"][53]["state"]["player"];
    assert_eq!(
        (after_erasure["speed"].as_bool(), after_erasure["low_gravity"].as_bool()),
        (Some(false), Some(false))
    );
    assert!(events(&report).any(|event| event["kind"] == "fireworks_started"));
    assert!(!events(&report).any(|event| event["kind"] == "player_died" || event["kind"] == "player_fall_damage"));
}

#[test]
fn the_first_fling_needs_its_exit_portal() {
    let (_folder, mut script) = scenario("portal_primer");
    script.actions.truncate(8);
    script.actions[3] = advance(1);
    script.actions[4] = advance(1);
    let report = script.run().expect("drop onto the pad");
    assert_eq!(report["steps"][7]["result"]["reason"], "outside_region");
    let feet = &report["steps"][7]["state"]["player"]["position"];
    assert!(
        (feet[1].as_f64().expect("height") - 4.4).abs() < 0.05,
        "stands on the pad: {feet}"
    );
}

#[test]
fn the_corridor_side_of_the_hidden_wall_refuses_portals() {
    let (_folder, mut script) = scenario("portal_primer");
    script.actions.truncate(9);
    script.actions.extend([
        walk([0.0, 1.0], 9),
        walk([1.0, 0.0], 70),
        advance(10),
        Action::Aim {
            target: [-40.0, 10.18, 12.1],
        },
        Action::Portal { end: End::B },
    ]);
    let report = script.run().expect("shot from the corridor");
    assert_eq!(report["steps"][13]["result"]["status"], "fizzled");
    assert_eq!(report["steps"][13]["result"]["reason"], "incompatible_material");
}

#[test]
fn the_bridge_drops_a_player_who_skips_the_plate() {
    let (_folder, mut script) = scenario("portal_primer");
    script.actions.truncate(35);
    script
        .actions
        .extend([walk([0.0, -1.0], 40), advance(60), check(DECK3.0, DECK3.1)]);
    let report = script.run().expect("walk onto the dark bridge");
    assert!(events(&report).any(|event| event["kind"] == "player_died"));
    assert_eq!(report["steps"][35]["state"]["active_switches"], json!([]));
    assert!(!events(&report).any(|event| event["kind"] == "checkpoint_reached" && event["checkpoint"] == 3));
}

#[test]
fn the_gate_holds_a_player_without_the_key() {
    let (_folder, mut script) = scenario("portal_primer");
    script.actions.truncate(38);
    script.actions.extend([
        walk([1.0, 0.0], 10),
        walk([0.0, -1.0], 85),
        advance(10),
        check(DECK3.0, DECK3.1),
    ]);
    let report = script.run().expect("walk into the gate");
    let last = &report["steps"][41];
    assert_eq!(last["result"]["reason"], "outside_region");
    assert_eq!(last["state"]["active_switches"], json!(["island"]));
    let feet = &last["state"]["player"]["position"];
    assert!(
        feet[2].as_f64().expect("depth") > -12.5,
        "stopped at the barrier: {feet}"
    );
    assert_eq!(last["state"]["player"]["checkpoint"], 2);
    assert!(!events(&report).any(|event| event["kind"] == "player_died"));
}

// Running past the speed pickup, the ramp jump lands on the refill deck; its
// pickup and the ladder back up make the second attempt.
#[test]
fn the_ramp_gap_needs_speed_and_the_refill_deck_offers_a_retry() {
    let (_folder, mut script) = scenario("portal_primer");
    script.actions.truncate(42);
    script.actions.extend([
        walk([0.0, -1.0], 10),
        walk([1.0, 0.0], 89),
        jump([1.0, 0.0], 40),
        advance(30),
        check(REFILL.0, REFILL.1),
        walk([0.0, 1.0], 12),
        advance(5),
        walk([-1.0, 0.0], 36),
        advance(10),
        walk([0.0, 1.0], 14),
        advance(5),
        walk([0.0, -1.0], 45),
        advance(10),
        check([0.0, 4.3, -18.0], [4.0, 4.5, -14.0]),
        walk([1.0, 0.0], 27),
        jump([1.0, 0.0], 40),
        advance(20),
        check(LANDING4.0, LANDING4.1),
    ]);
    let report = script.run().expect("refill and retry");
    assert_passed(&report["steps"][46]);
    assert_eq!(report["steps"][46]["state"]["player"]["speed"], false);
    assert_passed(&report["steps"][55]);
    assert_passed(&report["steps"][59]);
    assert_eq!(report["steps"][59]["state"]["player"]["checkpoint"], 4);
    assert!(!events(&report).any(|event| event["kind"] == "player_died"));
}

#[test]
fn the_summit_needs_low_gravity() {
    let (_folder, mut script) = scenario("portal_primer");
    script.actions.truncate(43);
    script.actions.extend([
        jump([1.0, 0.0], 30),
        walk([0.0, -1.0], 10),
        advance(5),
        check(LANDING4.0, LANDING4.1),
        walk([1.0, 0.0], 14),
        jump([1.0, 0.0], 1),
        advance(150),
        check(SUMMIT.0, SUMMIT.1),
    ]);
    let report = script.run().expect("jump without low gravity");
    assert_passed(&report["steps"][46]);
    assert_eq!(report["steps"][46]["state"]["player"]["low_gravity"], false);
    assert!(events(&report).any(|event| event["kind"] == "player_died"));
    assert_eq!(report["steps"][50]["result"]["reason"], "outside_region");
    assert_eq!(report["steps"][50]["state"]["player"]["checkpoint"], 4);
}

// Low gravity floats down to any floor it can reach, so nothing but the
// eraser doorway may lead into the tower: not the jump from checkpoint 4
// steered past the summit, nor the summit's lip beside the doorway, nor a
// jump over the wall.
#[test]
fn the_tower_keeps_out_a_player_who_still_has_low_gravity() {
    for (kept, detour) in [
        (48, vec![walk([1.0, 1.0], 60)]),
        (52, vec![walk([0.0, 1.0], 12), advance(10), walk([1.0, 0.0], 24)]),
        (52, vec![jump([1.0, 0.0], 50)]),
    ] {
        let (_folder, mut script) = scenario("portal_primer");
        script.actions.truncate(kept);
        script.actions.extend(detour);
        script.actions.extend([advance(250), check(FINISH.0, FINISH.1)]);
        let report = script.run().expect("detour around the doorway");
        let last = report["steps"].as_array().expect("steps").last().expect("check");
        assert_eq!(last["result"]["reason"], "outside_region", "{last}");
        let player = &last["state"]["player"];
        assert_eq!(player["low_gravity"], true, "{player}");
        assert!(
            player["position"][1].as_f64().expect("height") > 15.0,
            "on the summit or the roof: {player}"
        );
        assert!(!events(&report).any(|event| event["kind"] == "fireworks_started"));
    }
}

// Past the doorway the fall is at full gravity, and a body without pickups
// still has everything the finale needs.
#[test]
fn the_ledge_drop_kills_without_its_portals_and_needs_no_pickup_with_them() {
    let (_folder, mut script) = scenario("portal_primer");
    let finale = script.actions.split_off(54);
    script
        .actions
        .extend([walk([1.0, 0.0], 12), advance(100), check(SUMMIT.0, SUMMIT.1)]);
    let report = script.run().expect("step off the ledge");
    assert!(events(&report).any(|event| event["kind"] == "equipment_erased"));
    assert!(events(&report).any(|event| event["kind"] == "player_fall_damage"));
    assert!(events(&report).any(|event| event["kind"] == "player_died"));
    assert!(!events(&report).any(|event| event["kind"] == "fireworks_started"));
    let respawned = &report["steps"][56];
    assert_passed(respawned);
    assert_eq!(respawned["state"]["player"]["checkpoint"], 5);

    script.spawn = [57.62, 15.4, -14.96];
    script.actions = finale;
    let report = script.run().expect("finale from the ledge");
    for step in report["steps"].as_array().expect("steps") {
        if step["action"]["action"] == "check" {
            assert_passed(step);
        }
    }
    assert_eq!(report["initial"]["player"]["low_gravity"], false);
    assert!(events(&report).any(|event| event["kind"] == "fireworks_started"));
    assert!(!events(&report).any(|event| event["kind"] == "player_died"));
}
