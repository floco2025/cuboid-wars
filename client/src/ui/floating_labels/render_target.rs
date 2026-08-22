use bevy::{
    asset::RenderAssetUsages,
    camera::{ClearColorConfig, RenderTarget},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

pub fn setup_label_texture(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    width: u32,
    height: u32,
) -> (Handle<Image>, Entity) {
    let size = Extent3d {
        width,
        height,
        ..default()
    };

    // Fully transparent (BGRA, all zero) so the label texture starts clear.
    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 0],
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
