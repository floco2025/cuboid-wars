use bevy::{
    asset::RenderAssetUsages,
    camera::{ClearColorConfig, RenderTarget},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

use crate::constants::{LABEL_TEXTURE_HEIGHT, LABEL_TEXTURE_WIDTH};

pub fn setup_label_texture(commands: &mut Commands, images: &mut ResMut<Assets<Image>>) -> (Handle<Image>, Entity) {
    let size = Extent3d {
        width: LABEL_TEXTURE_WIDTH,
        height: LABEL_TEXTURE_HEIGHT,
        ..default()
    };

    let bg = Color::NONE.to_srgba();
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[
            (bg.blue * 255.0) as u8,
            (bg.green * 255.0) as u8,
            (bg.red * 255.0) as u8,
            (bg.alpha * 255.0) as u8,
        ],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

    let image_handle = images.add(image);

    let text_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: -1,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(image_handle.clone().into()),
        ))
        .id();

    (image_handle, text_camera)
}
