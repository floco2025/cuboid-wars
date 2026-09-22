use std::fs;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::script::{Action, End, Script};

fn chamber() -> (TempDir, Script) {
    super::fixtures::scenario("portal_turret")
}

fn events(report: &Value) -> impl Iterator<Item = &Value> {
    report["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .flat_map(|step| step["events"].as_array().expect("events"))
}

#[test]
fn a_blocked_direct_shot_becomes_a_lethal_portal_shot_after_reset() {
    let (_folder, script) = chamber();
    let report = script.run().expect("run chamber");
    let steps = report["steps"].as_array().expect("steps");
    assert_eq!(steps[3]["state"]["actors"][0]["health"], 50.0);
    assert!(
        steps[3]["events"]
            .as_array()
            .expect("direct events")
            .iter()
            .any(|event| event["kind"] == "bounce")
    );
    let reset = &steps[4]["state"];
    assert_eq!(reset["tick"], report["initial"]["tick"]);
    assert_eq!(reset["player"], report["initial"]["player"]);
    assert_eq!(reset["portals"], json!([]));
    assert_eq!(reset["projectiles"], json!([]));
    assert_eq!(reset["spawning_actors"][0]["position"], json!([6.0, 0.0, 2.0]));
    assert_eq!(
        steps.last().expect("last step")["state"]["actors"],
        json!([]),
        "{report:#}"
    );
    assert_eq!(steps.last().expect("last step")["state"]["player"]["health"], 500.0);
    assert!(events(&report).any(|event| event["kind"] == "portal_crossing"));
    assert!(events(&report).any(|event| event["kind"] == "actor_hit" && event["health"] == 0.0));
    assert!(events(&report).any(|event| event["kind"] == "actor_died" && event["killer"] == 1));
    let repeated = script.run().expect("repeat chamber");
    assert_eq!(repeated["steps"][15]["state"]["actors"], json!([]));
}

#[test]
fn bad_placement_and_an_unlinked_portal_cannot_damage_the_hidden_target() {
    let (_folder, mut script) = chamber();
    script.actions = vec![
        Action::Advance { ticks: 6 },
        Action::Aim {
            target: [0.0, 1.62, 2.0],
        },
        Action::Portal { end: End::B },
        Action::Advance { ticks: 4 },
        Action::Aim {
            target: [-10.0, 1.62, -8.0],
        },
        Action::Portal { end: End::A },
        Action::Advance { ticks: 4 },
        Action::Fire,
        Action::Advance { ticks: 30 },
    ];
    let report = script.run().expect("bad pair");
    assert_eq!(report["steps"][2]["result"]["reason"], "incompatible_material");
    assert!(!events(&report).any(|event| event["kind"] == "portal_crossing"));
    assert!(!events(&report).any(|event| event["kind"] == "actor_hit"));
    assert_eq!(report["steps"][8]["state"]["actors"][0]["health"], 50.0);
}

#[test]
fn shots_respect_cooldown_and_missing_equipment() {
    let (folder, mut script) = chamber();
    script.actions = vec![Action::Fire, Action::Fire];
    let report = script.run().expect("cooldown");
    assert_eq!(report["steps"][0]["result"]["status"], "fired");
    assert_eq!(report["steps"][1]["result"]["reason"], "cooldown");
    let mut settings: Value =
        serde_json::from_str(&fs::read_to_string(&script.settings).expect("settings")).expect("parse settings");
    settings["power_ups"]["single_shot"] = json!({"mode": "pickup", "duration_secs": null});
    settings["power_ups"]["portal_gun"] = json!({"mode": "pickup", "duration_secs": null});
    script.settings = folder.path().join("settings.json");
    fs::write(&script.settings, settings.to_string()).expect("write settings");
    script.actions = vec![Action::Fire, Action::Portal { end: End::A }];
    let report = script.run().expect("unarmed");
    for step in report["steps"].as_array().expect("steps") {
        assert_eq!(step["result"]["reason"], "missing_equipment");
    }
}

#[test]
fn a_valid_but_misaligned_exit_misses_the_turret() {
    let (_folder, mut script) = chamber();
    script.actions[9] = Action::Aim {
        target: [7.0, 1.62, 7.8],
    };
    let report = script.run().expect("misaligned pair");
    assert!(events(&report).any(|event| event["kind"] == "portal_crossing"));
    assert!(!events(&report).any(|event| event["kind"] == "actor_hit"));
    assert_eq!(report["steps"][15]["state"]["actors"][0]["health"], 50.0);
}

#[test]
fn overlapping_starts_and_invalid_aim_fail_explicitly() {
    let (_folder, mut script) = chamber();
    script.spawn = [0.0, 1.0, 2.0];
    let error = script.run().expect_err("player overlaps a wall");
    assert!(error.to_string().contains("overlaps geometry"), "{error:#}");
    script.spawn = [-10.0, 0.0, 2.0];
    script.actions = vec![Action::Aim {
        target: [-10.0, 1.62, 2.0],
    }];
    let error = script.run().expect_err("zero aim direction");
    let message = format!("{error:#}");
    assert!(
        message.contains("action 0") && message.contains("eye position"),
        "{message}"
    );
}
