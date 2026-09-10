use common::protocol::{Floor, LightBridge};

#[derive(Clone, Copy)]
struct Footprint {
    x1: f32,
    x2: f32,
    z1: f32,
    z2: f32,
}

impl Footprint {
    fn new((x1, x2, z1, z2): (f32, f32, f32, f32)) -> Self {
        Self { x1, x2, z1, z2 }
    }

    fn padded(self, pad: f32) -> Self {
        Self {
            x1: self.x1 - pad,
            x2: self.x2 + pad,
            z1: self.z1 - pad,
            z2: self.z2 + pad,
        }
    }

    fn subtract(self, other: Self) -> Vec<Self> {
        let x1 = self.x1.max(other.x1);
        let x2 = self.x2.min(other.x2);
        let z1 = self.z1.max(other.z1);
        let z2 = self.z2.min(other.z2);
        if x1 >= x2 || z1 >= z2 {
            return vec![self];
        }
        // Keep the long walking direction in one collider when a side is clipped.
        let pieces = if self.x2 - self.x1 >= self.z2 - self.z1 {
            [
                Self { z2: z1, ..self },
                Self { z1: z2, ..self },
                Self { x2: x1, z1, z2, ..self },
                Self { x1: x2, z1, z2, ..self },
            ]
        } else {
            [
                Self { x2: x1, ..self },
                Self { x1: x2, ..self },
                Self { x1, x2, z2: z1, ..self },
                Self { x1, x2, z1: z2, ..self },
            ]
        };
        pieces.into_iter().filter(|p| p.x1 < p.x2 && p.z1 < p.z2).collect()
    }
}

// Floors own their extensions; bridges fill up to those faces without coplanar overlap.
pub(super) fn flush_light_bridges(bridges: Vec<LightBridge>, floors: &[Floor], pad: f32) -> Vec<LightBridge> {
    let mut out: Vec<LightBridge> = Vec::new();
    for (index, bridge) in bridges.iter().enumerate() {
        let mut parts = vec![Footprint::new(bridge.bounds_xz()).padded(pad)];
        let floor_bounds = floors
            .iter()
            .filter(|floor| floor.level == bridge.level && floor.carrier == bridge.carrier)
            .map(|floor| floor.bounds_xz());
        // Every bridge keeps its core, even when a neighbour's kind is unpowered.
        let other_cores = bridges
            .iter()
            .enumerate()
            .filter(|(i, other)| *i != index && other.level == bridge.level && other.carrier == bridge.carrier)
            .map(|(_, other)| other.bounds_xz());
        let assigned_edges = out
            .iter()
            .filter(|other| other.level == bridge.level && other.carrier == bridge.carrier)
            .map(|other| other.bounds_xz());
        for bounds in floor_bounds.chain(other_cores).chain(assigned_edges) {
            let cut = Footprint::new(bounds);
            parts = parts.into_iter().flat_map(|part| part.subtract(cut)).collect();
        }
        out.extend(parts.into_iter().map(|part| LightBridge {
            x1: part.x1,
            x2: part.x2,
            z1: part.z1,
            z2: part.z2,
            ..*bridge
        }));
    }
    out
}

#[cfg(test)]
#[path = "tests/bridge_bounds.rs"]
mod tests;
