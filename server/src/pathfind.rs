use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

// Unweighted BFS with backtracking, shared by the actors' floor `NavGraph`
// and the missiles' 3D `AirGraph`. Only the traversal is shared — node
// types, traversability, and neighbor semantics stay domain-owned. Returns
// the node sequence from just after `start` to the first goal node
// (exclusive of `start`), or `None` when no goal is reachable.
pub(crate) fn bfs_path<N: Copy + Eq + Hash>(
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
mod tests {
    use super::*;

    // 1D line graph with a hole at 3: neighbors are n±1 in 0..=5, skipping 3.
    fn line_neighbors(n: i32) -> Vec<i32> {
        [n - 1, n + 1]
            .into_iter()
            .filter(|next| (0..=5).contains(next) && *next != 3)
            .collect()
    }

    #[test]
    fn bfs_path_finds_route_excluding_start() {
        let path = bfs_path(0, |n| *n == 2, line_neighbors).expect("2 is reachable from 0");
        assert_eq!(path, vec![1, 2]);
    }

    #[test]
    fn bfs_path_start_on_goal_is_empty() {
        let path = bfs_path(2, |n| *n == 2, line_neighbors).expect("already there");
        assert!(path.is_empty());
    }

    #[test]
    fn bfs_path_unreachable_is_none() {
        assert!(
            bfs_path(0, |n| *n == 5, line_neighbors).is_none(),
            "the hole at 3 seals off 5"
        );
    }
}
