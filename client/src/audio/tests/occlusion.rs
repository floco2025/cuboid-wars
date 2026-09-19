use std::time::Duration;

use bevy::app::TaskPoolPlugin;
use common::protocol::{Barrier, CarrierId, FieldKindId, Floor, MapLayout, Wall};

use super::*;

fn wall(x1: f32, z1: f32, x2: f32, z2: f32) -> Wall {
    Wall {
        x1,
        z1,
        x2,
        z2,
        width: 0.3,
        y: 0.0,
        height: 4.0,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

// Walls across the sound's path every 4 m from z = 0, a wall meeting the
// first one's end at a corner, a floor slab beyond the first wall, and a
// barrier in front of it all.
fn walled_world() -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout {
        walls: [0.0, 4.0, 8.0, 12.0, 16.0]
            .into_iter()
            .map(|z| wall(-5.0, z, 5.0, z))
            .chain([wall(5.0, 0.0, 5.0, -6.0)])
            .collect(),
        floors: vec![Floor {
            x1: -5.0,
            z1: 1.0,
            x2: 5.0,
            z2: 5.0,
            y: 4.0,
            thickness: 0.4,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        barriers: vec![Barrier {
            id: Default::default(),
            switch: None,
            initially_on: true,
            x1: -5.0,
            z1: -1.5,
            x2: 5.0,
            z2: -1.5,
            level: 0,
            levels: 1,
            kind: FieldKindId(0),
            y: 0.0,
            height: 4.0,
            width: 0.1,
            carrier: CarrierId::WORLD,
        }],
        ..default()
    })
}

const LISTENER: Vec3 = Vec3::new(0.0, 1.6, -3.0);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), TransformPlugin));
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_millis(20));
    app.insert_resource(time)
        .insert_resource(walled_world())
        .init_resource::<OcclusionClock>()
        .add_systems(Last, audio_occlusion_system);
    app.world_mut()
        .spawn((SpatialListener::new(0.3), Transform::from_translation(LISTENER)));
    app
}

fn occlusion(app: &App, emitter: Entity) -> (f32, f32, f32) {
    let occlusion = app.world().get::<AudioOcclusion>(emitter).expect("occlusion missing");
    (
        occlusion.layers,
        occlusion.factor.expect("emitter never probed"),
        occlusion.cutoff.get(),
    )
}

#[test]
fn every_solid_entered_is_a_layer_whatever_its_thickness_and_no_field_is() {
    let world = walled_world();
    let layers = |emitter: Vec3| sound_path_layers(&world, LISTENER, emitter);
    // Only the barrier in between.
    assert_eq!(layers(Vec3::new(0.0, 1.0, -0.5)), 0.0);
    // On the first wall's near face.
    assert_eq!(layers(Vec3::new(0.0, 1.0, -0.15)), 0.0);
    assert_eq!(layers(Vec3::new(0.0, 1.0, 2.0)), 1.0);
    assert_eq!(layers(Vec3::new(0.0, 1.0, 6.0)), 2.0);
    // The first wall and the floor slab above the room behind it.
    assert_eq!(layers(Vec3::new(0.0, 5.0, 4.0)), 2.0);
    // Five walls.
    assert_eq!(layers(Vec3::new(0.0, 1.0, 18.0)), AUDIO_OCCLUSION_MAX_LAYERS);
    // Through the corner where two walls meet.
    assert_eq!(
        sound_path_layers(&world, Vec3::new(3.0, 1.0, -2.0), Vec3::new(7.0, 1.0, 2.0)),
        1.0
    );
    assert_eq!(occlusion_gain(0.0), 1.0);
    assert!((occlusion_gain(1.0) - AUDIO_OCCLUSION_LAYER_GAIN).abs() < 1e-6);
    assert!((occlusion_gain(2.0) - AUDIO_OCCLUSION_LAYER_GAIN * AUDIO_OCCLUSION_LAYER_GAIN).abs() < 1e-6);
    assert_eq!(occlusion_cutoff(0.0), f32::INFINITY);
    assert!((occlusion_cutoff(1.0) - AUDIO_OCCLUSION_CUTOFF_HZ).abs() < 1e-2);
    assert!((occlusion_cutoff(2.0) - AUDIO_OCCLUSION_CUTOFF_HZ * AUDIO_OCCLUSION_LAYER_CUTOFF_RATIO).abs() < 1e-2);
    let half = occlusion_cutoff(0.5);
    assert!(half > AUDIO_OCCLUSION_CUTOFF_HZ && half < occlusion_cutoff(0.25));
}

#[test]
fn a_new_sound_is_probed_at_once_and_playing_sounds_on_the_clock() {
    let mut app = app();
    let behind = app
        .world_mut()
        .spawn((AudioOcclusion::default(), Transform::from_xyz(0.0, 1.0, 6.0)))
        .id();
    app.update();
    let (layers, factor, cutoff) = occlusion(&app, behind);
    assert!(layers == 2.0 && factor == 2.0);
    assert!((cutoff - AUDIO_OCCLUSION_CUTOFF_HZ * AUDIO_OCCLUSION_LAYER_CUTOFF_RATIO).abs() < 1e-2);
    let gain = app
        .world()
        .get::<AudioOcclusion>(behind)
        .expect("occlusion missing")
        .gain();
    assert!((gain - AUDIO_OCCLUSION_LAYER_GAIN * AUDIO_OCCLUSION_LAYER_GAIN).abs() < 1e-6);
    // Moving into the open waits for the next probe; a new sound does not.
    app.world_mut()
        .entity_mut(behind)
        .insert(Transform::from_xyz(0.0, 1.0, -8.0));
    let open = app
        .world_mut()
        .spawn((AudioOcclusion::default(), Transform::from_xyz(0.0, 1.0, -8.0)))
        .id();
    app.update();
    assert_eq!(occlusion(&app, behind), (2.0, 2.0, cutoff));
    assert_eq!(occlusion(&app, open), (0.0, 0.0, f32::INFINITY));
    let mut updates = 0;
    while occlusion(&app, behind).0 > 0.0 {
        app.update();
        updates += 1;
        assert!(updates < 10, "the clock never probed the moved sound");
    }
    let (_, factor, cutoff) = occlusion(&app, behind);
    assert!(factor > 0.0 && factor < 2.0);
    assert!(cutoff > AUDIO_OCCLUSION_CUTOFF_HZ * AUDIO_OCCLUSION_LAYER_CUTOFF_RATIO && cutoff.is_finite());
    for _ in 0..200 {
        app.update();
    }
    assert_eq!(occlusion(&app, behind), (0.0, 0.0, f32::INFINITY));
}
