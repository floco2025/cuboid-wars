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

#[cfg(test)]
mod tests {
    use super::*;

    fn cross_image(width: u32, height: u32) -> Image {
        Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![0; (width * height * 4) as usize],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        )
    }

    #[test]
    fn valid_cross_becomes_a_six_layer_cubemap() {
        let cubemap = create_cubemap_from_cross(&cross_image(8, 6)).expect("4x3 cross converts");
        assert_eq!(cubemap.texture_descriptor.size.width, 2);
        assert_eq!(cubemap.texture_descriptor.size.height, 2);
        assert_eq!(cubemap.texture_descriptor.size.depth_or_array_layers, 6);
        assert_eq!(
            cubemap
                .texture_view_descriptor
                .as_ref()
                .and_then(|descriptor| descriptor.dimension),
            Some(TextureViewDimension::Cube)
        );
    }

    #[test]
    fn malformed_dimensions_are_rejected() {
        // Height that is not 3 face rows.
        assert!(create_cubemap_from_cross(&cross_image(8, 5)).is_none());
        // Width not divisible into 4 faces.
        assert!(create_cubemap_from_cross(&cross_image(6, 6)).is_none());
        // Degenerate image.
        assert!(create_cubemap_from_cross(&cross_image(0, 0)).is_none());
    }

    #[test]
    fn faces_land_in_cubemap_layer_order() {
        let mut image = cross_image(8, 6);
        // Tag every pixel of each cross region with a distinct byte.
        // Cross layout:     [top]
        //               [left][front][right][back]
        //                   [bottom]
        let regions = [
            (4, 2, 1u8), // right  -> +X (layer 0)
            (0, 2, 2),   // left   -> -X (layer 1)
            (2, 0, 3),   // top    -> +Y (layer 2)
            (2, 4, 4),   // bottom -> -Y (layer 3)
            (2, 2, 5),   // front  -> +Z (layer 4)
            (6, 2, 6),   // back   -> -Z (layer 5)
        ];
        let data = image.data.as_mut().expect("test image has data");
        for (x0, y0, tag) in regions {
            for y in y0..y0 + 2 {
                for x in x0..x0 + 2u32 {
                    let offset = ((y * 8 + x) * 4) as usize;
                    data[offset..offset + 4].fill(tag);
                }
            }
        }

        let cubemap = create_cubemap_from_cross(&image).expect("tagged cross converts");
        let cubemap_data = cubemap.data.as_ref().expect("cubemap has data");
        let face_bytes = 2 * 2 * 4;
        for (layer, expected_tag) in [1u8, 2, 3, 4, 5, 6].into_iter().enumerate() {
            let face = &cubemap_data[layer * face_bytes..(layer + 1) * face_bytes];
            assert!(
                face.iter().all(|byte| *byte == expected_tag),
                "layer {layer} should hold tag {expected_tag}, got {face:?}"
            );
        }
    }
}
