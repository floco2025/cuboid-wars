use crate::{
    map::grass::setup_grass_materials_system,
    materials::{GrassMaterial, TerrainMaterial},
    test_fixtures,
};
use bevy::{app::TaskPoolPlugin, asset::AssetPlugin, image::ImagePlugin, prelude::*};

pub fn app() -> App {
    let mut app = App::new();
    let mut assets: serde_json::Value = serde_json::from_str(test_fixtures::ASSETS_JSON).expect("fixture catalog");
    let material = assets["aliases"]
        .as_object()
        .expect("fixture aliases")
        .values()
        .next()
        .expect("fixture alias")
        .clone();
    assets["aliases"]["terrain"] = material;
    app.add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin::default(),
        ImagePlugin::default(),
    ))
    .init_asset::<GrassMaterial>()
    .init_asset::<TerrainMaterial>()
    .insert_resource(test_fixtures::client_settings())
    .insert_resource(serde_json::from_value::<crate::config::AssetSet>(assets).expect("fixture asset set"))
    .add_systems(Update, setup_grass_materials_system);
    app
}
