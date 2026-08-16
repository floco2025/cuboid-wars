use bevy::{core_pipeline::Skybox, light::NotShadowCaster, prelude::*, render::view::ColorGrading};

use crate::{
    cameras::MainCameraMarker,
    config::{AssetSet, ClientSettings, SkyboxDef},
};
use common::protocol::MapSettings;

// The map names its skybox in `MapSettings` (from `SInit`), so both setup
// systems run gated on that resource existing — once, via the `Local` latch —
// instead of at Startup.
fn selected_skybox<'a>(asset_set: &'a AssetSet, map_settings: &MapSettings) -> &'a SkyboxDef {
    asset_set.skybox(&map_settings.skybox).unwrap_or_else(|| {
        let (fallback_name, fallback) = asset_set.fallback_skybox();
        error!(
            "map skybox {:?} is not defined in assets.json `skyboxes`; falling back to {fallback_name:?}",
            map_settings.skybox
        );
        fallback
    })
}

pub fn setup_skybox_from_cross_system(
    mut done: Local<bool>,
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    map_settings: Res<MapSettings>,
) {
    if *done {
        return;
    }
    *done = true;
    let skybox = selected_skybox(&asset_set, &map_settings);

    // Load the cross-layout skybox image
    let cross_image_handle: Handle<Image> = asset_server.load(skybox.image.clone());

    // Store the handle in a resource so we can use it once loaded
    commands.insert_resource(SkyboxCrossImage(cross_image_handle));
    commands.insert_resource(SkyboxSettings {
        brightness: skybox.brightness,
        rotation_period_secs: skybox.rotation_period_secs,
        sun_step_radians: skybox.sun_step_degrees.to_radians(),
    });
}

#[derive(Resource)]
pub struct SkyboxCrossImage(pub(super) Handle<Image>);

#[derive(Resource)]
pub struct SkyboxCubemap(pub Handle<Image>);

#[derive(Resource)]
pub struct SkyboxSettings {
    brightness: f32,
    rotation_period_secs: f32,
    sun_step_radians: f32,
}

// The scene's sun — rotated in lockstep with the skybox so shadows keep
// tracking the sky's light direction.
#[derive(Component)]
pub struct SunLightMarker;

// The visible sun disc, kept opposite the directional light's forward so it
// always sits where the shadows say it should.
#[derive(Component)]
pub struct SunDisc {
    distance: f32,
    // Base emissive luminance, kept so rain dimming can recompute the
    // material's emissive absolutely each frame.
    luminance: f32,
}

pub fn setup_sun_disc_system(
    mut done: Local<bool>,
    mut commands: Commands,
    asset_set: Res<AssetSet>,
    map_settings: Res<MapSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if *done {
        return;
    }
    *done = true;
    let sun_disc = selected_skybox(&asset_set, &map_settings).sun_disc;
    if sun_disc.radius <= 0.0 {
        return;
    }
    commands.spawn((
        SunDisc {
            distance: sun_disc.distance,
            luminance: sun_disc.luminance,
        },
        Mesh3d(meshes.add(Sphere::new(sun_disc.radius))),
        // NOT unlit: `StandardMaterial` ignores emissive when unlit (see
        // barriers/pulsate.rs), which renders a ~1-nit gray ball against a
        // 1000-nit sky. Black base + huge emissive = pure self-luminance.
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::rgb(sun_disc.luminance, sun_disc.luminance * 0.94, sun_disc.luminance * 0.78),
            ..default()
        })),
        NotShadowCaster,
        Transform::from_translation(Vec3::Y * sun_disc.distance),
    ));
}

// Park the disc a fixed distance from the camera along the direction the
// directional light comes FROM (`back()`), so it tracks the stepped sun
// rotation and geometry occludes it near the horizon like a real sun.
pub fn sun_disc_system(
    camera: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>, Without<SunDisc>)>,
    sun_light: Query<&Transform, (With<SunLightMarker>, Without<SunDisc>, Without<MainCameraMarker>)>,
    mut disc: Query<(&mut Transform, &SunDisc)>,
) {
    let (Ok(camera), Ok(light), Ok((mut disc_transform, disc))) =
        (camera.single(), sun_light.single(), disc.single_mut())
    else {
        return;
    };
    disc_transform.translation = camera.translation + Vec3::from(light.back()) * disc.distance;
}

// Add skybox to cameras once the cubemap is ready
pub fn skybox_update_camera_system(
    cubemap: Option<Res<SkyboxCubemap>>,
    settings: Res<SkyboxSettings>,
    mut cameras: Query<Entity, (With<Camera3d>, Without<Skybox>)>,
    mut commands: Commands,
) {
    let Some(cubemap) = cubemap else {
        return;
    };

    if cameras.is_empty() {
        return;
    }

    for entity in &mut cameras {
        commands.entity(entity).insert(Skybox {
            image: Some(cubemap.0.clone()),
            brightness: settings.brightness,
            rotation: Quat::IDENTITY,
        });
    }
}

// Darken the world while it rains. Every value is recomputed absolutely
// from its config base × a dim factor, never incrementally, so the system
// is idempotent per frame and snaps back to the exact base on clear skies.
// Sky, sun disc, directional light, and ambient all follow the smoothed
// rain intensity; wall/actor lights stay lit — windows glowing in the rain.
pub fn rain_dim_system(
    rain: Res<crate::vfx::RainIntensity>,
    client_settings: Res<ClientSettings>,
    settings: Option<Res<SkyboxSettings>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut skyboxes: Query<&mut Skybox>,
    mut sun_light: Query<&mut DirectionalLight, With<SunLightMarker>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    disc: Query<(&MeshMaterial3d<StandardMaterial>, &SunDisc)>,
    mut gradings: Query<&mut ColorGrading>,
) {
    let weather = &client_settings.weather;
    let sky_factor = 1.0 - rain.current * (1.0 - weather.sky_dim);
    let light_factor = 1.0 - rain.current * (1.0 - weather.light_dim);

    // Heavy rain washes the world gray: post-tonemap saturation on both
    // cameras follows the intensity.
    for mut grading in &mut gradings {
        grading.global.post_saturation = 1.0 - rain.current * (1.0 - weather.saturation);
    }

    if let Some(settings) = settings {
        for mut skybox in &mut skyboxes {
            skybox.brightness = settings.brightness * sky_factor;
        }
    }
    for mut light in &mut sun_light {
        light.illuminance = client_settings.lighting.directional_brightness * light_factor;
    }
    ambient.brightness = client_settings.lighting.ambient_brightness * light_factor;
    for (material, sun_disc) in &disc {
        if let Some(mut material) = materials.get_mut(&material.0) {
            let luminance = sun_disc.luminance * sky_factor;
            material.emissive = LinearRgba::rgb(luminance, luminance * 0.94, luminance * 0.78);
        }
    }
}

// Barely-perceptible ambient sky drift. The angle accumulates from frame
// deltas (not elapsed time) so it can't snap when `Time` wraps.
pub fn skybox_rotate_system(
    time: Res<Time>,
    settings: Res<SkyboxSettings>,
    mut angle: Local<f32>,
    mut sun_angle: Local<f32>,
    mut skyboxes: Query<&mut Skybox>,
    mut sun: Query<&mut Transform, With<SunLightMarker>>,
) {
    use std::f32::consts::TAU;

    if settings.rotation_period_secs <= 0.0 {
        return;
    }
    let delta_angle = time.delta_secs() * TAU / settings.rotation_period_secs;
    *angle = (*angle + delta_angle) % TAU;
    let rotation = Quat::from_rotation_y(*angle);
    for mut skybox in &mut skyboxes {
        skybox.rotation = rotation;
    }

    // The skybox shader samples the cubemap with the inverse rotation, so
    // the sky visibly rotates by `rotation` — sweep the sun the same way.
    // But where the sky can drift continuously (cubemap sampling has no
    // aliasing), a continuously rotating light re-rasterizes the shadow map
    // every frame and edge texels shimmer. Advance the sun in discrete
    // steps instead: shadows are pixel-stable between steps and the sky
    // never leads by more than one step.
    let mut pending = *angle - *sun_angle;
    if pending < 0.0 {
        pending += TAU;
    }
    let applied = if settings.sun_step_radians > 0.0 {
        if pending < settings.sun_step_radians {
            return;
        }
        (pending / settings.sun_step_radians).floor() * settings.sun_step_radians
    } else {
        pending
    };
    for mut transform in &mut sun {
        transform.rotate_y(applied);
    }
    *sun_angle = (*sun_angle + applied) % TAU;
}
