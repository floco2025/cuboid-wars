use anyhow::{Result, ensure};
use bevy_math::Vec3;
use bincode::{Decode, Encode};
use serde::Deserialize;

use crate::config::validate_positive_finite;

#[derive(Debug, Clone, Deserialize, Encode, Decode)]
pub struct GroundsSettings {
    pub level: u8,
    pub margin: f32,
    pub return_secs: f32,
}

impl GroundsSettings {
    pub fn validate(&self, path: &str) -> Result<()> {
        validate_positive_finite(self.margin, &format!("{path}.margin"))?;
        validate_positive_finite(self.return_secs, &format!("{path}.return_secs"))?;
        ensure!(self.margin <= 200.0, "{path}.margin exceeds 200 metres");
        ensure!(self.return_secs <= 30.0, "{path}.return_secs exceeds 30 seconds");
        Ok(())
    }
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct Grounds {
    pub half_size: [f32; 2],
    pub y: f32,
    pub settings: GroundsSettings,
}

pub struct GroundsMesh {
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

pub struct GroundDecoration {
    pub position: Vec3,
    pub scale: Vec3,
    pub tree: bool,
}

impl Grounds {
    pub fn decorations(&self) -> Vec<GroundDecoration> {
        (0..180)
            .map(|i| {
                let angle = i as f32 * 2.399963;
                let distance = 22.0 + (i % 30) as f32 * 7.2;
                let direction = Vec3::new(angle.cos(), 0.0, angle.sin());
                let radius = (self.half_size[0] / direction.x.abs().max(0.001))
                    .min(self.half_size[1] / direction.z.abs().max(0.001))
                    + distance;
                let x = direction.x * radius;
                let z = direction.z * radius;
                let tree = i % 4 != 0;
                let size = 0.8 + (i % 7) as f32 * 0.13;
                GroundDecoration {
                    position: Vec3::new(x, self.height(x, z) - 0.2, z),
                    scale: if tree {
                        Vec3::new(1.0, 1.0, 1.0) * size
                    } else {
                        Vec3::new(1.8, 1.0, 1.4) * size
                    },
                    tree,
                }
            })
            .collect()
    }

    pub fn distance_outside_map(&self, x: f32, z: f32) -> f32 {
        (x.abs() - self.half_size[0]).max(z.abs() - self.half_size[1])
    }

    pub fn outside_boundary(&self, pos: Vec3) -> bool {
        self.distance_outside_map(pos.x, pos.z) > self.settings.margin
    }

    pub fn height(&self, x: f32, z: f32) -> f32 {
        let distance = self.distance_outside_map(x, z).max(0.0);
        let blend = ((distance - 10.0) / 45.0).clamp(0.0, 1.0);
        let blend = blend * blend * (3.0 - 2.0 * blend);
        let hills = (x * 0.021 + 0.6).sin() * (z * 0.017).cos() * 3.5 + (x * 0.043 - z * 0.031).sin() * 1.2;
        let distant = ((distance - 180.0) / 240.0).clamp(0.0, 1.0);
        self.y + blend * hills + distant * (18.0 + 16.0 * (x * 0.006 + z * 0.004).sin().powi(2))
    }

    pub fn mesh(&self, distant: bool) -> GroundsMesh {
        // The shared inner rings keep collision and rendering on identical triangles.
        let mut distances: Vec<f32> = (0..=100).map(|i| i as f32 * 4.0).collect();
        if distant {
            distances.extend((1..=36).map(|i| 400.0 + i as f32 * 25.0));
        }
        let segments = 64usize;
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for side in 0..4 {
            let offset = vertices.len() as u32;
            for &distance in &distances {
                let hx = self.half_size[0] + distance;
                let hz = self.half_size[1] + distance;
                for step in 0..=segments {
                    let t = step as f32 / segments as f32 * 2.0 - 1.0;
                    let (x, z) = match side {
                        0 => (hx, t * hz),
                        1 => (-t * hx, hz),
                        2 => (-hx, -t * hz),
                        _ => (t * hx, -hz),
                    };
                    vertices.push(Vec3::new(x, self.height(x, z), z));
                }
            }
            for ring in 0..distances.len() - 1 {
                for step in 0..segments {
                    let a = offset + (ring * (segments + 1) + step) as u32;
                    let b = a + (segments + 1) as u32;
                    triangles.extend([[a, a + 1, b], [a + 1, b + 1, b]]);
                }
            }
        }
        GroundsMesh { vertices, triangles }
    }
}

#[derive(Default)]
pub struct BoundaryTimer(pub f32);

impl BoundaryTimer {
    pub fn tick(&mut self, outside: bool, delta_secs: f32, delay_secs: f32) -> Option<f32> {
        if !outside {
            self.0 = 0.0;
            return None;
        }
        self.0 += delta_secs;
        Some((delay_secs - self.0).max(0.0))
    }
}

#[cfg(test)]
#[path = "tests/grounds.rs"]
mod tests;
