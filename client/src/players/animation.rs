use std::time::Duration;

use bevy::{gltf::Gltf, prelude::*, world_serialization::WorldInstanceReady};
use common::{
    physics::{CharacterMovementResult, CharacterSupport},
    protocol::{MapSettings, PlayerId, PlayerMoveIntent, Position},
};

use super::PlayerMap;
use crate::{
    characters::CharacterModel,
    constants::{
        LADDER_RUNG_SPACING, PLAYER_ANIMATION_APEX_SPEED, PLAYER_ANIMATION_BLEND_SECS,
        PLAYER_ANIMATION_CLIMB_RUNGS_PER_CYCLE, PLAYER_ANIMATION_LANDING_MIN_AIR_SECS, PLAYER_ANIMATION_RUN_SPEED,
        PLAYER_ANIMATION_STANDSTILL_SPEED, PLAYER_ANIMATION_STRAFE_RATIO, PLAYER_ANIMATION_TAKEOFF_BLEND_SECS,
        PLAYER_ANIMATION_WALK_SPEED,
    },
};

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

// Indices match the named clip order exported by player.py.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayerClip {
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
    pub(super) const ALL: [Self; 10] = [
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
        !matches!(self, Self::Jump | Self::Fall | Self::Land)
    }
}

// The player a model animates; the rig is built when its scene is ready.
#[derive(Component)]
pub(crate) struct PlayerModel {
    pub owner: Entity,
}

#[derive(Clone)]
pub(crate) struct PlayerAnimationSource {
    pub(super) owner: Entity,
    pub(super) graph: Handle<AnimationGraph>,
    pub(super) clips: Vec<AnimationNodeIndex>,
    pub(super) climb_clip: Handle<AnimationClip>,
}

impl PlayerAnimationSource {
    // `None` when the GLB lacks the clips `PlayerClip::ALL` indexes.
    fn from_clips(owner: Entity, clips: &[Handle<AnimationClip>], graphs: &mut Assets<AnimationGraph>) -> Option<Self> {
        if clips.len() < PlayerClip::ALL.len() {
            return None;
        }
        let handles = PlayerClip::ALL.map(|clip| clips[clip as usize].clone());
        let climb_clip = handles[PlayerClip::Climb as usize].clone();
        let (graph, clips) = AnimationGraph::from_clips(handles);
        Some(Self {
            owner,
            graph: graphs.add(graph),
            clips,
            climb_clip,
        })
    }
}

#[derive(Component)]
pub(crate) struct PlayerAnimationPlayback {
    pub(super) source: PlayerAnimationSource,
    pub(super) state: AnimationState,
}

pub(super) struct AnimationState {
    pub(super) clip: PlayerClip,
    pub(super) support: CharacterSupport,
    pub(super) airborne_secs: f32,
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
    pub(super) fn select(
        &mut self,
        motion: PlayerAnimationMotion,
        running: bool,
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
            return (PlayerClip::Climb, 1.0);
        }
        if motion.support == CharacterSupport::Airborne {
            let rising = motion.velocity.y > PLAYER_ANIMATION_APEX_SPEED
                || (self.clip == PlayerClip::Jump && motion.velocity.y >= -PLAYER_ANIMATION_APEX_SPEED);
            return (if rising { PlayerClip::Jump } else { PlayerClip::Fall }, 1.0);
        }
        if stunned {
            return (PlayerClip::Stunned, 1.0);
        }
        let speed = local_velocity.x.hypot(local_velocity.z);
        if speed < PLAYER_ANIMATION_STANDSTILL_SPEED {
            if landed || (self.clip == PlayerClip::Land && !finished) {
                return (PlayerClip::Land, 1.0);
            }
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
        let (clip, reference_speed) = if running {
            (PlayerClip::Run, PLAYER_ANIMATION_RUN_SPEED)
        } else {
            (PlayerClip::Walk, PLAYER_ANIMATION_WALK_SPEED)
        };
        let rate = (speed / reference_speed).clamp(0.4, 2.5);
        (clip, if local_velocity.z < 0.0 { -rate } else { rate })
    }
}

pub(super) fn climb_playback_rate(vertical_speed: f32, duration: f32) -> f32 {
    vertical_speed * duration / (LADDER_RUNG_SPACING * PLAYER_ANIMATION_CLIMB_RUNGS_PER_CYCLE)
}

pub(crate) fn player_animation_setup_system(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    models: Query<(&CharacterModel, &PlayerModel)>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let Ok((model, player_model)) = models.get(ready.entity) else {
        return;
    };
    let Some(source) = model
        .clips(&gltfs)
        .and_then(|clips| PlayerAnimationSource::from_clips(player_model.owner, clips, &mut graphs))
    else {
        error!("{} lacks the {} player clips", model.scene(), PlayerClip::ALL.len());
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
    settings: Res<MapSettings>,
    clips: Res<Assets<AnimationClip>>,
    owners: Query<(&PlayerId, &PlayerAnimationMotion, &PlayerMoveIntent, &Transform)>,
    mut animations: Query<(
        &mut PlayerAnimationPlayback,
        &mut AnimationPlayer,
        &mut AnimationTransitions,
    )>,
) {
    let movement = &settings.movement.player;
    // Equal configured speeds mean the map always uses running locomotion.
    let always_running = movement.walk_speed == movement.run_speed;
    for (mut playback, mut player, mut transitions) in &mut animations {
        let Ok((id, motion, intent, transform)) = owners.get(playback.source.owner) else {
            continue;
        };
        let current = playback.source.clips[playback.state.clip as usize];
        let finished = player.animation(current).is_none_or(|active| active.is_finished());
        let local_velocity = transform.rotation.inverse() * motion.velocity;
        let (clip, mut speed) = playback.state.select(
            *motion,
            intent.is_running() || always_running,
            local_velocity,
            players.get(id).is_some_and(|info| info.stunned),
            finished,
            time.delta_secs(),
        );
        if clip == PlayerClip::Climb {
            speed = clips
                .get(&playback.source.climb_clip)
                .map_or(0.0, |clip| climb_playback_rate(motion.velocity.y, clip.duration()));
        }
        let index = playback.source.clips[clip as usize];
        if clip != playback.state.clip {
            let blend = if clip == PlayerClip::Jump {
                PLAYER_ANIMATION_TAKEOFF_BLEND_SECS
            } else {
                PLAYER_ANIMATION_BLEND_SECS
            };
            let active = transitions.play(&mut player, index, Duration::from_secs_f32(blend));
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
