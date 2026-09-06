use bevy::{
    image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};

use crate::config::MaterialDef;

impl MaterialDef {
    #[must_use]
    pub fn standard_material(
        &self,
        asset_server: &AssetServer,
        anisotropy: u16,
        mipmaps_enabled: bool,
    ) -> StandardMaterial {
        StandardMaterial {
            base_color_texture: Some(load_texture(
                asset_server,
                &self.textures.base_color,
                self.repeat,
                false,
                anisotropy,
                mipmaps_enabled,
            )),
            normal_map_texture: Some(load_texture(
                asset_server,
                &self.textures.normal,
                self.repeat,
                self.linear_data_textures,
                anisotropy,
                mipmaps_enabled,
            )),
            occlusion_texture: Some(load_texture(
                asset_server,
                &self.textures.occlusion,
                self.repeat,
                self.linear_data_textures,
                anisotropy,
                mipmaps_enabled,
            )),
            metallic_roughness_texture: Some(load_texture(
                asset_server,
                &self.textures.metallic_roughness,
                self.repeat,
                self.linear_data_textures,
                anisotropy,
                mipmaps_enabled,
            )),
            metallic: self.metallic,
            perceptual_roughness: self.perceptual_roughness,
            ..default()
        }
    }
}

fn load_texture(
    asset_server: &AssetServer,
    path: &str,
    repeat: bool,
    linear: bool,
    anisotropy: u16,
    mipmaps_enabled: bool,
) -> Handle<Image> {
    if !repeat && !linear {
        return asset_server.load(path.to_owned());
    }

    asset_server
        .load_builder()
        .with_settings(move |settings: &mut ImageLoaderSettings| {
            settings.is_srgb = !linear;
            if repeat {
                settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                    address_mode_u: ImageAddressMode::Repeat,
                    address_mode_v: ImageAddressMode::Repeat,
                    address_mode_w: ImageAddressMode::Repeat,
                    mag_filter: ImageFilterMode::Linear,
                    min_filter: ImageFilterMode::Linear,
                    mipmap_filter: if mipmaps_enabled {
                        ImageFilterMode::Linear
                    } else {
                        ImageFilterMode::Nearest
                    },
                    anisotropy_clamp: if mipmaps_enabled { anisotropy } else { 1 },
                    ..default()
                });
            }
        })
        .load(path.to_owned())
}
