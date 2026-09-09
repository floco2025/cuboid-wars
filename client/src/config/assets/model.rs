use anyhow::Result;
use bevy::prelude::Component;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ModelDef {
    pub scene: String,
    pub scale: f32,
    #[serde(default)]
    pub x_offset: f32,
    // Model origin offset from the character's feet.
    #[serde(default)]
    pub y_offset: f32,
    #[serde(default)]
    pub z_offset: f32,
    // Rotation applied to the model around the X axis at spawn; 180 for a
    // GLB authored head-down.
    #[serde(default)]
    pub x_rotation_degrees: f32,
    // Which clip in the GLB to play; a multi-clip export needs an explicit
    // index.
    #[serde(default)]
    pub animation_index: usize,
    #[serde(default)]
    pub animation_speed: Option<f32>,
    #[serde(default)]
    pub wheels: Option<WheelModelDef>,
    #[serde(default)]
    pub aim_rig: Option<AimRigDef>,
    #[serde(default = "default_true")]
    pub rotate_with_facing: bool,
}

fn default_true() -> bool {
    true
}

impl ModelDef {
    // The index of the `#Scene<n>` label in the scene reference.
    #[must_use]
    pub fn scene_index(&self) -> Option<usize> {
        self.scene
            .split_once('#')
            .and_then(|(_, label)| label.strip_prefix("Scene"))
            .and_then(|index| index.parse().ok())
    }
}

// The GLB path of a `path#Scene<n>` scene reference.
#[must_use]
pub fn gltf_path(scene: &str) -> String {
    scene.split('#').next().unwrap_or_default().to_owned()
}

#[derive(Debug, Clone, Deserialize, Component)]
#[serde(deny_unknown_fields)]
pub struct AimRigDef {
    pub yaw_node: String,
    pub pitch_node: String,
    pub muzzle_node: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelModelDef {
    pub radius: f32,
    pub track: f32,
    pub wheelbase: f32,
    pub idle_animation: usize,
    pub drive_animation: usize,
    pub drive_cycle_secs: f32,
}

pub(super) fn validate_model(path: &str, model: &ModelDef) -> Result<()> {
    anyhow::ensure!(!model.scene.trim().is_empty(), "`{path}.scene` must not be empty");
    anyhow::ensure!(
        model.scene_index().is_some(),
        "`{path}.scene` must reference a `#Scene<n>` label, got {}",
        model.scene
    );
    anyhow::ensure!(
        model.scale.is_finite() && model.scale > 0.0,
        "`{path}.scale` must be positive and finite, got {}",
        model.scale
    );
    for (field, value) in [
        ("x_offset", model.x_offset),
        ("y_offset", model.y_offset),
        ("z_offset", model.z_offset),
        ("x_rotation_degrees", model.x_rotation_degrees),
    ] {
        anyhow::ensure!(value.is_finite(), "`{path}.{field}` must be finite, got {value}");
    }
    if let Some(speed) = model.animation_speed {
        anyhow::ensure!(
            speed.is_finite() && speed > 0.0,
            "`{path}.animation_speed` must be positive and finite, got {speed}"
        );
    }
    if let Some(wheels) = model.wheels {
        anyhow::ensure!(
            model.animation_speed.is_none(),
            "`{path}` cannot combine wheels with animation_speed"
        );
        for (field, value) in [
            ("radius", wheels.radius),
            ("track", wheels.track),
            ("wheelbase", wheels.wheelbase),
            ("drive_cycle_secs", wheels.drive_cycle_secs),
        ] {
            anyhow::ensure!(
                value.is_finite() && value > 0.0,
                "`{path}.wheels.{field}` must be positive and finite"
            );
        }
        anyhow::ensure!(
            wheels.idle_animation != wheels.drive_animation,
            "`{path}.wheels` needs distinct idle_animation and drive_animation clips"
        );
    }
    if let Some(rig) = &model.aim_rig {
        for (field, name) in [
            ("yaw_node", &rig.yaw_node),
            ("pitch_node", &rig.pitch_node),
            ("muzzle_node", &rig.muzzle_node),
        ] {
            anyhow::ensure!(!name.trim().is_empty(), "`{path}.aim_rig.{field}` must not be empty");
        }
        anyhow::ensure!(
            rig.yaw_node != rig.pitch_node && rig.yaw_node != rig.muzzle_node && rig.pitch_node != rig.muzzle_node,
            "`{path}.aim_rig` needs distinct yaw, pitch, and muzzle nodes"
        );
    }
    Ok(())
}
