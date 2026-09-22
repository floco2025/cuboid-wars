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

#[test]
fn falling_into_a_floor_portal_launches_the_player_across_a_gap_to_a_checkpoint() {
    let (_folder, script) = scenario("portal_movement");
    let report = script.run().expect("movement scenario");
    assert_eq!(
        report["steps"][8]["result"]["status"], "passed",
        "{}",
        report["steps"][8]
    );
    assert_eq!(report["steps"][8]["state"]["player"]["checkpoint"], 1);
    let crossing = events(&report)
        .find(|event| event["kind"] == "player_portal_crossing")
        .expect("player crossed");
    assert!(crossing["velocity_before"][1].as_f64().expect("fall speed") < -20.0);
    assert!(crossing["velocity_after"][0].as_f64().expect("launch speed") < -20.0);
    assert!(events(&report).any(|event| event["kind"] == "player_landed"));
    assert_eq!(script.run().expect("repeat movement route"), report);
}

#[test]
fn an_unlinked_floor_portal_does_not_complete_the_fling_route() {
    let (_folder, mut script) = scenario("portal_movement");
    script.actions[5] = Action::Inspect;
    let report = script.run().expect("incomplete pair");
    assert!(!events(&report).any(|event| event["kind"] == "player_portal_crossing"));
    assert_eq!(report["steps"][8]["result"]["status"], "failed");
}

#[test]
fn a_valid_exit_aimed_beside_the_landing_platform_fails_the_route_check() {
    let (folder, mut script) = scenario("portal_movement");
    let mut layout: Value =
        serde_json::from_str(&std::fs::read_to_string(&script.layout).expect("layout")).expect("map");
    layout["map"]["levels"][2]["walls"][0]["r0"] = json!(4);
    layout["map"]["levels"][2]["walls"][0]["r1"] = json!(5);
    script.layout = folder.path().join("miss.json");
    std::fs::write(&script.layout, layout.to_string()).expect("test map");
    script.actions[4] = Action::Aim {
        target: [-0.2, 10.4, -2.0],
    };
    let report = script.run().expect("misplaced exit");
    assert!(events(&report).any(|event| event["kind"] == "player_portal_crossing"));
    assert_eq!(report["steps"][8]["result"]["status"], "failed");
    assert_eq!(report["steps"][8]["state"]["player"]["checkpoint"], 0);
}

#[test]
fn walking_cannot_pass_a_wall_and_a_jump_requires_ground_support() {
    let (_folder, mut script) = scenario("portal_turret");
    script.actions = vec![
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 90,
            run: false,
            jump: false,
        },
        Action::Reset,
        Action::Move {
            direction: [0.0, 0.0],
            ticks: 1,
            run: false,
            jump: true,
        },
        Action::Move {
            direction: [0.0, 0.0],
            ticks: 1,
            run: false,
            jump: true,
        },
        Action::Check {
            min: [-12.0, -1.0, 0.0],
            max: [-8.0, 10.0, 4.0],
            grounded: true,
        },
        Action::Advance { ticks: 40 },
    ];
    let report = script.run().expect("walk and jump");
    assert!(
        report["steps"][0]["state"]["player"]["position"][0]
            .as_f64()
            .expect("x")
            < -0.4
    );
    assert!(events(&report).any(|event| event["kind"] == "player_step" && event["blocked"] == true));
    let jumps: Vec<_> = events(&report)
        .filter(|event| event["kind"] == "jump")
        .map(|e| e["accepted"].clone())
        .collect();
    assert_eq!(jumps, vec![json!(true), json!(false)]);
    assert_eq!(report["steps"][4]["result"]["reason"], "not_grounded");
    assert_eq!(report["steps"][5]["state"]["player"]["support"], "ground");
}

#[test]
fn a_jump_clears_a_gap_that_walking_cannot() {
    let (folder, mut script) = scenario("portal_movement");
    let layout = json!({"map": {"fireworks": null, "grid_cols": 10, "grid_rows": 10,
        "checkpoints": [{"level":0,"cols":[2,3],"rows":[3,4],"number":0,"type":"individual"}],
        "levels": [{"name":"Gap", "floors":[{"col":2,"row":3,"all":"solid"}, {"col":4,"row":3,"all":"solid"}]}]
    }});
    script.layout = folder.path().join("gap.json");
    std::fs::write(&script.layout, layout.to_string()).expect("test map");
    script.spawn = [-9.0, 0.0, -6.0];
    script.actions = vec![
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 28,
            run: false,
            jump: true,
        },
        Action::Advance { ticks: 10 },
        Action::Check {
            min: [-4.0, -0.1, -8.0],
            max: [0.0, 0.1, -4.0],
            grounded: true,
        },
    ];
    let jumping = script.run().expect("jump gap");
    assert_eq!(
        jumping["steps"][2]["result"]["status"], "passed",
        "{}",
        jumping["steps"][2]
    );
    script.actions[0] = Action::Move {
        direction: [1.0, 0.0],
        ticks: 28,
        run: false,
        jump: false,
    };
    let walking = script.run().expect("walk gap");
    assert_eq!(walking["steps"][2]["result"]["status"], "failed");
}

#[test]
fn a_wall_portal_turns_held_movement_and_reports_the_owners_exit_position() {
    let (folder, mut script) = scenario("portal_movement");
    let floors: Vec<_> = (0..10)
        .flat_map(|col| (0..10).map(move |row| json!({"col":col,"row":row,"all":"solid"})))
        .collect();
    let layout = json!({"map": {"fireworks": null, "grid_cols":10,"grid_rows":10,
        "checkpoints":[{"level":0,"cols":[2,3],"rows":[3,4],"number":0,"type":"individual"}],
        "levels":[{"name":"Turn", "floors":floors, "walls":[
            {"c0":2,"r0":2,"c1":3,"r1":2,"all":"portal"},
            {"c0":7,"r0":3,"c1":7,"r1":4,"all":"portal"}
        ]}]
    }});
    script.layout = folder.path().join("turn.json");
    std::fs::write(&script.layout, layout.to_string()).expect("test map");
    script.spawn = [-10.0, 0.0, -6.0];
    script.actions = vec![
        Action::Aim {
            target: [-10.0, 1.62, -11.8],
        },
        Action::Portal { end: End::A },
        Action::Advance { ticks: 4 },
        Action::Aim {
            target: [7.8, 1.62, -6.0],
        },
        Action::Portal { end: End::B },
        Action::Move {
            direction: [0.0, -1.0],
            ticks: 50,
            run: false,
            jump: false,
        },
        Action::Check {
            min: [3.0, -0.1, -6.1],
            max: [4.5, 0.1, -5.9],
            grounded: true,
        },
        Action::Reset,
    ];
    let report = script.run().expect("turn through portal");
    let result = &report["steps"][6];
    assert_eq!(result["result"]["status"], "passed", "{result}");
    assert_eq!(
        result["state"]["player"]["position"],
        result["state"]["player"]["reported_position"]
    );
    assert_eq!(
        events(&report)
            .filter(|event| event["kind"] == "player_portal_crossing")
            .count(),
        1
    );
    assert_eq!(report["steps"][7]["state"], report["initial"]);
}

#[test]
fn a_void_fall_is_reported_and_respawn_establishes_a_new_owned_body() {
    let (_folder, mut script) = scenario("portal_movement");
    script.spawn = [0.0, -26.0, 10.0];
    script.actions = vec![
        Action::Advance { ticks: 1 },
        Action::Advance { ticks: 75 },
        Action::Check {
            min: [-12.0, 13.0, -8.0],
            max: [-8.0, 13.4, -4.0],
            grounded: true,
        },
    ];
    let report = script.run().expect("void and respawn");
    assert_eq!(report["steps"][0]["state"]["player"], Value::Null);
    assert_eq!(
        events(&report)
            .filter(|event| event["kind"] == "player_fell_out_of_world")
            .count(),
        1
    );
    assert!(events(&report).any(|event| event["kind"] == "player_relocated"));
    assert_eq!(report["steps"][2]["result"]["status"], "passed");
    let player = &report["steps"][2]["state"]["player"];
    assert_eq!(player["generation"], 1);
    assert_eq!(player["position"], player["reported_position"]);
    assert_eq!(player["momentum"], json!([0.0, 0.0, 0.0]));
}
