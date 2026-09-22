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

#[test]
fn relay_course_connects_all_checkpoints_and_triggers_the_finish() {
    let (_folder, script) = scenario("portal_relay");
    let report = script.run().expect("relay course");
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
    assert_eq!(crossings.len(), 3);
    assert!(crossings[1]["velocity_after"][0].as_f64().expect("horizontal launch") < -20.0);
    assert!(crossings[2]["velocity_after"][1].as_f64().expect("vertical launch") > 25.0);
    assert_eq!(report["initial"]["open_fields"], json!(["relay_bridge"]));
    assert_eq!(report["steps"][11]["state"]["active_switches"], json!(["route_bridge"]));
    assert_eq!(report["steps"][11]["state"]["open_fields"], json!([]));
    assert!(events(&report).any(|event| event["kind"] == "fireworks_started"));
    assert!(!events(&report).any(|event| event["kind"] == "player_died" || event["kind"] == "player_fall_damage"));
}

#[test]
fn vertical_launch_accepts_a_range_of_air_steering_times() {
    let (_folder, mut script) = scenario("portal_relay");
    script.actions.truncate(35);
    for ticks in [34, 37, 40, 43, 46] {
        script.actions[31] = Action::Advance { ticks };
        let report = script.run().expect("air steering timing");
        assert_passed(&report["steps"][34]);
        assert_eq!(report["steps"][34]["state"]["player"]["checkpoint"], 4, "delay {ticks}");
    }
}

#[test]
fn missing_the_air_catch_respawns_at_the_fling_landing() {
    let (_folder, mut script) = scenario("portal_relay");
    script.actions.truncate(32);
    script.actions.extend([
        Action::Move {
            direction: [0.0, -1.0],
            ticks: 90,
            run: true,
            jump: false,
        },
        Action::Advance { ticks: 180 },
        Action::Check {
            min: [0.0, 17.4, 8.0],
            max: [16.0, 17.8, 16.0],
            grounded: true,
        },
    ]);
    let report = script.run().expect("miss and retry");
    assert!(events(&report).any(|event| event["kind"] == "player_died"));
    assert!(events(&report).any(|event| event["kind"] == "player_relocated"));
    assert_passed(&report["steps"][34]);
    let state = &report["steps"][34]["state"];
    assert_eq!(state["player"]["checkpoint"], 3);
    assert_eq!(state["player"]["generation"], 1);
    assert_eq!(state["player"]["portal_gun"], true);
    assert_eq!(state["portals"], json!([]));
}

#[test]
fn bypassing_the_plate_leaves_the_bridge_impassable() {
    let (_folder, mut script) = scenario("portal_relay");
    script.actions.truncate(14);
    script.actions[7] = Action::Move {
        direction: [0.0, 0.0],
        ticks: 8,
        run: false,
        jump: false,
    };
    let report = script.run().expect("jump beside plate");
    assert_passed(&report["steps"][11]);
    assert_eq!(report["steps"][11]["state"]["active_switches"], json!([]));
    assert_eq!(report["steps"][11]["state"]["open_fields"], json!(["relay_bridge"]));
    assert_eq!(report["steps"][13]["result"]["status"], "failed");
}

#[test]
fn lower_portal_targets_accept_shots_while_standing_back_from_the_ledge() {
    let (_folder, mut script) = scenario("portal_relay");
    // These feet positions are inside the upper slabs, with the portal targets
    // across the small horizontal gaps. A shot must clear the upper slab's lip.
    for (spawn, target) in [
        ([10.0, 26.4, -12.3], [10.0, 13.2, -8.0]),
        ([10.0, 17.6, 15.8], [10.0, 0.0, 20.0]),
    ] {
        script.spawn = spawn;
        script.actions = vec![
            Action::Advance { ticks: 2 },
            Action::Aim { target },
            Action::Portal { end: End::A },
        ];
        let report = script.run().expect("shot from upper ledge");
        let shot = &report["steps"][2];
        assert_eq!(shot["state"]["player"]["support"], "ground", "{shot}");
        assert_eq!(shot["result"]["status"], "submitted", "{shot}");
        let portal = &shot["result"]["portal"];
        assert_eq!(portal["normal"], json!([0.0, 1.0, 0.0]));
        for (axis, expected) in target.into_iter().enumerate() {
            let actual = portal["position"][axis].as_f64().expect("portal coordinate");
            assert!((actual - f64::from(expected)).abs() < 0.01, "{shot}");
        }
    }
}
