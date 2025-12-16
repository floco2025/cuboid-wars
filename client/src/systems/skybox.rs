use bevy::{
    core_pipeline::Skybox,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

pub fn setup_skybox_from_cross(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Load the cross-layout skybox image
    let cross_image_handle: Handle<Image> = asset_server.load("skybox.png");

    // Store the handle in a resource so we can use it once loaded
    commands.insert_resource(SkyboxCrossImage(cross_image_handle));
}

#[derive(Resource)]
pub struct SkyboxCrossImage(Handle<Image>);

#[derive(Resource)]
pub struct SkyboxCubemap(pub Handle<Image>);

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

    // Convert the cross layout to cubemap
    let cubemap = create_cubemap_from_cross(image);
    let cubemap_handle = images.add(cubemap);

    commands.insert_resource(SkyboxCubemap(cubemap_handle));
    commands.remove_resource::<SkyboxCrossImage>();
}

fn create_cubemap_from_cross(cross_image: &Image) -> Image {
    // Parse the cross layout (assumes 4x3 layout)
    let width = cross_image.texture_descriptor.size.width;
    let height = cross_image.texture_descriptor.size.height;
    let face_size = width / 4; // Each face is 1/4 of the width

    if height != face_size * 3 {
        error!("skybox cross image has unexpected dimensions: {}x{}", width, height);
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
        Default::default(),
    );
    cubemap.texture_view_descriptor = Some(bevy::render::render_resource::TextureViewDescriptor {
        dimension: Some(bevy::render::render_resource::TextureViewDimension::Cube),
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
        let dst_face_offset = (face_idx * face_size as usize * face_size as usize * bytes_per_pixel);

        for y in 0..face_size {
            let src_y = y_offset + y;
            let src_offset = (src_y * width * bytes_per_pixel as u32 + x_offset * bytes_per_pixel as u32) as usize;
            let dst_offset = dst_face_offset + (y * face_size * bytes_per_pixel as u32) as usize;
            let row_bytes = (face_size * bytes_per_pixel as u32) as usize;

            cubemap_data[dst_offset..dst_offset + row_bytes].copy_from_slice(&data[src_offset..src_offset + row_bytes]);
        }
    }

    cubemap
}

// Add skybox to cameras once the cubemap is ready
pub fn skybox_update_camera_system(
    cubemap: Option<Res<SkyboxCubemap>>,
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
            image: cubemap.0.clone(),
            brightness: 1000.0,
            rotation: Quat::IDENTITY,
        });
    }
}
