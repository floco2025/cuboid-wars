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
