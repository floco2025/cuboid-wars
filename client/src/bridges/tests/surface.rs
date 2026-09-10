use super::*;
use common::protocol::{BridgeKindId, CarrierId, Floor, Wall};

fn bridge_surface_rects(bridge: &LightBridge, walls: &[Wall]) -> Vec<Rect> {
    bridge_visuals(&MapLayout {
        light_bridges: vec![*bridge],
        walls: walls.to_vec(),
        ..Default::default()
    })
    .remove(0)
    .surfaces
}

fn bridge() -> LightBridge {
    LightBridge {
        x1: -0.25,
        x2: 4.25,
        z1: -0.25,
        z2: 4.25,
        y: 4.0,
        thickness: 0.1,
        level: 1,
        kind: BridgeKindId(0),
        carrier: CarrierId(1),
    }
}

fn wall(x1: f32, z1: f32, x2: f32, z2: f32) -> Wall {
    Wall {
        x1,
        z1,
        x2,
        z2,
        y: 4.0,
        height: 3.6,
        width: 0.5,
        level: 1,
        carrier: CarrierId(1),
    }
}

#[test]
fn surfaces_meet_all_four_wall_faces_without_overlapping_their_bottoms() {
    for (wall, expected) in [
        (wall(-0.25, 0.0, 4.25, 0.0), Rect::new(-0.25, 0.25, 4.25, 4.25)),
        (wall(-0.25, 4.0, 4.25, 4.0), Rect::new(-0.25, -0.25, 4.25, 3.75)),
        (wall(0.0, -0.25, 0.0, 4.25), Rect::new(0.25, -0.25, 4.25, 4.25)),
        (wall(4.0, -0.25, 4.0, 4.25), Rect::new(-0.25, -0.25, 3.75, 4.25)),
    ] {
        assert_eq!(bridge_surface_rects(&bridge(), &[wall]), [expected]);
        let reversed = Wall {
            x1: wall.x2,
            x2: wall.x1,
            z1: wall.z2,
            z2: wall.z1,
            ..wall
        };
        assert_eq!(bridge_surface_rects(&bridge(), &[reversed]), [expected]);
    }
}

#[test]
fn partial_and_intersecting_walls_leave_every_exposed_patch_covered_once() {
    let walls = [wall(0.0, 0.0, 0.0, 2.0), wall(0.0, 2.0, 3.0, 2.0)];
    let surfaces = bridge_surface_rects(&bridge(), &walls);
    let xs = [-0.25, 0.0, 0.25, 3.0, 4.25];
    let zs = [-0.25, 0.0, 1.75, 2.0, 2.25, 4.25];
    for x in xs.windows(2) {
        for z in zs.windows(2) {
            let point = Vec2::new(f32::midpoint(x[0], x[1]), f32::midpoint(z[0], z[1]));
            let under_wall =
                Rect::new(-0.25, 0.0, 0.25, 2.0).contains(point) || Rect::new(0.0, 1.75, 3.0, 2.25).contains(point);
            let covering = surfaces.iter().filter(|rect| rect.contains(point)).count();
            assert_eq!(covering, usize::from(!under_wall), "point {point}");
        }
    }
    assert!(surfaces.iter().all(|rect| rect.width() > 0.0 && rect.height() > 0.0));
}

#[test]
fn walls_outside_the_surface_height_or_on_other_carriers_do_not_clip() {
    let base = wall(-0.25, 0.0, 4.25, 0.0);
    let walls = [
        Wall { y: 4.1, ..base },
        Wall { y: 0.0, ..base },
        Wall {
            carrier: CarrierId::WORLD,
            ..base
        },
        wall(-0.25, 5.0, 4.25, 5.0),
    ];
    assert_eq!(
        bridge_surface_rects(&bridge(), &walls),
        [Rect::new(-0.25, -0.25, 4.25, 4.25)]
    );
}

#[test]
fn wall_tops_at_the_surface_height_do_not_overlap() {
    let wall = Wall {
        y: 0.0,
        height: 4.0,
        level: 0,
        ..wall(-0.25, 0.0, 4.25, 0.0)
    };
    assert_eq!(
        bridge_surface_rects(&bridge(), &[wall]),
        [Rect::new(-0.25, 0.25, 4.25, 4.25)]
    );
}

#[test]
fn a_fully_covered_surface_emits_no_rectangles() {
    let wall = Wall {
        width: 4.5,
        ..wall(-0.25, 2.0, 4.25, 2.0)
    };
    assert!(bridge_surface_rects(&bridge(), &[wall]).is_empty());
}

#[test]
fn connected_bridge_frames_cover_only_the_outline_once_including_concave_corners() {
    let cuts = [
        0.0, 0.125, 0.25, 0.5, 3.5, 3.75, 3.875, 4.0, 4.125, 4.25, 4.5, 7.5, 7.75, 7.875, 8.0,
    ];
    for mask in 1..16 {
        let bridges: Vec<_> = (0..4)
            .filter(|cell| mask & (1 << cell) != 0)
            .map(|cell| {
                let x = (cell % 2) as f32 * 4.0;
                let z = (cell / 2) as f32 * 4.0;
                LightBridge {
                    x1: x,
                    x2: x + 4.0,
                    z1: z,
                    z2: z + 4.0,
                    thickness: 0.25,
                    ..bridge()
                }
            })
            .collect();
        let layout = MapLayout {
            light_bridges: bridges,
            ..Default::default()
        };
        let visual = bridge_visuals(&layout).remove(0);
        let inside = |point: Vec2| visual.surfaces.iter().any(|rect| rect.contains(point));
        for xs in cuts.windows(2) {
            for zs in cuts.windows(2) {
                let point = Vec2::new(f32::midpoint(xs[0], xs[1]), f32::midpoint(zs[0], zs[1]));
                let on_edge = inside(point)
                    && [-0.25, 0.0, 0.25]
                        .into_iter()
                        .any(|x| [-0.25, 0.0, 0.25].into_iter().any(|z| !inside(point + Vec2::new(x, z))));
                let count = visual.frames.iter().filter(|rect| rect.contains(point)).count();
                assert_eq!(count, usize::from(on_edge), "mask {mask}, point {point}");
            }
        }
    }
}

#[test]
fn frames_are_clipped_by_solids_but_not_by_another_carrier() {
    let bridge = bridge();
    let floor = Floor {
        x1: bridge.x1,
        x2: bridge.x2,
        z1: bridge.z1,
        z2: 0.25,
        y: bridge.y,
        thickness: 0.5,
        carrier: bridge.carrier,
        level: bridge.level,
    };
    let wall = wall(-0.25, 4.0, 4.25, 4.0);
    let layout = MapLayout {
        light_bridges: vec![bridge],
        walls: vec![wall],
        floors: vec![floor],
        ..Default::default()
    };
    let visual = bridge_visuals(&layout).remove(0);
    for rect in visual.frames.iter().chain(&visual.surfaces) {
        assert!(rect.min.y >= 0.25 && rect.max.y <= 3.75);
    }
    assert!(!visual.frames.is_empty());
    let layout = MapLayout {
        walls: vec![Wall {
            carrier: CarrierId::WORLD,
            ..wall
        }],
        floors: vec![Floor {
            carrier: CarrierId::WORLD,
            ..floor
        }],
        ..layout
    };
    let visual = bridge_visuals(&layout).remove(0);
    assert!(visual.frames.iter().any(|rect| rect.contains(Vec2::new(2.0, -0.2))));
}

#[test]
fn adjacent_kinds_have_their_own_frames_without_coplanar_overlap() {
    let a = LightBridge {
        x1: 0.0,
        x2: 4.0,
        z1: 0.0,
        z2: 4.0,
        ..bridge()
    };
    let b = LightBridge {
        x1: 4.0,
        x2: 8.0,
        kind: BridgeKindId(1),
        ..a
    };
    let layout = MapLayout {
        light_bridges: vec![a, b],
        ..Default::default()
    };
    let visuals = bridge_visuals(&layout);
    assert_eq!(visuals.len(), 2);
    assert!(visuals[0].frames.iter().any(|rect| rect.contains(Vec2::new(3.95, 2.0))));
    assert!(visuals[1].frames.iter().any(|rect| rect.contains(Vec2::new(4.05, 2.0))));
    for a in &visuals[0].frames {
        for b in &visuals[1].frames {
            let overlap = a.max.min(b.max) - a.min.max(b.min);
            assert!(overlap.x <= 0.0 || overlap.y <= 0.0);
        }
    }
}
