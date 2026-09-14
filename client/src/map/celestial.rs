use bevy::{
    camera::visibility::RenderLayers,
    light::{CascadeShadowConfigBuilder, NotShadowCaster},
    prelude::*,
    render::view::ColorGrading,
};
use common::{
    celestial::{CelestialClockAnchor, CelestialCycleSettings, CelestialTime, celestial_directions},
    config::NetworkConfig,
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

// This frame's sun, moon, and phases, derived once by
// `celestial_state_system` for everything that follows the sky.
#[derive(Resource, Default, Clone, Copy)]
pub(super) struct SkyState {
    pub(super) sun_direction: Vec3,
    pub(super) moon_direction: Vec3,
    pub(super) celestial_pole: Vec3,
    pub(super) star_rotation_radians: f32,
    pub(super) moon_illuminated_fraction: f32,
    pub(super) sun_altitude: f32,
    pub(super) twilight: f32,
    pub(super) daylight: f32,
    pub(super) rain: f32,
    pub(super) seconds: f32,
    // The two directional lights: quantized directions, illuminance, and
    // which of them owns the shadow map.
    pub(super) sun_light_direction: Vec3,
    pub(super) moon_light_direction: Vec3,
    pub(super) sun_illuminance: f32,
    pub(super) moon_illuminance: f32,
    pub(super) sun_casts_shadows: bool,
    // The configured ambient level for this phase and weather; the sky probe
    // scales itself to it, so the JSON keeps setting how much ambient there is.
    pub(super) ambient_brightness: f32,
}

pub fn celestial_state_system(
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    tick: Res<ServerTick>,
    network: Res<NetworkConfig>,
    anchor: Res<CelestialClockAnchor>,
    cycle: Res<CelestialCycleSettings>,
    map: Res<MapSettings>,
    settings: Res<ClientSettings>,
    rain: Res<RainIntensity>,
    mut state: ResMut<SkyState>,
    mut sun_casts_shadows: Local<bool>,
) {
    let celestial_time = fractional_celestial_time(
        *anchor,
        tick.0,
        fixed_time.overstep_fraction(),
        network.server_hz,
        *cycle,
    );
    let directions = celestial_directions(map.celestial, celestial_time);
    let rain = rain.cloud_cover();
    let seconds = time.elapsed_secs_wrapped();

    // One set of altitude edges for direct light, ambient, grading, fog, and
    // (mirrored in sky.wgsl) the dome, so the scene turns over together.
    let sun_altitude = directions.sun_altitude_radians;
    let twilight = smoothstep(-12.0_f32.to_radians(), -3.0_f32.to_radians(), sun_altitude);
    let daylight = smoothstep(-4.0_f32.to_radians(), 10.0_f32.to_radians(), sun_altitude);

    let lighting = settings.lighting;
    let step = lighting.shadow_step_degrees.to_radians();
    let sun_height = sun_altitude.sin().max(0.0);
    let moon_height = directions.moon_altitude_radians.sin().max(0.0);
    let direct_weather = 1.0_f32.lerp(LIGHTING_RAIN_DIRECT_FACTOR, rain);
    // A cloud drifting across the sun takes the direct light with it.
    let clouds = settings.sky.clouds;
    let cloud_over_sun = cumulus_toward(
        directions.sun,
        seconds,
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
    let ambient_brightness = lighting
        .night_ambient_brightness
        .lerp(lighting.twilight_ambient_brightness, twilight)
        .lerp(lighting.day_ambient_brightness, daylight)
        * 1.0_f32.lerp(LIGHTING_RAIN_AMBIENT_FACTOR, rain);

    *state = SkyState {
        sun_direction: directions.sun,
        moon_direction: directions.moon,
        celestial_pole: directions.celestial_pole,
        star_rotation_radians: directions.star_rotation_radians,
        moon_illuminated_fraction: directions.moon_illuminated_fraction,
        sun_altitude,
        twilight,
        daylight,
        rain,
        seconds,
        sun_light_direction: quantized_direction(directions.sun, step),
        moon_light_direction: quantized_direction(directions.moon, step),
        sun_illuminance,
        moon_illuminance,
        sun_casts_shadows: *sun_casts_shadows,
        ambient_brightness,
    };
}

pub fn sky_material_system(
    state: Res<SkyState>,
    sky_assets: Option<Res<SkyAssets>>,
    mut materials: ResMut<Assets<ProceduralSkyMaterial>>,
) {
    let Some(sky_assets) = sky_assets else {
        return;
    };
    let Some(mut material) = materials.get_mut(&sky_assets.material) else {
        return;
    };
    material.sun_direction = state.sun_direction.extend(0.0);
    material.moon_direction = state.moon_direction.extend(0.0);
    material.pole_rotation = state.celestial_pole.extend(state.star_rotation_radians);
    material.time_weather_phase = Vec4::new(state.seconds, state.rain, state.moon_illuminated_fraction, 0.0);
}

pub fn celestial_lights_system(
    state: Res<SkyState>,
    settings: Res<ClientSettings>,
    mut sun_lights: Query<(&mut DirectionalLight, &mut Transform), (With<SunLightMarker>, Without<MoonLightMarker>)>,
    mut moon_lights: Query<(&mut DirectionalLight, &mut Transform), (With<MoonLightMarker>, Without<SunLightMarker>)>,
) {
    let shadows = settings.rendering.directional_shadows;
    if let Ok((mut light, mut transform)) = sun_lights.single_mut() {
        light.illuminance = state.sun_illuminance;
        light.shadow_maps_enabled = shadows && state.sun_casts_shadows && state.sun_illuminance > 0.0;
        *transform = Transform::default().looking_to(-state.sun_light_direction, Vec3::Y);
    }
    if let Ok((mut light, mut transform)) = moon_lights.single_mut() {
        light.illuminance = state.moon_illuminance;
        light.shadow_maps_enabled = shadows && !state.sun_casts_shadows && state.moon_illuminance > 0.0;
        *transform = Transform::default().looking_to(-state.moon_light_direction, Vec3::Y);
    }
}

pub fn scene_ambient_system(
    state: Res<SkyState>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut gradings: Query<&mut ColorGrading, Or<(With<MainCameraMarker>, With<RearviewCameraMarker>)>>,
) {
    // The sky probe carries the ambient; this is the fill under it.
    ambient.brightness = state.ambient_brightness * AMBIENT_FILL_UNDER_SKY_PROBE;
    let color = mix_color(AMBIENT_NIGHT_COLOR, AMBIENT_TWILIGHT_COLOR, state.twilight)
        .lerp(Vec3::from_array(AMBIENT_DAY_COLOR), state.daylight)
        .lerp(Vec3::from_array(AMBIENT_OVERCAST_COLOR), state.rain);
    ambient.color = Color::linear_rgb(color.x, color.y, color.z);
    let saturation = SCENE_NIGHT_SATURATION
        .lerp(SCENE_TWILIGHT_SATURATION, state.twilight)
        .lerp(SCENE_DAY_SATURATION, state.daylight);
    for mut grading in &mut gradings {
        grading.global.post_saturation = saturation;
    }
}

pub fn distance_fog_system(
    state: Res<SkyState>,
    settings: Res<ClientSettings>,
    mut commands: Commands,
    mut cameras: Query<(Entity, Option<&mut DistanceFog>), With<Camera3d>>,
) {
    let horizon = mix_color(SKY_NIGHT_HORIZON_COLOR, SKY_TWILIGHT_HORIZON_COLOR, state.twilight);
    let horizon = horizon.lerp(Vec3::from_array(SKY_DAY_HORIZON_COLOR), state.daylight);
    // Fog is compared against lit, exposed colours in the shader, like the
    // sky dome's output, so it takes the sky's own horizon brightness. An
    // overcast deck is grey only by day; at night it is as dark as the sky.
    let horizon = horizon.lerp(Vec3::from_array(SKY_OVERCAST_COLOR) * 1.3, state.rain * state.daylight);
    let sky = settings.sky;
    let brightness = sky
        .night_brightness
        .lerp(sky.twilight_brightness, state.twilight)
        .lerp(sky.day_brightness, state.daylight);
    let fog_color = horizon * brightness;
    for (entity, fog) in &mut cameras {
        let value = DistanceFog {
            color: Color::linear_rgb(fog_color.x, fog_color.y, fog_color.z),
            directional_light_color: Color::NONE,
            falloff: FogFalloff::Linear {
                start: FOG_CLEAR_RANGE[0].lerp(FOG_RAIN_RANGE[0], state.rain),
                end: FOG_CLEAR_RANGE[1].lerp(FOG_RAIN_RANGE[1], state.rain),
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
