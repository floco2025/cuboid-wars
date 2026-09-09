use std::collections::{HashMap, HashSet, VecDeque};

use crate::{config::ServerGameplayConfig, map::MapConfig};
use anyhow::{Result, bail};
use bevy::prelude::Resource;

use super::{LadderLink, NavGraph, NavGraphs, NavNode};

// A zone's roam region, in the frame of the zone's carrier; sorted, so
// `contains` bisects and a roam route can pick a node by index.
#[derive(Clone)]
pub(crate) struct ActorTerritory {
    pub(crate) roam: Vec<NavNode>,
}

impl ActorTerritory {
    #[must_use]
    pub(crate) fn contains(&self, node: NavNode) -> bool {
        self.roam.binary_search(&node).is_ok()
    }
}

#[derive(Clone, Resource)]
pub struct ActorTerritories(Vec<ActorTerritory>);

impl ActorTerritories {
    pub fn new(graphs: &NavGraphs, map: &MapConfig, config: &ServerGameplayConfig) -> Result<Self> {
        let mut territories = Vec::with_capacity(map.actor_spawn_zones.len());
        for (index, zone) in map.actor_spawn_zones.iter().enumerate() {
            let graph = graphs.get(zone.carrier);
            let seeds = graph.zone_nodes(zone);
            if seeds.is_empty() {
                bail!(
                    "actor spawn zone {index} on carrier {} has no traversable navigation cells",
                    zone.carrier.0
                );
            }
            let kind = config.expect_actor(&zone.kind);
            let mut roam: Vec<_> = expand_region(graph, graph.ladder_links(&zone.kind), &seeds, kind.roam_steps)
                .into_iter()
                .collect();
            roam.sort_unstable();
            territories.push(ActorTerritory { roam });
        }
        Ok(Self(territories))
    }

    #[must_use]
    pub(crate) fn get(&self, zone_index: usize) -> &ActorTerritory {
        self.0
            .get(zone_index)
            .expect("actor spawn zone was validated when territories were built")
    }
}

fn expand_region(graph: &NavGraph, ladders: &[LadderLink], seeds: &[NavNode], max_steps: usize) -> HashSet<NavNode> {
    let mut depths: HashMap<NavNode, usize> = seeds.iter().copied().map(|node| (node, 0)).collect();
    let mut queue: VecDeque<_> = seeds.iter().copied().collect();
    while let Some(node) = queue.pop_front() {
        let depth = depths[&node];
        if depth >= max_steps {
            continue;
        }
        for next in graph.route_neighbors(node, ladders) {
            if let std::collections::hash_map::Entry::Vacant(entry) = depths.entry(next) {
                entry.insert(depth + 1);
                queue.push_back(next);
            }
        }
    }
    depths.into_keys().collect()
}
