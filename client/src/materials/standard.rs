use bevy::{
    image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};

use super::MaterialTextures;
use crate::config::MaterialDef;

impl MaterialDef {
    #[must_use]
    pub fn standard_material(
        &self,
        material_textures: &mut MaterialTextures,
        anisotropy: u16,
        mipmaps_enabled: bool,
    ) -> StandardMaterial {
        let Some(textures) = &self.textures else {
            return StandardMaterial {
                metallic: self.metallic,
                perceptual_roughness: self.perceptual_roughness,
                ..default()
            };
        };
        StandardMaterial {
            base_color_texture: Some(load_texture(
                material_textures,
                &textures.base_color,
                self.repeat,
                false,
                anisotropy,
                mipmaps_enabled,
            )),
            normal_map_texture: Some(load_texture(
                material_textures,
                &textures.normal,
                self.repeat,
                self.linear_data_textures,
                anisotropy,
                mipmaps_enabled,
            )),
            occlusion_texture: Some(load_texture(
                material_textures,
                &textures.occlusion,
                self.repeat,
                self.linear_data_textures,
                anisotropy,
                mipmaps_enabled,
            )),
            metallic_roughness_texture: Some(load_texture(
                material_textures,
                &textures.metallic_roughness,
                self.repeat,
                self.linear_data_textures,
                anisotropy,
                mipmaps_enabled,
            )),
            metallic: self.metallic,
            perceptual_roughness: self.perceptual_roughness,
            // Bevy samples OpenGL-convention normals; the packs are DirectX and say so in the name.
            flip_normal_map_y: textures
                .normal_is_directx()
                .expect("normal map name carries no -dx or -gl convention"),
            ..default()
        }
    }
}

pub(super) fn load_texture(
    material_textures: &mut MaterialTextures,
    path: &str,
    repeat: bool,
    linear: bool,
    anisotropy: u16,
    mipmaps_enabled: bool,
) -> Handle<Image> {
    let mut settings = ImageLoaderSettings {
        is_srgb: !linear,
        ..default()
    };
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
    material_textures.load(path, settings)
}
