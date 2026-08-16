use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension},
};

use super::skybox::{SkyboxCrossImage, SkyboxCubemap};

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
