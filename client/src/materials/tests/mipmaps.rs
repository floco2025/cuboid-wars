use super::*;
use bevy::{app::TaskPoolPlugin, asset::AssetPlugin, ecs::message::Messages};

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

// Replacing a texture must re-prepare every material bound to it, which
// only happens if the nudge really queues `Modified`.
#[test]
fn image_replacement_marks_dependent_materials_modified() {
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<StandardMaterial>();
    let mut images = Assets::<Image>::default();
    let image = images.add(Image::default());
    let (dependent, _unrelated) = {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let dependent = materials.add(StandardMaterial {
            base_color_texture: Some(image.clone()),
            ..default()
        });
        (dependent, materials.add(StandardMaterial::default()))
    };
    app.update();
    app.world_mut()
        .resource_mut::<Messages<AssetEvent<StandardMaterial>>>()
        .clear();

    mark_materials_using_images_changed(
        &mut app.world_mut().resource_mut::<Assets<StandardMaterial>>(),
        &HashSet::from([image.id()]),
    );
    app.update();

    let modified: Vec<_> = app
        .world()
        .resource::<Messages<AssetEvent<StandardMaterial>>>()
        .iter_current_update_messages()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(modified, vec![dependent.id()]);
}
