use bevy::prelude::*;

use crate::{
    config::{AssetSet, ClientSettings},
    constants::{GRASS_WIND_DIRECTION_DEGREES, GRASS_WIND_SPEED, GRASS_WIND_STRENGTH},
    materials::{GrassMaterial, GrassWindExtension, MaterialTextures, TerrainMaterial, terrain_material},
};

pub(super) fn grass_material() -> GrassMaterial {
    let wind_direction = Vec2::from_angle(GRASS_WIND_DIRECTION_DEGREES.to_radians());
    GrassMaterial {
        base: StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.95,
            reflectance: 0.1,
            // Both faces draw with the one upward normal: a flipped back-face
            // normal would point down and render half the blades black.
            cull_mode: None,
            ..default()
        },
        extension: GrassWindExtension {
            wind: Vec4::new(
                wind_direction.x,
                wind_direction.y,
                GRASS_WIND_STRENGTH,
                GRASS_WIND_SPEED,
            ),
        },
    }
}

#[derive(Resource)]
pub struct GrassMaterials {
    pub(crate) grass: Handle<GrassMaterial>,
    pub(crate) terrain: Handle<TerrainMaterial>,
}

pub fn setup_grass_materials_system(
    mut commands: Commands,
    settings: Res<ClientSettings>,
    asset_set: Res<AssetSet>,
    mut textures: ResMut<MaterialTextures>,
    mut grass: ResMut<Assets<GrassMaterial>>,
    mut terrain: ResMut<Assets<TerrainMaterial>>,
) {
    commands.insert_resource(GrassMaterials {
        grass: grass.add(grass_material()),
        terrain: terrain.add(terrain_material(
            &mut textures,
            asset_set.terrain_material_def(),
            settings.rendering.texture_anisotropy,
            settings.rendering.mipmaps,
            settings.grass.base_color(),
        )),
    });
}
