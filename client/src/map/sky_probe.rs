use std::f32::consts::PI;

use bevy::{
    asset::RenderAssetUsages,
    light::GeneratedEnvironmentMapLight,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension},
};
use half::f16;

use super::{celestial::SkyState, clouds::cumulus_toward};
use crate::{
    cameras::MainCameraMarker,
    config::{ClientSettings, SkyConfig},
    constants::{
        SKY_CLOUD_COLOR, SKY_CLOUD_SCALE, SKY_CLOUD_SHADOW_COLOR, SKY_DAY_HORIZON_COLOR, SKY_DAY_ZENITH_COLOR,
        SKY_NIGHT_HORIZON_COLOR, SKY_NIGHT_ZENITH_COLOR, SKY_OVERCAST_COLOR, SKY_PROBE_GROUND_BOUNCE,
        SKY_PROBE_GROUND_GREYING, SKY_PROBE_REFRESH_SECS, SKY_SUN_HALO_LUMINANCE, SKY_SUN_HALO_SIZE_DEGREES,
        SKY_SUNSET_COLOR, SKY_TWILIGHT_HORIZON_COLOR, SKY_TWILIGHT_ZENITH_COLOR,
    },
};

// Cubemap edge in texels; the diffuse and specular filters Bevy runs over it
// need nothing sharper than the sky's gradient, halo, and cloud masses.
const PROBE_SIZE: u32 = 16;
const FACES: u32 = 6;
// Just under the horizon the sky blends into the ground's bounce, so a
// reflection of the skyline has no hard seam.
const HORIZON_BLEND: f32 = 0.12;

// The small HDR cubemap the main camera lights the scene from. It mirrors
// `sky.wgsl` at probe fidelity — gradient, sunset band, sun halo, clouds,
// haze; no stars, no discs — and fills the lower half with the meadow's
// bounce, since the dome shows sky there but the ground is what surrounds a
// surface below the horizon. The radiance is rendered every
// `SKY_PROBE_REFRESH_SECS` and the image crossfades to it over the same
// interval: a probe that stepped would step the ambient light with it,
// and under moving overcast, where every render shifts the bright gaps
// between the clouds, that step flickers every leaf.
#[derive(Resource)]
pub struct SkyProbe {
    image: Handle<Image>,
    refreshed_at: f32,
    fade: Option<ProbeFade>,
}

struct ProbeFade {
    from: Vec<Vec3>,
    to: Vec<Vec3>,
}

impl SkyProbe {
    // Renders a fresh target and starts fading from whatever is shown now.
    fn retarget(&mut self, now: f32, target: Vec<Vec3>) {
        let from = self.shown(now).unwrap_or_else(|| target.clone());
        self.refreshed_at = now;
        self.fade = Some(ProbeFade { from, to: target });
    }

    // The radiance the image shows at `now`, `None` before the first render.
    fn shown(&self, now: f32) -> Option<Vec<Vec3>> {
        let fade = self.fade.as_ref()?;
        let progress = ((now - self.refreshed_at) / SKY_PROBE_REFRESH_SECS).clamp(0.0, 1.0);
        Some(
            fade.from
                .iter()
                .zip(&fade.to)
                .map(|(from, to)| from.lerp(*to, progress))
                .collect(),
        )
    }

    fn fading(&self, now: f32) -> bool {
        self.fade.is_some() && now - self.refreshed_at < SKY_PROBE_REFRESH_SECS
    }
}

pub fn setup_sky_probe_system(
    mut commands: Commands,
    probe: Option<Res<SkyProbe>>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<Entity, (With<MainCameraMarker>, Without<GeneratedEnvironmentMapLight>)>,
) {
    let image = match probe {
        Some(probe) => probe.image.clone(),
        None => {
            let mut image = Image::new_fill(
                Extent3d {
                    width: PROBE_SIZE,
                    height: PROBE_SIZE,
                    depth_or_array_layers: FACES,
                },
                TextureDimension::D2,
                &[0; 8],
                TextureFormat::Rgba16Float,
                RenderAssetUsages::all(),
            );
            image.texture_view_descriptor = Some(TextureViewDescriptor {
                dimension: Some(TextureViewDimension::Cube),
                ..default()
            });
            let handle = images.add(image);
            commands.insert_resource(SkyProbe {
                image: handle.clone(),
                refreshed_at: f32::NEG_INFINITY,
                fade: None,
            });
            handle
        }
    };
    for camera in &cameras {
        commands.entity(camera).insert(GeneratedEnvironmentMapLight {
            environment_map: image.clone(),
            intensity: 1.0,
            ..default()
        });
    }
}

pub fn refresh_sky_probe_system(
    time: Res<Time>,
    state: Res<SkyState>,
    settings: Res<ClientSettings>,
    probe: Option<ResMut<SkyProbe>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(mut probe) = probe else {
        return;
    };
    let now = time.elapsed_secs();
    let due = now - probe.refreshed_at >= SKY_PROBE_REFRESH_SECS;
    if due {
        let target = probe_radiance(&state, settings.sky, settings.grass.base_color());
        probe.retarget(now, target);
    }
    if (due || probe.fading(now))
        && let Some(shown) = probe.shown(now)
        && let Some(mut image) = images.get_mut(&probe.image)
    {
        image.data = Some(probe_bytes(&shown));
    }
}

const LUMINANCE: Vec3 = Vec3::new(0.2126, 0.7152, 0.0722);

// Radiance in cd/m² for every texel of the six faces, face-major. The dome
// is art-directed, not physical, so its colours are scaled until the sky's
// mean luminance is the configured ambient level: the JSON still sets how
// much ambient there is, the sky sets its colour and where it comes from.
fn probe_radiance(state: &SkyState, sky: SkyConfig, ground_albedo: Color) -> Vec<Vec3> {
    let count = (FACES * PROBE_SIZE * PROBE_SIZE) as usize;
    let mut directions = Vec::with_capacity(count);
    let mut radiance = Vec::with_capacity(count);
    let mut sky_total = Vec3::ZERO;
    let mut sky_count = 0.0f32;
    for face in 0..FACES {
        for v in 0..PROBE_SIZE {
            for u in 0..PROBE_SIZE {
                let direction = face_direction(face, u, v);
                let sky = sky_radiance(direction, state, sky);
                if direction.y > 0.0 {
                    sky_total += sky;
                    sky_count += 1.0;
                }
                directions.push(direction);
                radiance.push(sky);
            }
        }
    }
    let sky_mean = sky_total / sky_count.max(1.0);
    let scale = state.ambient_brightness / sky_mean.dot(LUMINANCE).max(1e-4);
    // A horizontal surface sees the mean sky radiance over its hemisphere plus
    // the sun's illuminance, and reflects both by its albedo — damped and
    // greyed, since the meadow is not the only ground and a full-strength
    // green bounce paints every wall lime.
    let albedo = ground_albedo.to_linear().to_vec3();
    let albedo = albedo.lerp(Vec3::splat(albedo.dot(LUMINANCE)), SKY_PROBE_GROUND_GREYING);
    let sunlight = state.sun_illuminance * state.sun_direction.y.max(0.0) / PI;
    let ground = albedo * (sky_mean * scale + Vec3::splat(sunlight)) * SKY_PROBE_GROUND_BOUNCE;
    directions
        .iter()
        .zip(&radiance)
        .map(|(direction, sky)| {
            let below = 1.0 - smoothstep(-HORIZON_BLEND, 0.0, direction.y);
            (*sky * scale).lerp(ground, below)
        })
        .collect()
}

// The image's `Rgba16Float` texels.
fn probe_bytes(radiance: &[Vec3]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(radiance.len() * 8);
    for color in radiance {
        for channel in [color.x, color.y, color.z, 1.0] {
            bytes.extend_from_slice(&f16::from_f32(channel).to_le_bytes());
        }
    }
    bytes
}

// The direction through a texel's centre, in the cube face layout the GPU
// samples: +X, -X, +Y, -Y, +Z, -Z, rows running down each face.
fn face_direction(face: u32, u: u32, v: u32) -> Vec3 {
    let s = (u as f32 + 0.5) / PROBE_SIZE as f32 * 2.0 - 1.0;
    let t = (v as f32 + 0.5) / PROBE_SIZE as f32 * 2.0 - 1.0;
    match face {
        0 => Vec3::new(1.0, -t, -s),
        1 => Vec3::new(-1.0, -t, s),
        2 => Vec3::new(s, 1.0, t),
        3 => Vec3::new(s, -1.0, -t),
        4 => Vec3::new(s, -t, 1.0),
        _ => Vec3::new(-s, -t, -1.0),
    }
    .normalize()
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> Vec3 {
    Vec3::from_array(a).lerp(Vec3::from_array(b), t)
}

// The dome's exposed colour along `direction`; the same terms as the
// fragment shader, minus the stars and the bodies' discs.
fn sky_radiance(direction: Vec3, state: &SkyState, sky: SkyConfig) -> Vec3 {
    let gradient = direction.y.clamp(0.0, 1.0).powf(0.42);
    let night_sky = mix(SKY_NIGHT_HORIZON_COLOR, SKY_NIGHT_ZENITH_COLOR, gradient) * sky.night_brightness;
    let twilight_sky = mix(SKY_TWILIGHT_HORIZON_COLOR, SKY_TWILIGHT_ZENITH_COLOR, gradient) * sky.twilight_brightness;
    let day_sky = mix(SKY_DAY_HORIZON_COLOR, SKY_DAY_ZENITH_COLOR, gradient) * sky.day_brightness;
    let sky_gradient = night_sky
        .lerp(twilight_sky, state.twilight)
        .lerp(day_sky, state.daylight);
    let mut color = sky_gradient;

    let sun = state.sun_direction;
    let ray_flat = Vec2::new(direction.x, direction.z);
    let sun_flat = Vec2::new(sun.x, sun.z);
    let toward_sun = if ray_flat.length() > 0.0001 && sun_flat.length() > 0.0001 {
        ray_flat.normalize().dot(sun_flat.normalize()).max(0.0)
    } else {
        0.0
    };
    let sunset = 1.0 - smoothstep(0.02, 0.30, state.sun_altitude.abs());
    let sunset_band = (-direction.y.abs() * 8.0).exp() * toward_sun.powi(7) * sunset;
    color += Vec3::from_array(SKY_SUNSET_COLOR) * sunset_band;

    let separation = direction.dot(sun).clamp(-1.0, 1.0).acos();
    let halo = (-(separation / SKY_SUN_HALO_SIZE_DEGREES.to_radians()).powf(1.35)).exp() * SKY_SUN_HALO_LUMINANCE;
    color += Vec3::new(1.0, 0.62, 0.30) * halo;

    let rain = state.rain;
    let coverage = sky.clouds.clear_coverage.lerp(sky.clouds.overcast_coverage, rain);
    let opacity = cumulus_toward(
        direction,
        state.seconds,
        coverage,
        SKY_CLOUD_SCALE,
        sky.clouds.movement_speed_degrees_per_second.to_radians(),
    );
    let sun_height = smoothstep(-0.08, 0.43, state.sun_altitude);
    let mut cloud = cloud_shading(0.6, state, sky, sun_height, sunset);
    cloud += Vec3::from_array(SKY_SUNSET_COLOR) * sunset_band * opacity * 0.32;
    let haze = 1.0 - smoothstep(0.0, 0.3, direction.y);
    let haze_rgb = sky_gradient.lerp(
        Vec3::from_array(SKY_OVERCAST_COLOR) * sky.day_brightness * 1.6,
        rain * state.daylight,
    );
    cloud = cloud.lerp(haze_rgb, haze * 0.7);
    color.lerp(cloud, opacity).max(Vec3::ZERO)
}

fn cloud_shading(lit: f32, state: &SkyState, sky: SkyConfig, sun_height: f32, sunset: f32) -> Vec3 {
    let night = mix(SKY_NIGHT_HORIZON_COLOR, SKY_NIGHT_ZENITH_COLOR, 0.45) * sky.night_brightness * 1.75;
    let dusk = mix(SKY_TWILIGHT_HORIZON_COLOR, SKY_TWILIGHT_ZENITH_COLOR, 0.25) * sky.twilight_brightness * 1.15;
    let golden = Vec3::ONE.lerp(Vec3::new(1.0, 0.78, 0.55), sunset * 0.7);
    let day_lit = Vec3::from_array(SKY_CLOUD_COLOR) * golden * sky.day_brightness * 0.7f32.lerp(1.1, sun_height);
    let underside = Vec3::from_array(SKY_SUNSET_COLOR) * sky.day_brightness * 0.8;
    let day_shadow = (Vec3::from_array(SKY_CLOUD_SHADOW_COLOR) * sky.day_brightness).lerp(underside, sunset * 0.6);
    let fair = day_shadow.lerp(day_lit, lit);
    let storm = Vec3::from_array(SKY_OVERCAST_COLOR) * sky.day_brightness * 0.8f32.lerp(1.2, lit);
    let dark = night.lerp(dusk, state.twilight) * 0.7f32.lerp(1.0, lit);
    dark.lerp(fair.lerp(storm, state.rain), state.daylight)
}

#[cfg(test)]
#[path = "tests/sky_probe.rs"]
mod tests;
