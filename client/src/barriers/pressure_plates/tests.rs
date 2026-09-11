use bevy::{gltf::GltfMaterialName, prelude::*};
use common::protocol::{
    Barrier, BarrierId, BarrierKindId, BridgeId, BridgeKindId, CarrierId, HexColor, KindDef, LightBridge, MapLayout,
    PlateState, PressurePlate, SwitchDef, SwitchId,
};

use super::{animation::PlatePlayback, *};
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::AssetSet,
    test_assets::{headless_asset_app, settle},
    test_fixtures::map_settings,
};

fn part(world: &World, root: Entity, name: &str) -> Entity {
    let mut pending = vec![root];
    while let Some(entity) = pending.pop() {
        if world.get::<Name>(entity).is_some_and(|actual| actual.as_str() == name) {
            return entity;
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
    }
    panic!("model lacks {name}");
}

fn pose(world: &World, root: Entity, name: &str) -> Transform {
    *world
        .get::<Transform>(part(world, root, name))
        .expect("model node transform missing")
}

fn switch(id: &str, color: Option<HexColor>) -> SwitchDef {
    SwitchDef {
        id: id.to_owned(),
        plate_color: color,
        policy: Default::default(),
    }
}

#[test]
fn model_tracks_switches_colors_locks_and_layout_replacement() {
    let mut settings = map_settings();
    settings.switches = vec![
        switch("door", None),
        switch("bridge", None),
        switch("show", Some(HexColor([230, 140, 30]))),
    ];
    settings.barrier_kinds = vec![KindDef {
        id: "door".into(),
        color: HexColor([34, 204, 51]),
    }];
    settings.bridge_kinds = vec![KindDef {
        id: "bridge".into(),
        color: HexColor([48, 216, 255]),
    }];
    let layout = MapLayout {
        barriers: vec![Barrier {
            id: BarrierId(0),
            kind: BarrierKindId(0),
            switch: Some(SwitchId(0)),
            switch_inverted: false,
            x1: 0.0,
            z1: 0.0,
            x2: 2.0,
            z2: 0.0,
            width: 0.1,
            y: 0.0,
            height: 2.0,
            level: 0,
            levels: 1,
            carrier: CarrierId::WORLD,
        }],
        light_bridges: vec![LightBridge {
            id: BridgeId(0),
            kind: BridgeKindId(0),
            switch: Some(SwitchId(1)),
            switch_inverted: false,
            x1: 0.0,
            z1: 0.0,
            x2: 2.0,
            z2: 2.0,
            y: 0.0,
            thickness: 0.1,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        pressure_plates: (0..4)
            .map(|index| PressurePlate {
                level: 0,
                center_x: index as f32 * 4.0,
                center_y: 2.0,
                center_z: 1.0,
                switch: SwitchId((index % 3) as u16),
                carrier: CarrierId::WORLD,
            })
            .collect(),
        ..default()
    };
    let mut app = headless_asset_app(|app| {
        let carrier = app.world_mut().spawn(Transform::from_xyz(5.0, 0.0, 0.0)).id();
        app.insert_resource(AssetSet::load_default().expect("asset configuration invalid"))
            .insert_resource(settings)
            .insert_resource(CarrierEntities::new(vec![carrier]))
            .insert_resource(CarrierStoreys::from_layout(&layout))
            .insert_resource(layout)
            .insert_resource(PlateState {
                active_switches: vec![SwitchId(0)],
                ..default()
            })
            .insert_resource(LockedSwitches(vec![SwitchId(2)]))
            .init_resource::<PressurePlateModel>()
            .add_systems(
                Update,
                (
                    pressure_plates_spawn_system,
                    pressure_plates_attach_system,
                    pressure_plates_visibility_system,
                    pressure_plates_animation_system,
                )
                    .chain(),
            );
    });
    settle(&mut app, |world| {
        world.query::<&PlatePlayback>().iter(world).count() == 4
    });
    let mut roots: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<PressurePlateMarker>>()
        .iter(app.world())
        .collect();
    roots.sort_by(|a, b| {
        app.world()
            .get::<Transform>(*a)
            .expect("plate transform missing")
            .translation
            .x
            .total_cmp(
                &app.world()
                    .get::<Transform>(*b)
                    .expect("plate transform missing")
                    .translation
                    .x,
            )
    });
    let colors = [[34, 204, 51], [48, 216, 255], [230, 140, 30], [34, 204, 51]];
    for (root, color) in roots.iter().zip(colors) {
        let mut pending = vec![*root];
        let mut accents = 0;
        let mut lights = 0;
        while let Some(entity) = pending.pop() {
            if let Some(name) = app.world().get::<GltfMaterialName>(entity) {
                let handle = app
                    .world()
                    .get::<MeshMaterial3d<StandardMaterial>>(entity)
                    .expect("plate material handle missing");
                let material = app
                    .world()
                    .resource::<Assets<StandardMaterial>>()
                    .get(&handle.0)
                    .expect("plate material asset missing");
                if name.0 == "PressurePlateAccent" {
                    assert_eq!(material.base_color, Color::srgb_u8(color[0], color[1], color[2]));
                    accents += 1;
                } else if name.0 == "PressurePlateLight" {
                    assert!(material.emissive.blue > 2.9);
                    lights += 1;
                }
            }
            if let Some(children) = app.world().get::<Children>(entity) {
                pending.extend(children.iter());
            }
        }
        assert!(accents > 0 && lights > 0, "tint or emissive material binding missing");
        let transform = app.world().get::<Transform>(*root).expect("plate transform missing");
        assert_eq!(transform.scale, Vec3::new(1.7, 1.0, 1.7));
        let global = app
            .world()
            .get::<GlobalTransform>(*root)
            .expect("plate world transform missing")
            .translation();
        assert!((global.x - transform.translation.x - 5.0).abs() < 0.0001);
    }
    assert_eq!(app.world().resource::<PressurePlateModel>().materials.len(), 4);
    assert_eq!(
        *app.world()
            .get::<Visibility>(roots[2])
            .expect("plate visibility missing"),
        Visibility::Hidden
    );
    let initial = pose(app.world(), roots[1], "PressurePlatePanel");
    let pressed = pose(app.world(), roots[0], "PressurePlatePanel");
    assert!((initial.translation.y - pressed.translation.y - 0.03).abs() < 0.0001);
    let shutter = pose(app.world(), roots[0], "PressurePlateShutters");
    assert!(
        (pose(app.world(), roots[1], "PressurePlateShutters").translation.y - shutter.translation.y - 0.014).abs()
            < 0.0001
    );
    for side in 1..=4 {
        let name = format!("PressurePlateStatus{side}");
        let angle = pose(app.world(), roots[0], &name)
            .rotation
            .angle_between(pose(app.world(), roots[1], &name).rotation);
        assert!((angle - std::f32::consts::PI).abs() < 0.001);
    }
    app.world_mut().resource_mut::<PlateState>().active_switches.clear();
    for _ in 0..3 {
        app.update();
    }
    let middle = pose(app.world(), roots[0], "PressurePlatePanel").translation.y;
    assert!(middle > pressed.translation.y && middle < initial.translation.y);
    app.world_mut().resource_mut::<PlateState>().active_switches = vec![SwitchId(0)];
    app.update();
    let reversed = pose(app.world(), roots[0], "PressurePlatePanel").translation.y;
    assert!(reversed < middle && reversed > pressed.translation.y);
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(pose(app.world(), roots[0], "PressurePlatePanel"), pressed);
    assert_eq!(pose(app.world(), roots[3], "PressurePlatePanel"), pressed);
    assert_eq!(pose(app.world(), roots[1], "PressurePlatePanel"), initial);
    app.world_mut().resource_mut::<PlateState>().active_switches.clear();
    app.world_mut().resource_mut::<LockedSwitches>().0.clear();
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(pose(app.world(), roots[0], "PressurePlatePanel"), initial);
    assert_eq!(
        *app.world()
            .get::<Visibility>(roots[2])
            .expect("plate visibility missing"),
        Visibility::Visible
    );
    app.world_mut().resource_mut::<MapLayout>().pressure_plates.truncate(1);
    app.update();
    settle(&mut app, |world| {
        world.query::<&PlatePlayback>().iter(world).count() == 1
    });
    assert!(roots.iter().all(|root| app.world().get_entity(*root).is_err()));
    assert_eq!(
        app.world_mut()
            .query::<&PressurePlateMarker>()
            .iter(app.world())
            .count(),
        1
    );
}
