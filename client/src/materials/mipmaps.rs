use std::collections::{HashMap, HashSet};

use bevy::{
    asset::AssetId,
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::TextureUsages,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use bevy_mod_mipmap_generator::{MipmapGeneratorSettings, check_image_compatible, generate_mips_texture};

use crate::config::RenderSettings;

const MAX_PENDING_MIPMAP_TASKS: usize = 2;

#[derive(Default)]
pub struct MaterialMipmapState {
    processed: HashSet<AssetId<Image>>,
    pending: HashMap<AssetId<Image>, (Handle<Image>, Task<Option<Image>>)>,
}

pub fn generate_material_mipmaps_system(
    mut state: Local<MaterialMipmapState>,
    materials: Res<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    render_settings: Res<RenderSettings>,
) {
    // The upstream plugin's event-driven system can miss our textures because a
    // StandardMaterial may be added before its Image assets have loaded. Scanning
    // materials and retrying unloaded images makes mip generation robust for our
    // asset-server-loaded material set.
    finish_mipmap_tasks(&mut state, &mut images);

    let settings = MipmapGeneratorSettings {
        anisotropic_filtering: render_settings.texture_anisotropy,
        ..default()
    };

    let thread_pool = AsyncComputeTaskPool::get();
    for material in materials.iter().map(|(_, material)| material) {
        for image_handle in standard_material_images(material) {
            if state.pending.len() >= MAX_PENDING_MIPMAP_TASKS {
                return;
            }

            let image_id = image_handle.id();
            if state.processed.contains(&image_id) || state.pending.contains_key(&image_id) {
                continue;
            }

            let Some(image) = images.get_mut(image_handle) else {
                continue;
            };

            // Runtime render targets, such as floating label textures, are not
            // source texture assets and can use formats the mipmap generator
            // does not support. They render correctly without generated mips.
            if image
                .texture_descriptor
                .usage
                .contains(TextureUsages::RENDER_ATTACHMENT)
            {
                state.processed.insert(image_id);
                continue;
            }

            configure_mipmap_sampler(image, render_settings.texture_anisotropy);

            if image.texture_descriptor.mip_level_count > 1 {
                state.processed.insert(image_id);
                continue;
            }

            if let Err(error) = check_image_compatible(image) {
                debug!("skipping mipmap generation for incompatible image: {error}");
                state.processed.insert(image_id);
                continue;
            }

            let mut image = image.clone();
            let settings = settings.clone();
            let task = thread_pool.spawn(async move {
                let mut added_cache_size = 0;
                generate_mips_texture(&mut image, &settings, &mut added_cache_size)
                    .map(|()| image)
                    .map_err(|error| {
                        warn!("failed to generate mipmaps: {error}");
                    })
                    .ok()
            });
            state.pending.insert(image_id, (image_handle.clone(), task));
        }
    }
}

fn finish_mipmap_tasks(state: &mut MaterialMipmapState, images: &mut Assets<Image>) {
    let mut completed = Vec::new();
    for (image_id, (image_handle, task)) in &mut state.pending {
        let Some(image) = block_on(poll_once(task)) else {
            continue;
        };

        if let Some(image) = image
            && let Some(target) = images.get_mut(image_handle)
        {
            *target = image;
        }
        completed.push(*image_id);
    }

    for image_id in completed {
        state.pending.remove(&image_id);
        state.processed.insert(image_id);
    }
}

fn standard_material_images(material: &StandardMaterial) -> impl Iterator<Item = &Handle<Image>> {
    [
        material.base_color_texture.as_ref(),
        material.emissive_texture.as_ref(),
        material.metallic_roughness_texture.as_ref(),
        material.normal_map_texture.as_ref(),
        material.occlusion_texture.as_ref(),
        material.depth_map.as_ref(),
    ]
    .into_iter()
    .flatten()
}

fn configure_mipmap_sampler(image: &mut Image, anisotropy: u16) {
    let mut descriptor = match image.sampler.clone() {
        ImageSampler::Default => ImageSamplerDescriptor::linear(),
        ImageSampler::Descriptor(descriptor) => descriptor,
    };
    descriptor.mag_filter = ImageFilterMode::Linear;
    descriptor.min_filter = ImageFilterMode::Linear;
    descriptor.mipmap_filter = ImageFilterMode::Linear;
    descriptor.anisotropy_clamp = anisotropy;
    image.sampler = ImageSampler::Descriptor(descriptor);
}
