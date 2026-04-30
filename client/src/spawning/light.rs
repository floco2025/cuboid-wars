use bevy::{gltf::GltfAssetLabel, prelude::*, scene::SceneRoot};

use crate::{
    config::AssetSet,
    markers::{MapLevel, WallLightMarker},
};
use common::{map::compute_player_level, protocol::WallLight};

// Spawn a wall light from precomputed layout data (world-space position and yaw).
pub fn spawn_wall_light_from_layout(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
    light: &WallLight,
) {
    let wall_light = asset_set.wall_light_model();
    let light_scene: Handle<Scene> = asset_server.load(GltfAssetLabel::Scene(0).from_asset(wall_light.scene.clone()));
    let level = MapLevel(compute_player_level(light.pos.y));

    let model_yaw = Quat::from_rotation_y(light.yaw);
    let (sin_yaw, cos_yaw) = light.yaw.sin_cos();
    let light_pos = Vec3::new(
        wall_light.inward_offset.mul_add(sin_yaw, light.pos.x),
        light.pos.y,
        wall_light.inward_offset.mul_add(cos_yaw, light.pos.z),
    );

    commands.spawn((
        WallLightMarker,
        level,
        SceneRoot(light_scene),
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
            shadows_enabled: false,
            color: Color::srgb(1.0, 0.95, 0.85),
            ..default()
        },
        Transform::from_xyz(light_pos.x, light_pos.y, light_pos.z),
        Visibility::Visible,
        InheritedVisibility::default(),
        ViewVisibility::default(),
    ));
}
