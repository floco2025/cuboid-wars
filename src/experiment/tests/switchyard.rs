use serde_json::{Value, json};

use super::{
    fixtures::scenario,
    script::{Action, End},
};

// Boundaries in the shipped solution, before each independent wing and the finale.
const ARCHIVE: usize = 24;
const SIGNAL: usize = 50;
const DISPATCHED: usize = 13;
const CANOPY_READY: usize = 59;

fn events(report: &Value) -> impl Iterator<Item = &Value> {
    report["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .flat_map(|step| step["events"].as_array().expect("events"))
}

fn last(report: &Value) -> &Value {
    &report["steps"].as_array().expect("steps").last().expect("last step")["state"]
}

fn validate_route(report: &Value) {
    let mut failures = Vec::new();
    for step in report["steps"].as_array().expect("steps") {
        let status = step["result"]["status"].as_str().unwrap_or("");
        if matches!(status, "failed" | "short" | "rejected" | "fizzled") {
            failures.push(format!(
                "{}: {} {} at {}",
                step["index"], step["action"], step["result"], step["state"]["player"]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(!events(report).any(|e| matches!(
        e["kind"].as_str(),
        Some("player_died" | "player_fall_damage" | "player_crushed")
    )));
}

fn walk(col: f32, row: f32) -> Action {
    Action::WalkTo {
        target: [col - 24.0, row - 24.0],
        ticks: 300,
    }
}

fn advance(ticks: u32) -> Action {
    Action::Advance { ticks }
}

fn shot(end: End, col: f32, y: f32, row: f32) -> [Action; 3] {
    [
        Action::Aim {
            target: [col - 24.0, y, row - 24.0],
        },
        Action::Portal { end },
        advance(8),
    ]
}

fn finish(report: &Value) {
    validate_route(report);
    assert!(events(report).any(|e| e["kind"] == "fireworks_started"));
    assert_eq!(last(report)["player"]["keys"], json!(["cyan", "green", "amber"]));
    assert_eq!(last(report)["player"]["checkpoint"], 1);
}

#[test]
fn switchyard_route_reaches_the_signal_without_damage() {
    let (_folder, script) = scenario("switchyard");
    assert!(script.actions.iter().all(|action| !matches!(
        action,
        Action::Place { .. } | Action::Teleport { .. } | Action::Reset { .. }
    )));
    let report = script.run().expect("switchyard course");
    finish(&report);
    assert_eq!(
        events(&report)
            .filter(|e| e["kind"] == "player_portal_crossing")
            .count(),
        3
    );
    let steps = report["steps"].as_array().expect("steps");
    let before = &steps[9]["state"]["portals"][0];
    let after = &steps[12]["state"]["portals"][0];
    assert_ne!(before["carrier"], 0);
    assert!(
        (after["position"][0].as_f64().expect("x") - before["position"][0].as_f64().expect("x") - 26.0).abs() < 0.01
    );
    assert_eq!(steps[38]["state"]["player"]["keys"], json!(["cyan", "green"]));
}

#[test]
fn either_wing_can_be_solved_first() {
    let (_folder, mut script) = scenario("switchyard");
    let actions = script.actions.clone();
    script.actions = actions[ARCHIVE..SIGNAL]
        .iter()
        .chain(&actions[..ARCHIVE])
        .chain(&actions[SIGNAL..])
        .cloned()
        .collect();
    finish(&script.run().expect("archive first"));
}

#[test]
fn the_loading_interlock_blocks_walking_after_dispatch_and_can_be_recalled() {
    let (_folder, mut script) = scenario("switchyard");
    script.actions.truncate(DISPATCHED);
    script.actions.push(Action::ClearPortals);
    script.actions.extend([walk(11.0, 14.0), walk(11.0, 7.0)]);
    let blocked = script.run().expect("closed loading door");
    let p = &last(&blocked)["player"]["position"];
    assert!(p[2].as_f64().expect("z") > -14.0, "{p}");
    assert_eq!(last(&blocked)["player"]["keys"], json!([]));
    script
        .actions
        .extend([walk(20.5, 15.5), advance(180), walk(11.0, 14.0), walk(11.0, 7.0)]);
    let recalled = script.run().expect("recall the carriage");
    let p = &last(&recalled)["player"]["position"];
    assert!(p[2].as_f64().expect("z") < -16.0, "{p}");
    assert!(
        !last(&recalled)["active_switches"]
            .as_array()
            .expect("switches")
            .contains(&json!("dispatch"))
    );
}

#[test]
fn the_carriage_at_departure_does_not_reach_arrival() {
    let (_folder, mut script) = scenario("switchyard");
    script.actions.truncate(17);
    script.actions[10] = walk(20.5, 14.0);
    let report = script.run().expect("undispatched carriage");
    assert!(events(&report).any(|e| e["kind"] == "player_portal_crossing"));
    assert!(last(&report)["player"]["position"][0].as_f64().expect("x") < 0.0);
    assert_eq!(last(&report)["active_switches"], json!([]));
}

#[test]
fn the_vault_requires_its_key_even_with_the_portal_prepared() {
    let (_folder, mut script) = scenario("switchyard");
    script.actions.truncate(46);
    script.actions[36] = walk(19.0, 39.5);
    let report = script.run().expect("vault without green key");
    assert_eq!(last(&report)["player"]["keys"], json!(["cyan"]));
    assert!(last(&report)["player"]["position"][2].as_f64().expect("z") > 5.0);
    assert!(
        !last(&report)["active_switches"]
            .as_array()
            .expect("switches")
            .contains(&json!("undercroft-return"))
    );
}

#[test]
fn taking_the_key_before_preparing_a_portal_has_a_safe_recovery_route() {
    let (_folder, mut script) = scenario("switchyard");
    let remaining = script.actions[43..].to_vec();
    script.actions = vec![
        walk(22.0, 29.5),
        walk(19.0, 29.5),
        walk(19.5, 33.5),
        advance(8),
        walk(15.0, 33.5),
        advance(30),
        walk(15.0, 43.0),
        walk(9.5, 43.0),
        advance(8),
    ];
    script.actions.extend(shot(End::A, 4.1, 4.65, 32.0));
    script.actions.extend([
        walk(15.0, 43.0),
        walk(16.2, 41.5),
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 65,
            crouch: false,
            jump: false,
        },
        advance(8),
    ]);
    script.actions.extend(shot(End::B, 19.9, 4.65, 38.0));
    script.actions.extend([
        walk(19.0, 38.0),
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 16,
            crouch: false,
            jump: false,
        },
        advance(10),
    ]);
    // The archive's completion and return, then the other wing and the finale.
    script.actions.extend_from_slice(&remaining[..SIGNAL - 43]);
    let (_, original) = scenario("switchyard");
    script.actions.extend_from_slice(&original.actions[..ARCHIVE]);
    script.actions.extend_from_slice(&original.actions[SIGNAL..]);
    let report = script.run().expect("early key recovery");
    assert!(
        report["steps"][5]["state"]["player"]["position"][1]
            .as_f64()
            .expect("catch floor")
            .abs()
            < 0.01
    );
    assert!(
        (report["steps"][8]["state"]["player"]["position"][1]
            .as_f64()
            .expect("perch")
            - 1.6)
            .abs()
            < 0.01
    );
    finish(&report);
}

#[test]
fn the_archive_vault_is_hidden_from_the_entry_walkway() {
    let (_folder, mut script) = scenario("switchyard");
    script.actions.clear();
    // Sample every metre of the safe approach against both long vault walls,
    // including targets close to its southern doorway where an oblique shot helps.
    let targets: Vec<_> = (25..36)
        .flat_map(|row| {
            [4.1, 7.9]
                .into_iter()
                .map(move |col| [col - 24.0, 4.65, row as f32 + 0.2 - 24.0])
        })
        .collect();
    for row in 25..44 {
        script.actions.push(Action::Probe {
            eye: Some([-5.0, 4.82, row as f32 - 24.0]),
            targets: targets.clone(),
        });
    }
    let report = script.run().expect("entry walkway sightlines");
    for step in report["steps"].as_array().expect("steps") {
        for shot in step["result"]["shots"].as_array().expect("shots") {
            assert_ne!(shot["status"], "placed", "a vault shot from the entry walkway: {step}");
        }
    }
}

#[test]
fn the_canopy_must_move_and_its_destination_cannot_be_shot_through_the_screen() {
    let (_folder, mut script) = scenario("switchyard");
    script.actions.truncate(CANOPY_READY);
    script.actions.extend([
        walk(43.0, 28.5),
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 30,
            crouch: false,
            jump: false,
        },
        advance(60),
    ]);
    let report = script.run().expect("canopy before dispatch");
    validate_route(&report);
    assert!(last(&report)["player"]["position"][1].as_f64().expect("y") < 7.0);
    assert!(!events(&report).any(|e| e["kind"] == "fireworks_started"));

    let (_second_folder, mut script) = scenario("switchyard");
    script.actions.truncate(CANOPY_READY + 2);
    script.actions.push(Action::ClearPortals);
    script.actions.extend([walk(42.0, 29.0)]);
    script.actions.extend(shot(End::A, 42.0, 12.6, 26.0));
    let blocked = script.run().expect("canopy behind the screen");
    assert!(last(&blocked)["portals"].as_array().expect("portals").is_empty());
}

#[test]
fn both_ticket_gates_block_an_unearned_finish() {
    for (skip, end, maximum_x) in [(0, ARCHIVE, 10.0), (ARCHIVE, SIGNAL, 12.0)] {
        let (_folder, mut script) = scenario("switchyard");
        script.actions.drain(skip..end);
        // Stop at the first check in the signal room, before any aiming.
        let entry = script
            .actions
            .iter()
            .position(|a| matches!(a, Action::WalkTo { target, .. } if *target == [18.0, 5.0]))
            .expect("signal approach");
        script.actions.truncate(entry + 2);
        let report = script.run().expect("missing ticket");
        assert!(last(&report)["player"]["position"][0].as_f64().expect("x") < maximum_x);
        assert_eq!(last(&report)["player"]["checkpoint"], 0);
    }
}
