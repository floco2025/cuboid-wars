use bevy::{
    asset::RenderAssetUsages,
    core_pipeline::Skybox,
    light::NotShadowCaster,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension},
};

use crate::{
    cameras::MainCameraMarker,
    config::{AssetSet, SkyboxDef},
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
pub struct SkyboxCrossImage(Handle<Image>);

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

pub fn skybox_convert_cross_to_cubemap_system(
    mut commands: Commands,
    cross_image: Option<Res<SkyboxCrossImage>>,
    mut images: ResMut<Assets<Image>>,
    cubemap: Option<Res<SkyboxCubemap>>,
) {
    // If we already have a cubemap, we're done
    if cubemap.is_some() {
        return;
    }

    let Some(cross_image) = cross_image else {
        return;
    };

    let Some(image) = images.get(&cross_image.0) else {
        return;
    };

    let Some(cubemap) = create_cubemap_from_cross(image) else {
        // Malformed image: drop the source so we don't re-log and retry every
        // frame. The game runs without a skybox rather than crashing.
        commands.remove_resource::<SkyboxCrossImage>();
        return;
    };
    let cubemap_handle = images.add(cubemap);

    commands.insert_resource(SkyboxCubemap(cubemap_handle));
    commands.remove_resource::<SkyboxCrossImage>();
}

fn create_cubemap_from_cross(cross_image: &Image) -> Option<Image> {
    // Cross layout is 4 faces wide, 3 tall; each face is `width / 4` square.
    let width = cross_image.texture_descriptor.size.width;
    let height = cross_image.texture_descriptor.size.height;
    let face_size = width / 4;

    // Bail before the extraction loop reads out of bounds on a bad image.
    if face_size == 0 || !width.is_multiple_of(4) || height != face_size * 3 {
        error!("skybox cross image has unexpected dimensions: {width}x{height}; expected a 4x3 cross");
        return None;
    }

    // Create cubemap image
    let mut cubemap = Image::new(
        Extent3d {
            width: face_size,
            height: face_size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        vec![0; (face_size * face_size * 4 * 6) as usize],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    cubemap.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });

    // Extract each face from the cross layout
    // Layout:
    //     [top]
    // [left][front][right][back]
    //     [bottom]

    let data = cross_image.data.as_ref().expect("cross image has no data");
    let bytes_per_pixel = 4usize; // RGBA

    // Face order for cubemap: +X, -X, +Y, -Y, +Z, -Z
    // Map to cross positions: right, left, top, bottom, front, back
    let face_positions = [
        (face_size * 2, face_size), // +X (right)
        (0, face_size),             // -X (left)
        (face_size, 0),             // +Y (top)
        (face_size, face_size * 2), // -Y (bottom)
        (face_size, face_size),     // +Z (front)
        (face_size * 3, face_size), // -Z (back)
    ];

    let cubemap_data = cubemap.data.as_mut().expect("cubemap has no data");

    for (face_idx, (x_offset, y_offset)) in face_positions.iter().enumerate() {
        let dst_face_offset = face_idx * face_size as usize * face_size as usize * bytes_per_pixel;

        for y in 0..face_size {
            let src_y = y_offset + y;
            let src_offset = (src_y * width * bytes_per_pixel as u32 + x_offset * bytes_per_pixel as u32) as usize;
            let dst_offset = dst_face_offset + (y * face_size * bytes_per_pixel as u32) as usize;
            let row_bytes = (face_size * bytes_per_pixel as u32) as usize;

            cubemap_data[dst_offset..dst_offset + row_bytes].copy_from_slice(&data[src_offset..src_offset + row_bytes]);
        }
    }

    Some(cubemap)
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
