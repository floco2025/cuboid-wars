use std::f32::consts::{PI, TAU};

use bevy::{core_pipeline::Skybox, light::NotShadowCaster, prelude::*, render::view::ColorGrading};

use crate::{
    cameras::MainCameraMarker,
    config::{AssetSet, ClientSettings, LightingConfig, MoonLighting, SkyboxDef, SunLighting},
    constants::{MOON_DISC_COLOR, SUN_DISC_COLOR},
};
use common::protocol::{LightingBlend, MapSettings};

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

// Smoothing time constant for the lighting ease: hides the 4 Hz snapshot
// steps and doubles as the fade of any admin jump.
const LIGHT_FADE_TAU_SECS: f32 = 0.8;

// Rebuild granularity of the moon-phase mesh (lit-fraction percentage
// points). The continuous ease would otherwise rebuild the lune mesh every
// frame through a whole dusk; half a point is sub-pixel at disc size.
const PHASE_MESH_STEP_PERCENT: f32 = 0.5;

// Authoritative lighting blend from the snapshot (`target`, two preset
// names + a blend factor); the rendered channels ease toward the resolved
// look in look space, so any target change — a cycle step, a segment
// crossing, an admin jump — fades with one mechanism.
#[derive(Resource)]
pub struct LightingState {
    pub target: LightingBlend,
    // Set by the first snapshot. Until it is — and on the frame it lands —
    // the blend system snaps instead of fading, so login shows the server's
    // current lighting immediately.
    pub synced: bool,
    eased: bool,
    current: LevelTargets,
}

impl Default for LightingState {
    fn default() -> Self {
        Self {
            // Matches the Startup seed from `lighting.bright`; the first
            // snapshot snaps to the server's real blend.
            target: LightingBlend {
                from: "bright".to_owned(),
                to: "bright".to_owned(),
                blend: 0.0,
            },
            synced: false,
            eased: false,
            current: LevelTargets::default(),
        }
    }
}

// Floor for the log-domain intensity channels: keeps ln() defined for the
// disc's `0 = off`, and anything easing down to the floor snaps back to 0
// at the application boundary.
const LOG_INTENSITY_FLOOR: f32 = 1e-3;

// A lighting look flattened for the renderer — the sun and moon configs name
// their values differently, the channels underneath are the same. The
// intensity channels (`sky`, `illuminance`, `ambient`, `disc`) are stored as
// ln(value): brightness perception is roughly logarithmic, so every lerp —
// the preset blend and the frame-to-frame ease — moves in even perceptual
// steps instead of spending a whole dusk looking bright and then plunging.
// `linear_intensity` converts back at the single application site. `color`
// is linear RGB (a hue shift; per-channel log would distort it) and
// `phase_percent`/`saturation` are already perceptual ratios.
#[derive(Default, Clone)]
struct LevelTargets {
    sky: f32,
    illuminance: f32,
    ambient: f32,
    disc: f32,
    phase_percent: f32,
    color: Vec3,
    saturation: f32,
}

fn log_intensity(value: f32) -> f32 {
    value.max(LOG_INTENSITY_FLOOR).ln()
}

fn linear_intensity(log_value: f32) -> f32 {
    let value = log_value.exp();
    if value <= LOG_INTENSITY_FLOOR * 1.5 { 0.0 } else { value }
}

impl LevelTargets {
    fn sun(sun: &SunLighting) -> Self {
        let color = SUN_DISC_COLOR.to_linear();
        Self {
            sky: log_intensity(sun.sky_brightness),
            illuminance: log_intensity(sun.sun_illuminance),
            ambient: log_intensity(sun.ambient_brightness),
            disc: log_intensity(sun.sun_disc_luminance),
            phase_percent: 100.0,
            color: Vec3::new(color.red, color.green, color.blue),
            saturation: sun.saturation,
        }
    }

    fn moon(moon: &MoonLighting) -> Self {
        let color = MOON_DISC_COLOR.to_linear();
        Self {
            sky: log_intensity(moon.sky_brightness),
            illuminance: log_intensity(moon.moon_illuminance),
            ambient: log_intensity(moon.ambient_brightness),
            disc: log_intensity(moon.moon_disc_luminance),
            phase_percent: moon.moon_phase_percent,
            color: Vec3::new(color.red, color.green, color.blue),
            saturation: moon.saturation,
        }
    }

    fn lerp(a: &Self, b: &Self, s: f32) -> Self {
        let lerp = |a: f32, b: f32| a + (b - a) * s;
        Self {
            sky: lerp(a.sky, b.sky),
            illuminance: lerp(a.illuminance, b.illuminance),
            ambient: lerp(a.ambient, b.ambient),
            disc: lerp(a.disc, b.disc),
            phase_percent: lerp(a.phase_percent, b.phase_percent),
            color: a.color.lerp(b.color, s),
            saturation: lerp(a.saturation, b.saturation),
        }
    }
}

// Resolve a server-named preset against the configured looks. An unknown
// name (a server/client vocabulary mismatch) falls back to dim.
fn look(config: &LightingConfig, name: &str) -> LevelTargets {
    match name {
        "bright" => LevelTargets::sun(&config.bright),
        "dim" => LevelTargets::moon(&config.dim),
        "dark" => LevelTargets::moon(&config.dark),
        _ => {
            warn_once!("unknown lighting preset {name:?} from server; using dim");
            LevelTargets::moon(&config.dim)
        }
    }
}

fn blend_targets(config: &LightingConfig, blend: &LightingBlend) -> LevelTargets {
    LevelTargets::lerp(
        &look(config, &blend.from),
        &look(config, &blend.to),
        blend.blend.clamp(0.0, 1.0),
    )
}

// Drive the world's lighting toward the snapshot's blend. Every channel is
// a raw absolute value from `client.json::lighting`'s looks, eased in look
// space and written absolutely every frame — idempotent, no incremental
// drift. Wall/actor lights stay lit — windows glowing in the dark.
pub fn lighting_blend_system(
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
    let blend = if lighting.eased {
        1.0 - (-time.delta_secs() / LIGHT_FADE_TAU_SECS).exp()
    } else {
        // Keep snapping until the first snapshot's blend has been applied;
        // only later changes get the fade.
        lighting.eased = lighting.synced;
        1.0
    };
    let target = blend_targets(&client_settings.lighting, &lighting.target);
    lighting.current = LevelTargets::lerp(&lighting.current, &target, blend);
    let level = lighting.current.clone();

    // Low light mutes the world: post-tonemap saturation on both cameras.
    for mut grading in &mut gradings {
        grading.global.post_saturation = level.saturation;
    }

    for mut skybox in &mut skyboxes {
        skybox.brightness = linear_intensity(level.sky);
    }
    for mut light in &mut sun_light {
        light.illuminance = linear_intensity(level.illuminance);
    }
    ambient.brightness = linear_intensity(level.ambient);
    let emissive = level.color * linear_intensity(level.disc);
    for (material, mut disc) in &mut discs {
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.emissive = LinearRgba::rgb(emissive.x, emissive.y, emissive.z);
        }
        // The phase is a mesh shape, not a smoothable value — rebuild in
        // threshold steps and snap.
        if (level.phase_percent - disc.phase_percent).abs() > PHASE_MESH_STEP_PERCENT {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> LightingConfig {
        LightingConfig::default()
    }

    fn wire(from: &str, to: &str, blend: f32) -> LightingBlend {
        LightingBlend {
            from: from.to_owned(),
            to: to.to_owned(),
            blend,
        }
    }

    fn assert_targets_eq(actual: &LevelTargets, expected: &LevelTargets) {
        assert!((actual.sky - expected.sky).abs() < 1e-3, "sky");
        assert!((actual.illuminance - expected.illuminance).abs() < 1e-3, "illuminance");
        assert!((actual.ambient - expected.ambient).abs() < 1e-3, "ambient");
        assert!((actual.disc - expected.disc).abs() < 1e-3, "disc");
        assert!(
            (actual.phase_percent - expected.phase_percent).abs() < 1e-3,
            "phase_percent"
        );
        assert!(actual.color.abs_diff_eq(expected.color, 1e-3), "color");
        assert!((actual.saturation - expected.saturation).abs() < 1e-3, "saturation");
    }

    #[test]
    fn presets_resolve_to_their_configured_looks() {
        let config = config();
        assert_targets_eq(
            &blend_targets(&config, &wire("bright", "bright", 0.0)),
            &LevelTargets::sun(&config.bright),
        );
        assert_targets_eq(
            &blend_targets(&config, &wire("dim", "dim", 0.0)),
            &LevelTargets::moon(&config.dim),
        );
        assert_targets_eq(
            &blend_targets(&config, &wire("dark", "dark", 0.0)),
            &LevelTargets::moon(&config.dark),
        );
    }

    #[test]
    fn blends_are_halfway_per_channel() {
        let config = config();
        let mid = blend_targets(&config, &wire("bright", "dark", 0.5));
        assert_targets_eq(
            &mid,
            &LevelTargets::lerp(
                &LevelTargets::sun(&config.bright),
                &LevelTargets::moon(&config.dark),
                0.5,
            ),
        );
        assert!(
            (mid.phase_percent - LevelTargets::moon(&config.dim).phase_percent).abs() > 1.0,
            "a direct bright↔dark blend must not be the dim look"
        );
    }

    #[test]
    fn blend_factor_clamps() {
        let config = config();
        assert_targets_eq(
            &blend_targets(&config, &wire("bright", "dark", 2.0)),
            &LevelTargets::moon(&config.dark),
        );
        assert_targets_eq(
            &blend_targets(&config, &wire("bright", "dark", -1.0)),
            &LevelTargets::sun(&config.bright),
        );
    }

    #[test]
    fn unknown_preset_falls_back_to_dim() {
        let config = config();
        assert_targets_eq(
            &blend_targets(&config, &wire("sunset", "sunset", 0.0)),
            &LevelTargets::moon(&config.dim),
        );
    }

    #[test]
    fn intensity_channels_blend_in_log_space() {
        let config = config();
        let mid = blend_targets(&config, &wire("bright", "dark", 0.5));
        // A log-domain midpoint is the geometric mean in linear terms —
        // perceptually halfway, unlike the arithmetic mean.
        let expected = (config.bright.sun_illuminance * config.dark.moon_illuminance).sqrt();
        let actual = linear_intensity(mid.illuminance);
        assert!(
            (actual - expected).abs() / expected < 1e-3,
            "expected geometric mean {expected}, got {actual}"
        );
    }

    #[test]
    fn zero_intensity_round_trips_to_off() {
        assert_eq!(linear_intensity(log_intensity(0.0)), 0.0);
        assert!(linear_intensity(log_intensity(5.0)) > 4.9);
    }

    #[test]
    fn default_lighting_state_targets_bright() {
        let state = LightingState::default();
        assert_eq!(state.target, wire("bright", "bright", 0.0));
        assert!(!state.synced);
    }
}
