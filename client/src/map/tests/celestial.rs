use super::*;

#[test]
fn quantized_light_direction_is_stable_inside_a_step() {
    let step = 0.1_f32.to_radians();
    let altitude = 100.2 * step;
    let azimuth = 20.2 * step;
    let a = Vec3::new(
        azimuth.sin() * altitude.cos(),
        altitude.sin(),
        azimuth.cos() * altitude.cos(),
    );
    let b = Quat::from_rotation_y(0.1 * step) * a;
    assert!(quantized_direction(a, step).abs_diff_eq(quantized_direction(b, step), 1e-6));
}

#[test]
fn fractional_time_interpolates_only_while_running() {
    let cycle = CelestialCycleSettings {
        day_duration_secs: 600.0,
        lunar_cycle_days: 8.0,
    };
    let running = CelestialClockAnchor {
        anchor_tick: 10,
        solar_day_fraction: 0.25,
        lunar_phase_fraction: 0.5,
        running: true,
    };
    let interpolated = fractional_celestial_time(running, 10, 0.5, 30, cycle);
    assert!(interpolated.solar_day_fraction > 0.25);
    let held = fractional_celestial_time(
        CelestialClockAnchor {
            running: false,
            ..running
        },
        20,
        0.9,
        30,
        cycle,
    );
    assert_eq!(held.solar_day_fraction, 0.25);

    let future_anchor = CelestialClockAnchor {
        anchor_tick: 11,
        ..running
    };
    assert_eq!(
        fractional_celestial_time(future_anchor, 10, 0.9, 30, cycle).solar_day_fraction,
        0.25
    );
}

#[test]
fn every_scene_camera_gets_one_private_sky_dome() {
    let mut app = App::new();
    app.insert_resource(SkyAssets {
        mesh: Handle::default(),
        material: Handle::default(),
    })
    .add_systems(Update, attach_sky_to_cameras_system);

    let cameras = [
        (app.world_mut().spawn((Camera3d::default(), SkyRenderLayer(1))).id(), 1),
        (app.world_mut().spawn((Camera3d::default(), SkyRenderLayer(3))).id(), 3),
        (app.world_mut().spawn((Camera3d::default(), SkyRenderLayer(5))).id(), 5),
    ];

    app.update();
    app.update();

    let world = app.world_mut();
    let mut domes = world.query_filtered::<(&ChildOf, &RenderLayers), With<SkyDome>>();
    let attached = domes
        .iter(world)
        .map(|(parent, layers)| (parent.parent(), layers.clone()))
        .collect::<Vec<_>>();
    assert_eq!(attached.len(), cameras.len());
    for (camera, layer) in cameras {
        assert!(world.entity(camera).contains::<SkyAttached>());
        let camera_domes = attached
            .iter()
            .filter(|(parent, _)| *parent == camera)
            .collect::<Vec<_>>();
        assert_eq!(camera_domes.len(), 1);
        assert!(camera_domes[0].1.intersects(&RenderLayers::layer(layer)));
    }
}
