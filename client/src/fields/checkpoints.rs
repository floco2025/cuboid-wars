use bevy::{asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology};
use common::protocol::{CheckpointKind, MapLayout};

use super::{FieldMeshes, surface_frame_rects};
use crate::{
    carriers::{CarrierEntities, CarrierStoreys},
    constants::{
        CHECKPOINT_CLAIMED_EMISSIVE, CHECKPOINT_COLOR, CHECKPOINT_FLAG_WIND_SPEED, CHECKPOINT_FLAG_WIND_STRENGTH,
        CHECKPOINT_GROUP_PENNANT_SCALE, CHECKPOINT_OUTLINE_HEIGHT, CHECKPOINT_OUTLINE_WIDTH, CHECKPOINT_PENNANT_GAP,
        CHECKPOINT_PENNANT_HEIGHT, CHECKPOINT_PENNANT_HEM, CHECKPOINT_PENNANT_HOIST, CHECKPOINT_PENNANT_LENGTH,
        CHECKPOINT_PENNANT_SEGMENTS, CHECKPOINT_PENNANT_WEAVE, CHECKPOINT_POLE_COLOR, CHECKPOINT_POLE_HEIGHT,
        CHECKPOINT_POLE_RADIUS, CHECKPOINT_UNCLAIMED_COLOR, GRASS_WIND_DIRECTION_DEGREES,
    },
    materials::{FlagMaterial, FlagWindExtension},
    players::{MyPlayerId, PlayerMap},
};

#[derive(Component)]
pub struct CheckpointMarker;

// The group's claimed checkpoint from the snapshot, an index into `MapLayout.checkpoints`.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct SharedCheckpoint(pub Option<u16>);

// One pennant on a checkpoint's flag: the upper one shows the player's own
// claim, the lower one on group kinds the group's.
#[derive(Component)]
pub(crate) struct CheckpointPennant {
    index: u16,
    group: bool,
}

#[derive(Resource)]
pub(crate) struct CheckpointAssets {
    outline: Handle<StandardMaterial>,
    pole: Handle<StandardMaterial>,
    unclaimed: Handle<FlagMaterial>,
    claimed: Handle<FlagMaterial>,
    pole_mesh: Handle<Mesh>,
    pennant_mesh: Handle<Mesh>,
    group_pennant_mesh: Handle<Mesh>,
}

impl FromWorld for CheckpointAssets {
    fn from_world(world: &mut World) -> Self {
        let wind_direction = Vec2::from_angle(GRASS_WIND_DIRECTION_DEGREES.to_radians());
        let wind = FlagWindExtension {
            wind: Vec4::new(
                wind_direction.x,
                wind_direction.y,
                CHECKPOINT_FLAG_WIND_STRENGTH,
                CHECKPOINT_FLAG_WIND_SPEED,
            ),
            cloth: Vec4::new(
                CHECKPOINT_PENNANT_LENGTH,
                CHECKPOINT_PENNANT_HEM,
                CHECKPOINT_PENNANT_HOIST,
                CHECKPOINT_PENNANT_WEAVE,
            ),
        };
        let cloth = |color: Color, emissive: f32| {
            let linear = color.to_linear();
            FlagMaterial {
                base: StandardMaterial {
                    base_color: color,
                    emissive: LinearRgba::rgb(linear.red * emissive, linear.green * emissive, linear.blue * emissive),
                    perceptual_roughness: 0.9,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                },
                extension: wind.clone(),
            }
        };
        let (outline, pole) = {
            let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
            (
                materials.add(StandardMaterial {
                    base_color: CHECKPOINT_COLOR,
                    unlit: true,
                    ..default()
                }),
                materials.add(StandardMaterial {
                    base_color: CHECKPOINT_POLE_COLOR,
                    metallic: 0.8,
                    perceptual_roughness: 0.35,
                    ..default()
                }),
            )
        };
        let (unclaimed, claimed) = {
            let mut materials = world.resource_mut::<Assets<FlagMaterial>>();
            (
                materials.add(cloth(CHECKPOINT_UNCLAIMED_COLOR, 0.0)),
                materials.add(cloth(CHECKPOINT_COLOR, CHECKPOINT_CLAIMED_EMISSIVE)),
            )
        };
        let mut meshes = world.resource_mut::<Assets<Mesh>>();
        Self {
            outline,
            pole,
            unclaimed,
            claimed,
            pole_mesh: meshes.add(Cylinder::new(CHECKPOINT_POLE_RADIUS, CHECKPOINT_POLE_HEIGHT)),
            pennant_mesh: meshes.add(pennant_mesh(
                CHECKPOINT_PENNANT_LENGTH,
                CHECKPOINT_PENNANT_HEIGHT,
                CHECKPOINT_PENNANT_SEGMENTS,
            )),
            group_pennant_mesh: meshes.add(pennant_mesh(
                CHECKPOINT_PENNANT_LENGTH * CHECKPOINT_GROUP_PENNANT_SCALE,
                CHECKPOINT_PENNANT_HEIGHT * CHECKPOINT_GROUP_PENNANT_SCALE,
                CHECKPOINT_PENNANT_SEGMENTS,
            )),
        }
    }
}

// A pennant hanging from its hoist at the origin: the hoist edge runs down
// from y = 0 along the pole and the cloth tapers along +X to a tip at half
// height, in `segments` columns so the wind shader can ripple it. `UV_0` is
// each vertex's fraction of the way along and down the cloth, for its hem
// and hoist band; `UV_1` is its position in metres, for the weave.
pub(crate) fn pennant_mesh(length: f32, height: f32, segments: usize) -> Mesh {
    let columns = segments + 1;
    let mut positions = Vec::with_capacity(2 * columns);
    let mut uvs = Vec::with_capacity(2 * columns);
    let mut metres = Vec::with_capacity(2 * columns);
    for column in 0..columns {
        let reach = column as f32 / segments as f32;
        let x = length * reach;
        let (top, bottom) = (-height * 0.5 * reach, -height + height * 0.5 * reach);
        positions.push([x, top, 0.0]);
        positions.push([x, bottom, 0.0]);
        uvs.push([reach, 0.0]);
        uvs.push([reach, 1.0]);
        metres.push([x, top]);
        metres.push([x, bottom]);
    }
    let mut indices = Vec::with_capacity(6 * segments);
    for column in 0..segments as u32 {
        let (top, bottom) = (2 * column, 2 * column + 1);
        indices.extend([top, top + 2, bottom + 2, top, bottom + 2, bottom]);
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 2 * columns]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, metres);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

// Whether a pennant shows its claim: the player's own saved checkpoint, or
// the group's claimed one.
pub(crate) fn pennant_claimed(pennant: &CheckpointPennant, mine: Option<u16>, shared: Option<u16>) -> bool {
    let claim = if pennant.group { shared } else { mine };
    claim == Some(pennant.index)
}

pub(crate) fn checkpoints_spawn_system(
    mut commands: Commands,
    layout: Res<MapLayout>,
    carriers: Res<CarrierEntities>,
    storeys: Res<CarrierStoreys>,
    field_meshes: Res<FieldMeshes>,
    assets: Res<CheckpointAssets>,
    existing: Query<Entity, With<CheckpointMarker>>,
) {
    if !layout.is_changed() {
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    for (index, checkpoint) in layout.checkpoints.iter().enumerate() {
        let index = u16::try_from(index).expect("checkpoint index exceeds u16");
        let footprint = Rect::new(checkpoint.min_x, checkpoint.min_z, checkpoint.max_x, checkpoint.max_z);
        let center = footprint.center();
        commands
            .spawn((
                CheckpointMarker,
                storeys.tag(checkpoint.carrier, checkpoint.level, 0),
                ChildOf(carriers.get(checkpoint.carrier)),
                Transform::from_xyz(center.x, checkpoint.y, center.y),
                Visibility::Inherited,
            ))
            .with_children(|parent| {
                for rect in surface_frame_rects(&[footprint], CHECKPOINT_OUTLINE_WIDTH) {
                    let offset = rect.center() - center;
                    parent.spawn((
                        Mesh3d(field_meshes.frame.clone()),
                        MeshMaterial3d(assets.outline.clone()),
                        Transform::from_xyz(offset.x, CHECKPOINT_OUTLINE_HEIGHT / 2.0, offset.y).with_scale(Vec3::new(
                            rect.width(),
                            CHECKPOINT_OUTLINE_HEIGHT,
                            rect.height(),
                        )),
                    ));
                }
                parent.spawn((
                    Mesh3d(assets.pole_mesh.clone()),
                    MeshMaterial3d(assets.pole.clone()),
                    Transform::from_xyz(0.0, CHECKPOINT_POLE_HEIGHT / 2.0, 0.0),
                ));
                parent.spawn((
                    CheckpointPennant { index, group: false },
                    Mesh3d(assets.pennant_mesh.clone()),
                    MeshMaterial3d(assets.unclaimed.clone()),
                    Transform::from_xyz(0.0, CHECKPOINT_POLE_HEIGHT, 0.0),
                ));
                if checkpoint.kind != CheckpointKind::Individual {
                    parent.spawn((
                        CheckpointPennant { index, group: true },
                        Mesh3d(assets.group_pennant_mesh.clone()),
                        MeshMaterial3d(assets.unclaimed.clone()),
                        Transform::from_xyz(
                            0.0,
                            CHECKPOINT_POLE_HEIGHT - CHECKPOINT_PENNANT_HEIGHT - CHECKPOINT_PENNANT_GAP,
                            0.0,
                        ),
                    ));
                }
            });
    }
}

// Recolours each pennant as the player's saved checkpoint and the group's claim change.
pub(crate) fn checkpoint_pennants_system(
    my_player_id: Res<MyPlayerId>,
    players: Res<PlayerMap>,
    shared: Res<SharedCheckpoint>,
    assets: Res<CheckpointAssets>,
    mut pennants: Query<(&CheckpointPennant, &mut MeshMaterial3d<FlagMaterial>)>,
) {
    let mine = players.get(&my_player_id.0).and_then(|info| info.checkpoint);
    for (pennant, mut material) in &mut pennants {
        let wanted = if pennant_claimed(pennant, mine, shared.0) {
            &assets.claimed
        } else {
            &assets.unclaimed
        };
        // A material write re-extracts the pennant; an equal one would do that every frame.
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

#[cfg(test)]
#[path = "tests/checkpoints.rs"]
mod tests;
