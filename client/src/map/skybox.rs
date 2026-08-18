use std::f32::consts::{PI, TAU};

use bevy::{core_pipeline::Skybox, light::NotShadowCaster, prelude::*, render::view::ColorGrading};

use crate::{
    cameras::MainCameraMarker,
    config::{AssetSet, ClientSettings, MoonLighting, SkyboxDef},
    constants::{MOON_DISC_COLOR, SUN_DISC_COLOR},
};
use common::protocol::{Lighting, MapSettings};

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

// The visible sun/moon disc, kept opposite the directional light's forward
// so it always sits where the shadows say it should. Its mesh is the lit
// lune of a sphere at the level's phase; regenerated when the phase changes.
#[derive(Component)]
pub struct SkyDisc {
    distance: f32,
    radius: f32,
    mesh: Handle<Mesh>,
    phase_percent: f32,
}

pub fn setup_sky_disc_system(
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
    let mesh = meshes.add(phase_mesh(100.0, sun_disc.radius));
    commands.spawn((
        SkyDisc {
            distance: sun_disc.distance,
            radius: sun_disc.radius,
            mesh: mesh.clone(),
            phase_percent: 100.0,
        },
        Mesh3d(mesh),
        // NOT unlit: `StandardMaterial` ignores emissive when unlit (see
        // barriers/pulsate.rs), which renders a ~1-nit gray ball against a
        // 1000-nit sky. Black base + huge emissive = pure self-luminance;
        // zero reflectance so the sun light can't put a specular sheen on
        // the unlit part.
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::rgb(sun_disc.luminance, sun_disc.luminance * 0.94, sun_disc.luminance * 0.78),
            reflectance: 0.0,
            ..default()
        })),
        NotShadowCaster,
        Transform::from_translation(Vec3::Y * sun_disc.distance),
    ));
}

// The lit part of the disc at a given phase: the lune between the outer
// circle arc and the spherical terminator (an ellipse arc), as a flat
// row-strip mesh. 100% is the full circle, 50% a half disc, below that a
// crescent. Unit-scale coordinates times `radius`; the +X side is lit and
// the mesh faces +Z (the billboard turns +Z at the camera).
fn phase_mesh(phase_percent: f32, radius: f32) -> Mesh {
    use bevy::{
        asset::RenderAssetUsages,
        render::mesh::{Indices, PrimitiveTopology},
    };

    const ROWS: usize = 48;
    // A zero-lit moon still gets a hairline sliver — an attached empty mesh
    // upsets the renderer's slab allocator; luminance 0 is the off switch.
    let lit = (phase_percent / 100.0).clamp(0.01, 1.0);
    let terminator = (PI * lit).cos();

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity((ROWS + 1) * 2);
    let mut indices: Vec<u32> = Vec::with_capacity(ROWS * 6);
    for row in 0..=ROWS {
        let y = row as f32 / ROWS as f32 * 2.0 - 1.0;
        let span = (1.0 - y * y).max(0.0).sqrt();
        positions.push([terminator * span * radius, y * radius, 0.0]);
        positions.push([span * radius, y * radius, 0.0]);
    }
    for row in 0..ROWS as u32 {
        let a = row * 2;
        indices.extend_from_slice(&[a, a + 1, a + 3, a, a + 3, a + 2]);
    }
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

// Park the disc a fixed distance from the camera along the direction the
// directional light comes FROM (`back()`), so it tracks the stepped sun
// rotation and geometry occludes it near the horizon like a real sun. The
// flat phase mesh must also face the camera: +Z toward it, so `look_to`
// points -Z away along the light direction.
pub fn sky_disc_system(
    camera: Query<&Transform, (With<Camera3d>, With<MainCameraMarker>, Without<SkyDisc>)>,
    sun_light: Query<&Transform, (With<SunLightMarker>, Without<SkyDisc>, Without<MainCameraMarker>)>,
    mut disc: Query<(&mut Transform, &SkyDisc)>,
) {
    let (Ok(camera), Ok(light), Ok((mut disc_transform, disc))) =
        (camera.single(), sun_light.single(), disc.single_mut())
    else {
        return;
    };
    let away = Vec3::from(light.back());
    disc_transform.translation = camera.translation + away * disc.distance;
    disc_transform.look_to(away, Vec3::Y);
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

// Smoothing time constant of the fade between lighting levels (secs).
const LIGHT_FADE_TAU_SECS: f32 = 0.8;

// Authoritative lighting level from the snapshot (`target`); each channel
// is smoothed locally so a level change is a dusk fade, not a switch flip.
#[derive(Resource, Default)]
pub struct LightingState {
    pub target: Lighting,
    // Set by the first snapshot. Until it is — and on the frame it lands —
    // the dim system snaps instead of fading, so login shows the server's
    // current level immediately.
    pub synced: bool,
    eased: bool,
    sky: f32,
    sun: f32,
    ambient: f32,
    disc: f32,
    disc_color: Vec3,
    saturation: f32,
}

// A lighting level flattened for the renderer — sun and moon levels name
// their values differently, the channels underneath are the same.
struct LevelTargets {
    sky: f32,
    illuminance: f32,
    ambient: f32,
    disc: f32,
    phase_percent: f32,
    color: Color,
    saturation: f32,
}

impl LevelTargets {
    fn moon(moon: &MoonLighting) -> Self {
        Self {
            sky: moon.sky_brightness,
            illuminance: moon.moon_illuminance,
            ambient: moon.ambient_brightness,
            disc: moon.moon_disc_luminance,
            phase_percent: moon.moon_phase_percent,
            color: MOON_DISC_COLOR,
            saturation: moon.saturation,
        }
    }
}

// Drive the world's lighting to the current level. Every channel is a raw
// absolute value from `client.json::lighting`, eased toward the level's
// setting and written absolutely every frame — idempotent, no incremental
// drift. Wall/actor lights stay lit — windows glowing in the dark.
pub fn lighting_dim_system(
    time: Res<Time>,
    client_settings: Res<ClientSettings>,
    mut lighting: ResMut<LightingState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut skyboxes: Query<&mut Skybox>,
    mut sun_light: Query<&mut DirectionalLight, With<SunLightMarker>>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut discs: Query<(&MeshMaterial3d<StandardMaterial>, &mut SkyDisc)>,
    mut gradings: Query<&mut ColorGrading>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let levels = &client_settings.lighting;
    let level = match lighting.target {
        Lighting::Bright => {
            let sun = &levels.bright;
            LevelTargets {
                sky: sun.sky_brightness,
                illuminance: sun.sun_illuminance,
                ambient: sun.ambient_brightness,
                disc: sun.sun_disc_luminance,
                phase_percent: 100.0,
                color: SUN_DISC_COLOR,
                saturation: sun.saturation,
            }
        }
        Lighting::Dim => LevelTargets::moon(&levels.dim),
        Lighting::Dark => LevelTargets::moon(&levels.dark),
    };
    let blend = if lighting.eased {
        1.0 - (-time.delta_secs() / LIGHT_FADE_TAU_SECS).exp()
    } else {
        // Keep snapping until the first snapshot's level has been applied;
        // only later changes get the fade.
        lighting.eased = lighting.synced;
        1.0
    };
    lighting.sky += (level.sky - lighting.sky) * blend;
    lighting.sun += (level.illuminance - lighting.sun) * blend;
    lighting.ambient += (level.ambient - lighting.ambient) * blend;
    lighting.disc += (level.disc - lighting.disc) * blend;
    let target_color = level.color.to_linear();
    let color_step = (Vec3::new(target_color.red, target_color.green, target_color.blue) - lighting.disc_color) * blend;
    lighting.disc_color += color_step;
    lighting.saturation += (level.saturation - lighting.saturation) * blend;

    // Low light mutes the world: post-tonemap saturation on both cameras
    // follows the smoothed value.
    for mut grading in &mut gradings {
        grading.global.post_saturation = lighting.saturation;
    }

    for mut skybox in &mut skyboxes {
        skybox.brightness = lighting.sky;
    }
    for mut light in &mut sun_light {
        light.illuminance = lighting.sun;
    }
    ambient.brightness = lighting.ambient;
    let emissive = lighting.disc_color * lighting.disc;
    for (material, mut disc) in &mut discs {
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.emissive = LinearRgba::rgb(emissive.x, emissive.y, emissive.z);
        }
        // The phase is a mesh shape, not a smoothable value — regenerate on
        // level change and snap.
        if (level.phase_percent - disc.phase_percent).abs() > f32::EPSILON {
            disc.phase_percent = level.phase_percent;
            let _ = meshes.insert(&disc.mesh, phase_mesh(disc.phase_percent, disc.radius));
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
