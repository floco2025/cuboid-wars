use std::collections::HashMap;

use bevy::{
    image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
};

use crate::{config::MaterialDef, constants::TEXTURE_ANISOTROPY};

impl MaterialDef {
    #[must_use]
    pub fn standard_material(&self, asset_server: &AssetServer) -> StandardMaterial {
        StandardMaterial {
            base_color_texture: Some(load_texture(
                asset_server,
                &self.textures.base_color,
                self.repeat,
                false,
            )),
            normal_map_texture: Some(load_texture(
                asset_server,
                &self.textures.normal,
                self.repeat,
                self.data_textures_linear,
            )),
            occlusion_texture: Some(load_texture(
                asset_server,
                &self.textures.occlusion,
                self.repeat,
                self.data_textures_linear,
            )),
            metallic_roughness_texture: Some(load_texture(
                asset_server,
                &self.textures.metallic_roughness,
                self.repeat,
                self.data_textures_linear,
            )),
            metallic: self.metallic,
            perceptual_roughness: self.perceptual_roughness,
            ..default()
        }
    }

    #[must_use]
    pub fn standard_item_material(&self, asset_server: &AssetServer, item_color: Color) -> StandardMaterial {
        let mut material = self.standard_material(asset_server);
        if self.base_color.as_deref() == Some("item_type_color") {
            material.base_color = item_color;
        }
        if self.emissive.as_deref() == Some("item_type_color") {
            material.emissive = LinearRgba::from(item_color) * self.emissive_strength.unwrap_or(1.0);
        }
        material
    }
}

#[derive(Default)]
pub struct MaterialHandleCache {
    standard: HashMap<String, Handle<StandardMaterial>>,
}

impl MaterialHandleCache {
    pub fn standard(
        &mut self,
        id: &str,
        material_def: &MaterialDef,
        asset_server: &AssetServer,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        self.standard
            .entry(id.to_owned())
            .or_insert_with(|| materials.add(material_def.standard_material(asset_server)))
            .clone()
    }
}

fn load_texture(asset_server: &AssetServer, path: &str, repeat: bool, linear: bool) -> Handle<Image> {
    if !repeat && !linear {
        return asset_server.load(path.to_owned());
    }

    asset_server.load_with_settings(path.to_owned(), move |settings: &mut ImageLoaderSettings| {
        settings.is_srgb = !linear;
        if repeat {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                address_mode_w: ImageAddressMode::Repeat,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                anisotropy_clamp: TEXTURE_ANISOTROPY,
                ..default()
            });
        }
    })
}
