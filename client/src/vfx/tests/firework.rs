use super::*;
use crate::test_fixtures::{LEVEL_HEIGHT, sizes};
use common::protocol::{CarrierId, Floor};

fn layout() -> MapLayout {
    MapLayout {
        floors: vec![
            Floor {
                x1: -20.0,
                z1: -15.0,
                x2: 20.0,
                z2: 15.0,
                y: 0.0,
                thickness: 0.4,
                level: 0,
                carrier: CarrierId::WORLD,
            },
            Floor {
                x1: -5.0,
                z1: -5.0,
                x2: 5.0,
                z2: 5.0,
                y: 3.0 * LEVEL_HEIGHT,
                thickness: 0.4,
                level: 3,
                carrier: CarrierId::WORLD,
            },
        ],
        ..Default::default()
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
    let layout = layout();
    let a = build_show(42, Some(&layout), sizes());
    let b = build_show(42, Some(&layout), sizes());
    assert_eq!(positions(&a), positions(&b), "cross-client sync relies on determinism");
    assert!(!a.is_empty());
}

#[test]
fn events_are_time_sorted() {
    let show = build_show(7, Some(&layout()), sizes());
    let times: Vec<f32> = show.iter().map(|event| event.at_secs).collect();
    let mut sorted = times.clone();
    sorted.sort_by(f32::total_cmp);
    assert_eq!(times, sorted);
}

#[test]
fn rockets_launch_outside_and_below_and_pop_safely_high() {
    let layout = layout();
    let field = show_field(Some(&layout), sizes());
    let show = build_show(123, Some(&layout), sizes());
    for event in &show {
        match &event.action {
            // Ground launches (fuse > star fuse) start outside the
            // footprint and below the ground floor; star second stages
            // start at sky height instead.
            FireworkAction::Launch { pos, fuse_secs, .. } if *fuse_secs > STAR_FUSE_SECS => {
                assert!(pos.y < 0.0, "launch origin above ground: {pos}");
                let planar = Vec3::new(pos.x - field.center.x, 0.0, pos.z - field.center.z).length();
                assert!(
                    planar > field.half_x.max(field.half_z),
                    "launch origin inside footprint: {pos}"
                );
            }
            FireworkAction::Launch { pos, .. } | FireworkAction::Embers { pos, .. } => {
                assert!(
                    pos.y >= field.sky_base,
                    "sky event below safe height: {pos} (base {})",
                    field.sky_base
                );
            }
            FireworkAction::LaserBeams { beams } => {
                for beam in beams {
                    let planar = Vec3::new(beam.pivot.x - field.center.x, 0.0, beam.pivot.z - field.center.z).length();
                    assert!(planar > field.half_x.max(field.half_z), "beam pivot inside footprint");
                    assert!(beam.start_dir.y > 0.4, "beam not aimed skyward: {}", beam.start_dir);
                }
            }
        }
    }
}

#[test]
fn a_running_show_ignores_a_new_seed() {
    let mut show = FireworkShow::default();
    show.start(1, None, sizes());
    show.elapsed = 5.0;
    show.events.pop_front();
    let remaining = show.events.len();

    show.start(2, None, sizes());

    assert_eq!(show.events.len(), remaining);
    assert_eq!(show.elapsed, 5.0);
}

#[test]
fn a_finished_show_starts_again() {
    let mut show = FireworkShow::default();
    show.start(1, None, sizes());
    show.elapsed = 40.0;
    show.events.clear();

    show.start(2, None, sizes());

    assert!(!show.events.is_empty());
    assert_eq!(show.elapsed, 0.0);
}
