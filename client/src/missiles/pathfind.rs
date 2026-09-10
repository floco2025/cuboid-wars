use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

// Unweighted BFS with backtracking, the driver behind the missiles' 3D
// `AirGraph`. Only the traversal lives here — node types, traversability, and
// neighbor semantics stay domain-owned. Returns the node sequence from just
// after `start` to the first goal node (exclusive of `start`), or `None` when
// no goal is reachable.
pub(super) fn bfs_path<N: Copy + Eq + Hash>(
    start: N,
    is_goal: impl Fn(&N) -> bool,
    mut neighbors: impl FnMut(N) -> Vec<N>,
) -> Option<Vec<N>> {
    let mut queue = VecDeque::from([start]);
    let mut came_from: HashMap<N, Option<N>> = HashMap::from([(start, None)]);
    let mut found = is_goal(&start).then_some(start);

    while found.is_none() {
        let Some(node) = queue.pop_front() else {
            break;
        };
        for next in neighbors(node) {
            if came_from.contains_key(&next) {
                continue;
            }
            came_from.insert(next, Some(node));
            if is_goal(&next) {
                found = Some(next);
                break;
            }
            queue.push_back(next);
        }
    }

    let goal = found?;
    let mut nodes = Vec::new();
    let mut cursor = goal;
    while cursor != start {
        nodes.push(cursor);
        cursor = came_from.get(&cursor).copied().flatten()?;
    }
    nodes.reverse();
    Some(nodes)
}

#[cfg(test)]
#[path = "pathfind_tests.rs"]
mod tests;
