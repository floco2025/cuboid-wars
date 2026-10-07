use std::time::Duration;

use common::{
    map::Carriers,
    protocol::{CarrierId, Position},
};
use serde_json::json;
use tempfile::TempDir;

use super::{
    fixtures::chamber,
    playback::{Controls, Playback},
    player::Owner,
    script::{Action, End, Script},
    session::Session,
};

fn moving_room() -> (TempDir, Script) {
    chamber(
        json!({"map": {
            "fireworks": null, "grid_cols": 8, "grid_rows": 8,
            "checkpoints": [{"level": 0, "cols": [0, 1], "rows": [0, 1], "type": "individual", "number": 0}],
            "levels": [{"floors": [{"col": 0, "row": 0, "all": "solid"}]}],
            "nested_maps": [{"map": "car", "level": 0, "from": [2, 2], "to": [4, 2], "travel_secs": 2.0, "pause_secs": 1.0}],
            "nested_geometry": {"car": {
                "grid_cols": 2, "grid_rows": 2,
                "levels": [{"floors": [
                    {"col": 0, "row": 0, "all": "solid"}, {"col": 1, "row": 0, "all": "solid"},
                    {"col": 0, "row": 1, "all": "solid"}, {"col": 1, "row": 1, "all": "solid"}
                ], "walls": [
                    {"c0": 0, "r0": 0, "c1": 1, "r1": 0, "all": "portal"},
                    {"c0": 1, "r0": 0, "c1": 2, "r1": 0, "all": "portal"}
                ]}]
            }}
        }}),
        [-4.0, 0.0, -4.0],
    )
}

#[test]
fn a_rider_and_its_portal_follow_a_carrier_through_travel_rest_and_reversal() {
    let (_folder, mut script) = moving_room();
    script.actions = vec![
        Action::Aim {
            target: [-4.0, 1.4, -8.0],
        },
        Action::Portal { end: End::A },
        Action::Advance { ticks: 20 },
        Action::Advance { ticks: 50 },
        Action::Advance { ticks: 60 },
    ];
    let report = script.run().expect("moving room");
    assert_eq!(report["steps"][1]["result"]["status"], "submitted");
    let initial = &report["steps"][1]["state"];
    let mut positions = Vec::new();
    for step in report["steps"].as_array().expect("steps").iter().skip(2) {
        let state = &step["state"];
        let carrier_delta = state["carriers"][0]["position"][0].as_f64().expect("carrier x")
            - initial["carriers"][0]["position"][0]
                .as_f64()
                .expect("initial carrier x");
        let portal_delta = state["portals"][0]["position"][0].as_f64().expect("portal x")
            - initial["portals"][0]["position"][0].as_f64().expect("initial portal x");
        let player_delta = state["player"]["position"][0].as_f64().expect("player x")
            - initial["player"]["position"][0].as_f64().expect("initial player x");
        let observed_delta = state["carriers"][0]["previous_position"][0]
            .as_f64()
            .expect("observed carrier x")
            - initial["carriers"][0]["previous_position"][0]
                .as_f64()
                .expect("initial observed carrier x");
        assert!((portal_delta - carrier_delta).abs() < 0.001);
        // The owner observes the completed tick, then the server advances:
        // its previous pose is the pose this owner step used, even at reversal.
        assert!((player_delta - observed_delta).abs() < 0.001, "{state}");
        assert_eq!(state["player"]["support"], "ground");
        assert_eq!(state["player"]["health"], 500.0);
        positions.push(carrier_delta);
    }
    assert!(positions[1] > positions[0] + 4.0);
    assert!(positions[2] < positions[1] - 4.0);
}

#[test]
fn a_carrier_local_body_resolves_to_world_space_on_creation_and_relocation() {
    let (_folder, script) = moving_room();
    let mut session = Session::new(&script).expect("moving session");
    for _ in 0..20 {
        session.advance().expect("ride");
    }
    let snapshot = server::network::capture_snapshot(session.server.world_mut());
    let (_, player) = snapshot
        .players
        .iter()
        .find(|(id, _)| *id == session.id)
        .expect("player");
    assert_ne!(player.movement.carrier, CarrierId::WORLD);
    let carriers = session.server.world().resource::<Carriers>();
    let expected = carriers
        .pose(player.movement.carrier)
        .transform_position(&player.movement.pos);
    let mut owner = Owner::new(player, carriers);
    assert!((owner.position.x - expected.x).abs() < 0.001);
    owner.teleport(Position {
        x: 100.0,
        y: 0.0,
        z: 100.0,
    });
    owner.relocate(player, carriers);
    assert!((owner.position.x - expected.x).abs() < 0.001);
    assert!((owner.position.z - expected.z).abs() < 0.001);
}

#[test]
fn pausing_graphical_playback_freezes_carriers_and_resumes_the_same_route() {
    let (_folder, mut script) = moving_room();
    script.actions = vec![Action::Advance { ticks: 50 }, Action::Advance { ticks: 80 }];
    let expected = script.run().expect("headless ride");
    let mut playback = Playback::new(script).expect("playback ride");
    while !playback.executor.finished() {
        let before = playback.executor.session.state();
        assert_eq!(
            playback
                .update(Controls::default(), Duration::from_secs(60))
                .expect("pause"),
            Duration::ZERO
        );
        assert_eq!(playback.executor.session.state(), before);
        playback
            .update(
                Controls {
                    next: true,
                    ..Default::default()
                },
                Duration::ZERO,
            )
            .expect("next");
        while playback.executor.running() {
            playback
                .update(Controls::default(), Duration::from_millis(43))
                .expect("frame");
            playback.take_frame();
        }
    }
    assert_eq!(
        json!({"initial": playback.executor.initial, "steps": playback.executor.steps}),
        expected
    );
}
