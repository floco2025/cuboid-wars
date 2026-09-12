use std::iter;

use bevy::{
    app::AnimationSystems,
    audio::{SpatialScale, Volume},
    prelude::*,
};
use common::{
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld},
    protocol::{CarrierId, Health, MapLayout, PlateState, PlayerId},
};
use rand::RngExt;

use super::{
    CuboidShake, LocalPlayerMarker, PlayerAnimationMotion, PlayerMap,
    animation::{PlayerAnimationPlayback, PlayerClip},
};
use crate::{
    audio::settings_volume,
    config::{AssetSet, ClientSettings},
    constants::{
        PLAYER_ANIMATION_STANDSTILL_SPEED, PLAYER_FOOTSTEP_CLIMB_PHASES, PLAYER_FOOTSTEP_CLIMB_REVERSE_PHASES,
        PLAYER_FOOTSTEP_MIN_INTERVAL_SECS, PLAYER_FOOTSTEP_PROBE_HEIGHT, PLAYER_FOOTSTEP_PROBE_REACH,
        PLAYER_FOOTSTEP_RUN_PHASES, PLAYER_FOOTSTEP_RUN_REVERSE_PHASES, PLAYER_FOOTSTEP_STRAFE_PHASES,
        PLAYER_FOOTSTEP_WALK_PHASES, PLAYER_FOOTSTEP_WALK_REVERSE_PHASES,
    },
};

#[derive(Component, Default)]
pub(super) struct FootstepPlayback {
    odd_step: bool,
    last_sound: Option<String>,
    last_accent: Option<String>,
    last_step_secs: Option<f64>,
    last_clip: Option<PlayerClip>,
}

pub(crate) fn footsteps_plugin(app: &mut App) {
    app.add_systems(Startup, preload_footsteps);
    app.add_systems(PostUpdate, player_footsteps_system.after(AnimationSystems));
}

#[derive(Resource)]
struct FootstepAssets {
    _sounds: Vec<Handle<AudioSource>>,
}

fn preload_footsteps(mut commands: Commands, assets: Res<AssetSet>, server: Res<AssetServer>) {
    let sounds = assets
        .footsteps
        .sets
        .values()
        .flat_map(|set| {
            set.samples
                .iter()
                .chain(set.accent.iter().flat_map(|accent| &accent.samples))
        })
        .map(|path| server.load(path.clone()))
        .collect();
    commands.insert_resource(FootstepAssets { _sounds: sounds });
}

fn contact_crossed(previous: f32, current: f32, reverse: bool, wrapped: bool, contacts: &[f32]) -> bool {
    contacts.iter().any(|&contact| {
        if reverse {
            if wrapped {
                contact < previous || contact >= current
            } else {
                contact < previous && contact >= current
            }
        } else if wrapped {
            contact > previous || contact <= current
        } else {
            contact > previous && contact <= current
        }
    })
}

fn contacts(clip: PlayerClip, reverse: bool) -> &'static [f32] {
    match (clip, reverse) {
        (PlayerClip::Walk, false) => PLAYER_FOOTSTEP_WALK_PHASES,
        (PlayerClip::Walk, true) => PLAYER_FOOTSTEP_WALK_REVERSE_PHASES,
        (PlayerClip::Run, false) => PLAYER_FOOTSTEP_RUN_PHASES,
        (PlayerClip::Run, true) => PLAYER_FOOTSTEP_RUN_REVERSE_PHASES,
        (PlayerClip::StrafeLeft | PlayerClip::StrafeRight, _) => PLAYER_FOOTSTEP_STRAFE_PHASES,
        (PlayerClip::Climb, false) => PLAYER_FOOTSTEP_CLIMB_PHASES,
        (PlayerClip::Climb, true) => PLAYER_FOOTSTEP_CLIMB_REVERSE_PHASES,
        _ => &[],
    }
}

fn sample_index(samples: &[String], odd: bool, previous: Option<&str>, rng: &mut impl RngExt) -> usize {
    if samples.len() == 1 {
        return 0;
    }
    let parity = usize::from(odd);
    let count = (samples.len() - parity).div_ceil(2);
    let excluded = previous
        .filter(|_| count > 1)
        .and_then(|path| samples.iter().skip(parity).step_by(2).position(|sample| sample == path));
    let choice = rng.random_range(0..count - usize::from(excluded.is_some()));
    parity + 2 * (choice + usize::from(excluded.is_some_and(|index| choice >= index)))
}

fn player_footsteps_system(
    mut commands: Commands,
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    settings: Res<ClientSettings>,
    assets: Res<AssetSet>,
    asset_server: Res<AssetServer>,
    clips: Res<Assets<AnimationClip>>,
    world: Res<CollisionWorld>,
    layout: Res<MapLayout>,
    carriers: Res<Carriers>,
    plates: Res<PlateState>,
    players: Res<PlayerMap>,
    owners: Query<(
        &PlayerId,
        &PlayerAnimationMotion,
        &Transform,
        &Health,
        Option<&CuboidShake>,
        Has<LocalPlayerMarker>,
    )>,
    mut animations: Query<(&PlayerAnimationPlayback, &AnimationPlayer, &mut FootstepPlayback)>,
) {
    let footstep_volume = 10.0_f32.powf(assets.footsteps.volume_db / 20.0)
        * settings_volume(settings.preferences.footstep_volume_db).to_linear();
    if footstep_volume == 0.0 || time.delta_secs() > 0.15 {
        return;
    }
    let now = time.elapsed_secs_f64();
    let mut rng = rand::rng();
    for (playback, animation, mut state) in &mut animations {
        let Ok((id, motion, transform, health, shake, local)) = owners.get(playback.source.owner) else {
            continue;
        };
        let clip = playback.state.clip;
        let climbing = motion.support == CharacterSupport::Ladder && clip == PlayerClip::Climb;
        let moving = match motion.support {
            CharacterSupport::Ground => {
                clip != PlayerClip::Climb
                    && motion.velocity.x.hypot(motion.velocity.z) >= PLAYER_ANIMATION_STANDSTILL_SPEED
            }
            CharacterSupport::Ladder => climbing && motion.velocity.y != 0.0,
            CharacterSupport::Airborne => false,
        };
        if health.0 <= 0.0
            || !moving
            || (state.last_clip != Some(clip)
                && state
                    .last_step_secs
                    .is_some_and(|last| now - last < PLAYER_FOOTSTEP_MIN_INTERVAL_SECS))
        {
            continue;
        }
        let index = playback.source.clips[clip as usize];
        let Some(active) = animation
            .animation(index)
            .filter(|active| !active.is_paused() && active.speed() != 0.0)
        else {
            continue;
        };
        let phases = contacts(clip, active.is_playback_reversed());
        if phases.is_empty() {
            continue;
        }
        let Some(duration) = clips
            .get(&playback.source.handles[clip as usize])
            .map(AnimationClip::duration)
            .filter(|duration| *duration > 0.0)
        else {
            continue;
        };
        let Some(previous) = active.last_seek_time() else {
            continue;
        };
        if !contact_crossed(
            previous / duration,
            active.seek_time() / duration,
            active.is_playback_reversed(),
            active.just_completed(),
            phases,
        ) {
            continue;
        }
        let feet = transform.translation - shake.map_or(Vec3::ZERO, |s| Vec3::new(s.offset_x, 0.0, s.offset_z));
        let binding = if climbing {
            assets.ladder_material_def().footstep.as_deref()
        } else {
            let keys = players.get(id).map_or(&[][..], |player| player.held_keys.as_slice());
            let passable = world.passable_barriers(keys, &plates.open_barriers);
            let mut nearest = None;
            for carrier in iter::once(CarrierId::WORLD).chain(carriers.carried_ids()) {
                // Queries use tick poses; the feet and contact sound use the rendered carrier pose.
                let offset = carriers.pose(carrier).translation
                    - carriers
                        .pose_between(carrier, fixed_time.overstep_fraction())
                        .translation;
                let origin = feet + offset + Vec3::Y * PLAYER_FOOTSTEP_PROBE_HEIGHT;
                if let Some(hit) =
                    world.support_surface_on_carrier(origin, PLAYER_FOOTSTEP_PROBE_REACH, carrier, &passable)
                {
                    let distance = origin.y - hit.point.y;
                    if nearest.as_ref().is_none_or(|(closest, _)| distance < *closest) {
                        nearest = Some((distance, hit));
                    }
                }
            }
            nearest
                .and_then(|(_, hit)| world.surface_material(&hit, &layout))
                .and_then(|alias| assets.material_by_id(alias).footstep.as_deref())
        };
        let set = assets.footsteps.resolve(binding);
        let surface_volume = 10.0_f32.powf(set.volume_db / 20.0);
        let ladder_volume = if climbing {
            10.0_f32.powf(assets.footsteps.ladder_volume_db / 20.0)
        } else {
            1.0
        };
        state.last_step_secs = Some(now);
        state.last_clip = Some(clip);
        state.odd_step = !state.odd_step;
        if surface_volume == 0.0 {
            continue;
        }
        let sample = &set.samples[sample_index(&set.samples, state.odd_step, state.last_sound.as_deref(), &mut rng)];
        let mut play = |sample: &str, gain: f32| {
            let volume = footstep_volume * surface_volume * ladder_volume * gain;
            let playback_settings = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume));
            let player = AudioPlayer::new(asset_server.load(sample.to_owned()));
            if local {
                commands.spawn((player, playback_settings));
            } else {
                commands.spawn((
                    player,
                    playback_settings
                        .with_spatial(true)
                        .with_spatial_scale(SpatialScale::new(settings.audio.spatial_distance_scale)),
                    Transform::from_translation(feet),
                ));
            }
        };
        play(sample, 1.0);
        if let Some(accent) = &set.accent {
            let accent_gain = 10.0_f32.powf(accent.volume_db / 20.0);
            if accent_gain > 0.0 {
                let sample = &accent.samples
                    [sample_index(&accent.samples, state.odd_step, state.last_accent.as_deref(), &mut rng)];
                play(sample, accent_gain);
                state.last_accent = Some(sample.clone());
            }
        }
        state.last_sound = Some(sample.clone());
    }
}

#[cfg(test)]
#[path = "tests/footsteps.rs"]
mod tests;
