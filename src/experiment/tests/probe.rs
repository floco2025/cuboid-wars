use serde_json::{Value, json};

use crate::experiment::{
    fixtures::{turret_room, walk, walled_floor},
    script::{Action, End},
};

const EYE: [f32; 3] = [0.0, 1.62, -6.0];
const PORTAL_WALL: [f32; 3] = [-10.0, 1.62, -11.8];

fn close(value: &Value, expected: [f32; 3]) -> bool {
    let value: [f32; 3] = serde_json::from_value(value.clone()).expect("a point");
    (0..3).all(|axis| (value[axis] - expected[axis]).abs() < 1e-3)
}

#[test]
fn a_probe_reports_each_shot_and_changes_nothing() {
    let (_folder, mut script) = walled_floor("solid");
    script.actions = vec![Action::Probe {
        eye: None,
        targets: vec![PORTAL_WALL, [7.8, 1.62, -6.0], [-10.0, 50.0, -6.0], [-10.0, 0.0, -4.0]],
    }];
    let report = script.run().expect("probe");
    let step = &report["steps"][0];
    let shots = step["result"]["shots"].as_array().expect("shots");
    assert_eq!(shots[0]["status"], "placed", "{}", shots[0]);
    assert!(close(&shots[0]["portal"]["position"], PORTAL_WALL), "{}", shots[0]);
    assert!(close(&shots[0]["portal"]["normal"], [0.0, 0.0, 1.0]), "{}", shots[0]);
    assert_eq!(shots[1]["status"], "incompatible_material", "{}", shots[1]);
    assert!(close(&shots[1]["hit"]["normal"], [-1.0, 0.0, 0.0]), "{}", shots[1]);
    assert_eq!(shots[2], json!({"target": [-10.0, 50.0, -6.0], "status": "no_surface"}));
    assert_eq!(shots[3]["status"], "incompatible_material", "{}", shots[3]);
    assert_eq!(step["state"], report["initial"]);
}

#[test]
fn a_reset_starts_over_where_it_says_or_at_the_scripts_spawn() {
    let (_folder, mut script) = walled_floor("solid");
    script.actions = vec![
        Action::Reset {
            spawn: Some([0.0, 0.0, -6.0]),
        },
        Action::Reset { spawn: None },
    ];
    let report = script.run().expect("resets");
    assert!(close(
        &report["steps"][0]["state"]["player"]["position"],
        [0.0, 0.0, -6.0]
    ));
    assert_eq!(report["steps"][1]["state"], report["initial"]);
}

#[test]
fn a_placed_portal_opens_where_the_probe_from_its_eye_said() {
    let (_folder, mut script) = walled_floor("solid");
    script.actions = vec![
        Action::Probe {
            eye: Some(EYE),
            targets: vec![PORTAL_WALL],
        },
        Action::Place {
            end: End::A,
            eye: EYE,
            target: PORTAL_WALL,
        },
    ];
    let report = script.run().expect("probe and place");
    let probed = &report["steps"][0]["result"]["shots"][0]["portal"];
    let placed = &report["steps"][1];
    assert_eq!(placed["result"]["status"], "submitted", "{placed}");
    for key in ["position", "normal", "yaw"] {
        assert_eq!(placed["result"]["portal"][key], probed[key], "{key}");
    }
    assert_eq!(placed["state"]["portals"].as_array().expect("portals").len(), 1);
    assert!(
        close(&placed["state"]["player"]["position"], [-10.0, 0.0, -6.0]),
        "{placed}"
    );
}

#[test]
fn a_reset_into_geometry_is_refused_and_changes_nothing() {
    let (_folder, mut script) = walled_floor("solid");
    script.actions = vec![Action::Reset {
        spawn: Some([-10.0, 0.0, -12.0]),
    }];
    let report = script.run().expect("refused reset");
    let step = &report["steps"][0];
    assert_eq!(step["result"]["status"], "rejected", "{step}");
    assert_eq!(step["state"], report["initial"]);
}

#[test]
fn a_teleport_moves_the_body_at_rest_and_keeps_what_the_script_built() {
    let (_folder, mut script) = walled_floor("solid");
    script.actions = vec![
        Action::Place {
            end: End::A,
            eye: EYE,
            target: PORTAL_WALL,
        },
        walk([1.0, 0.0], 6),
        Action::Teleport { feet: [0.0, 0.0, -6.0] },
        Action::Advance { ticks: 3 },
        Action::Teleport {
            feet: [-10.0, 0.0, -12.0],
        },
    ];
    let report = script.run().expect("teleports");
    let moved = &report["steps"][2];
    assert_eq!(moved["result"]["status"], "teleported", "{moved}");
    let settled = &report["steps"][3]["state"];
    assert!(close(&settled["player"]["position"], [0.0, 0.0, -6.0]), "{settled}");
    assert!(
        close(&settled["player"]["reported_position"], [0.0, 0.0, -6.0]),
        "the server adopts it: {settled}"
    );
    assert_eq!(settled["player"]["horizontal_velocity"], json!([0.0, 0.0, 0.0]));
    assert_eq!(settled["portals"].as_array().expect("portals").len(), 1);
    let refused = &report["steps"][4];
    assert_eq!(refused["result"]["status"], "rejected", "{refused}");
    assert_eq!(refused["state"], *settled);
}

#[test]
fn clearing_portals_keeps_the_session_and_allows_the_pair_to_be_reversed() {
    let (_folder, mut script) = turret_room();
    let place = |end, target| Action::Place {
        end,
        eye: [-10.0, 1.62, 2.0],
        target,
    };
    let north = [-10.0, 1.62, -8.0];
    let south = [6.0, 1.62, 7.8];
    script.actions = vec![
        place(End::A, north),
        Action::Advance { ticks: 4 },
        place(End::B, south),
        Action::Advance { ticks: 4 },
        Action::Fire,
        serde_json::from_value(json!({"action": "clear_portals"})).expect("clear action"),
        Action::ClearPortals,
        Action::Advance { ticks: 4 },
        place(End::A, south),
        Action::Advance { ticks: 4 },
        place(End::B, north),
    ];
    let report = script.run().expect("replace pair");
    let steps = &report["steps"];
    for index in [0, 2, 8, 10] {
        assert_eq!(steps[index]["result"]["status"], "submitted", "{}", steps[index]);
    }
    let before = &steps[4]["state"];
    assert_eq!(before["portals"].as_array().expect("old pair").len(), 2);
    assert!(
        !before["projectiles"]
            .as_array()
            .expect("projectile in flight")
            .is_empty()
    );
    let mut cleared = before.clone();
    cleared["portals"] = json!([]);
    for index in [5, 6] {
        assert_eq!(steps[index]["result"]["status"], "cleared");
        assert_eq!(steps[index]["state"], cleared, "nothing but portals changes");
    }
    let reversed = &steps[10]["state"]["portals"];
    assert_eq!(reversed.as_array().expect("new pair").len(), 2);
    assert_eq!(reversed[0]["position"], before["portals"][1]["position"]);
    assert_eq!(reversed[1]["position"], before["portals"][0]["position"]);
}
