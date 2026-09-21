use super::*;
use crate::test_fixtures::LEVEL_HEIGHT;
use common::constants::FIREWORK_SHOW_SECS;

fn map() -> MapDimensions {
    MapDimensions {
        width: 40.0,
        depth: 30.0,
        height: 4.0 * LEVEL_HEIGHT,
    }
}

fn positions(events: &VecDeque<FireworkEvent>) -> Vec<(f32, Vec3)> {
    events
        .iter()
        .map(|event| match &event.action {
            FireworkAction::Launch { pos, .. } | FireworkAction::Embers { pos, .. } => (event.at_secs, *pos),
            FireworkAction::LaserBeams { beams } => (event.at_secs, beams[0].pivot),
        })
        .collect()
}

#[test]
fn same_seed_builds_the_identical_show() {
    let a = build_show(42, map());
    let b = build_show(42, map());
    assert_eq!(positions(&a), positions(&b), "cross-client sync relies on determinism");
    assert!(!a.is_empty());
}

#[test]
fn events_are_time_sorted() {
    let show = build_show(7, map());
    let times: Vec<f32> = show.iter().map(|event| event.at_secs).collect();
    let mut sorted = times.clone();
    sorted.sort_by(f32::total_cmp);
    assert_eq!(times, sorted);
}

#[test]
fn rockets_launch_outside_and_below_and_pop_above_the_map() {
    let map = map();
    let half = (map.width / 2.0).max(map.depth / 2.0);
    let planar = |pos: Vec3| pos.xz().length();
    for event in &build_show(123, map) {
        match &event.action {
            // Ground launches (fuse > star fuse) start outside the
            // footprint and below the ground floor; star second stages
            // start at sky height instead.
            FireworkAction::Launch { pos, fuse_secs, .. } if *fuse_secs > STAR_FUSE_SECS => {
                assert!(pos.y < 0.0, "launch origin above ground: {pos}");
                assert!(planar(*pos) > half, "launch origin inside footprint: {pos}");
            }
            FireworkAction::Launch { pos, .. } | FireworkAction::Embers { pos, .. } => {
                assert!(
                    pos.y > map.height,
                    "sky event below the map top: {pos} (top {})",
                    map.height
                );
            }
            FireworkAction::LaserBeams { beams } => {
                for beam in beams {
                    assert!(planar(beam.pivot) > half, "beam pivot inside footprint");
                    assert!(beam.start_dir.y > 0.4, "beam not aimed skyward: {}", beam.start_dir);
                }
            }
        }
    }
}

#[test]
fn a_larger_map_gets_a_wider_and_higher_show() {
    let small = ShowField::new(map());
    let wide = ShowField::new(MapDimensions {
        width: 80.0,
        depth: 60.0,
        height: 4.0 * LEVEL_HEIGHT,
    });
    let tall = ShowField::new(MapDimensions {
        width: 40.0,
        depth: 30.0,
        height: 12.0 * LEVEL_HEIGHT,
    });

    assert!(wide.ring_radius > small.ring_radius);
    assert!(wide.sky_base > small.sky_base);
    assert!(
        tall.sky_base - 12.0 * LEVEL_HEIGHT > small.sky_base - 4.0 * LEVEL_HEIGHT,
        "a tall map lifts the sky beyond its extra storeys"
    );
}

#[test]
fn a_running_show_ignores_a_new_seed() {
    let mut show = FireworkShow::default();
    show.start(1, map());
    show.elapsed = 5.0;
    show.events.pop_front();
    let remaining = show.events.len();

    show.start(2, map());

    assert_eq!(show.events.len(), remaining);
    assert_eq!(show.elapsed, 5.0);
}

#[test]
fn a_finished_show_starts_again() {
    let mut show = FireworkShow::default();
    show.start(1, map());
    show.elapsed = 40.0;
    show.events.clear();

    show.start(2, map());

    assert!(!show.events.is_empty());
    assert_eq!(show.elapsed, 0.0);
}

#[test]
fn every_cue_of_a_show_lies_within_the_shared_show_length() {
    // A map far larger than any shipped one: the finale's pops are the last
    // cues, and they must not run past the show length on any map.
    let vast = MapDimensions {
        width: 600.0,
        depth: 600.0,
        height: 40.0 * LEVEL_HEIGHT,
    };
    for seed in [0, 1, 7, 42, 1234, u64::MAX] {
        let events = build_show(seed, vast);
        let last = events.iter().map(|event| event.at_secs).fold(0.0_f32, f32::max);
        assert!(
            last <= FIREWORK_SHOW_SECS,
            "seed {seed}: a cue at {last} s runs past the {FIREWORK_SHOW_SECS} s the server spaces shows by"
        );
    }
}
