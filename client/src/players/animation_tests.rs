use std::f32::consts::{FRAC_PI_2, PI};

use bevy::{
    animation::{AnimationTargetId, RepeatAnimation, graph::AnimationNodeType},
    prelude::*,
};
use common::{
    physics::{CharacterMovementResult, CharacterSupport},
    protocol::{MapSettings, PlayerId, PlayerMoveIntent, Position},
};

use super::{
    PlayerMap,
    animation::{
        AnimationState, PlayerAnimationMotion, PlayerAnimationPlayback, PlayerAnimationSource, PlayerClip, PlayerModel,
        climb_playback_rate, player_animation_setup_system, player_animation_update_system,
    },
};
use crate::{
    characters::load_character_model,
    config::ModelDef,
    constants::{LADDER_RUNG_SPACING, PLAYER_ANIMATION_CLIMB_RUNGS_PER_CYCLE, PLAYER_ANIMATION_RUN_SPEED},
    test_assets::{headless_asset_app, settle},
    test_fixtures::map_settings,
};

fn choose(
    state: &mut AnimationState,
    support: CharacterSupport,
    velocity: Vec3,
    intent: PlayerMoveIntent,
    finished: bool,
) -> (PlayerClip, f32) {
    let result = state.select(
        PlayerAnimationMotion { support, velocity },
        intent.is_running(),
        velocity,
        false,
        finished,
        0.12,
    );
    state.clip = result.0;
    result
}

#[test]
fn locomotion_distinguishes_walking_running_backwards_and_strafing() {
    let mut state = AnimationState::default();
    for (velocity, intent, expected, backwards) in [
        (
            Vec3::ZERO,
            PlayerMoveIntent::Running { direction: 0.0 },
            PlayerClip::Idle,
            false,
        ),
        (
            Vec3::Z * 3.0,
            PlayerMoveIntent::Walking { direction: 0.0 },
            PlayerClip::Walk,
            false,
        ),
        (
            Vec3::Z * 5.0,
            PlayerMoveIntent::Running { direction: 0.0 },
            PlayerClip::Run,
            false,
        ),
        (
            -Vec3::Z * 3.0,
            PlayerMoveIntent::Walking { direction: PI },
            PlayerClip::Walk,
            true,
        ),
        (
            Vec3::X * 3.0,
            PlayerMoveIntent::Walking { direction: FRAC_PI_2 },
            PlayerClip::StrafeLeft,
            false,
        ),
        (
            -Vec3::X * 3.0,
            PlayerMoveIntent::Walking { direction: -FRAC_PI_2 },
            PlayerClip::StrafeRight,
            false,
        ),
    ] {
        let (clip, speed) = choose(&mut state, CharacterSupport::Ground, velocity, intent, false);
        assert_eq!(clip, expected);
        assert_eq!(speed < 0.0, backwards);
    }
}

#[test]
fn ladder_pose_holds_and_reverses_without_becoming_a_jump_or_fall() {
    let mut state = AnimationState::default();
    for speed in [2.0, 0.0, -2.0] {
        let (clip, _) = choose(
            &mut state,
            CharacterSupport::Ladder,
            Vec3::Y * speed,
            PlayerMoveIntent::Idle,
            false,
        );
        assert_eq!(clip, PlayerClip::Climb);
    }
    assert_eq!(
        choose(
            &mut state,
            CharacterSupport::Ground,
            Vec3::ZERO,
            PlayerMoveIntent::Idle,
            false
        )
        .0,
        PlayerClip::Idle
    );
}

#[test]
fn climb_cadence_tracks_rungs_independently_of_clip_duration() {
    for duration in [1.0, 2.4, 3.0] {
        for speed in [-3.6_f32, -2.4, 0.0, 2.4, 3.6, 5.4] {
            let rate = climb_playback_rate(speed, duration);
            if speed == 0.0 {
                assert_eq!(rate, 0.0);
                continue;
            }
            let seconds_per_step = duration / (rate.abs() * PLAYER_ANIMATION_CLIMB_RUNGS_PER_CYCLE);
            assert!((speed.abs() * seconds_per_step - LADDER_RUNG_SPACING).abs() < 0.0001);
            assert_eq!(rate.is_sign_negative(), speed.is_sign_negative());
        }
    }
}

#[test]
fn moving_landings_and_movement_during_recovery_resume_locomotion() {
    for (intent, velocity, expected) in [
        (
            PlayerMoveIntent::Walking { direction: 0.0 },
            Vec3::Z * 3.0,
            PlayerClip::Walk,
        ),
        (
            PlayerMoveIntent::Running { direction: 0.0 },
            Vec3::Z * 5.0,
            PlayerClip::Run,
        ),
        (
            PlayerMoveIntent::Walking { direction: FRAC_PI_2 },
            Vec3::X * 3.0,
            PlayerClip::StrafeLeft,
        ),
    ] {
        for initial_clip in [PlayerClip::Jump, PlayerClip::Fall, PlayerClip::Land] {
            let mut state = AnimationState {
                clip: initial_clip,
                support: if initial_clip == PlayerClip::Land {
                    CharacterSupport::Ground
                } else {
                    CharacterSupport::Airborne
                },
                airborne_secs: 0.5,
            };
            assert_eq!(
                choose(&mut state, CharacterSupport::Ground, velocity, intent, false).0,
                expected
            );
            assert_eq!(
                choose(&mut state, CharacterSupport::Ground, velocity, intent, false).0,
                expected
            );
        }
    }
}

#[test]
fn jump_holds_through_apex_falls_lands_once_and_can_jump_again() {
    let mut state = AnimationState::default();
    for (support, velocity, finished, expected) in [
        (CharacterSupport::Airborne, 4.0, false, PlayerClip::Jump),
        (CharacterSupport::Airborne, 0.0, true, PlayerClip::Jump),
        (CharacterSupport::Airborne, -2.0, true, PlayerClip::Fall),
        (CharacterSupport::Ground, 0.0, false, PlayerClip::Land),
        (CharacterSupport::Ground, 0.0, false, PlayerClip::Land),
        (CharacterSupport::Ground, 0.0, true, PlayerClip::Idle),
        (CharacterSupport::Airborne, 4.0, false, PlayerClip::Jump),
    ] {
        assert_eq!(
            choose(
                &mut state,
                support,
                Vec3::Y * velocity,
                PlayerMoveIntent::Idle,
                finished
            )
            .0,
            expected
        );
    }
}

#[test]
fn walking_off_an_edge_falls_and_airborne_motion_takes_priority_over_stun() {
    let mut state = AnimationState::default();
    for (support, velocity, expected) in [
        (CharacterSupport::Airborne, -1.0, PlayerClip::Fall),
        (CharacterSupport::Ladder, 0.0, PlayerClip::Climb),
        (CharacterSupport::Ground, 0.0, PlayerClip::Stunned),
    ] {
        let (clip, _) = state.select(
            PlayerAnimationMotion {
                support,
                velocity: Vec3::Y * velocity,
            },
            true,
            Vec3::Y * velocity,
            true,
            false,
            0.12,
        );
        assert_eq!(clip, expected);
        state.clip = clip;
    }
}

#[test]
fn carrier_motion_and_reconciliation_do_not_drive_footsteps() {
    let start = Position::default();
    let mut motion = PlayerAnimationMotion::default();
    let step = CharacterMovementResult {
        impact_speed: 0.0,
        grounding: Default::default(),
        position: Position { x: 1.1, y: 0.2, z: 0.0 },
        vertical_velocity: 0.0,
        support: CharacterSupport::Ground,
        blocked: false,
        floor_velocity: Vec3::new(10.0, 2.0, 0.0),
        lifted: true,
        crushed: false,
    };
    motion.record_step(start, &step, Vec3::ZERO, Vec3::X * 0.1, 0.1);
    assert_eq!(motion.velocity, Vec3::ZERO);
    motion.record_step(start, &step, Vec3::X * 3.0, Vec3::X * 0.1, 0.1);
    assert!(motion.velocity.length() < 1e-5);
    let walking = CharacterMovementResult {
        position: Position {
            x: 1.4,
            ..step.position
        },
        ..step
    };
    motion.record_step(start, &walking, Vec3::X * 3.0, Vec3::X * 0.1, 0.1);
    assert!((motion.velocity.x - 3.0).abs() < 1e-5);
    motion.block_horizontal();
    assert_eq!(motion.velocity, Vec3::ZERO);
}

#[test]
fn playback_follows_map_speeds_without_restarting_each_frame() {
    let mut app = App::new();
    app.insert_resource(Time::<()>::default());
    app.insert_resource(map_settings());
    app.init_resource::<PlayerMap>();
    app.init_resource::<Assets<AnimationClip>>();
    app.add_systems(Update, player_animation_update_system);
    let owner = app
        .world_mut()
        .spawn((
            PlayerId(1),
            PlayerAnimationMotion {
                support: CharacterSupport::Ground,
                velocity: Vec3::Z * 3.0,
            },
            PlayerMoveIntent::Walking { direction: 0.0 },
            Transform::IDENTITY,
        ))
        .id();
    let clips: Vec<_> = (1..=PlayerClip::ALL.len()).map(AnimationNodeIndex::new).collect();
    let walk = clips[PlayerClip::Walk as usize];
    let rig = app
        .world_mut()
        .spawn((
            AnimationPlayer::default(),
            AnimationTransitions::new(),
            PlayerAnimationPlayback {
                source: PlayerAnimationSource {
                    owner,
                    graph: Handle::default(),
                    clips,
                    climb_clip: Handle::default(),
                },
                state: AnimationState::default(),
            },
        ))
        .id();
    app.update();
    app.world_mut()
        .get_mut::<AnimationPlayer>(rig)
        .expect("rig animation player missing")
        .animation_mut(walk)
        .expect("walk animation missing")
        .set_seek_time(0.3);
    app.update();
    let player = app
        .world()
        .get::<AnimationPlayer>(rig)
        .expect("rig animation player missing");
    assert_eq!(player.animation(walk).expect("walk animation missing").seek_time(), 0.3);
    for (run_speed, intent, velocity, expected) in [
        (
            4.0,
            PlayerMoveIntent::Walking { direction: 0.0 },
            Vec3::Z * 3.0,
            PlayerClip::Run,
        ),
        (
            7.0,
            PlayerMoveIntent::Walking { direction: 0.0 },
            Vec3::Z * 3.0,
            PlayerClip::Walk,
        ),
        (
            7.0,
            PlayerMoveIntent::Running { direction: 0.0 },
            Vec3::Z * 3.0,
            PlayerClip::Run,
        ),
        (
            4.0,
            PlayerMoveIntent::Walking { direction: PI },
            -Vec3::Z * 3.0,
            PlayerClip::Run,
        ),
        (
            4.0,
            PlayerMoveIntent::Walking { direction: FRAC_PI_2 },
            Vec3::X * 3.0,
            PlayerClip::StrafeLeft,
        ),
        (4.0, PlayerMoveIntent::Idle, Vec3::ZERO, PlayerClip::Idle),
    ] {
        app.world_mut().resource_mut::<MapSettings>().movement.player.run_speed = run_speed;
        app.world_mut().entity_mut(owner).insert((
            intent,
            PlayerAnimationMotion {
                support: CharacterSupport::Ground,
                velocity,
            },
        ));
        app.update();
        let playback = app
            .world()
            .get::<PlayerAnimationPlayback>(rig)
            .expect("rig playback missing");
        assert_eq!(playback.state.clip, expected);
        if expected == PlayerClip::Run {
            let active = app
                .world()
                .get::<AnimationPlayer>(rig)
                .expect("rig animation player missing")
                .animation(playback.source.clips[PlayerClip::Run as usize])
                .expect("run animation missing");
            let rate = (3.0 / PLAYER_ANIMATION_RUN_SPEED).clamp(0.4, 2.5);
            assert_eq!(active.speed(), if velocity.z < 0.0 { -rate } else { rate });
        }
    }
    app.world_mut()
        .get_mut::<PlayerAnimationMotion>(owner)
        .expect("player animation motion missing")
        .support = CharacterSupport::Ladder;
    app.update();
    let playback = app
        .world()
        .get::<PlayerAnimationPlayback>(rig)
        .expect("rig playback missing");
    assert_eq!(playback.state.clip, PlayerClip::Climb);
    let player = app
        .world()
        .get::<AnimationPlayer>(rig)
        .expect("rig animation player missing");
    assert_eq!(
        player
            .animation(playback.source.clips[PlayerClip::Climb as usize])
            .expect("climb animation missing")
            .speed(),
        0.0
    );
    for (vertical_speed, expected) in [(4.0, PlayerClip::Jump), (-2.0, PlayerClip::Fall)] {
        app.world_mut().entity_mut(owner).insert(PlayerAnimationMotion {
            support: CharacterSupport::Airborne,
            velocity: Vec3::Y * vertical_speed,
        });
        app.update();
        let playback = app
            .world()
            .get::<PlayerAnimationPlayback>(rig)
            .expect("rig playback missing");
        assert_eq!(playback.state.clip, expected);
        let index = playback.source.clips[expected as usize];
        let mut player = app
            .world_mut()
            .get_mut::<AnimationPlayer>(rig)
            .expect("rig animation player missing");
        let active = player.animation_mut(index).expect("airborne animation missing");
        assert_eq!(active.repeat_mode(), RepeatAnimation::Never);
        active.set_seek_time(0.2);
        app.update();
        let player = app
            .world()
            .get::<AnimationPlayer>(rig)
            .expect("rig animation player missing");
        assert_eq!(
            player.animation(index).expect("airborne animation missing").seek_time(),
            0.2
        );
    }
}

// Rotation of every joint the selected clip animates, sorted by target
// so two samples pair up.
fn animated_joint_rotations(app: &mut App) -> Vec<(AnimationTargetId, Quat)> {
    let targets = selected_clip_targets(app);
    let mut joints: Vec<_> = app
        .world_mut()
        .query::<(&AnimationTargetId, &Transform)>()
        .iter(app.world())
        .filter(|(id, _)| targets.contains(id))
        .map(|(id, transform)| (*id, transform.rotation))
        .collect();
    joints.sort_by_key(|(id, _)| *id);
    joints
}

fn selected_clip_targets(app: &mut App) -> Vec<AnimationTargetId> {
    let playback = app
        .world_mut()
        .query::<&PlayerAnimationPlayback>()
        .single(app.world())
        .expect("player animation rig missing");
    let index = playback.source.clips[playback.state.clip as usize];
    let graphs = app.world().resource::<Assets<AnimationGraph>>();
    let AnimationNodeType::Clip(clip) = &graphs
        .get(&playback.source.graph)
        .expect("player animation graph missing")
        .get(index)
        .expect("selected clip node missing")
        .node_type
    else {
        panic!("selected clip node is not a clip");
    };
    app.world()
        .resource::<Assets<AnimationClip>>()
        .get(clip)
        .expect("selected clip missing")
        .curves()
        .keys()
        .copied()
        .collect()
}

#[test]
fn selected_clips_animate_the_exported_skeleton_and_climb_follows_ladder_speed() {
    let mut app = headless_asset_app(|app| {
        app.init_resource::<PlayerMap>();
        app.insert_resource(map_settings());
        app.add_systems(Update, player_animation_update_system);
    });

    let owner = app
        .world_mut()
        .spawn((
            PlayerId(1),
            PlayerAnimationMotion::default(),
            PlayerMoveIntent::Idle,
            Transform::IDENTITY,
        ))
        .id();
    let model: ModelDef = serde_json::from_value(serde_json::json!({
        "scene": "models/player.glb#Scene0", "scale": 1.0
    }))
    .expect("player model definition is invalid");
    let server = app.world().resource::<AssetServer>().clone();
    app.world_mut()
        .spawn((load_character_model(&model, &server), PlayerModel { owner }))
        .observe(player_animation_setup_system);
    settle(&mut app, |world| {
        world.query::<&PlayerAnimationPlayback>().iter(world).next().is_some()
    });

    app.world_mut().entity_mut(owner).insert((
        PlayerAnimationMotion {
            support: CharacterSupport::Ground,
            velocity: Vec3::Z * 5.0,
        },
        PlayerMoveIntent::Running { direction: 0.0 },
    ));
    for _ in 0..6 {
        app.update();
    }
    let first = animated_joint_rotations(&mut app);
    assert!(
        !first.is_empty(),
        "the selected clip targets no joint of the loaded skeleton"
    );
    for _ in 0..4 {
        app.update();
    }
    let second = animated_joint_rotations(&mut app);
    assert!(
        first
            .iter()
            .zip(&second)
            .any(|((_, a), (_, b))| a.angle_between(*b) > 0.05),
        "the selected clip did not move the exported skeleton"
    );
    for speed in [2.4, 0.0, -3.6] {
        app.world_mut().entity_mut(owner).insert(PlayerAnimationMotion {
            support: CharacterSupport::Ladder,
            velocity: Vec3::Y * speed,
        });
        app.update();
        let (playback, player) = app
            .world_mut()
            .query::<(&PlayerAnimationPlayback, &AnimationPlayer)>()
            .single(app.world())
            .expect("player animation rig missing");
        let duration = app
            .world()
            .resource::<Assets<AnimationClip>>()
            .get(&playback.source.climb_clip)
            .expect("loaded climb clip missing")
            .duration();
        let active = player
            .animation(playback.source.clips[PlayerClip::Climb as usize])
            .expect("active climb animation missing");
        let rung_speed = active.speed() / duration * PLAYER_ANIMATION_CLIMB_RUNGS_PER_CYCLE;
        assert!((rung_speed * LADDER_RUNG_SPACING - speed).abs() < 0.0001);
    }
}

#[test]
fn exported_player_contains_every_motion_clip_and_a_skin() {
    let raw = include_bytes!("../../assets/models/player.glb");
    let length = u32::from_le_bytes(raw[12..16].try_into().expect("GLB JSON length missing")) as usize;
    let document: serde_json::Value = serde_json::from_slice(&raw[20..20 + length]).expect("GLB JSON is invalid");
    let animations = document["animations"]
        .as_array()
        .expect("player animation clips missing");
    assert_eq!(animations.len(), PlayerClip::ALL.len());
    for (animation, clip) in animations.iter().zip(PlayerClip::ALL) {
        assert_eq!(animation["name"], format!("{clip:?}"));
        assert!(
            !animation["channels"]
                .as_array()
                .expect("animation channels missing")
                .is_empty()
        );
    }
    assert!(!document["skins"].as_array().expect("player skin missing").is_empty());
    for image in document["images"].as_array().expect("embedded images missing") {
        assert!(image.get("uri").is_none(), "player texture depends on an external file");
        assert!(image["bufferView"].is_number());
        assert_eq!(image["mimeType"], "image/png");
    }
}
