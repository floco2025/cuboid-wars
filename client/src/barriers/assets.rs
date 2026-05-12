use bevy::prelude::*;

use crate::constants::*;
use common::{constants::BARRIER_THICKNESS, protocol::BarrierColor};

// One mesh + one material per `BarrierColor`, built at startup and cloned
// into every spawned barrier so Bevy's automatic batching collapses many
// barriers into a small number of draw calls (mirrors `ItemAssets`).
// Animation is driven by mutating the shared materials each frame; all
// barriers of the same color pulse together.
#[derive(Resource)]
pub struct BarrierAssets {
    pub(super) mesh: Handle<Mesh>,
    pub(super) red: Handle<StandardMaterial>,
    pub(super) blue: Handle<StandardMaterial>,
    pub(super) green: Handle<StandardMaterial>,
    pub(super) yellow: Handle<StandardMaterial>,
}

impl BarrierAssets {
    pub(super) fn material(&self, color: BarrierColor) -> &Handle<StandardMaterial> {
        match color {
            BarrierColor::Red => &self.red,
            BarrierColor::Blue => &self.blue,
            BarrierColor::Green => &self.green,
            BarrierColor::Yellow => &self.yellow,
        }
    }

    // Used by foreign systems (e.g., the wall-light emissive pass) that want
    // to skip barrier materials.
    pub fn material_handles(&self) -> [&Handle<StandardMaterial>; 4] {
        [&self.red, &self.blue, &self.green, &self.yellow]
    }
}

pub fn setup_barrier_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Unit X and Y so per-instance `Transform.scale` can encode the merged
    // segment's length and barrier height. Thickness stays baked in the mesh
    // — no instance ever wants a different thickness.
    let mesh = meshes.add(Cuboid::new(1.0, 1.0, BARRIER_THICKNESS));
    let red = materials.add(barrier_material(BARRIER_RED_COLOR));
    let blue = materials.add(barrier_material(BARRIER_BLUE_COLOR));
    let green = materials.add(barrier_material(BARRIER_GREEN_COLOR));
    let yellow = materials.add(barrier_material(BARRIER_YELLOW_COLOR));

    commands.insert_resource(BarrierAssets {
        mesh,
        red,
        blue,
        green,
        yellow,
    });
}

fn barrier_material(color: Color) -> StandardMaterial {
    let linear = color.to_linear();
    StandardMaterial {
        base_color: Color::srgba(linear.red, linear.green, linear.blue, BARRIER_BASE_ALPHA),
        emissive: linear * BARRIER_EMISSIVE_MIN,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    }
}
