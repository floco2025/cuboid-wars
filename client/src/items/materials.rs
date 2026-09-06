use bevy::prelude::*;

pub fn pickup_emissive(color: Color, brightness: f32) -> LinearRgba {
    (LinearRgba::from(color) * brightness).with_alpha(1.0)
}

pub fn pickup_material(color: Color, brightness: f32) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        emissive: pickup_emissive(color, brightness),
        metallic: 0.15,
        perceptual_roughness: 0.4,
        ..default()
    }
}
