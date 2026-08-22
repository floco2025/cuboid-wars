use std::collections::{HashMap, HashSet, VecDeque};

use crate::{config::ServerGameplayConfig, map::MapConfig};
use anyhow::{Result, bail};
use bevy::prelude::Resource;

use super::{NavGraph, NavNode};

#[derive(Clone)]
pub(crate) struct ActorTerritory {
    pub(crate) roam: HashSet<NavNode>,
    pub(crate) roam_nodes: Vec<NavNode>,
}

#[derive(Clone, Resource)]
pub struct ActorTerritories(Vec<ActorTerritory>);

impl ActorTerritories {
    pub fn new(graph: &NavGraph, map: &MapConfig, config: &ServerGameplayConfig) -> Result<Self> {
        let mut territories = Vec::with_capacity(map.actor_spawn_zones.len());
        for (index, zone) in map.actor_spawn_zones.iter().enumerate() {
            let seeds = graph.zone_nodes(zone);
            if seeds.is_empty() {
                bail!("actor spawn zone {index} has no traversable navigation cells");
            }
            let kind = config.expect_actor(&zone.kind);
            let roam = expand_region(graph, &seeds, kind.roam_steps);
            let mut roam_nodes: Vec<_> = roam.iter().copied().collect();
            roam_nodes.sort_unstable();
            territories.push(ActorTerritory { roam, roam_nodes });
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

fn expand_region(graph: &NavGraph, seeds: &[NavNode], max_steps: usize) -> HashSet<NavNode> {
    let mut depths: HashMap<NavNode, usize> = seeds.iter().copied().map(|node| (node, 0)).collect();
    let mut queue: VecDeque<_> = seeds.iter().copied().collect();
    while let Some(node) = queue.pop_front() {
        let depth = depths[&node];
        if depth >= max_steps {
            continue;
        }
        for &next in graph.neighbors(node) {
            if let std::collections::hash_map::Entry::Vacant(entry) = depths.entry(next) {
                entry.insert(depth + 1);
                queue.push_back(next);
            }
        }
    }
    depths.into_keys().collect()
}
