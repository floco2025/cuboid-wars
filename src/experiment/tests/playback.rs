use std::{
    f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU},
    time::Duration,
};

use bevy::math::Vec2;
use common::{math::angle_delta_radians, physics::CharacterSupport};
use serde_json::json;
use tempfile::TempDir;

use super::{
    fixtures::{scenario, turret_room},
    playback::{Controls, Playback},
    script::{Action, Script},
};

// The Primer's first chamber: two portal shots, a run off the lobby, and the
// fling to checkpoint 1.
fn fling() -> (TempDir, Script) {
    let (folder, mut script) = scenario("portal_primer");
    script.actions.truncate(8);
    (folder, script)
}

#[test]
fn graphical_pacing_and_observation_preserve_the_headless_trace() {
    let (_folder, script) = fling();
    let expected = script.run().expect("headless route");
    for render_delta in [
        Duration::from_millis(7),
        Duration::from_millis(43),
        Duration::from_millis(250),
    ] {
        let mut playback = Playback::new(script.clone()).expect("viewer route");
        while !playback.executor.finished() {
            let state = playback.executor.session.state();
            assert_eq!(
                playback
                    .update(Controls::default(), Duration::from_secs(120))
                    .expect("pause"),
                Duration::ZERO
            );
            assert_eq!(state, playback.executor.session.state());
            playback
                .update(
                    Controls {
                        next: true,
                        ..Default::default()
                    },
                    Duration::ZERO,
                )
                .expect("start action");
            while playback.executor.running() {
                playback.update(Controls::default(), render_delta).expect("paced tick");
                // Graphical sampling must not advance the simulation or change results.
                let before = playback.executor.session.state();
                playback.take_frame();
                assert_eq!(before, playback.executor.session.state());
            }
            assert!(playback.paused);
        }
        assert_eq!(
            json!({"initial":playback.executor.initial, "steps":playback.executor.steps}),
            expected
        );
    }
}

#[test]
fn an_airborne_pause_freezes_the_owner_server_and_action_progress() {
    let (_folder, mut script) = fling();
    script.actions = vec![Action::Move {
        direction: [0.0, 0.0],
        ticks: 30,
        crouch: false,
        jump: true,
    }];
    let expected = script.run().expect("headless jump");
    let mut playback = Playback::new(script).expect("viewer jump");
    playback
        .update(
            Controls {
                next: true,
                ..Default::default()
            },
            Duration::from_millis(100),
        )
        .expect("take off");
    assert!(playback.executor.running());
    assert_ne!(playback.executor.session.owner.motion.support, CharacterSupport::Ground);
    playback
        .update(
            Controls {
                play_pause: true,
                ..Default::default()
            },
            Duration::ZERO,
        )
        .expect("pause");
    let state = playback.executor.session.state();
    let tick = playback.executor.session.tick();
    for _ in 0..4 {
        assert_eq!(
            playback
                .update(Controls::default(), Duration::from_secs(60))
                .expect("frozen frame"),
            Duration::ZERO
        );
        playback.take_frame();
        assert_eq!(state, playback.executor.session.state());
        assert_eq!(tick, playback.executor.session.tick());
        assert!(playback.executor.steps.is_empty());
    }
    playback
        .update(
            Controls {
                play_pause: true,
                ..Default::default()
            },
            Duration::from_secs(2),
        )
        .expect("resume");
    assert!(playback.paused);
    assert_eq!(json!(playback.executor.steps), expected["steps"]);
}

#[test]
fn restart_and_script_reset_clear_simulation_state_and_pending_view_frames() {
    let (_folder, mut script) = fling();
    script.actions.push(Action::Reset);
    let mut playback = Playback::new(script).expect("viewer route");
    let initial = playback.executor.initial.clone();
    let first = playback.take_frame().expect("initial frame");
    while playback.executor.steps.len() < playback.executor.script.actions.len() - 1 {
        playback
            .update(
                Controls {
                    next: true,
                    ..Default::default()
                },
                Duration::from_secs(30),
            )
            .expect("action");
        assert!(playback.paused);
    }
    assert_ne!(playback.executor.session.state(), initial);
    playback
        .update(
            Controls {
                next: true,
                ..Default::default()
            },
            Duration::ZERO,
        )
        .expect("script reset");
    assert_eq!(playback.executor.session.state(), initial);
    assert!(playback.take_frame().expect("reset frame").reset);
    playback
        .update(
            Controls {
                restart: true,
                ..Default::default()
            },
            Duration::from_secs(90),
        )
        .expect("restart");
    assert!(playback.executor.steps.is_empty());
    assert!(playback.paused);
    assert_eq!(playback.executor.session.state(), initial);
    let frame = playback.take_frame().expect("restart frame");
    assert!(frame.reset);
    assert_eq!(frame.snapshot.tick, first.snapshot.tick);
    assert!(frame.projectiles.is_empty());
    assert!(frame.snapshot.portals.is_empty());
}

#[test]
fn space_plays_the_whole_route_at_the_tick_rate_and_stops_at_completion() {
    let (_folder, script) = fling();
    let expected = script.run().expect("headless route");
    for render_delta in [
        Duration::from_millis(7),
        Duration::from_millis(43),
        Duration::from_millis(250),
    ] {
        let mut playback = Playback::new(script.clone()).expect("viewer route");
        playback
            .update(
                Controls {
                    play_pause: true,
                    ..Default::default()
                },
                Duration::ZERO,
            )
            .expect("play all");
        let mut elapsed = Duration::ZERO;
        let mut simulated = Duration::ZERO;
        for _ in 0..10_000 {
            if playback.executor.finished() {
                break;
            }
            assert!(!playback.paused, "continuous playback stopped at an action boundary");
            simulated += playback
                .update(Controls::default(), render_delta)
                .expect("automatic action");
            elapsed += render_delta;
            assert!(simulated <= elapsed, "atomic actions ran ahead of the playback clock");
            playback.take_frame();
        }
        assert!(playback.executor.finished());
        assert!(playback.paused);
        assert_eq!(
            json!({"initial": playback.executor.initial, "steps": playback.executor.steps}),
            expected
        );
        let state = playback.executor.session.state();
        playback
            .update(
                Controls {
                    play_pause: true,
                    ..Default::default()
                },
                Duration::from_secs(10),
            )
            .expect("finished play");
        assert!(playback.paused);
        assert_eq!(state, playback.executor.session.state());
    }
}

#[test]
fn enter_finishes_only_the_current_action_then_space_resumes_the_sequence() {
    let (_folder, mut script) = fling();
    script.actions = vec![
        Action::Advance { ticks: 4 },
        Action::Inspect,
        Action::Advance { ticks: 3 },
    ];
    let expected = script.run().expect("headless sequence");
    for pause_first in [false, true] {
        let mut playback = Playback::new(script.clone()).expect("viewer sequence");
        playback
            .update(
                Controls {
                    play_pause: true,
                    ..Default::default()
                },
                Duration::from_millis(70),
            )
            .expect("play");
        assert!(playback.executor.running());
        assert!(playback.executor.steps.is_empty());
        if pause_first {
            playback
                .update(
                    Controls {
                        play_pause: true,
                        ..Default::default()
                    },
                    Duration::from_secs(120),
                )
                .expect("pause all");
            let state = playback.executor.session.state();
            assert_eq!(
                playback
                    .update(Controls::default(), Duration::from_secs(120))
                    .expect("wait"),
                Duration::ZERO
            );
            assert_eq!(state, playback.executor.session.state());
        }
        playback
            .update(
                Controls {
                    next: true,
                    ..Default::default()
                },
                Duration::from_secs(10),
            )
            .expect("finish one action");
        assert!(playback.paused);
        assert_eq!(playback.executor.steps.len(), 1);
        playback
            .update(
                Controls {
                    next: true,
                    ..Default::default()
                },
                Duration::from_secs(10),
            )
            .expect("inspect only");
        assert!(playback.paused);
        assert_eq!(playback.executor.steps.len(), 2);
        playback
            .update(
                Controls {
                    play_pause: true,
                    ..Default::default()
                },
                Duration::from_secs(10),
            )
            .expect("resume all");
        assert!(playback.executor.finished());
        assert!(playback.paused);
        assert_eq!(json!(playback.executor.steps), expected["steps"]);
    }
}

#[test]
fn continuous_playback_handles_script_reset_and_restart_returns_to_paused() {
    let (_folder, mut script) = fling();
    script.actions = vec![
        Action::Advance { ticks: 1 },
        Action::Reset,
        Action::Advance { ticks: 30 },
        Action::Inspect,
    ];
    let expected = script.run().expect("headless reset sequence");
    let mut playback = Playback::new(script).expect("viewer sequence");
    playback
        .update(
            Controls {
                play_pause: true,
                ..Default::default()
            },
            Duration::from_millis(100),
        )
        .expect("play across reset");
    assert!(!playback.paused);
    assert_eq!(playback.executor.steps.len(), 2);
    assert!(playback.take_frame().expect("reset frame").reset);
    playback
        .update(Controls::default(), Duration::from_secs(10))
        .expect("finish");
    assert_eq!(json!(playback.executor.steps), expected["steps"]);
    playback
        .update(
            Controls {
                restart: true,
                ..Default::default()
            },
            Duration::from_secs(10),
        )
        .expect("restart");
    assert!(playback.paused);
    assert!(playback.executor.steps.is_empty());
    assert_eq!(
        playback
            .update(Controls::default(), Duration::from_secs(10))
            .expect("wait after restart"),
        Duration::ZERO
    );
}

#[test]
fn menu_pause_holds_continuous_playback_until_explicit_resume() {
    let (_folder, mut script) = fling();
    script.actions = vec![Action::Advance { ticks: 30 }, Action::Inspect];
    let mut playback = Playback::new(script).expect("viewer sequence");
    playback
        .update(
            Controls {
                play_pause: true,
                ..Default::default()
            },
            Duration::from_millis(100),
        )
        .expect("play");
    playback.pause();
    let state = playback.executor.session.state();
    assert_eq!(
        playback
            .update(Controls::default(), Duration::from_secs(120))
            .expect("menu pause"),
        Duration::ZERO
    );
    assert_eq!(state, playback.executor.session.state());
    playback
        .update(
            Controls {
                play_pause: true,
                ..Default::default()
            },
            Duration::from_secs(10),
        )
        .expect("resume");
    assert!(playback.executor.finished());
}

#[test]
fn the_shown_body_faces_its_travel_not_the_held_input() {
    let (_folder, mut script) = turret_room();
    let east = |ticks| Action::Move {
        direction: [1.0, 0.0],
        ticks,
        crouch: false,
        jump: false,
    };
    script.actions = vec![
        east(12),
        Action::Advance { ticks: 10 },
        east(12),
        Action::Move {
            direction: [0.0, 1.0],
            ticks: 10,
            crouch: false,
            jump: true,
        },
    ];
    let mut playback = Playback::new(script).expect("viewer route");
    let shown = |playback: &mut Playback| {
        let frame = playback.take_frame().expect("frame");
        frame.snapshot.players[0].1.movement.face_yaw
    };
    let step = |playback: &mut Playback| {
        playback
            .update(
                Controls {
                    next: true,
                    ..Default::default()
                },
                Duration::from_secs(10),
            )
            .expect("action");
    };
    assert_eq!(playback.facing, FRAC_PI_2);
    assert_eq!(shown(&mut playback), FRAC_PI_2);
    step(&mut playback);
    step(&mut playback);
    let owner = &playback.executor.session.owner.motion;
    assert_eq!(owner.horizontal_velocity.0.length(), 0.0);
    assert!(angle_delta_radians(playback.facing, FRAC_PI_2).abs() < 1e-3);
    step(&mut playback);
    step(&mut playback);
    // The jump keeps the run's eastward speed while the script steers north.
    let owner = &playback.executor.session.owner.motion;
    assert_eq!(owner.face_yaw.0, 0.0);
    assert!(
        angle_delta_radians(playback.facing, FRAC_PI_2).abs() < FRAC_PI_4,
        "facing {}",
        playback.facing
    );
    assert_eq!(shown(&mut playback), playback.facing);
}

#[test]
fn an_aim_holds_the_view_on_its_target_until_the_next_move() {
    let (_folder, mut script) = fling();
    let [x, y, z] = script.spawn;
    script.actions = vec![
        Action::Aim {
            target: [x, y, z + 10.0],
        },
        Action::Advance { ticks: 2 },
        Action::Move {
            direction: [1.0, 0.0],
            ticks: 5,
            crouch: false,
            jump: false,
        },
    ];
    let mut playback = Playback::new(script).expect("viewer route");
    let step = |playback: &mut Playback| {
        playback
            .update(
                Controls {
                    next: true,
                    ..Default::default()
                },
                Duration::from_secs(10),
            )
            .expect("action");
    };
    assert_eq!((playback.facing, playback.pitch), (FRAC_PI_2, 0.0));
    for _ in 0..2 {
        // North of the feet, so below the eye.
        step(&mut playback);
        assert!(playback.facing.abs() < 1e-3, "facing {}", playback.facing);
        assert!((-0.3..-0.1).contains(&playback.pitch), "pitch {}", playback.pitch);
    }
    let frame = playback.take_frame().expect("frame");
    assert_eq!(frame.snapshot.players[0].1.movement.face_yaw, playback.facing);
    step(&mut playback);
    assert!(angle_delta_radians(playback.facing, FRAC_PI_2).abs() < 1e-3);
    assert_eq!(playback.pitch, 0.0);
}

#[test]
fn a_mouse_look_holds_while_paused_and_the_next_control_eases_it_away() {
    let (_folder, mut script) = fling();
    script.actions = vec![Action::Inspect, Action::Inspect];
    let mut playback = Playback::new(script).expect("viewer route");
    let frame = Duration::from_millis(16);
    let player = Vec2::new(playback.facing + PI, playback.pitch);
    // More than half a turn away, as --look or the mouse would leave it.
    let looked = player + Vec2::new(3.5, -0.3);
    let mut view = looked;
    for _ in 0..50 {
        view = playback.view(view, frame);
    }
    assert!(view.distance(looked) < 1e-4, "view {view}");
    playback
        .update(
            Controls {
                next: true,
                ..Default::default()
            },
            Duration::ZERO,
        )
        .expect("instant action");
    assert!(playback.paused);
    // A hand resting on the mouse must not stop the return.
    let nudge = Vec2::new(0.01, 0.0);
    view = playback.view(view, frame);
    view = playback.view(view + nudge, frame);
    for _ in 0..300 {
        view = playback.view(view, frame);
    }
    let settled = player + Vec2::new(TAU, 0.0) + nudge;
    assert!(view.distance(settled) < 1e-3, "view {view}");
}

#[test]
fn continuous_playback_holds_after_an_aim_without_adding_ticks_and_enter_skips_the_hold() {
    let (_folder, mut script) = fling();
    let [x, y, z] = script.spawn;
    let aim = Action::Aim {
        target: [x, y, z + 10.0],
    };
    script.actions = vec![
        aim.clone(),
        Action::Advance { ticks: 3 },
        aim,
        Action::Advance { ticks: 3 },
    ];
    let expected = script.run().expect("headless sequence");
    let mut playback = Playback::new(script).expect("viewer sequence");
    playback
        .update(
            Controls {
                play_pause: true,
                ..Default::default()
            },
            Duration::ZERO,
        )
        .expect("play");
    let tick = playback.executor.session.tick();
    assert_eq!(
        playback
            .update(Controls::default(), Duration::from_millis(500))
            .expect("hold"),
        Duration::ZERO
    );
    assert!(!playback.paused);
    assert!(!playback.executor.running());
    assert_eq!(playback.executor.steps.len(), 1);
    playback
        .update(Controls::default(), Duration::from_millis(150))
        .expect("hold ends");
    assert!(playback.executor.running());
    assert_eq!(playback.executor.session.tick(), tick + 1);
    playback
        .update(Controls::default(), Duration::from_millis(100))
        .expect("second aim");
    assert!(!playback.executor.running());
    assert_eq!(playback.executor.steps.len(), 3);
    playback
        .update(
            Controls {
                next: true,
                ..Default::default()
            },
            Duration::ZERO,
        )
        .expect("skip the hold");
    assert!(playback.executor.running());
    playback
        .update(Controls::default(), Duration::from_secs(1))
        .expect("finish");
    assert!(playback.paused);
    assert_eq!(json!(playback.executor.steps), expected["steps"]);
}
