use crate::{
    map::grass::setup_grass_materials_system,
    materials::{GrassMaterial, TerrainMaterial},
    test_fixtures,
};
use bevy::{app::TaskPoolPlugin, asset::AssetPlugin, image::ImagePlugin, prelude::*};

pub fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin::default(),
        ImagePlugin::default(),
    ))
    .init_asset::<GrassMaterial>()
    .init_asset::<TerrainMaterial>()
    .insert_resource(test_fixtures::client_settings())
    .insert_resource(test_fixtures::asset_set())
    .add_systems(Update, setup_grass_materials_system);
    app
}
