use std::time::Duration;

use common::physics::CharacterSupport;
use serde_json::json;

use super::{
    fixtures::scenario,
    playback::{Controls, Playback},
    script::Action,
};

#[test]
fn graphical_pacing_and_observation_preserve_the_headless_trace() {
    let (_folder, script) = scenario("portal_relay");
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
    let (_folder, mut script) = scenario("portal_movement");
    script.actions = vec![Action::Move {
        direction: [0.0, 0.0],
        ticks: 30,
        run: false,
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
    let (_folder, mut script) = scenario("portal_movement");
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
    let (_folder, script) = scenario("portal_relay");
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
    let (_folder, mut script) = scenario("portal_movement");
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
    let (_folder, mut script) = scenario("portal_movement");
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
    let (_folder, mut script) = scenario("portal_movement");
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
