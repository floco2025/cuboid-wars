use bevy::prelude::*;
use common::protocol::{HexColor, MapLayout, MapSettings, SwitchId};

use super::visibility::{LockedSwitches, plate_visibility};
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    config::AssetSet,
};

#[derive(Component)]
pub struct PressurePlateMarker;

#[derive(Component)]
pub struct PlateSwitchMarker(pub SwitchId);

#[derive(Component)]
pub(super) struct PlateColor(pub HexColor);

const PLATE_SIDE_CELLS: f32 = 0.5;
const PLATE_Y_OFFSET: f32 = 0.01;

pub fn pressure_plates_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    settings: Res<MapSettings>,
    assets: Res<AssetSet>,
    existing: Query<Entity, With<PressurePlateMarker>>,
    locked: Res<LockedSwitches>,
    carriers: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let side = settings.geometry.grid_cell_size * PLATE_SIDE_CELLS;
    for plate in &layout.pressure_plates {
        let color = settings
            .pressure_plate_color(plate.switch, &layout)
            .unwrap_or(assets.pressure_plate().default_color);
        commands.spawn((
            PressurePlateMarker,
            PlateSwitchMarker(plate.switch),
            PlateColor(color),
            storeys.tag(plate.carrier, plate.level, 0),
            ChildOf(carriers.get(plate.carrier)),
            Transform::from_xyz(plate.center_x, plate.center_y + PLATE_Y_OFFSET, plate.center_z)
                .with_scale(Vec3::new(side, 1.0, side)),
            plate_visibility(plate.switch, &locked.0),
        ));
    }
}
