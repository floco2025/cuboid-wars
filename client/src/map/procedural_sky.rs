use super::skybox::{CelestialLightMarker, LightingState, SkyboxCubemap};
use crate::{
    constants::{
        SKY_CLEAR_FOG_RANGE, SKY_CLEAR_HORIZON, SKY_CLEAR_ZENITH, SKY_CLOUD_COLOR, SKY_FACE_SIZE, SKY_NIGHT_HORIZON,
        SKY_NIGHT_OVERCAST, SKY_NIGHT_ZENITH, SKY_RAIN_AMBIENT_LIGHT, SKY_RAIN_BRIGHTNESS, SKY_RAIN_DIRECT_LIGHT,
        SKY_RAIN_FOG_RANGE, SKY_RAIN_HORIZON, SKY_RAIN_ZENITH,
    },
    vfx::RainIntensity,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::Exposure,
    core_pipeline::Skybox,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension},
};

#[derive(Resource)]
pub(super) struct ProceduralSky {
    clear: Vec<Vec3>,
    overcast: Vec<Vec3>,
    night: Vec<Vec3>,
    last_blend: (u8, u8),
}

fn horizon(rain: f32, daylight: f32) -> Vec3 {
    SKY_NIGHT_HORIZON
        .lerp(SKY_CLEAR_HORIZON, daylight)
        .lerp(SKY_NIGHT_OVERCAST.lerp(SKY_RAIN_HORIZON, daylight), rain)
}

fn noise(p: Vec3) -> f32 {
    let q = p.floor();
    let t = p - q;
    let t = t * t * (Vec3::splat(3.0) - t * 2.0);
    let hash = |offset: Vec3| {
        let v = ((q + offset).dot(Vec3::new(127.1, 311.7, 74.7))).sin() * 43758.545;
        v - v.floor()
    };
    let layer = |z| {
        hash(Vec3::new(0.0, 0.0, z))
            .lerp(hash(Vec3::new(1.0, 0.0, z)), t.x)
            .lerp(
                hash(Vec3::new(0.0, 1.0, z)).lerp(hash(Vec3::new(1.0, 1.0, z)), t.x),
                t.y,
            )
    };
    layer(0.0).lerp(layer(1.0), t.z)
}

fn sky_colors(direction: Vec3) -> (Vec3, Vec3, Vec3) {
    let elevation = direction.y.max(0.0).powf(0.45);
    let mut clear = horizon(0.0, 1.0).lerp(SKY_CLEAR_ZENITH, elevation);
    let p = Vec3::new(direction.x, 0.0, direction.z) * (3.0 / (direction.y.max(0.0) + 0.2));
    let cloud = noise(p) * 0.57 + noise(p * 2.0) * 0.28 + noise(p * 4.0) * 0.15;
    let coverage = ((cloud - 0.52) * 5.0).clamp(0.0, 1.0) * (direction.y * 7.0).clamp(0.0, 1.0);
    clear = clear.lerp(SKY_CLOUD_COLOR, coverage);
    let overcast = horizon(1.0, 1.0).lerp(SKY_RAIN_ZENITH * (0.7 + cloud * 0.6), elevation);
    let mut night = horizon(0.0, 0.0).lerp(SKY_NIGHT_ZENITH, elevation);
    let star = noise(direction * 700.0);
    if star > 0.89 && direction.y > 0.1 {
        night += Vec3::splat((star - 0.89) * 3.5);
    }
    (clear, overcast, night)
}

pub(super) fn setup(commands: &mut Commands, images: &mut Assets<Image>) {
    let mut sky = ProceduralSky {
        clear: Vec::new(),
        overcast: Vec::new(),
        night: Vec::new(),
        last_blend: (255, 255),
    };
    for face in 0..6 {
        for row in 0..SKY_FACE_SIZE {
            for column in 0..SKY_FACE_SIZE {
                let s = (column as f32 + 0.5) * 2.0 / SKY_FACE_SIZE as f32 - 1.0;
                let t = (row as f32 + 0.5) * 2.0 / SKY_FACE_SIZE as f32 - 1.0;
                let direction = match face {
                    0 => Vec3::new(1.0, -t, -s),
                    1 => Vec3::new(-1.0, -t, s),
                    2 => Vec3::new(s, 1.0, t),
                    3 => Vec3::new(s, -1.0, -t),
                    4 => Vec3::new(s, -t, 1.0),
                    _ => Vec3::new(-s, -t, -1.0),
                }
                .normalize();
                let (clear, overcast, night) = sky_colors(direction);
                sky.clear.push(clear);
                sky.overcast.push(overcast);
                sky.night.push(night);
            }
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: SKY_FACE_SIZE,
            height: SKY_FACE_SIZE,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        vec![0; (SKY_FACE_SIZE * SKY_FACE_SIZE * 6 * 4) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    commands.insert_resource(SkyboxCubemap(images.add(image)));
    commands.insert_resource(sky);
}

pub(super) fn procedural_sky_system(
    mut commands: Commands,
    mut sky: Option<ResMut<ProceduralSky>>,
    cubemap: Option<Res<SkyboxCubemap>>,
    mut images: ResMut<Assets<Image>>,
    rain: Res<RainIntensity>,
    lighting: Res<LightingState>,
    mut cameras: Query<(Entity, &mut Skybox, Option<&mut DistanceFog>, Option<&Exposure>), With<Camera3d>>,
    mut light: Query<&mut DirectionalLight, With<CelestialLightMarker>>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let (Some(sky), Some(cubemap)) = (sky.as_mut(), cubemap) else {
        return;
    };
    let (daylight, brightness) = lighting.environment();
    let rain = rain.current.clamp(0.0, 1.0);
    let blend = ((rain * 100.0).round() as u8, (daylight * 100.0).round() as u8);
    if blend != sky.last_blend {
        if let Some(mut image) = images.get_mut(&cubemap.0) {
            let data = image.data.as_mut().expect("procedural sky pixels missing");
            for (i, pixel) in data.chunks_exact_mut(4).enumerate() {
                let day = sky.clear[i].lerp(sky.overcast[i], rain);
                let night = sky.night[i].lerp(horizon(1.0, 0.0), rain);
                let color = night.lerp(day, daylight).clamp(Vec3::ZERO, Vec3::ONE) * 255.0;
                pixel.copy_from_slice(&[color.x.round() as u8, color.y.round() as u8, color.z.round() as u8, 255]);
            }
        }
        sky.last_blend = blend;
    }
    let sky_brightness = brightness * 1.0_f32.lerp(SKY_RAIN_BRIGHTNESS, rain);
    let fog = horizon(rain, daylight);
    let fog_color = Color::srgb(fog.x, fog.y, fog.z).to_linear() * sky_brightness;
    for (entity, mut skybox, fog, exposure) in &mut cameras {
        skybox.brightness = sky_brightness;
        let fog_color = fog_color * exposure.copied().unwrap_or_default().exposure();
        let value = DistanceFog {
            color: Color::linear_rgb(fog_color.red, fog_color.green, fog_color.blue),
            directional_light_color: Color::NONE,
            falloff: FogFalloff::Linear {
                start: SKY_CLEAR_FOG_RANGE[0].lerp(SKY_RAIN_FOG_RANGE[0], rain),
                end: SKY_CLEAR_FOG_RANGE[1].lerp(SKY_RAIN_FOG_RANGE[1], rain),
            },
            ..default()
        };
        if let Some(mut fog) = fog {
            *fog = value;
        } else {
            commands.entity(entity).insert(value);
        }
    }
    for mut light in &mut light {
        light.illuminance *= 1.0_f32.lerp(SKY_RAIN_DIRECT_LIGHT, rain);
    }
    ambient.brightness *= 1.0_f32.lerp(SKY_RAIN_AMBIENT_LIGHT, rain);
}

#[cfg(test)]
#[path = "tests/procedural_sky.rs"]
mod tests;
