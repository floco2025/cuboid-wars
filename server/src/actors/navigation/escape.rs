use bevy::prelude::Vec3;
use common::protocol::Position;

pub(crate) fn evade_clearance(start_distance_sq: f32, body_clearance: f32, tier: u8) -> f32 {
    let multiplier: f32 = match tier {
        0 => 4.0,
        1 => 2.0,
        _ => 1.0,
    };
    start_distance_sq.min((body_clearance * multiplier).powi(2))
}

pub(crate) fn segment_threat_distance_sq(from: Vec3, to: Vec3, threats: &[Position]) -> f32 {
    let movement = to - from;
    threats
        .iter()
        .map(|threat| {
            let threat = Vec3::from(*threat);
            let fraction = if movement.length_squared() > 0.0 {
                ((threat - from).dot(movement) / movement.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            threat.distance_squared(from + movement * fraction)
        })
        .fold(f32::INFINITY, f32::min)
}
