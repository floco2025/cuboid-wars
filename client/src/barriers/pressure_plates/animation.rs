use bevy::{gltf::GltfMaterialName, prelude::*, world_serialization::WorldInstanceReady};
use common::protocol::{PlateState, SwitchId};

use super::{PlateSwitchMarker, model::PressurePlateModel, spawn::PlateColor};
use crate::config::AssetSet;

#[derive(Component)]
pub(crate) struct PlatePlayback {
    switch: SwitchId,
    index: AnimationNodeIndex,
    duration: f32,
}

pub(super) fn pressure_plate_ready(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    roots: Query<(&PlateSwitchMarker, &PlateColor)>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    mut meshes: Query<(&GltfMaterialName, &mut MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut model: ResMut<PressurePlateModel>,
    assets: Res<AssetSet>,
    state: Res<PlateState>,
) {
    let Ok((switch, color)) = roots.get(ready.entity) else {
        return;
    };
    let Some(animation) = model.animation.clone() else {
        return;
    };
    for child in children.iter_descendants(ready.entity) {
        if let Ok(mut player) = players.get_mut(child) {
            let time = if state.is_active(switch.0) {
                animation.duration
            } else {
                0.0
            };
            player.play(animation.index).pause().set_seek_time(time);
            commands.entity(child).insert((
                AnimationGraphHandle(animation.graph.clone()),
                PlatePlayback {
                    switch: switch.0,
                    index: animation.index,
                    duration: animation.duration,
                },
            ));
        }
        let Ok((name, mut handle)) = meshes.get_mut(child) else {
            continue;
        };
        let tint = match name.0.as_str() {
            "PressurePlateAccent" => color.0.0,
            "PressurePlateLight" => [0; 3],
            _ => continue,
        };
        let key = (handle.0.id(), tint);
        let replacement = if let Some(material) = model.materials.get(&key) {
            material.clone()
        } else {
            let Some(mut material) = materials.get(&handle.0).cloned() else {
                continue;
            };
            if name.0 == "PressurePlateAccent" {
                material.base_color = Color::srgb_u8(tint[0], tint[1], tint[2]);
            } else {
                let def = assets.pressure_plate();
                let [r, g, b] = def.light_color;
                material.base_color = Color::srgb(r, g, b);
                material.emissive = material.base_color.to_linear() * def.emissive_luminance;
            }
            let replacement = materials.add(material);
            model.materials.insert(key, replacement.clone());
            replacement
        };
        handle.0 = replacement;
    }
}

pub(crate) fn pressure_plates_animation_system(
    time: Res<Time>,
    state: Res<PlateState>,
    mut players: Query<(&PlatePlayback, &mut AnimationPlayer)>,
) {
    for (plate, mut player) in &mut players {
        let Some(animation) = player.animation(plate.index) else {
            continue;
        };
        let current = animation.seek_time();
        let direction = if state.is_active(plate.switch) { 1.0 } else { -1.0 };
        let next = (current + direction * time.delta_secs()).clamp(0.0, plate.duration);
        if next != current {
            player
                .animation_mut(plate.index)
                .expect("plate animation missing from its player")
                .set_seek_time(next);
        }
    }
}
