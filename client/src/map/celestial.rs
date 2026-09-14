use bevy::{
    camera::visibility::RenderLayers,
    ecs::system::SystemParam,
    light::{CascadeShadowConfigBuilder, NotShadowCaster},
    prelude::*,
    render::view::ColorGrading,
};
use common::{
    celestial::{CelestialClockAnchor, CelestialCycleSettings, CelestialTime, celestial_directions},
    protocol::{MapSettings, ServerTick, sequence_is_newer},
};

use crate::{
    cameras::{MainCameraMarker, RearviewCameraMarker, SkyRenderLayer},
    config::ClientSettings,
    constants::{
        AMBIENT_DAY_COLOR, AMBIENT_FILL_UNDER_SKY_PROBE, AMBIENT_NIGHT_COLOR, AMBIENT_OVERCAST_COLOR,
        AMBIENT_TWILIGHT_COLOR, CLOUD_SUN_DIMMING, FOG_CLEAR_RANGE, FOG_RAIN_RANGE, LIGHTING_RAIN_AMBIENT_FACTOR,
        LIGHTING_RAIN_DIRECT_FACTOR, SCENE_DAY_SATURATION, SCENE_NIGHT_SATURATION, SCENE_TWILIGHT_SATURATION,
        SHADOW_CASCADE_DISTANCE, SHADOW_FIRST_CASCADE_BOUND, SKY_CLOUD_SCALE, SKY_DAY_HORIZON_COLOR,
        SKY_NIGHT_HORIZON_COLOR, SKY_OVERCAST_COLOR, SKY_TWILIGHT_HORIZON_COLOR,
    },
    map::clouds::cumulus_toward,
    materials::ProceduralSkyMaterial,
    vfx::RainIntensity,
};

const SKY_RADIUS: f32 = 500.0;
const SHADOW_HANDOVER_RATIO: f32 = 1.2;

#[derive(Component)]
pub struct SunLightMarker;

#[derive(Component)]
pub struct MoonLightMarker;

#[derive(Component)]
pub(super) struct SkyAttached;

#[derive(Component)]
pub(super) struct SkyDome;

// This frame's sun and phases, as `celestial_sky_system` derives them, for
// everything else that follows the sky.
#[derive(Resource, Default, Clone, Copy)]
pub(super) struct SkyState {
    pub(super) sun_direction: Vec3,
    pub(super) sun_altitude: f32,
    pub(super) twilight: f32,
    pub(super) daylight: f32,
    pub(super) rain: f32,
    pub(super) sun_illuminance: f32,
    // The configured ambient level for this phase and weather; the sky probe
    // scales itself to it, so the JSON keeps setting how much ambient there is.
    pub(super) ambient_brightness: f32,
    pub(super) seconds: f32,
}

#[derive(Resource)]
pub struct SkyAssets {
    mesh: Handle<Mesh>,
    material: Handle<ProceduralSkyMaterial>,
}

pub fn setup_scene_lighting_system(
    mut commands: Commands,
    client_settings: Res<ClientSettings>,
    mut cluster_settings: ResMut<bevy::light::cluster::GlobalClusterSettings>,
) {
    let shadows = client_settings.rendering.directional_shadows;
    commands.spawn((
        DirectionalLight {
            illuminance: 0.0,
            shadow_maps_enabled: shadows,
            ..default()
        },
        CascadeShadowConfigBuilder {
            maximum_distance: SHADOW_CASCADE_DISTANCE,
            first_cascade_far_bound: SHADOW_FIRST_CASCADE_BOUND,
            ..default()
        }
        .build(),
        Transform::default(),
        SunLightMarker,
    ));
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.68, 0.78, 1.0),
            illuminance: 0.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::default(),
        MoonLightMarker,
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: client_settings.lighting.day_ambient_brightness,
        affects_lightmapped_meshes: false,
    });
    commands.insert_resource(bevy::light::DirectionalLightShadowMap {
        size: client_settings.rendering.shadow_map_size as usize,
    });

    // Dense lit maps can overflow Bevy's default clustered-light Z list.
    const CLUSTER_Z_SLICE_CAPACITY: usize = 8192;
    if let Some(gpu) = cluster_settings.gpu_clustering.as_mut() {
        gpu.initial_z_slice_list_capacity = gpu.initial_z_slice_list_capacity.max(CLUSTER_Z_SLICE_CAPACITY);
    }
}

pub fn setup_sky_system(
    mut done: Local<bool>,
    mut commands: Commands,
    settings: Res<ClientSettings>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ProceduralSkyMaterial>>,
) {
    if *done {
        return;
    }
    *done = true;
    let mesh = meshes.add(
        Sphere::new(SKY_RADIUS)
            .mesh()
            .ico(5)
            .expect("sky sphere subdivision is valid"),
    );
    let material = materials.add(ProceduralSkyMaterial::from_config(settings.sky));
    commands.insert_resource(SkyAssets { mesh, material });
}

// One camera-private sphere prevents a portal camera from seeing the sky
// belonging to another eye. Parenting ties its lifetime to the camera; the
// inverse local rotation keeps the resulting dome world-aligned.
pub fn attach_sky_to_cameras_system(
    mut commands: Commands,
    assets: Option<Res<SkyAssets>>,
    cameras: Query<(Entity, &SkyRenderLayer), (With<Camera3d>, Without<SkyAttached>)>,
) {
    let Some(assets) = assets else {
        return;
    };
    for (camera, layer) in &cameras {
        let dome = commands
            .spawn((
                SkyDome,
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(assets.material.clone()),
                RenderLayers::layer(layer.0),
                NotShadowCaster,
                Transform::default(),
            ))
            .id();
        commands.entity(camera).insert(SkyAttached).add_child(dome);
    }
}

pub fn align_sky_domes_system(
    cameras: Query<&Transform, (With<Camera3d>, Without<SkyDome>)>,
    mut domes: Query<(&ChildOf, &mut Transform), (With<SkyDome>, Without<Camera3d>)>,
) {
    for (parent, mut transform) in &mut domes {
        if let Ok(camera) = cameras.get(parent.parent()) {
            transform.translation = Vec3::ZERO;
            transform.rotation = camera.rotation.inverse();
        }
    }
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix_color(a: [f32; 3], b: [f32; 3], t: f32) -> Vec3 {
    Vec3::from_array(a).lerp(Vec3::from_array(b), t)
}

fn fractional_celestial_time(
    anchor: CelestialClockAnchor,
    tick: u32,
    overstep: f32,
    server_hz: u32,
    cycle: CelestialCycleSettings,
) -> CelestialTime {
    let mut current = anchor.at_tick(tick, server_hz, cycle);
    if anchor.running && !sequence_is_newer(anchor.anchor_tick, tick) {
        let partial_days = overstep / server_hz.max(1) as f32 / cycle.day_duration_secs;
        current.solar_day_fraction = (current.solar_day_fraction + partial_days).rem_euclid(1.0);
        current.lunar_phase_fraction =
            (current.lunar_phase_fraction + partial_days / cycle.lunar_cycle_days).rem_euclid(1.0);
    }
    current
}

fn quantized_direction(direction: Vec3, step_radians: f32) -> Vec3 {
    if step_radians <= 0.0 {
        return direction;
    }
    let altitude = (direction.y.asin() / step_radians).round() * step_radians;
    let azimuth = (direction.x.atan2(direction.z) / step_radians).round() * step_radians;
    Vec3::new(
        azimuth.sin() * altitude.cos(),
        altitude.sin(),
        azimuth.cos() * altitude.cos(),
    )
}

#[derive(SystemParam)]
pub struct CelestialRender<'w, 's> {
    sky_materials: ResMut<'w, Assets<ProceduralSkyMaterial>>,
    sun_lights: Query<
        'w,
        's,
        (&'static mut DirectionalLight, &'static mut Transform),
        (With<SunLightMarker>, Without<MoonLightMarker>),
    >,
    moon_lights: Query<
        'w,
        's,
        (&'static mut DirectionalLight, &'static mut Transform),
        (With<MoonLightMarker>, Without<SunLightMarker>),
    >,
    ambient: ResMut<'w, GlobalAmbientLight>,
    sky_state: ResMut<'w, SkyState>,
    gradings: Query<'w, 's, &'static mut ColorGrading, Or<(With<MainCameraMarker>, With<RearviewCameraMarker>)>>,
    cameras: Query<'w, 's, (Entity, Option<&'static mut DistanceFog>), With<Camera3d>>,
}

pub fn celestial_sky_system(
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    tick: Res<ServerTick>,
    network: Res<common::config::NetworkConfig>,
    anchor: Res<CelestialClockAnchor>,
    cycle: Res<CelestialCycleSettings>,
    map: Res<MapSettings>,
    settings: Res<ClientSettings>,
    rain: Res<RainIntensity>,
    sky_assets: Option<Res<SkyAssets>>,
    mut render: CelestialRender,
    mut commands: Commands,
    mut sun_casts_shadows: Local<bool>,
) {
    let Some(sky_assets) = sky_assets else {
        return;
    };
    let celestial_time = fractional_celestial_time(
        *anchor,
        tick.0,
        fixed_time.overstep_fraction(),
        network.server_hz,
        *cycle,
    );
    let directions = celestial_directions(map.celestial, celestial_time);
    let rain = rain.cloud_cover();

    if let Some(mut material) = render.sky_materials.get_mut(&sky_assets.material) {
        material.sun_direction = directions.sun.extend(0.0);
        material.moon_direction = directions.moon.extend(0.0);
        material.pole_rotation = directions.celestial_pole.extend(directions.star_rotation_radians);
        material.time_weather_phase = Vec4::new(
            time.elapsed_secs_wrapped(),
            rain,
            directions.moon_illuminated_fraction,
            0.0,
        );
    }

    // One set of altitude edges for direct light, ambient, grading, fog, and
    // (mirrored in sky.wgsl) the dome, so the scene turns over together.
    let sun_altitude = directions.sun_altitude_radians;
    let twilight = smoothstep(-12.0_f32.to_radians(), -3.0_f32.to_radians(), sun_altitude);
    let daylight = smoothstep(-4.0_f32.to_radians(), 10.0_f32.to_radians(), sun_altitude);

    let lighting = settings.lighting;
    let step = lighting.shadow_step_degrees.to_radians();
    let sun_direction = quantized_direction(directions.sun, step);
    let moon_direction = quantized_direction(directions.moon, step);
    let sun_height = directions.sun_altitude_radians.sin().max(0.0);
    let moon_height = directions.moon_altitude_radians.sin().max(0.0);
    let direct_weather = 1.0_f32.lerp(LIGHTING_RAIN_DIRECT_FACTOR, rain);
    // A cloud drifting across the sun takes the direct light with it.
    let clouds = settings.sky.clouds;
    let cloud_over_sun = cumulus_toward(
        directions.sun,
        time.elapsed_secs_wrapped(),
        clouds.clear_coverage.lerp(clouds.overcast_coverage, rain),
        SKY_CLOUD_SCALE,
        clouds.movement_speed_degrees_per_second.to_radians(),
    );
    let sun_illuminance = lighting.max_sun_illuminance
        * daylight
        * sun_height.sqrt()
        * direct_weather
        * (1.0 - CLOUD_SUN_DIMMING * cloud_over_sun);
    let moon_illuminance =
        lighting.max_full_moon_illuminance * moon_height.sqrt() * directions.moon_illuminated_fraction * direct_weather;
    // The shadow map changes hands only when the other body is clearly the
    // stronger one, so a crossing around sunrise cannot flip it every frame.
    if *sun_casts_shadows {
        if moon_illuminance > sun_illuminance * SHADOW_HANDOVER_RATIO {
            *sun_casts_shadows = false;
        }
    } else if sun_illuminance > moon_illuminance * SHADOW_HANDOVER_RATIO {
        *sun_casts_shadows = true;
    }
    let sun_stronger = *sun_casts_shadows;
    if let Ok((mut light, mut transform)) = render.sun_lights.single_mut() {
        light.illuminance = sun_illuminance;
        light.shadow_maps_enabled = settings.rendering.directional_shadows && sun_stronger && sun_illuminance > 0.0;
        *transform = Transform::default().looking_to(-sun_direction, Vec3::Y);
    }
    if let Ok((mut light, mut transform)) = render.moon_lights.single_mut() {
        light.illuminance = moon_illuminance;
        light.shadow_maps_enabled = settings.rendering.directional_shadows && !sun_stronger && moon_illuminance > 0.0;
        *transform = Transform::default().looking_to(-moon_direction, Vec3::Y);
    }

    let ambient_brightness = lighting
        .night_ambient_brightness
        .lerp(lighting.twilight_ambient_brightness, twilight)
        .lerp(lighting.day_ambient_brightness, daylight)
        * 1.0_f32.lerp(LIGHTING_RAIN_AMBIENT_FACTOR, rain);
    // The sky probe carries the ambient; this is the fill under it.
    render.ambient.brightness = ambient_brightness * AMBIENT_FILL_UNDER_SKY_PROBE;
    *render.sky_state = SkyState {
        sun_direction: directions.sun,
        sun_altitude,
        twilight,
        daylight,
        rain,
        sun_illuminance,
        ambient_brightness,
        seconds: time.elapsed_secs_wrapped(),
    };
    let ambient_color = mix_color(AMBIENT_NIGHT_COLOR, AMBIENT_TWILIGHT_COLOR, twilight)
        .lerp(Vec3::from_array(AMBIENT_DAY_COLOR), daylight)
        .lerp(Vec3::from_array(AMBIENT_OVERCAST_COLOR), rain);
    render.ambient.color = Color::linear_rgb(ambient_color.x, ambient_color.y, ambient_color.z);
    let saturation = SCENE_NIGHT_SATURATION
        .lerp(SCENE_TWILIGHT_SATURATION, twilight)
        .lerp(SCENE_DAY_SATURATION, daylight);
    for mut grading in &mut render.gradings {
        grading.global.post_saturation = saturation;
    }

    let horizon = mix_color(SKY_NIGHT_HORIZON_COLOR, SKY_TWILIGHT_HORIZON_COLOR, twilight);
    let horizon = horizon.lerp(Vec3::from_array(SKY_DAY_HORIZON_COLOR), daylight);
    // Fog is compared against lit, exposed colours in the shader, like the
    // sky dome's output, so it takes the sky's own horizon brightness. An
    // overcast deck is grey only by day; at night it is as dark as the sky.
    let horizon = horizon.lerp(Vec3::from_array(SKY_OVERCAST_COLOR) * 1.3, rain * daylight);
    let sky = settings.sky;
    let brightness = sky
        .night_brightness
        .lerp(sky.twilight_brightness, twilight)
        .lerp(sky.day_brightness, daylight);
    let fog_color = horizon * brightness;
    for (entity, fog) in &mut render.cameras {
        let value = DistanceFog {
            color: Color::linear_rgb(fog_color.x, fog_color.y, fog_color.z),
            directional_light_color: Color::NONE,
            falloff: FogFalloff::Linear {
                start: FOG_CLEAR_RANGE[0].lerp(FOG_RAIN_RANGE[0], rain),
                end: FOG_CLEAR_RANGE[1].lerp(FOG_RAIN_RANGE[1], rain),
            },
            ..default()
        };
        if let Some(mut fog) = fog {
            *fog = value;
        } else {
            commands.entity(entity).insert(value);
        }
    }
}

#[cfg(test)]
#[path = "tests/celestial.rs"]
mod tests;
