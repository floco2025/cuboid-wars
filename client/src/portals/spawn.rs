use bevy::{light::NotShadowCaster, prelude::*};

use crate::constants::{PORTAL_A_COLOR, PORTAL_B_COLOR, PORTAL_EMISSIVE, PORTAL_SURFACE_OFFSET};
use common::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH},
    physics::PortalFrame,
    protocol::{Portal, PortalEnd},
};

#[derive(Component)]
pub struct PortalMarker;

// One shared unit-disc mesh with per-end emissive materials, so every portal
// instance batches. Opaque on purpose: translucent Blend materials render
// pale gray in this app, and the oval reads fine as a solid glowing decal.
#[derive(Resource)]
pub struct PortalAssets {
    mesh: Handle<Mesh>,
    material_a: Handle<StandardMaterial>,
    material_b: Handle<StandardMaterial>,
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

// Flat glowing oval on the surface, oriented by the shared aperture frame —
// the visual covers exactly the trigger area the physics uses.
pub fn spawn_portal(commands: &mut Commands, assets: &PortalAssets, portal: &Portal) -> Entity {
    let frame = PortalFrame::from_portal(portal);
    let material = match portal.end {
        PortalEnd::A => assets.material_a.clone(),
        PortalEnd::B => assets.material_b.clone(),
    };
    commands
        .spawn((
            PortalMarker,
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(material),
            Transform {
                translation: frame.center + frame.normal * PORTAL_SURFACE_OFFSET,
                rotation: Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal)),
                scale: Vec3::new(PORTAL_HALF_WIDTH * 2.0, PORTAL_HALF_HEIGHT * 2.0, 1.0),
            },
            NotShadowCaster,
        ))
        .id()
}
