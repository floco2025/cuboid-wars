use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

use super::{
    fixtures::scenario,
    script::{Action, End, Script},
};

fn events(report: &Value) -> impl Iterator<Item = &Value> {
    report["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .flat_map(|step| step["events"].as_array().expect("events"))
}

fn omit_items(folder: &TempDir, script: &mut Script, omit: impl Fn(&Value) -> bool) {
    let mut layout: Value = serde_json::from_str(&fs::read_to_string(&script.layout).expect("layout")).expect("map");
    layout["map"]["items"]
        .as_array_mut()
        .expect("items")
        .retain(|item| !omit(item));
    script.layout = folder.path().join("variant.json");
    fs::write(&script.layout, layout.to_string()).expect("write variant");
}

fn final_flight() -> (TempDir, Script) {
    let (folder, mut script) = scenario("portal_choices");
    script.spawn = [29.0, 28.6, -29.0];
    script.actions = vec![
        Action::Advance { ticks: 2 },
        Action::Move {
            direction: [0.0, 1.0],
            ticks: 20,
            run: false,
            jump: false,
        },
        Action::Aim {
            target: [109.0, 11.0, 15.0],
        },
        Action::Portal { end: End::B },
        Action::Advance { ticks: 4 },
        Action::Aim {
            target: [29.0, 0.0, 0.0],
        },
        Action::Portal { end: End::A },
        Action::Move {
            direction: [0.0, 1.0],
            ticks: 83,
            run: true,
            jump: true,
        },
        Action::Advance { ticks: 120 },
        Action::Check {
            min: [86.0, 8.7, 12.0],
            max: [92.0, 9.0, 18.0],
            grounded: true,
        },
    ];
    (folder, script)
}

#[test]
fn decision_course_uses_both_boosts_and_suspended_erasure_to_reach_the_finish() {
    let (_folder, script) = scenario("portal_choices");
    let report = script.run().expect("decision course");
    for step in report["steps"].as_array().expect("steps") {
        match step["action"]["action"].as_str() {
            Some("check") => assert_eq!(step["result"]["status"], "passed", "{step}"),
            Some("portal") => assert_eq!(step["result"]["status"], "submitted", "{step}"),
            _ => {}
        }
    }
    let checkpoints: Vec<_> = events(&report)
        .filter(|e| e["kind"] == "checkpoint_reached")
        .map(|e| e["checkpoint"].clone())
        .collect();
    assert_eq!(checkpoints, [json!(1), json!(2), json!(3), json!(4)]);
    assert_eq!(
        events(&report)
            .filter(|e| e["kind"] == "player_portal_crossing")
            .count(),
        3
    );
    for item in ["speed", "low_gravity"] {
        assert!(events(&report).any(|e| e["kind"] == "item_collected" && e["item"] == item));
    }
    assert_eq!(events(&report).filter(|e| e["kind"] == "equipment_erased").count(), 2);
    assert!(events(&report).any(|e| e["kind"] == "fireworks_started"));
    assert!(!events(&report).any(|e| e["kind"] == "player_died" || e["kind"] == "player_fall_damage"));
    let final_player = &report["steps"].as_array().expect("steps").last().expect("finish")["state"]["player"];
    assert_eq!(final_player["portal_gun"], true);
    assert_eq!(final_player["speed"], false);
    assert_eq!(final_player["low_gravity"], false);
}

#[test]
fn lower_portal_pads_can_be_aimed_at_from_safe_grounded_approaches() {
    let (_folder, mut script) = scenario("portal_choices");
    for (spawn, target) in [
        ([-49.0, 26.4, -38.6], [-49.0, 13.2, -30.0]),
        ([29.0, 28.6, -25.0], [29.0, 0.0, 0.0]),
    ] {
        // These stances keep the capsule back from the lip. A successful shot
        // to the actual pad verifies the ledge does not hide the target.
        script.spawn = spawn;
        script.actions = vec![
            Action::Advance { ticks: 2 },
            Action::Aim { target },
            Action::Portal { end: End::A },
        ];
        let report = script.run().expect("pad sightline from approach");
        let shot = &report["steps"][2];
        assert_eq!(shot["state"]["player"]["support"], "ground");
        assert_eq!(shot["result"]["status"], "submitted", "{spawn:?}: {}", shot["result"]);
        for (axis, coordinate) in target.into_iter().enumerate() {
            assert!(
                (shot["result"]["portal"]["position"][axis]
                    .as_f64()
                    .expect("hit coordinate")
                    - f64::from(coordinate))
                .abs()
                    < 0.01
            );
        }
        let vertical = report["steps"][1]["result"]["direction"][1]
            .as_f64()
            .expect("aim direction");
        assert!(
            vertical.abs() <= 60.0_f64.to_radians().sin(),
            "{spawn:?}: aim is too steep"
        );
    }
}

#[test]
fn first_drop_accepts_a_range_of_run_off_timings() {
    let (_folder, mut script) = scenario("portal_choices");
    script.actions.truncate(10);
    for ticks in [26, 28, 30] {
        script.actions[7] = Action::Move {
            direction: [0.0, 1.0],
            ticks,
            run: true,
            jump: false,
        };
        let report = script.run().expect("run off toward the offset first pad");
        assert_eq!(
            report["steps"][9]["result"]["status"], "passed",
            "ticks={ticks}: {}",
            report["steps"][9]["state"]
        );
        assert_eq!(report["steps"][9]["state"]["player"]["checkpoint"], 1);
    }
}

#[test]
fn first_exit_requires_a_shot_from_outside_the_launch_platform() {
    let (_folder, mut script) = scenario("portal_choices");
    script.actions = vec![
        Action::Aim {
            target: [-23.0, 23.6, -16.1],
        },
        Action::Portal { end: End::B },
    ];
    for x in [-53.0, -49.0, -45.0] {
        for z in [-59.0, -40.0, -38.2] {
            script.spawn = [x, 26.4, z];
            let report = script.run().expect("shot from launch area");
            assert_ne!(
                report["steps"][1]["result"]["status"], "submitted",
                "{x}, {z}: {report}"
            );
        }
    }
    script.spawn = [-21.0, 26.4, -59.0];
    let report = script.run().expect("shot from preparation balcony");
    assert_eq!(report["steps"][1]["result"]["status"], "submitted");
}

#[test]
fn the_starting_balcony_cannot_jump_or_drop_directly_to_the_first_landing() {
    let (_folder, mut script) = scenario("portal_choices");
    for (spawn, direction) in [
        ([-44.4, 26.4, -38.4], [1.0, 0.0]),
        ([-44.4, 26.4, -56.4], [1.0, 1.0]),
        ([-28.0, 26.4, -56.4], [0.0, 1.0]),
        ([-18.4, 26.4, -56.4], [0.0, 1.0]),
    ] {
        script.spawn = spawn;
        for jump in [false, true] {
            script.actions = vec![
                Action::Advance { ticks: 2 },
                Action::Move {
                    direction,
                    ticks: 90,
                    run: true,
                    jump,
                },
            ];
            let report = script.run().expect("attempted balcony shortcut");
            assert_eq!(report["steps"][0]["state"]["player"]["support"], "ground");
            assert!(
                !events(&report).any(|e| e["kind"] == "player_step"
                    && e["support"] == "ground"
                    && (e["position"][1].as_f64().expect("height") - 17.6).abs() < 0.1),
                "spawn={spawn:?}, jump={jump}"
            );
        }
    }
}

#[test]
fn the_high_ledge_cannot_jump_directly_to_the_finish_with_low_gravity() {
    let (_folder, mut script) = final_flight();
    script.actions = vec![
        Action::Advance { ticks: 2 },
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 33,
            run: false,
            jump: false,
        },
        Action::Move {
            direction: [0.0, 1.0],
            ticks: 23,
            run: false,
            jump: false,
        },
        Action::Move {
            direction: [53.4, 39.4],
            ticks: 180,
            run: true,
            jump: true,
        },
    ];
    let report = script.run().expect("attempted finish shortcut");
    let launch = &report["steps"][2]["state"]["player"];
    assert_eq!(launch["support"], "ground");
    assert_eq!(launch["low_gravity"], true);
    assert!(!events(&report).any(|e| e["kind"] == "checkpoint_reached" && e["checkpoint"] == 4));
    assert!(!events(&report).any(|e| e["kind"] == "player_step"
        && e["support"] == "ground"
        && e["position"][0].as_f64().expect("x coordinate") >= 86.0));
}

#[test]
fn the_entry_runway_refills_speed_before_the_second_launch() {
    let (_folder, mut script) = scenario("portal_choices");
    script.spawn = [-25.0, 17.6, -33.0];
    script.actions = vec![
        Action::Advance { ticks: 2 },
        Action::Aim {
            target: [17.0, 19.8, -29.0],
        },
        Action::Portal { end: End::B },
        Action::Move {
            direction: [0.0, 1.0],
            ticks: 20,
            run: false,
            jump: false,
        },
        Action::Aim {
            target: [-16.1, 19.22, -29.0],
        },
        Action::Portal { end: End::A },
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 48,
            run: true,
            jump: false,
        },
        Action::Advance { ticks: 30 },
        Action::Check {
            min: [0.0, 21.9, -32.0],
            max: [12.0, 22.2, -24.0],
            grounded: true,
        },
    ];
    let report = script.run().expect("refill on approach");
    assert_eq!(report["steps"][0]["state"]["player"]["speed"], false);
    assert!(events(&report).any(|e| e["kind"] == "item_collected" && e["item"] == "speed"));
    assert_eq!(
        report["steps"][8]["result"]["status"], "passed",
        "{}",
        report["steps"][8]["state"]
    );
}

#[test]
fn running_without_speed_cannot_reach_the_ramp_landing() {
    let (folder, mut script) = scenario("portal_choices");
    omit_items(&folder, &mut script, |item| item["type"] == "speed");
    script.actions.truncate(18);
    // Match the approach distance at ordinary walking speed, so this tests
    // launch energy rather than missing the entrance with a slower approach.
    script.actions[12] = Action::Move {
        direction: [0.0, 1.0],
        ticks: 22,
        run: false,
        jump: false,
    };
    for ticks in [50, 60, 70] {
        script.actions[15] = Action::Move {
            direction: [1.0, 0.0],
            ticks,
            run: true,
            jump: false,
        };
        let report = script.run().expect("unboosted launch");
        assert_eq!(
            events(&report)
                .filter(|e| e["kind"] == "player_portal_crossing")
                .count(),
            2
        );
        assert!(!events(&report).any(|e| e["kind"] == "checkpoint_reached" && e["checkpoint"] == 2));
        assert_eq!(report["steps"][17]["result"]["status"], "failed");
    }
}

#[test]
fn low_gravity_is_required_to_jump_to_the_high_drop_ledge() {
    let (folder, mut script) = scenario("portal_choices");
    script.spawn = [9.0, 22.0, -29.0];
    script.actions = vec![
        Action::Advance { ticks: 2 },
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 10,
            run: false,
            jump: false,
        },
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 60,
            run: true,
            jump: true,
        },
        Action::Advance { ticks: 35 },
        Action::Check {
            min: [28.0, 28.5, -32.0],
            max: [36.0, 28.8, -24.0],
            grounded: true,
        },
    ];
    let boosted = script.run().expect("low gravity jump");
    assert_eq!(boosted["initial"]["player"]["speed"], false);
    assert_eq!(boosted["steps"][1]["state"]["player"]["low_gravity"], true);
    assert!(
        boosted["steps"]
            .as_array()
            .expect("jump steps")
            .iter()
            .all(|step| step["state"]["player"]["speed"] == false)
    );
    assert_eq!(boosted["steps"][4]["result"]["status"], "passed", "{boosted}");
    omit_items(&folder, &mut script, |item| item["type"] == "low_gravity");
    let ordinary = script.run().expect("ordinary jump");
    assert_eq!(ordinary["steps"][4]["result"]["status"], "failed");
    assert!(!events(&ordinary).any(|e| e["kind"] == "checkpoint_reached" && e["checkpoint"] == 3));
}

#[test]
fn final_flight_tolerates_placement_and_jump_approach_variation() {
    let (_folder, mut script) = final_flight();
    for x in [108.8, 109.0, 109.2] {
        for ticks in [80, 83, 86] {
            script.actions[2] = Action::Aim {
                target: [x, 11.0, 15.0],
            };
            script.actions[7] = Action::Move {
                direction: [0.0, 1.0],
                ticks,
                run: true,
                jump: true,
            };
            let report = script.run().expect("varied final flight");
            assert_eq!(
                report["steps"][9]["result"]["status"], "passed",
                "x={x}, ticks={ticks}: {}",
                report["steps"][9]["state"]
            );
            assert!(events(&report).any(|e| e["kind"] == "equipment_erased"));
        }
    }
}

#[test]
fn final_jump_reaches_the_portal_and_finish_at_walking_speed_without_a_speed_pickup() {
    let (folder, mut script) = final_flight();
    // Exercise the shipped settings as a player launches them, including the
    // global configuration, and make collecting a speed boost impossible.
    script.gameplay = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config/server/gameplay.json");
    omit_items(&folder, &mut script, |item| item["type"] == "speed");
    for ticks in [122, 125, 128] {
        script.actions[7] = Action::Move {
            direction: [0.0, 1.0],
            ticks,
            run: false,
            jump: true,
        };
        let report = script.run().expect("walking jump with low gravity only");
        assert_eq!(report["initial"]["player"]["speed"], false);
        assert_eq!(report["steps"][6]["state"]["player"]["low_gravity"], true);
        assert!(
            report["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .all(|step| step["state"]["player"]["speed"] == false)
        );
        assert!(events(&report).any(|e| e["kind"] == "jump" && e["accepted"] == true));
        assert!(events(&report).any(|e| e["kind"] == "player_portal_crossing"));
        assert_eq!(
            report["steps"][9]["result"]["status"], "passed",
            "ticks={ticks}: {}",
            report["steps"][9]["state"]
        );
        assert_eq!(report["steps"][9]["state"]["player"]["checkpoint"], 4);
        assert!(!events(&report).any(|e| e["kind"] == "player_died" || e["kind"] == "player_fall_damage"));
    }
}

#[test]
fn keeping_low_gravity_overshoots_even_with_full_countersteering() {
    let (folder, mut script) = final_flight();
    omit_items(&folder, &mut script, |item| {
        item["type"] == "equipment_eraser" && item["level"] == 5
    });
    let baseline = script.run().expect("unsteered flight without eraser");
    let crossing_tick = events(&baseline)
        .find(|e| e["kind"] == "player_portal_crossing")
        .expect("crossed the floor portal")["tick"]
        .as_u64()
        .expect("crossing tick");
    let start_tick = baseline["steps"][7]["state"]["tick"].as_u64().expect("start tick");
    // Begin countersteering immediately after transit, so the approach still
    // enters the portal and every variant tests the actual exit trajectory.
    script.actions[8] = Action::Advance {
        ticks: u32::try_from(crossing_tick - start_tick).expect("flight ticks"),
    };
    script.actions.insert(9, Action::Inspect);
    for direction in [[0.0, 0.0], [1.0, 0.0], [-1.0, 0.0]] {
        script.actions[9] = Action::Move {
            direction,
            ticks: 180,
            run: true,
            jump: false,
        };
        let report = script.run().expect("no suspended eraser");
        assert!(events(&report).any(|e| e["kind"] == "player_portal_crossing"));
        assert!(!events(&report).any(|e| e["kind"] == "equipment_erased"));
        assert!(
            !events(&report).any(|e| e["kind"] == "checkpoint_reached" && e["checkpoint"] == 4),
            "{}",
            report["steps"][10]["state"]
        );
        assert_eq!(report["steps"][10]["result"]["status"], "failed");
    }
}

#[test]
fn other_ramp_angles_and_a_normal_gravity_drop_miss_the_final_landing() {
    let (folder, mut script) = final_flight();
    script.actions[8] = Action::Advance { ticks: 300 };
    for target in [[89.0, 11.0, 31.0], [89.0, 11.0, 3.0]] {
        script.actions[2] = Action::Aim { target };
        let report = script.run().expect("other ramp exit");
        assert_eq!(
            report["steps"][3]["result"]["status"], "submitted",
            "{}",
            report["steps"][3]["result"]
        );
        assert!(
            events(&report).any(|e| e["kind"] == "player_portal_crossing"),
            "{}",
            report["steps"][6]["result"]
        );
        // Both exits aim across the actual finish, so the failure is launch height,
        // not being in a parallel lane with no landing to reach.
        assert!(events(&report).any(|e| e["kind"] == "player_step"
            && (86.0..=92.0).contains(&e["position"][0].as_f64().expect("x coordinate"))
            && (12.0..=18.0).contains(&e["position"][2].as_f64().expect("z coordinate"))
            && e["position"][1].as_f64().expect("height") > 20.0));
        assert!(!events(&report).any(|e| e["kind"] == "checkpoint_reached" && e["checkpoint"] == 4));
        if target[2] > 15.0 {
            assert!(events(&report).any(|e| e["kind"] == "item_collected" && e["item"] == "speed"));
        }
        assert_eq!(report["steps"][9]["result"]["status"], "failed");
    }
    script.actions[2] = Action::Aim {
        target: [109.0, 11.0, 15.0],
    };
    // Start directly above the entrance to isolate exit energy. Without low
    // gravity, the new approach jump cannot span the gap to this pad either.
    script.spawn = [29.0, 28.6, 0.0];
    script.actions[1] = Action::Advance { ticks: 1 };
    script.actions[7] = Action::Advance { ticks: 1 };
    omit_items(&folder, &mut script, |item| item["type"] == "low_gravity");
    let report = script.run().expect("full gravity fall");
    assert!(events(&report).any(|e| e["kind"] == "player_portal_crossing"));
    assert!(!events(&report).any(|e| e["kind"] == "checkpoint_reached" && e["checkpoint"] == 4));
    assert_eq!(report["steps"][9]["result"]["status"], "failed");
}

#[test]
fn every_checkpoint_recovers_from_a_missed_jump_with_pickups_available() {
    for (end, checkpoint) in [(10, 1), (18, 2), (22, 3), (31, 4)] {
        let (_folder, mut script) = scenario("portal_choices");
        script.actions.truncate(end);
        script.actions.extend([
            Action::Move {
                direction: [0.0, -1.0],
                ticks: 90,
                run: true,
                jump: false,
            },
            Action::Advance { ticks: 300 },
        ]);
        let mut executor = super::executor::Executor::new(script).expect("recovery course");
        while !executor.finished() {
            executor.start_next().expect("next action");
            while executor.running() {
                executor.tick().expect("tick");
            }
        }
        let player = &executor.steps.last().expect("respawn")["state"]["player"];
        assert_eq!(player["checkpoint"], checkpoint, "{player}");
        assert_eq!(player["generation"], 1, "{player}");
        assert_eq!(player["support"], "ground", "{player}");
        assert_eq!(player["portal_gun"], true);
        assert!(
            executor
                .session
                .server
                .world()
                .resource::<server::items::ItemMap>()
                .values()
                .all(|item| !item.is_hidden())
        );
    }
}

#[test]
fn losing_speed_after_the_upper_eraser_has_a_walkable_refill_and_retry() {
    let (_folder, mut script) = scenario("portal_choices");
    script.actions.truncate(18);
    script.actions.extend([
        Action::Move {
            direction: [-1.0, 0.0],
            ticks: 20,
            run: false,
            jump: false,
        },
        Action::Advance { ticks: 60 },
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 50,
            run: false,
            jump: false,
        },
        Action::Aim {
            target: [17.0, 19.8, -29.0],
        },
        Action::Portal { end: End::B },
        Action::Advance { ticks: 4 },
        Action::Aim {
            target: [21.9, 14.82, -29.0],
        },
        Action::Portal { end: End::A },
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 50,
            run: true,
            jump: false,
        },
        Action::Advance { ticks: 30 },
    ]);
    let report = script.run().expect("retry");
    assert_eq!(report["steps"][17]["state"]["player"]["speed"], false);
    assert_eq!(report["steps"][19]["state"]["player"]["support"], "ground");
    assert_eq!(report["steps"][20]["state"]["player"]["speed"], true);
    assert_eq!(report["steps"][22]["result"]["status"], "submitted");
    assert_eq!(report["steps"][25]["result"]["status"], "submitted");
    let player = &report["steps"][27]["state"]["player"];
    assert_eq!(player["support"], "ground");
    assert_eq!(player["checkpoint"], 2);
    assert!((player["position"][1].as_f64().expect("height") - 22.0).abs() < 0.01);
    assert_eq!(
        events(&report)
            .filter(|e| e["kind"] == "player_portal_crossing")
            .count(),
        3
    );
    assert!(!events(&report).any(|e| e["kind"] == "player_died" || e["kind"] == "player_fall_damage"));
}

#[test]
fn an_unboosted_player_on_the_ramp_can_drop_to_the_refill_and_retry() {
    let (_folder, mut script) = scenario("portal_choices");
    script.spawn = [17.0, 20.3, -29.0];
    script.actions = vec![
        Action::Advance { ticks: 30 },
        Action::Move {
            direction: [-1.0, 0.0],
            ticks: 40,
            run: false,
            jump: false,
        },
        Action::Advance { ticks: 60 },
        Action::Aim {
            target: [17.0, 19.8, -29.0],
        },
        Action::Portal { end: End::B },
        Action::Advance { ticks: 4 },
        Action::Aim {
            target: [21.9, 14.82, -29.0],
        },
        Action::Portal { end: End::A },
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 49,
            run: true,
            jump: false,
        },
        Action::Advance { ticks: 30 },
        Action::Check {
            min: [0.0, 21.9, -32.0],
            max: [12.0, 22.2, -24.0],
            grounded: true,
        },
    ];
    let report = script.run().expect("retry from ramp");
    assert_eq!(report["steps"][0]["state"]["player"]["speed"], false);
    assert_eq!(
        report["steps"][2]["state"]["player"]["speed"], true,
        "{}",
        report["steps"][2]["state"]
    );
    assert_eq!(report["steps"][4]["result"]["status"], "submitted");
    assert_eq!(report["steps"][7]["result"]["status"], "submitted");
    assert_eq!(
        report["steps"][10]["result"]["status"], "passed",
        "{}",
        report["steps"][10]["state"]
    );
    assert!(!events(&report).any(|e| e["kind"] == "player_died" || e["kind"] == "player_fall_damage"));
}

#[test]
fn final_sprint_approach_accepts_varied_takeoffs_and_a_missed_jump() {
    let (folder, mut script) = final_flight();
    script.gameplay = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config/server/gameplay.json");
    omit_items(&folder, &mut script, |item| item["type"] == "speed");
    script.actions.insert(7, Action::Advance { ticks: 1 });
    for (reposition, approach_ticks) in [(-30_i32, 103), (-20, 97), (-10, 90), (0, 83), (3, 81)] {
        script.actions[7] = if reposition == 0 {
            Action::Advance { ticks: 1 }
        } else {
            Action::Move {
                direction: [0.0, if reposition < 0 { -1.0 } else { 1.0 }],
                ticks: reposition.unsigned_abs(),
                run: false,
                jump: false,
            }
        };
        for jump in [true, false] {
            script.actions[8] = Action::Move {
                direction: [0.0, 1.0],
                ticks: approach_ticks,
                run: true,
                jump,
            };
            let report = script.run().expect("takeoff");
            assert_eq!(report["steps"][7]["state"]["player"]["support"], "ground");
            assert_eq!(report["steps"][7]["state"]["player"]["low_gravity"], true);
            assert!(
                report["steps"]
                    .as_array()
                    .expect("steps")
                    .iter()
                    .all(|step| step["state"]["player"]["speed"] == false)
            );
            assert!(events(&report).any(|e| e["kind"] == "player_portal_crossing"));
            assert_eq!(
                report["steps"][10]["result"]["status"], "passed",
                "reposition={reposition}, jump={jump}: {}",
                report["steps"][10]["state"]
            );
            assert_eq!(report["steps"][10]["state"]["player"]["checkpoint"], 4);
            assert!(!events(&report).any(|e| e["kind"] == "player_died" || e["kind"] == "player_fall_damage"));
        }
    }
}
