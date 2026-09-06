use bevy::{light::NotShadowCaster, prelude::*};

use bevy::camera::visibility::RenderLayers;

use crate::constants::{
    MAIN_VIEW_RENDER_LAYER, PORTAL_A_COLOR, PORTAL_B_COLOR, PORTAL_EMISSIVE, PORTAL_RIM_OFFSET, PORTAL_SURFACE_OFFSET,
};
use common::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_RIM_SCALE},
    map::MovingFloors,
    physics::PortalFrame,
    protocol::{Portal, PortalEnd, PortalPairId},
};

// Every rendered disc of one portal end: the main surface and each replica
// on a portal camera's layer. Names the end so an anchored portal's discs
// can follow its tile.
#[derive(Component)]
pub(super) struct PortalSurface {
    pub(super) pair: PortalPairId,
    pub(super) end: PortalEnd,
}

// One shared unit-disc mesh with per-end emissive fallback/rim materials.
#[derive(Resource)]
pub struct PortalAssets {
    mesh: Handle<Mesh>,
    material_a: Handle<StandardMaterial>,
    material_b: Handle<StandardMaterial>,
}

impl PortalAssets {
    pub(crate) fn material(&self, end: PortalEnd) -> Handle<StandardMaterial> {
        match end {
            PortalEnd::A => self.material_a.clone(),
            PortalEnd::B => self.material_b.clone(),
        }
    }
}

impl FromWorld for PortalAssets {
    fn from_world(world: &mut World) -> Self {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(Circle::new(0.5));
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        Self {
            mesh,
            material_a: materials.add(portal_material(PORTAL_A_COLOR)),
            material_b: materials.add(portal_material(PORTAL_B_COLOR)),
        }
    }
}

fn portal_material(color: Color) -> StandardMaterial {
    let linear = color.to_linear();
    StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(
            linear.red * PORTAL_EMISSIVE,
            linear.green * PORTAL_EMISSIVE,
            linear.blue * PORTAL_EMISSIVE,
        ),
        ..default()
    }
}

pub fn spawn_portal(commands: &mut Commands, assets: &PortalAssets, portal: &Portal, floors: &MovingFloors) -> Entity {
    spawn_portal_visual(commands, assets, portal, floors, MAIN_VIEW_RENDER_LAYER)
}

pub(super) fn spawn_portal_visual(
    commands: &mut Commands,
    assets: &PortalAssets,
    portal: &Portal,
    floors: &MovingFloors,
    render_layer: usize,
) -> Entity {
    let frame = PortalFrame::from_portal(portal, floors);
    let material = assets.material(portal.end);
    let render_layer = RenderLayers::layer(render_layer);
    commands
        .spawn((
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform {
                translation: frame.center + frame.normal * PORTAL_SURFACE_OFFSET,
                rotation: Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal)),
                scale: Vec3::new(PORTAL_HALF_WIDTH * 2.0, PORTAL_HALF_HEIGHT * 2.0, 1.0),
            },
            NotShadowCaster,
            render_layer.clone(),
            PortalSurface {
                pair: portal.pair,
                end: portal.end,
            },
        ))
        .with_children(|children| {
            children.spawn((
                Mesh3d(assets.mesh.clone()),
                MeshMaterial3d(material),
                Transform::from_translation(Vec3::NEG_Z * PORTAL_RIM_OFFSET).with_scale(Vec3::splat(PORTAL_RIM_SCALE)),
                NotShadowCaster,
                render_layer,
            ));
        })
        .id()
}
