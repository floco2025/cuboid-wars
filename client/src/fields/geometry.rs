use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use common::protocol::{Barrier, BarrierId, BarrierKindId, CarrierId, Checkpoint, Eraser, Floor, MapLayout, SwitchId};

use super::surface::{clip_surface_rects, floor_bounds, surface_frame_rects};

const MERGE_EPSILON: f32 = 1e-4;

#[derive(Clone, Copy, Debug)]
pub(crate) struct VisualField {
    // One of the barriers a merged pane covers; they all share its kind and controls.
    pub barrier: Option<BarrierId>,
    pub kind: Option<BarrierKindId>,
    pub switch: Option<SwitchId>,
    pub switch_inverted: bool,
    pub carrier: CarrierId,
    pub level: u8,
    pub levels: u8,
    pub rect: Rect,
    pub thickness: f32,
    axis: usize,
    plane: f32,
}

impl VisualField {
    pub fn checkpoint_perimeter(checkpoint: &Checkpoint, height: f32, thickness: f32) -> [Self; 4] {
        let c = checkpoint;
        [
            (c.min_x, c.min_z, c.max_x, c.min_z),
            (c.min_x, c.max_z, c.max_x, c.max_z),
            (c.min_x, c.min_z, c.min_x, c.max_z),
            (c.max_x, c.min_z, c.max_x, c.max_z),
        ]
        .map(|(x1, z1, x2, z2)| {
            let (axis, plane, rect) = segment_rect(Vec3::new(x1, c.y, z1), Vec3::new(x2, c.y + height, z2));
            Self {
                barrier: None,
                kind: None,
                switch: None,
                switch_inverted: false,
                carrier: c.carrier,
                level: c.level,
                levels: 1,
                rect,
                thickness,
                axis,
                plane,
            }
        })
    }

    pub fn from_barrier(barrier: &Barrier) -> Self {
        let (axis, plane, rect) = segment_rect(
            Vec3::new(barrier.x1, barrier.y, barrier.z1),
            Vec3::new(barrier.x2, barrier.y + barrier.height, barrier.z2),
        );
        Self {
            barrier: Some(barrier.id),
            kind: Some(barrier.kind),
            switch: barrier.switch,
            switch_inverted: barrier.switch_inverted,
            carrier: barrier.carrier,
            level: barrier.level,
            levels: barrier.levels,
            rect,
            thickness: barrier.width,
            axis,
            plane,
        }
    }

    pub fn from_eraser(eraser: &Eraser, floor_thickness: f32) -> Self {
        // The trigger fills a storey; its visible panel leaves the same ceiling space as a barrier.
        let (axis, plane, rect) = segment_rect(
            Vec3::new(eraser.x1, eraser.y, eraser.z1),
            Vec3::new(eraser.x2, eraser.y + eraser.height - floor_thickness, eraser.z2),
        );
        Self {
            barrier: None,
            kind: None,
            switch: None,
            switch_inverted: false,
            carrier: eraser.carrier,
            level: eraser.level,
            levels: 1,
            rect,
            thickness: eraser.width,
            axis,
            plane,
        }
    }

    pub fn transform(&self) -> Transform {
        let center = self.rect.center();
        let mut translation = Vec3::new(0.0, center.y, 0.0);
        translation[self.axis] = center.x;
        translation[2 - self.axis] = self.plane;
        Transform::from_translation(translation).with_rotation(if self.axis == 0 {
            Quat::IDENTITY
        } else {
            Quat::from_rotation_y(-FRAC_PI_2)
        })
    }

    pub fn panel_rects(&self, layout: &MapLayout) -> Vec<Rect> {
        self.exposed_rects(vec![self.rect], layout, 0.0)
    }

    pub fn frame_rects(&self, layout: &MapLayout) -> Vec<Rect> {
        self.exposed_rects(
            surface_frame_rects(&[self.rect], self.thickness),
            layout,
            self.thickness / 2.0,
        )
    }

    fn exposed_rects(&self, surfaces: Vec<Rect>, layout: &MapLayout, depth: f32) -> Vec<Rect> {
        clip_surface_rects(surfaces, layout, self.carrier, [self.axis, 1], self.plane, depth)
    }

    fn can_merge(&self, other: &Self, floors: &[Floor], floor_thickness: f32, stack: bool) -> bool {
        if self.kind != other.kind
            || self.switch != other.switch
            || self.switch_inverted != other.switch_inverted
            || self.carrier != other.carrier
            || self.axis != other.axis
            || !near(self.plane, other.plane)
            || !near(self.thickness, other.thickness)
        {
            return false;
        }
        if !stack {
            return near(self.rect.min.y, other.rect.min.y)
                && near(self.rect.max.y, other.rect.max.y)
                && self.rect.min.x.max(other.rect.min.x) <= self.rect.max.x.min(other.rect.max.x) + MERGE_EPSILON;
        }
        if !near(self.rect.min.x, other.rect.min.x) || !near(self.rect.max.x, other.rect.max.x) {
            return false;
        }
        let joint = self.rect.min.y.max(other.rect.min.y);
        if joint > self.rect.max.y.min(other.rect.max.y) + floor_thickness + MERGE_EPSILON {
            return false;
        }
        let mut covered: Vec<_> = floors
            .iter()
            .filter(|floor| floor.carrier == self.carrier)
            .filter_map(|floor| {
                let (min, max) = floor_bounds(floor);
                (near(floor.y, joint)
                    && min[2 - self.axis] <= self.plane
                    && self.plane <= max[2 - self.axis]
                    && min[self.axis] < self.rect.max.x
                    && self.rect.min.x < max[self.axis])
                    .then_some((min[self.axis], max[self.axis]))
            })
            .collect();
        covered.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Wall trim covers only the ends; a separating floor must span the entire opening.
        let mut end = self.rect.min.x;
        for (min, max) in covered {
            if min > end + MERGE_EPSILON {
                return true;
            }
            end = end.max(max);
        }
        end < self.rect.max.x - MERGE_EPSILON
    }
}

pub(crate) fn merge_fields(
    fields: impl IntoIterator<Item = VisualField>,
    floors: &[Floor],
    floor_thickness: f32,
) -> Vec<VisualField> {
    // Stack before joining neighbors so authored order cannot leave internal frames in a rectangular grid.
    let stacked = merge_runs(fields, floors, floor_thickness, true);
    merge_runs(stacked, floors, floor_thickness, false)
}

fn merge_runs(
    fields: impl IntoIterator<Item = VisualField>,
    floors: &[Floor],
    floor_thickness: f32,
    stack: bool,
) -> Vec<VisualField> {
    let mut merged: Vec<VisualField> = Vec::new();
    for mut field in fields {
        let mut index = 0;
        while index < merged.len() {
            if field.can_merge(&merged[index], floors, floor_thickness, stack) {
                let other = merged.swap_remove(index);
                let last_level = field
                    .level
                    .saturating_add(field.levels.saturating_sub(1))
                    .max(other.level.saturating_add(other.levels.saturating_sub(1)));
                field.level = field.level.min(other.level);
                field.levels = (last_level - field.level).saturating_add(1);
                field.rect.min = field.rect.min.min(other.rect.min);
                field.rect.max = field.rect.max.max(other.rect.max);
                index = 0;
            } else {
                index += 1;
            }
        }
        merged.push(field);
    }
    merged
}

fn segment_rect(start: Vec3, end: Vec3) -> (usize, f32, Rect) {
    let axis = if start.z == end.z { 0 } else { 2 };
    let min = start.min(end);
    let max = start.max(end);
    (axis, start[2 - axis], Rect::new(min[axis], min.y, max[axis], max.y))
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= MERGE_EPSILON
}

#[cfg(test)]
#[path = "tests/geometry.rs"]
mod tests;
