use serde_json::{Value, json};

use super::{
    fixtures::{chamber, turret_room},
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
fn walking_cannot_pass_a_wall_and_a_jump_requires_ground_support() {
    let (_folder, mut script) = turret_room();
    script.actions = vec![
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 90,
            crouch: false,
            jump: false,
        },
        Action::Reset { spawn: None },
        Action::Move {
            direction: [0.0, 0.0],
            ticks: 1,
            crouch: false,
            jump: true,
        },
        Action::Move {
            direction: [0.0, 0.0],
            ticks: 1,
            crouch: false,
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
    let layout = json!({"map": {"fireworks": null, "grid_cols": 10, "grid_rows": 10,
        "checkpoints": [{"level":0,"cols":[2,3],"rows":[3,4],"number":0,"type":"individual"}],
        "levels": [{"name":"Gap", "floors":[{"col":2,"row":3,"all":"solid"}, {"col":4,"row":3,"all":"solid"}]}]
    }});
    let (_folder, mut script) = chamber(layout, [-11.0, 0.0, -6.0]);
    script.actions = vec![
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 19,
            crouch: false,
            jump: false,
        },
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 28,
            crouch: false,
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
        jumping["steps"][3]["result"]["status"], "passed",
        "{}",
        jumping["steps"][3]
    );
    script.actions[1] = Action::Move {
        direction: [1.0, 0.0],
        ticks: 28,
        crouch: false,
        jump: false,
    };
    let walking = script.run().expect("walk gap");
    assert_eq!(walking["steps"][3]["result"]["status"], "failed");
}

#[test]
fn a_walk_ends_standing_on_its_point_or_says_how_far_short_a_wall_stopped_it() {
    let (_folder, mut script) = turret_room();
    script.actions = vec![
        Action::WalkTo {
            target: [-6.0, -2.0],
            ticks: 120,
        },
        Action::WalkTo {
            target: [6.0, -2.0],
            ticks: 120,
        },
    ];
    let report = script.run().expect("walk");
    let arrived = &report["steps"][0];
    assert_eq!(arrived["result"]["status"], "arrived", "{arrived}");
    let position = &arrived["state"]["player"]["position"];
    let off = (position[0].as_f64().expect("x") + 6.0).hypot(position[2].as_f64().expect("z") + 2.0);
    assert!(off < 0.1, "{position}");
    let blocked = &report["steps"][1]["result"];
    assert_eq!(blocked["status"], "short", "{blocked}");
    assert!(blocked["distance"].as_f64().expect("distance") > 5.0, "{blocked}");
}

#[test]
fn a_wall_portal_turns_held_movement_and_reports_the_owners_exit_position() {
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
    let (_folder, mut script) = chamber(layout, [-10.0, 0.0, -6.0]);
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
            ticks: 75,
            crouch: false,
            jump: false,
        },
        Action::Check {
            min: [1.0, -0.1, -6.1],
            max: [2.5, 0.1, -5.9],
            grounded: true,
        },
        Action::Reset { spawn: None },
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
    assert_eq!(script.run().expect("repeat the turn"), report);
}

#[test]
fn a_body_landing_on_a_portal_on_a_low_ramp_sinks_through_the_floor_under_it_and_crosses() {
    let floors: Vec<_> = (0..10)
        .flat_map(|col| (0..10).map(move |row| json!({"col":col,"row":row,"all":"solid"})))
        .collect();
    let layout = json!({"map": {"fireworks": null, "grid_cols":10,"grid_rows":10,
        "checkpoints":[{"level":0,"cols":[0,1],"rows":[0,1],"number":0,"type":"individual"}],
        "ramps":[{"lower_level":0,"levels":1,"cols":[2,8],"rows":[4,6],"direction":"W","shape":"solid",
            "all":"solid","top":"portal"}],
        "levels":[
            {"name":"Slope", "floors":floors, "walls":[{"c0":8,"r0":2,"c1":9,"r1":2,"all":"portal"}]},
            {"name":"Above", "floors":[], "walls":[]}
        ]
    }});
    let (_folder, mut script) = chamber(layout, [-18.0, 0.0, -18.0]);
    script.actions = vec![
        Action::Place {
            end: End::A,
            eye: [17.0, 1.62, 0.0],
            target: [7.6, 0.8, 0.0],
        },
        Action::Advance { ticks: 4 },
        Action::Place {
            end: End::B,
            eye: [14.0, 1.62, -6.0],
            target: [14.0, 1.62, -12.0],
        },
        Action::Advance { ticks: 4 },
        Action::Teleport { feet: [7.6, 4.0, 0.0] },
        Action::Advance { ticks: 40 },
    ];
    let report = script.run().expect("drop onto the ramp's portal");
    for step in &report["steps"].as_array().expect("steps")[..3] {
        assert_ne!(step["result"]["status"], "fizzled", "{step}");
    }
    assert!(
        events(&report).any(|event| event["kind"] == "player_portal_crossing"),
        "{}",
        report["steps"][5]["state"]["player"]
    );
}

#[test]
fn a_void_fall_is_reported_and_respawn_establishes_a_new_owned_body() {
    let (_folder, mut script) = turret_room();
    script.spawn = [0.0, -26.0, 10.0];
    script.actions = vec![
        Action::Advance { ticks: 1 },
        Action::Advance { ticks: 75 },
        Action::Check {
            min: [-12.0, -0.1, 0.0],
            max: [-8.0, 0.1, 4.0],
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
    assert_eq!(player["horizontal_velocity"], json!([0.0, 0.0, 0.0]));
}
