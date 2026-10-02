use serde_json::{Value, json};

use crate::experiment::{
    fixtures::chamber,
    script::{Action, End, Script},
};
use tempfile::TempDir;

const EYE: [f32; 3] = [0.0, 1.62, -6.0];
const PORTAL_WALL: [f32; 3] = [-10.0, 1.62, -11.8];

// A floor with a portalable wall north of the spawn and a solid one east of it.
fn walled_floor() -> (TempDir, Script) {
    let floors: Vec<_> = (0..10)
        .flat_map(|col| (0..10).map(move |row| json!({"col": col, "row": row, "all": "solid"})))
        .collect();
    let layout = json!({"map": {"fireworks": null, "grid_cols": 10, "grid_rows": 10,
        "checkpoints": [{"level": 0, "cols": [2, 3], "rows": [3, 4], "number": 0, "type": "individual"}],
        "levels": [{"name": "Probe", "floors": floors, "walls": [
            {"c0": 2, "r0": 2, "c1": 3, "r1": 2, "all": "portal"},
            {"c0": 7, "r0": 3, "c1": 7, "r1": 4, "all": "solid"},
        ]}],
    }});
    chamber(layout, [-10.0, 0.0, -6.0])
}

fn close(value: &Value, expected: [f32; 3]) -> bool {
    let value: [f32; 3] = serde_json::from_value(value.clone()).expect("a point");
    (0..3).all(|axis| (value[axis] - expected[axis]).abs() < 1e-3)
}

#[test]
fn a_probe_reports_each_shot_and_changes_nothing() {
    let (_folder, mut script) = walled_floor();
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
fn a_placed_portal_opens_where_the_probe_from_its_eye_said() {
    let (_folder, mut script) = walled_floor();
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
