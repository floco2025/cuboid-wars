use std::collections::{HashMap, VecDeque};

use bevy::{
    asset::{AssetHandleProvider, RenderAssetUsages},
    image::ImageLoaderSettings,
    prelude::*,
};

use super::mipmaps::{MaterialMipmapState, generate_material_mipmaps_system, queue_images};

const MAX_LOADING_TEXTURES: usize = 2;

pub fn material_textures_plugin(app: &mut App) {
    app.init_resource::<MaterialTextures>()
        .init_resource::<MaterialMipmapState>()
        .add_systems(
            Update,
            (load_material_textures_system, generate_material_mipmaps_system).chain(),
        );
}

#[derive(Resource)]
pub struct MaterialTextures {
    provider: AssetHandleProvider,
    handles: HashMap<String, Handle<Image>>,
    queued: VecDeque<TextureRequest>,
    loading: Vec<LoadingTexture>,
}

struct TextureRequest {
    path: String,
    settings: ImageLoaderSettings,
    target: Handle<Image>,
}

struct LoadingTexture {
    path: String,
    source: Option<Handle<Image>>,
    target: Handle<Image>,
}

impl FromWorld for MaterialTextures {
    fn from_world(world: &mut World) -> Self {
        Self {
            provider: world.resource::<Assets<Image>>().get_handle_provider(),
            handles: default(),
            queued: default(),
            loading: default(),
        }
    }
}

impl MaterialTextures {
    pub(crate) fn is_loading(&self) -> bool {
        !self.queued.is_empty() || !self.loading.is_empty()
    }

    pub(super) fn load(&mut self, path: &str, mut settings: ImageLoaderSettings) -> Handle<Image> {
        if let Some(handle) = self.handles.get(path) {
            return handle.clone();
        }
        let target = self.provider.reserve_handle().typed();
        // Only the finished target is uploaded. The decoded source stays on
        // the CPU so it can move into the mipmap pass without an interim GPU copy.
        settings.asset_usage = RenderAssetUsages::MAIN_WORLD;
        self.handles.insert(path.to_owned(), target.clone());
        self.queued.push_back(TextureRequest {
            path: path.to_owned(),
            settings,
            target: target.clone(),
        });
        target
    }
}

fn load_material_textures_system(
    mut textures: ResMut<MaterialTextures>,
    server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut mipmaps: ResMut<MaterialMipmapState>,
) {
    textures.loading.retain_mut(|loading| {
        if let Some(source) = &loading.source {
            if let Some(image) = images.remove(source.id()) {
                images
                    .insert(loading.target.id(), image)
                    .expect("queued texture handle lost its reserved slot");
                // A caller can discard a texture slot while customizing its material;
                // completion must not depend on a material event referencing this image.
                queue_images(
                    &mut mipmaps,
                    [("catalog texture", &loading.target)].into_iter(),
                    loading.path.clone(),
                );
                loading.source = None;
            } else if server.load_state(source.id()).is_failed() {
                return false;
            }
        }
        // Keep the slot until the mipmap pass finishes and the renderer takes
        // the pixels. Bounding only file reads would still accumulate decoded images.
        !images
            .get(&loading.target)
            .is_some_and(|image| image.asset_usage == RenderAssetUsages::RENDER_WORLD && image.data.is_none())
    });

    while textures.loading.len() < MAX_LOADING_TEXTURES {
        let Some(request) = textures.queued.pop_front() else {
            break;
        };
        let source = server
            .load_builder()
            .with_settings(move |settings: &mut ImageLoaderSettings| *settings = request.settings.clone())
            .load(request.path.clone());
        textures.loading.push(LoadingTexture {
            path: request.path,
            source: Some(source),
            target: request.target,
        });
    }
}
