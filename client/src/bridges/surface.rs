use std::mem;

use bevy::prelude::*;
use common::protocol::{LightBridge, MapLayout};

use crate::fields::{clip_surface_rects, surface_frame_rects};

pub(super) struct BridgeVisual {
    pub bridge: LightBridge,
    pub surfaces: Vec<Rect>,
    pub frames: Vec<Rect>,
}

pub(super) fn bridge_visuals(layout: &MapLayout) -> Vec<BridgeVisual> {
    let mut groups: Vec<BridgeVisual> = Vec::new();
    for bridge in &layout.light_bridges {
        let (x1, x2, z1, z2) = bridge.bounds_xz();
        let surface = Rect::new(x1, z1, x2, z2);
        if let Some(group) = groups.iter_mut().find(|group| {
            let other = &group.bridge;
            other.kind == bridge.kind
                && other.carrier == bridge.carrier
                && other.level == bridge.level
                && other.y == bridge.y
                && other.thickness == bridge.thickness
        }) {
            group.surfaces.push(surface);
        } else {
            groups.push(BridgeVisual {
                bridge: *bridge,
                surfaces: vec![surface],
                frames: Vec::new(),
            });
        }
    }
    for group in &mut groups {
        let bridge = &group.bridge;
        // Keep bridge rims inside their footprint so different kinds meet without overlapping frames.
        let frames = surface_frame_rects(&group.surfaces, 2.0 * bridge.thickness)
            .into_iter()
            .flat_map(|frame| {
                group.surfaces.iter().map(move |surface| Rect {
                    min: frame.min.max(surface.min),
                    max: frame.max.min(surface.max),
                })
            })
            .filter(|rect| rect.min.x < rect.max.x && rect.min.y < rect.max.y)
            .collect();
        group.frames = clip_surface_rects(frames, layout, bridge.carrier, [0, 2], bridge.y, bridge.thickness / 2.0);
        group.surfaces = clip_surface_rects(
            mem::take(&mut group.surfaces),
            layout,
            bridge.carrier,
            [0, 2],
            bridge.y,
            0.0,
        );
    }
    groups
}

#[cfg(test)]
#[path = "tests/surface.rs"]
mod tests;
