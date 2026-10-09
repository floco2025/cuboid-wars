use std::collections::VecDeque;

use bevy::prelude::*;
use common::{
    map::Carriers,
    physics::CollisionWorld,
    protocol::{CarrierId, FieldId, MapLayout, Wall},
};

use super::{
    air_graph::{AirGraph, AirGrid},
    search::{AirSearch, SearchBudget, SearchProgress},
};
use crate::{
    constants::MISSILE_SEARCH_WINDOW_MARGIN_CELLS,
    test_fixtures::{WALL_HEIGHT, WALL_THICKNESS, geometry},
};

// One world grid of `levels` storeys, plus its sky layer.
pub(super) fn test_graph(cols: i32, rows: i32, levels: usize) -> AirGraph {
    AirGraph {
        grids: vec![AirGrid {
            carrier: CarrierId::WORLD,
            geometry: geometry(cols, rows),
            layers: levels as i32 + 1,
        }],
    }
}

pub(super) fn wall(x1: f32, z1: f32, x2: f32, z2: f32) -> Wall {
    Wall {
        x1,
        z1,
        x2,
        z2,
        width: WALL_THICKNESS,
        y: 0.0,
        height: WALL_HEIGHT,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

pub(super) fn world(layout: &MapLayout) -> CollisionWorld {
    CollisionWorld::from_map_layout(layout)
}

// A route search run to its end with a fresh budget each tick.
pub(super) fn air_path(
    graph: &AirGraph,
    carriers: &Carriers,
    world: &CollisionWorld,
    open_fields: &[FieldId],
    from: Vec3,
    to: Vec3,
    radius: f32,
    fuse_distance: f32,
) -> Option<VecDeque<Vec3>> {
    let mut search = AirSearch::new(
        graph,
        carriers,
        open_fields,
        from,
        to,
        radius,
        fuse_distance,
        MISSILE_SEARCH_WINDOW_MARGIN_CELLS,
    );
    loop {
        match search.advance(graph, carriers, world, &mut SearchBudget::new(128)) {
            SearchProgress::Pending => {}
            SearchProgress::Found(path) => return Some(path),
            SearchProgress::Unreachable | SearchProgress::WindowLimited | SearchProgress::NodeLimited => {
                return None;
            }
        }
    }
}
