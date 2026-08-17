use bevy::{gltf::GltfAssetLabel, prelude::*};

use crate::{
    config::AssetSet,
    constants::{
        WALL_LIGHT_FLICKER_DEPTH, WALL_LIGHT_FLICKER_HZ_A, WALL_LIGHT_FLICKER_HZ_B, WALL_LIGHT_FLICKER_THRESHOLD,
    },
    map::MapLevel,
};

// Marker for the lamp scene root and its emitting `PointLight` child entities.
#[derive(Component)]
pub struct WallLightMarker;
use common::{map::level_for_y, protocol::WallLight};

// Per-light flicker state; phase is derived from the light's position so
// neighbouring lights never dip in sync.
#[derive(Component)]
pub struct WallLightFlicker {
    base_intensity: f32,
    phase: f32,
}

// Spawn a wall light from precomputed layout data (world-space position and yaw).
pub fn spawn_wall_light_from_layout(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    light: &WallLight,
) {
    let wall_light = asset_set.wall_light_model();
    let light_scene: Handle<WorldAsset> =
        asset_server.load(GltfAssetLabel::Scene(0).from_asset(wall_light.scene.clone()));
    let level = MapLevel(level_for_y(light.pos.y));

    let model_yaw = Quat::from_rotation_y(light.yaw);
    let (sin_yaw, cos_yaw) = light.yaw.sin_cos();
    let light_pos = Vec3::new(
        wall_light.offset_from_wall.mul_add(sin_yaw, light.pos.x),
        light.pos.y,
        wall_light.offset_from_wall.mul_add(cos_yaw, light.pos.z),
    );

    commands.spawn((
        WallLightMarker,
        level,
        WorldAssetRoot(light_scene),
        Transform::from_xyz(light.pos.x, light.pos.y, light.pos.z)
            .with_scale(Vec3::splat(wall_light.scale))
            .with_rotation(model_yaw),
        GlobalTransform::default(),
        Visibility::Visible,
        InheritedVisibility::default(),
        ViewVisibility::default(),
    ));

    commands.spawn((
        WallLightMarker,
        level,
        PointLight {
            intensity: wall_light.brightness,
            range: wall_light.range,
            radius: wall_light.radius,
            shadow_maps_enabled: false,
            color: Color::srgb(1.0, 0.95, 0.85),
            ..default()
        },
        WallLightFlicker {
            base_intensity: wall_light.brightness,
            phase: light_pos.x.mul_add(12.9898, light_pos.z * 78.233).sin().abs() * 100.0,
        },
        Transform::from_xyz(light_pos.x, light_pos.y, light_pos.z),
        Visibility::Visible,
        InheritedVisibility::default(),
        ViewVisibility::default(),
    ));
}

// Occasional subtle dips in wall-light intensity. Only the `PointLight`
// component is written (never materials), and only while a dip is active —
// steady-state frames touch nothing, so no per-frame light re-extraction.
pub fn wall_light_flicker_system(time: Res<Time>, mut lights: Query<(&mut PointLight, &WallLightFlicker)>) {
    let t = time.elapsed_secs_wrapped();
    for (mut light, flicker) in &mut lights {
        let a = (t.mul_add(WALL_LIGHT_FLICKER_HZ_A, flicker.phase)).sin();
        let b = (t.mul_add(WALL_LIGHT_FLICKER_HZ_B, flicker.phase * 1.7)).sin();
        let burst = ((a * b) - WALL_LIGHT_FLICKER_THRESHOLD).max(0.0) / (1.0 - WALL_LIGHT_FLICKER_THRESHOLD);
        let target = flicker.base_intensity * WALL_LIGHT_FLICKER_DEPTH.mul_add(-burst, 1.0);
        if (light.intensity - target).abs() > flicker.base_intensity * 0.002 {
            light.intensity = target;
        }
    }
}
