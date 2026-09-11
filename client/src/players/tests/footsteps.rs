use std::time::Duration;

use bevy::app::TaskPoolPlugin;
use rand::{SeedableRng, rngs::StdRng};
use serde_json::{from_value, json};

use super::*;
use crate::{
    players::animation::{AnimationState, PlayerAnimationSource},
    test_fixtures,
};

#[test]
fn contacts_cross_once_in_each_direction_and_across_loop_boundary() {
    let markers = [0.1, 0.6];
    assert!(contact_crossed(0.05, 0.1, false, false, &markers));
    assert!(!contact_crossed(0.1, 0.2, false, false, &markers));
    assert!(contact_crossed(0.15, 0.1, true, false, &markers));
    assert!(!contact_crossed(0.1, 0.05, true, false, &markers));
    assert!(contact_crossed(0.9, 0.15, false, true, &markers));
    assert!(contact_crossed(0.15, 0.9, true, true, &markers));
    assert!(!contact_crossed(0.9, 0.05, false, true, &markers));
    assert!(!contact_crossed(0.05, 0.9, true, true, &markers));
}

#[test]
fn faster_playback_crosses_contacts_more_often_without_changing_phase() {
    for speed in [0.5_f32, 1.0, 2.0] {
        let mut count = 0;
        for frame in 0..800 {
            let previous = frame as f32 / 100.0 * speed;
            let current = (frame + 1) as f32 / 100.0 * speed;
            count += usize::from(contact_crossed(
                previous.fract(),
                current.fract(),
                false,
                current.floor() > previous.floor(),
                &[0.25, 0.75],
            ));
        }
        assert_eq!(count, (16.0 * speed) as usize);
    }
}

#[test]
fn selection_uses_list_parity_and_avoids_repeats_when_that_group_has_choices() {
    let mut rng = StdRng::seed_from_u64(1);
    let samples = ["step08.ogg", "step03.ogg", "step02.ogg", "step01.ogg", "step00.ogg"].map(str::to_owned);
    for len in 1..=samples.len() {
        for odd in [false, true] {
            let eligible: Vec<_> = (0..len)
                .filter(|index| len == 1 || index % 2 == usize::from(odd))
                .collect();
            let mut previous = None;
            let mut seen = vec![false; len];
            for _ in 0..100 {
                let index = sample_index(&samples[..len], odd, previous, &mut rng);
                assert!(eligible.contains(&index));
                if eligible.len() > 1 {
                    assert_ne!(previous, Some(samples[index].as_str()));
                }
                seen[index] = true;
                previous = Some(samples[index].as_str());
            }
            assert!(eligible.iter().all(|&index| seen[index]));
        }
    }
}

fn sound_app(local: bool, support: CharacterSupport, volume_db: f32, clip: PlayerClip) -> App {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
    app.init_asset::<AudioSource>().init_asset::<AnimationClip>();
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f32(0.1));
    app.insert_resource(time).init_resource::<Time<Fixed>>();
    let settings = test_fixtures::client_settings();
    let mut definition: serde_json::Value =
        serde_json::from_str(include_str!("../../tests/fixtures/assets.json")).expect("asset fixture rejected");
    definition["materials"]["test"]["footstep"] = serde_json::json!("rungs");
    definition["footsteps"]["sets"]["rungs"] = serde_json::json!({"samples":["sounds/test-rung.ogg"],"volume_db":-3});
    let mut assets: AssetSet = serde_json::from_value(definition).expect("ladder asset fixture rejected");
    assets.footsteps.volume_db = volume_db;
    let sample = assets.footsteps.resolve(None).samples[0].clone();
    let analysis: AudioAnalysis =
        serde_json::from_value(serde_json::json!({"version":1,"sounds":{sample:{"suggested_gain_db":-6.0},"sounds/test-rung.ogg":{"suggested_gain_db":-2.0}}}))
            .expect("analysis fixture rejected");
    let layout = MapLayout::default();
    app.insert_resource(settings)
        .insert_resource(assets)
        .insert_resource(analysis)
        .insert_resource(CollisionWorld::from_map_layout(&layout))
        .insert_resource(Carriers::from_layout(&layout))
        .insert_resource(layout)
        .init_resource::<PlateState>()
        .init_resource::<PlayerMap>();
    let owner = app
        .world_mut()
        .spawn((
            PlayerId(1),
            PlayerAnimationMotion {
                support,
                velocity: if support == CharacterSupport::Ladder {
                    Vec3::ZERO
                } else {
                    Vec3::Z
                },
            },
            Transform::default(),
            Visibility::Hidden,
            Health(100.0),
        ))
        .id();
    if local {
        app.world_mut().entity_mut(owner).insert(LocalPlayerMarker);
    }
    let mut animation_clip = AnimationClip::default();
    animation_clip.set_duration(1.0);
    let handle = app
        .world_mut()
        .resource_mut::<Assets<AnimationClip>>()
        .add(animation_clip);
    let index = AnimationNodeIndex::new(1);
    let mut player = AnimationPlayer::default();
    let active = player.play(index).repeat();
    active.set_seek_time(0.01).seek_to(0.04);
    app.world_mut().spawn((
        player,
        FootstepPlayback::default(),
        PlayerAnimationPlayback {
            source: PlayerAnimationSource {
                owner,
                graph: Handle::default(),
                clips: vec![index; 10],
                handles: vec![handle; 10],
                climb_clip: Handle::default(),
            },
            state: AnimationState {
                clip,
                ..Default::default()
            },
        },
    ));
    app.add_systems(Update, player_footsteps_system);
    app
}

#[test]
fn hidden_local_body_plays_balanced_flat_audio_and_remote_body_plays_spatial_audio() {
    for local in [true, false] {
        for db in [-19.0, 0.0, 20.0] {
            let mut app = sound_app(local, CharacterSupport::Ground, -6.0, PlayerClip::Walk);
            app.world_mut()
                .resource_mut::<ClientSettings>()
                .preferences
                .footstep_volume_db = db;
            app.update();
            let world = app.world_mut();
            let mut query = world.query::<&PlaybackSettings>();
            let playback = query.single(world).expect("one footstep missing");
            assert_eq!(playback.spatial, !local);
            assert!((playback.volume.to_linear() - 10.0_f32.powf((-12.0 + db) / 20.0)).abs() < 0.0001);
        }
    }
}

#[test]
fn steps_and_accents_alternate_across_surface_changes_and_pauses() {
    let mut app = sound_app(true, CharacterSupport::Ground, 0.0, PlayerClip::Walk);
    {
        let mut assets = app.world_mut().resource_mut::<AssetSet>();
        for (name, set) in &mut assets.footsteps.sets {
            set.samples = [8, 3, 2, 1, 0]
                .map(|number| format!("{name}/step{number}.ogg"))
                .to_vec();
            set.accent = Some(
                from_value(json!({
                    "samples": ([6, 5, 4].map(|number| format!("{name}/accent{number}.ogg"))),
                    "volume_db": -6.0
                }))
                .expect("accent fixture rejected"),
            );
        }
    }
    for step in 1..=8 {
        let world = app.world_mut();
        if step == 4 {
            world.resource_mut::<AssetSet>().footsteps.default = "rungs".to_owned();
        }
        for mut motion in world.query::<&mut PlayerAnimationMotion>().iter_mut(world) {
            motion.velocity = Vec3::Z;
        }
        world.resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.1));
        app.update();
        let world = app.world_mut();
        let state = world
            .query::<&FootstepPlayback>()
            .single(world)
            .expect("footstep state missing");
        let assets = world.resource::<AssetSet>();
        let set = assets.footsteps.resolve(None);
        let accent = set.accent.as_ref().expect("test accent missing");
        for (samples, played) in [
            (&set.samples, state.last_sound.as_ref()),
            (&accent.samples, state.last_accent.as_ref()),
        ] {
            let index = samples
                .iter()
                .position(|sample| Some(sample) == played)
                .expect("sample missing from active set");
            assert_eq!(index % 2, step % 2);
        }
        assert_eq!(world.query::<&PlaybackSettings>().iter(world).count(), step * 2);
        for mut motion in world.query::<&mut PlayerAnimationMotion>().iter_mut(world) {
            motion.velocity = Vec3::ZERO;
        }
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&PlaybackSettings>().iter(world).count(), step * 2);
    }
}

#[test]
fn airborne_ladder_hold_idle_and_muted_players_do_not_emit_steps() {
    for (support, volume, clip) in [
        (CharacterSupport::Airborne, 0.0, PlayerClip::Walk),
        (CharacterSupport::Ladder, 0.0, PlayerClip::Climb),
        (CharacterSupport::Ground, 0.0, PlayerClip::Idle),
        (CharacterSupport::Ground, -1000.0, PlayerClip::Walk),
    ] {
        let mut app = sound_app(true, support, volume, clip);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&PlaybackSettings>().iter(world).count(), 0);
    }
}

#[test]
fn footstep_volume_off_mutes_ground_and_ladder_sounds_even_with_asset_boosts() {
    for climbing in [false, true] {
        let (support, clip) = if climbing {
            (CharacterSupport::Ladder, PlayerClip::Climb)
        } else {
            (CharacterSupport::Ground, PlayerClip::Walk)
        };
        let mut app = sound_app(true, support, 20.0, clip);
        app.world_mut()
            .resource_mut::<ClientSettings>()
            .preferences
            .footstep_volume_db = -20.0;
        if climbing {
            let phase = contacts(clip, false)[0];
            climb_step(&mut app, 1.0, phase - 0.01, phase + 0.01);
        }
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<&PlaybackSettings>().iter(world).count(), 0);
    }
}

fn climb_step(app: &mut App, velocity: f32, previous: f32, current: f32) {
    let world = app.world_mut();
    for mut motion in world.query::<&mut PlayerAnimationMotion>().iter_mut(world) {
        motion.velocity = Vec3::Y * velocity;
    }
    for mut player in world.query::<&mut AnimationPlayer>().iter_mut(world) {
        player
            .animation_mut(AnimationNodeIndex::new(1))
            .expect("climb playback missing")
            .set_speed(velocity)
            .set_seek_time(previous)
            .seek_to(current);
    }
}

#[test]
fn climbing_up_and_down_uses_ladder_material_without_a_floor() {
    for local in [true, false] {
        for velocity in [-2.0, -0.1, 0.1, 2.0] {
            let mut app = sound_app(local, CharacterSupport::Ladder, -6.0, PlayerClip::Climb);
            app.world_mut()
                .resource_mut::<ClientSettings>()
                .preferences
                .footstep_volume_db = -4.0;
            app.world_mut()
                .resource_mut::<AssetSet>()
                .footsteps
                .sets
                .get_mut("rungs")
                .expect("ladder sound set missing")
                .accent = Some(
                from_value(json!({
                    "samples": ["sounds/test-rung.ogg"], "volume_db": -6.0
                }))
                .expect("accent fixture rejected"),
            );
            let phase = contacts(PlayerClip::Climb, velocity < 0.0)[0];
            let direction = if velocity < 0.0 { -1.0 } else { 1.0 };
            climb_step(&mut app, velocity, phase - direction * 0.01, phase + direction * 0.01);
            app.update();
            let world = app.world_mut();
            let mut playbacks: Vec<_> = world.query::<&PlaybackSettings>().iter(world).collect();
            playbacks.sort_by(|a, b| a.volume.to_linear().total_cmp(&b.volume.to_linear()));
            assert_eq!(playbacks.len(), 2);
            for (playback, db) in playbacks.iter().zip([-23.0, -17.0]) {
                assert_eq!(playback.spatial, !local);
                assert!((playback.volume.to_linear() - 10.0_f32.powf(db / 20.0)).abs() < 0.0001);
            }
            let state = world
                .query::<&FootstepPlayback>()
                .single(world)
                .expect("step state missing");
            assert_eq!(state.last_sound.as_deref(), Some("sounds/test-rung.ogg"));
        }
    }
}

#[test]
fn closely_spaced_rung_contacts_survive_fast_climbing_and_holding_stops_them() {
    let mut app = sound_app(true, CharacterSupport::Ladder, 0.0, PlayerClip::Climb);
    let phases = contacts(PlayerClip::Climb, false);
    for phase in [phases[1], phases[0]] {
        climb_step(&mut app, 5.4, phase - 0.01, phase + 0.01);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(0.04));
        app.update();
    }
    let world = app.world_mut();
    assert_eq!(world.query::<&PlaybackSettings>().iter(world).count(), 2);
    climb_step(&mut app, 0.0, phases[0] - 0.01, phases[0] + 0.01);
    app.update();
    let world = app.world_mut();
    assert_eq!(world.query::<&PlaybackSettings>().iter(world).count(), 2);
}
