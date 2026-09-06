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
mod tests {
    use super::*;
    use common::protocol::{BridgeKindId, CarrierId};

    const PAD: f32 = 0.25;

    fn bridge(x1: f32, z1: f32, kind: u16) -> LightBridge {
        LightBridge {
            x1,
            x2: x1 + 4.0,
            z1,
            z2: z1 + 4.0,
            y: 4.0,
            thickness: 0.1,
            level: 1,
            kind: BridgeKindId(kind),
            carrier: CarrierId::WORLD,
        }
    }

    fn floor(x1: f32, z1: f32) -> Floor {
        Floor {
            x1: x1 - PAD,
            x2: x1 + 4.0 + PAD,
            z1: z1 - PAD,
            z2: z1 + 4.0 + PAD,
            y: 4.0,
            thickness: 0.4,
            level: 1,
            carrier: CarrierId::WORLD,
        }
    }

    fn contains(bounds: (f32, f32, f32, f32), x: f32, z: f32) -> bool {
        let (x1, x2, z1, z2) = bounds;
        x1 < x && x < x2 && z1 < z && z < z2
    }

    #[test]
    fn lone_bridge_has_the_same_width_as_a_floor() {
        let bridges = flush_light_bridges(vec![bridge(0.0, 0.0, 0)], &[], PAD);
        assert_eq!(bridges.len(), 1);
        assert_eq!(bridges[0].bounds_xz(), floor(0.0, 0.0).bounds_xz());
    }

    #[test]
    fn straight_walkway_meets_both_landings_in_one_slab() {
        let bridges = flush_light_bridges(vec![bridge(0.0, 0.0, 0)], &[floor(-4.0, 0.0), floor(4.0, 0.0)], PAD);
        assert_eq!(bridges.len(), 1);
        assert_eq!(bridges[0].bounds_xz(), (PAD, 4.0 - PAD, -PAD, 4.0 + PAD));
    }

    #[test]
    fn floors_and_bridges_on_other_levels_or_carriers_do_not_clip() {
        let original = bridge(0.0, 0.0, 0);
        let floors = [
            Floor {
                level: 2,
                ..floor(0.0, 0.0)
            },
            Floor {
                carrier: CarrierId(1),
                ..floor(0.0, 0.0)
            },
        ];
        let bridges = flush_light_bridges(
            vec![
                original,
                LightBridge { level: 2, ..original },
                LightBridge {
                    carrier: CarrierId(1),
                    ..original
                },
            ],
            &[],
            PAD,
        );
        assert_eq!(bridges.len(), 3);
        for bridge in &bridges {
            assert_eq!(bridge.bounds_xz(), floor(0.0, 0.0).bounds_xz());
            assert_eq!(bridge.y, original.y);
            assert_eq!(bridge.thickness, original.thickness);
        }
        let bridges = flush_light_bridges(vec![original], &floors, PAD);
        assert_eq!(bridges.len(), 1);
        assert_eq!(bridges[0].bounds_xz(), floor(0.0, 0.0).bounds_xz());
    }

    #[test]
    fn every_two_by_two_arrangement_has_no_gaps_overlaps_or_stolen_cores() {
        let cuts = [-PAD, 0.0, PAD, 4.0 - PAD, 4.0, 4.0 + PAD, 8.0 - PAD, 8.0, 8.0 + PAD];
        for arrangement in 0..256 {
            let mut floors = Vec::new();
            let mut cores = Vec::new();
            for cell in 0..4 {
                let x = (cell % 2) as f32 * 4.0;
                let z = (cell / 2) as f32 * 4.0;
                match (arrangement >> (cell * 2)) & 3 {
                    0 => {}
                    1 => floors.push(floor(x, z)),
                    kind => cores.push(bridge(x, z, kind - 2)),
                }
            }
            let bridges = flush_light_bridges(cores.clone(), &floors, PAD);
            for xs in cuts.windows(2) {
                for zs in cuts.windows(2) {
                    let x = f32::midpoint(xs[0], xs[1]);
                    let z = f32::midpoint(zs[0], zs[1]);
                    let floor_here = floors.iter().any(|floor| contains(floor.bounds_xz(), x, z));
                    let padded_core_here = cores
                        .iter()
                        .any(|core| contains((core.x1 - PAD, core.x2 + PAD, core.z1 - PAD, core.z2 + PAD), x, z));
                    let covering: Vec<_> = bridges
                        .iter()
                        .filter(|bridge| contains(bridge.bounds_xz(), x, z))
                        .collect();
                    assert_eq!(
                        covering.len(),
                        usize::from(!floor_here && padded_core_here),
                        "arrangement {arrangement}, ({x}, {z})"
                    );
                    if !floor_here {
                        for core in &cores {
                            if contains(core.bounds_xz(), x, z) {
                                assert_eq!(covering[0].kind, core.kind, "a neighbour took another kind's core");
                            }
                        }
                    }
                }
            }
        }
    }
}
