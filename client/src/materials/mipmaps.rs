use std::collections::{HashMap, HashSet};

use bevy::{
    asset::AssetId,
    image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::TextureUsages,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use bevy_mod_mipmap_generator::{MipmapGeneratorSettings, check_image_compatible, generate_mips_texture};

use crate::config::ClientSettings;

const MAX_PENDING_MIPMAP_TASKS: usize = 2;

#[derive(Default)]
pub struct MaterialMipmapState {
    processed: HashSet<AssetId<Image>>,
    pending: HashMap<AssetId<Image>, (Handle<Image>, Task<Option<Image>>)>,
}

pub fn generate_material_mipmaps_system(
    mut state: Local<MaterialMipmapState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
    client_settings: Res<ClientSettings>,
) {
    // The upstream plugin's event-driven system can miss our textures because a
    // StandardMaterial may be added before its Image assets have loaded. Scanning
    // materials and retrying unloaded images makes mip generation robust for our
    // asset-server-loaded material set.
    let updated_images = finish_mipmap_tasks(&mut state, &mut images);
    mark_materials_using_images_changed(&mut materials, &updated_images);

    let settings = MipmapGeneratorSettings {
        anisotropic_filtering: client_settings.rendering.texture_anisotropy,
        ..default()
    };

    let thread_pool = AsyncComputeTaskPool::get();
    for (material_id, material) in materials.iter() {
        let material_label = asset_server
            .get_path(material_id)
            .map_or_else(|| format!("{material_id:?}"), |path| path.to_string());
        for (texture_slot, image_handle) in standard_material_images(material) {
            if state.pending.len() >= MAX_PENDING_MIPMAP_TASKS {
                return;
            }

            let image_id = image_handle.id();
            if state.processed.contains(&image_id) || state.pending.contains_key(&image_id) {
                continue;
            }

            let Some(mut image) = images.get_mut(image_handle) else {
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

            configure_mipmap_sampler(&mut image, client_settings.rendering.texture_anisotropy);

            if image.texture_descriptor.mip_level_count > 1 {
                state.processed.insert(image_id);
                continue;
            }

            if let Err(error) = check_image_compatible(&image) {
                debug!(
                    "skipping mipmap generation for {} ({texture_slot} of material {material_label}, format {:?}, size {}x{}x{}): {error}",
                    image_label(image_handle),
                    image.texture_descriptor.format,
                    image.texture_descriptor.size.width,
                    image.texture_descriptor.size.height,
                    image.texture_descriptor.size.depth_or_array_layers,
                );
                state.processed.insert(image_id);
                continue;
            }

            let mut image = image.clone();
            let settings = settings.clone();
            let image_label = image_label(image_handle);
            let material_label = material_label.clone();
            let format = image.texture_descriptor.format;
            let size = image.texture_descriptor.size;
            let task = thread_pool.spawn(async move {
                let mut added_cache_size = 0;
                generate_mips_texture(&mut image, &settings, &mut added_cache_size)
                    .map(|()| image)
                    .map_err(|error| {
                        warn!(
                            "failed to generate mipmaps for {image_label} ({texture_slot} of material {material_label}, format {format:?}, size {}x{}x{}): {error}",
                            size.width,
                            size.height,
                            size.depth_or_array_layers,
                        );
                    })
                    .ok()
            });
            state.pending.insert(image_id, (image_handle.clone(), task));
        }
    }
}

fn finish_mipmap_tasks(state: &mut MaterialMipmapState, images: &mut Assets<Image>) -> HashSet<AssetId<Image>> {
    let mut completed = Vec::new();
    let mut updated = HashSet::new();
    for (image_id, (image_handle, task)) in &mut state.pending {
        let Some(image) = block_on(poll_once(task)) else {
            continue;
        };

        if let Some(image) = image
            && let Some(mut target) = images.get_mut(image_handle)
        {
            *target = image;
            updated.insert(*image_id);
        }
        completed.push(*image_id);
    }

    for image_id in completed {
        state.pending.remove(&image_id);
        state.processed.insert(image_id);
    }

    updated
}

fn mark_materials_using_images_changed(
    materials: &mut Assets<StandardMaterial>,
    updated_images: &HashSet<AssetId<Image>>,
) {
    if updated_images.is_empty() {
        return;
    }

    let affected_materials = materials
        .iter()
        .filter_map(|(material_id, material)| material_uses_any_image(material, updated_images).then_some(material_id))
        .collect::<Vec<_>>();

    for material_id in affected_materials {
        // Rebuild the bind group so it sees the replacement GPU image and sampler.
        let _ = materials.get_mut(material_id).as_deref_mut();
    }
}

fn material_uses_any_image(material: &StandardMaterial, image_ids: &HashSet<AssetId<Image>>) -> bool {
    standard_material_images(material).any(|(_, image)| image_ids.contains(&image.id()))
}

fn standard_material_images(material: &StandardMaterial) -> impl Iterator<Item = (&'static str, &Handle<Image>)> {
    [
        ("base color texture", material.base_color_texture.as_ref()),
        ("emissive texture", material.emissive_texture.as_ref()),
        (
            "metallic/roughness texture",
            material.metallic_roughness_texture.as_ref(),
        ),
        ("normal-map texture", material.normal_map_texture.as_ref()),
        ("occlusion texture", material.occlusion_texture.as_ref()),
        ("depth-map texture", material.depth_map.as_ref()),
    ]
    .into_iter()
    .filter_map(|(slot, handle)| handle.map(|handle| (slot, handle)))
}

fn image_label(handle: &Handle<Image>) -> String {
    handle
        .path()
        .map_or_else(|| format!("image {:?}", handle.id()), ToString::to_string)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_image_match_checks_every_texture_slot() {
        let mut images = Assets::<Image>::default();
        let handles = (0..6).map(|_| images.add(Image::default())).collect::<Vec<_>>();
        let material = StandardMaterial {
            base_color_texture: Some(handles[0].clone()),
            emissive_texture: Some(handles[1].clone()),
            metallic_roughness_texture: Some(handles[2].clone()),
            normal_map_texture: Some(handles[3].clone()),
            occlusion_texture: Some(handles[4].clone()),
            depth_map: Some(handles[5].clone()),
            ..default()
        };

        for handle in handles {
            assert!(material_uses_any_image(&material, &HashSet::from([handle.id()])));
        }
        let unrelated = images.add(Image::default());
        assert!(!material_uses_any_image(&material, &HashSet::from([unrelated.id()])));
    }

    #[test]
    fn material_images_report_their_texture_slots() {
        let mut images = Assets::<Image>::default();
        let handle = images.add(Image::default());
        let material = StandardMaterial {
            base_color_texture: Some(handle.clone()),
            normal_map_texture: Some(handle),
            ..default()
        };

        let slots = standard_material_images(&material)
            .map(|(slot, _)| slot)
            .collect::<Vec<_>>();

        assert_eq!(slots, ["base color texture", "normal-map texture"]);
    }
}
