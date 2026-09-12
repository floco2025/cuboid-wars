use bevy_math::Vec3;
use rapier3d::prelude::ColliderSet;

use super::{CollisionWorld, colliders::ColliderKind};
use crate::{math::from_rapier, protocol::CarrierId};

#[derive(Default)]
struct CarrierBounds {
    local: Option<(Vec3, Vec3)>,
    translation: Vec3,
    previous: Vec3,
    revision: u64,
}

pub(super) struct WorldBounds {
    carriers: Vec<CarrierBounds>,
    revision: u64,
    combined: Option<(Vec3, Vec3)>,
}

impl WorldBounds {
    pub fn new(colliders: &ColliderSet, carrier_count: usize) -> Self {
        let mut carriers: Vec<CarrierBounds> = (0..=carrier_count).map(|_| CarrierBounds::default()).collect();
        for (_, collider) in colliders.iter() {
            let carrier = ColliderKind::carrier_from_user_data(collider.user_data);
            let aabb = collider.compute_aabb();
            let bounds = (from_rapier(aabb.mins), from_rapier(aabb.maxs));
            union(&mut carriers[usize::from(carrier.0)].local, bounds);
        }
        let mut bounds = Self {
            carriers,
            revision: 0,
            combined: None,
        };
        bounds.refresh();
        bounds
    }

    pub fn finish_placement(&mut self) {
        for carrier in &mut self.carriers {
            carrier.previous = carrier.translation;
        }
    }

    pub fn set_pose(&mut self, carrier: CarrierId, translation: Vec3) -> bool {
        let bounds = &mut self.carriers[usize::from(carrier.0)];
        if bounds.translation == translation {
            return false;
        }
        bounds.previous = bounds.translation;
        bounds.translation = translation;
        self.changed(carrier);
        true
    }

    pub fn changed(&mut self, carrier: CarrierId) {
        self.revision += 1;
        self.carriers[usize::from(carrier.0)].revision = self.revision;
    }

    pub fn refresh(&mut self) {
        self.combined = None;
        for carrier in &self.carriers {
            if let Some((min, max)) = carrier.local {
                union(
                    &mut self.combined,
                    (min + carrier.translation, max + carrier.translation),
                );
            }
        }
    }
}

impl CollisionWorld {
    pub fn geometry_bounds(&self) -> Option<(Vec3, Vec3)> {
        self.bounds.combined
    }

    pub fn geometry_revision(&self) -> u64 {
        self.bounds.revision
    }

    pub fn region_revision(&self, min: Vec3, max: Vec3) -> u64 {
        self.bounds
            .carriers
            .iter()
            .filter_map(|carrier| {
                let (local_min, local_max) = carrier.local?;
                let swept_min = local_min + carrier.translation.min(carrier.previous);
                let swept_max = local_max + carrier.translation.max(carrier.previous);
                (swept_min.cmple(max).all() && swept_max.cmpge(min).all()).then_some(carrier.revision)
            })
            .max()
            .unwrap_or(0)
    }
}

fn union(target: &mut Option<(Vec3, Vec3)>, (min, max): (Vec3, Vec3)) {
    *target = Some(target.map_or((min, max), |(previous_min, previous_max)| {
        (previous_min.min(min), previous_max.max(max))
    }));
}
