use std::time::Duration;

use bevy::{gltf::GltfAssetLabel, prelude::*, world_serialization::WorldInstanceReady};
use common::{
    physics::{CharacterMovementResult, CharacterSupport},
    protocol::{PlayerId, PlayerMoveIntent, Position},
};

use super::PlayerMap;
use crate::{config::ModelDef, constants::*};

#[derive(Component, Debug, Clone, Copy)]
pub struct PlayerAnimationMotion {
    pub support: CharacterSupport,
    pub velocity: Vec3,
}

impl Default for PlayerAnimationMotion {
    fn default() -> Self {
        Self {
            support: CharacterSupport::Ground,
            velocity: Vec3::ZERO,
        }
    }
}

impl PlayerAnimationMotion {
    pub fn record_step(
        &mut self,
        start: Position,
        step: &CharacterMovementResult,
        control_velocity: Vec3,
        external_displacement: Vec3,
        delta: f32,
    ) {
        self.support = step.support;
        // Carriers, reconciliation and knockback must not make an idle player walk.
        let travelled =
            (Vec3::from(step.position) - Vec3::from(start) - external_displacement) / delta - step.floor_velocity;
        let direction = control_velocity.normalize_or_zero();
        let speed = travelled.dot(direction).clamp(0.0, control_velocity.length());
        self.velocity = (direction * speed).with_y(step.vertical_velocity);
    }

    pub fn block_horizontal(&mut self) {
        self.velocity.x = 0.0;
        self.velocity.z = 0.0;
    }
}

// Indices match the named clip order exported by player_robot.py.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerClip {
    Idle,
    Walk,
    Run,
    Climb,
    Jump,
    Fall,
    Land,
    Stunned,
    StrafeLeft,
    StrafeRight,
}

impl PlayerClip {
    const ALL: [Self; 10] = [
        Self::Idle,
        Self::Walk,
        Self::Run,
        Self::Climb,
        Self::Jump,
        Self::Fall,
        Self::Land,
        Self::Stunned,
        Self::StrafeLeft,
        Self::StrafeRight,
    ];

    fn looping(self) -> bool {
        !matches!(self, Self::Jump | Self::Land)
    }
}

#[derive(Component, Clone)]
pub(crate) struct PlayerAnimationSource {
    owner: Entity,
    graph: Handle<AnimationGraph>,
    clips: Vec<AnimationNodeIndex>,
}

impl PlayerAnimationSource {
    pub fn load(
        owner: Entity,
        model: &ModelDef,
        asset_server: &AssetServer,
        graphs: &mut Assets<AnimationGraph>,
    ) -> Self {
        let (graph, clips) =
            AnimationGraph::from_clips(PlayerClip::ALL.map(|clip| {
                asset_server.load(GltfAssetLabel::Animation(clip as usize).from_asset(model.scene.clone()))
            }));
        Self {
            owner,
            graph: graphs.add(graph),
            clips,
        }
    }
}

#[derive(Component)]
pub(crate) struct PlayerAnimationPlayback {
    source: PlayerAnimationSource,
    state: AnimationState,
}

struct AnimationState {
    clip: PlayerClip,
    support: CharacterSupport,
    airborne_secs: f32,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            clip: PlayerClip::Idle,
            support: CharacterSupport::Ground,
            airborne_secs: 0.0,
        }
    }
}

impl AnimationState {
    fn select(
        &mut self,
        motion: PlayerAnimationMotion,
        intent: PlayerMoveIntent,
        local_velocity: Vec3,
        stunned: bool,
        finished: bool,
        delta: f32,
    ) -> (PlayerClip, f32) {
        let landed = self.support == CharacterSupport::Airborne
            && motion.support == CharacterSupport::Ground
            && self.airborne_secs >= PLAYER_ANIMATION_LANDING_MIN_AIR_SECS;
        self.support = motion.support;
        self.airborne_secs = if motion.support == CharacterSupport::Airborne {
            self.airborne_secs + delta
        } else {
            0.0
        };
        if motion.support == CharacterSupport::Ladder {
            return (
                PlayerClip::Climb,
                (motion.velocity.y / PLAYER_ANIMATION_CLIMB_SPEED).clamp(-2.5, 2.5),
            );
        }
        if motion.support == CharacterSupport::Airborne {
            let rising = motion.velocity.y > PLAYER_ANIMATION_APEX_SPEED
                || (self.clip == PlayerClip::Jump && motion.velocity.y >= -PLAYER_ANIMATION_APEX_SPEED);
            return (if rising { PlayerClip::Jump } else { PlayerClip::Fall }, 1.0);
        }
        if stunned {
            return (PlayerClip::Stunned, 1.0);
        }
        if landed || (self.clip == PlayerClip::Land && !finished) {
            return (PlayerClip::Land, 1.0);
        }
        let speed = local_velocity.x.hypot(local_velocity.z);
        if speed < PLAYER_ANIMATION_STANDSTILL_SPEED {
            return (PlayerClip::Idle, 1.0);
        }
        if local_velocity.x.abs() > local_velocity.z.abs() * PLAYER_ANIMATION_STRAFE_RATIO {
            return (
                if local_velocity.x > 0.0 {
                    PlayerClip::StrafeLeft
                } else {
                    PlayerClip::StrafeRight
                },
                (speed / PLAYER_ANIMATION_WALK_SPEED).clamp(0.4, 2.5),
            );
        }
        let (clip, reference_speed) = if intent.is_running() {
            (PlayerClip::Run, PLAYER_ANIMATION_RUN_SPEED)
        } else {
            (PlayerClip::Walk, PLAYER_ANIMATION_WALK_SPEED)
        };
        let rate = (speed / reference_speed).clamp(0.4, 2.5);
        (clip, if local_velocity.z < 0.0 { -rate } else { rate })
    }
}

pub(crate) fn player_animation_setup_system(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    sources: Query<&PlayerAnimationSource>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Ok(source) = sources.get(ready.entity) else {
        return;
    };
    for child in children.iter_descendants(ready.entity) {
        let Ok(mut player) = players.get_mut(child) else {
            continue;
        };
        let mut transitions = AnimationTransitions::new();
        transitions
            .play(&mut player, source.clips[PlayerClip::Idle as usize], Duration::ZERO)
            .repeat();
        commands.entity(child).insert((
            AnimationGraphHandle(source.graph.clone()),
            transitions,
            PlayerAnimationPlayback {
                source: source.clone(),
                state: AnimationState::default(),
            },
        ));
    }
}

pub(crate) fn player_animation_update_system(
    time: Res<Time>,
    players: Res<PlayerMap>,
    owners: Query<(&PlayerId, &PlayerAnimationMotion, &PlayerMoveIntent, &Transform)>,
    mut animations: Query<(
        &mut PlayerAnimationPlayback,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
    )>,
) {
    for (mut playback, mut player, mut transitions) in &mut animations {
        let Ok((id, motion, intent, transform)) = owners.get(playback.source.owner) else {
            continue;
        };
        let current = playback.source.clips[playback.state.clip as usize];
        let finished = player.animation(current).is_none_or(|active| active.is_finished());
        let local_velocity = transform.rotation.inverse() * motion.velocity;
        let (clip, speed) = playback.state.select(
            *motion,
            *intent,
            local_velocity,
            players.get(id).is_some_and(|info| info.stunned),
            finished,
            time.delta_secs(),
        );
        let index = playback.source.clips[clip as usize];
        if clip != playback.state.clip {
            let active = transitions.play(&mut player, index, Duration::from_secs_f32(PLAYER_ANIMATION_BLEND_SECS));
            if clip.looping() {
                active.repeat();
            }
            playback.state.clip = clip;
        }
        if let Some(active) = player.animation_mut(index) {
            active.set_speed(speed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        f32::consts::{FRAC_PI_2, PI},
        time::Instant,
    };

    use bevy::{
        gltf::{Gltf, GltfMaterial, GltfPlugin},
        image::{CompressedImageFormatSupport, CompressedImageFormats, ImagePlugin},
        mesh::MeshPlugin,
        time::TimeUpdateStrategy,
        world_serialization::WorldSerializationPlugin,
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
            intent,
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
            let (clip, rate) = choose(
                &mut state,
                CharacterSupport::Ladder,
                Vec3::Y * speed,
                PlayerMoveIntent::Idle,
                false,
            );
            assert_eq!(clip, PlayerClip::Climb);
            assert_eq!(rate, speed / PLAYER_ANIMATION_CLIMB_SPEED);
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
                PlayerMoveIntent::Idle,
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
            position: Position { x: 1.1, y: 0.2, z: 0.0 },
            vertical_velocity: 0.0,
            support: CharacterSupport::Ground,
            blocked: false,
            floor_velocity: Vec3::new(10.0, 2.0, 0.0),
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
    fn playback_switches_clips_without_restarting_each_frame() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.init_resource::<PlayerMap>();
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
    }

    #[test]
    fn bevy_loads_embedded_materials_and_animates_the_exported_skeleton() {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin {
                file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
                ..default()
            },
            TransformPlugin,
            WorldSerializationPlugin,
            ImagePlugin::default(),
            MeshPlugin,
            AnimationPlugin,
            GltfPlugin::default(),
        ));
        app.insert_resource(CompressedImageFormatSupport(CompressedImageFormats::NONE));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 30.0)));
        app.init_resource::<PlayerMap>();
        app.add_systems(Update, player_animation_update_system);
        app.finish();
        app.cleanup();

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
            "scene": "models/player_robot.glb#Scene0", "scale": 1.0
        }))
        .expect("player model definition is invalid");
        let server = app.world().resource::<AssetServer>().clone();
        let gltf_handle: Handle<Gltf> = server.load("models/player_robot.glb");
        let source = PlayerAnimationSource::load(
            owner,
            &model,
            &server,
            &mut app.world_mut().resource_mut::<Assets<AnimationGraph>>(),
        );
        app.world_mut()
            .spawn((WorldAssetRoot(server.load(model.scene)), source))
            .observe(player_animation_setup_system);

        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            app.update();
            if app
                .world_mut()
                .query::<&PlayerAnimationPlayback>()
                .iter(app.world())
                .next()
                .is_some()
                && server.is_loaded_with_dependencies(&gltf_handle)
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "player GLB or animation hierarchy failed to load"
            );
            std::thread::sleep(Duration::from_millis(5));
        }

        let gltfs = app.world().resource::<Assets<Gltf>>();
        let gltf = gltfs.get(&gltf_handle).expect("loaded player GLB missing");
        let materials = app.world().resource::<Assets<GltfMaterial>>();
        let images = app.world().resource::<Assets<Image>>();
        let mut textured = 0;
        for handle in &gltf.materials {
            let material = materials.get(handle).expect("GLB material missing");
            if let Some(color) = &material.base_color_texture {
                textured += 1;
                assert!(
                    images
                        .get(color)
                        .expect("base color image missing")
                        .texture_descriptor
                        .format
                        .is_srgb()
                );
                for data in [&material.normal_map_texture, &material.metallic_roughness_texture] {
                    let image = images
                        .get(data.as_ref().expect("PBR data map missing"))
                        .expect("PBR image missing");
                    assert!(!image.texture_descriptor.format.is_srgb());
                    assert!(image.data.as_ref().is_some_and(|bytes| !bytes.is_empty()));
                }
            }
        }
        assert!(textured >= 3, "shell, rubber and metal textures are missing");

        let thigh = app
            .world_mut()
            .query::<(Entity, &Name)>()
            .iter(app.world())
            .find(|(_, name)| name.as_str() == "Thigh.L")
            .map(|(entity, _)| entity)
            .expect("thigh joint missing from player GLB");
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
        let first = app
            .world()
            .get::<Transform>(thigh)
            .expect("thigh transform missing")
            .rotation;
        for _ in 0..4 {
            app.update();
        }
        let second = app
            .world()
            .get::<Transform>(thigh)
            .expect("thigh transform missing")
            .rotation;
        assert!(
            first.angle_between(second) > 0.05,
            "running clip did not move the exported skeleton"
        );
    }

    #[test]
    fn exported_robot_contains_every_motion_clip_and_a_skin() {
        let raw = include_bytes!("../../assets/models/player_robot.glb");
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
        assert!(!document["skins"].as_array().expect("robot skin missing").is_empty());
        for image in document["images"].as_array().expect("embedded images missing") {
            assert!(image.get("uri").is_none(), "player texture depends on an external file");
            assert!(image["bufferView"].is_number());
            assert_eq!(image["mimeType"], "image/png");
        }
    }
}
