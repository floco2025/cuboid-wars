use bevy::{
    asset::AssetPath,
    image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};

// Load a texture with repeat addressing so UVs beyond 0..1 tile instead of clamping.
pub fn load_repeating_texture(asset_server: &AssetServer, path: impl Into<AssetPath<'static>>) -> Handle<Image> {
    asset_server.load_with_settings(path, |settings: &mut ImageLoaderSettings| {
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            address_mode_w: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            anisotropy_clamp: 8,
            ..default()
        });
    })
}

pub fn load_repeating_texture_linear(asset_server: &AssetServer, path: impl Into<AssetPath<'static>>) -> Handle<Image> {
    asset_server.load_with_settings(path, |settings: &mut ImageLoaderSettings| {
        settings.is_srgb = false;
        settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
            address_mode_u: ImageAddressMode::Repeat,
            address_mode_v: ImageAddressMode::Repeat,
            address_mode_w: ImageAddressMode::Repeat,
            mag_filter: ImageFilterMode::Linear,
            min_filter: ImageFilterMode::Linear,
            mipmap_filter: ImageFilterMode::Linear,
            anisotropy_clamp: 8,
            ..default()
        });
    })
}
