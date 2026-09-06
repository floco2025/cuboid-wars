use bevy::prelude::*;
use common::protocol::{LightBridge, Wall};

pub(super) fn bridge_surface_rects(bridge: &LightBridge, walls: &[Wall]) -> Vec<Rect> {
    let (x1, x2, z1, z2) = bridge.bounds_xz();
    let mut surfaces = vec![Rect::new(x1, z1, x2, z2)];
    // Only trim visuals: the slab below a wall must still block portal shots from underneath.
    for wall in walls
        .iter()
        .filter(|wall| wall.carrier == bridge.carrier && wall.y <= bridge.y && bridge.y <= wall.y + wall.height)
    {
        let start = Vec2::new(wall.x1, wall.z1);
        let end = Vec2::new(wall.x2, wall.z2);
        let along = (end - start).normalize_or_zero().abs();
        let padding = Vec2::new(along.y, along.x) * (wall.width / 2.0);
        let footprint = Rect {
            min: start.min(end) - padding,
            max: start.max(end) + padding,
        };
        surfaces = surfaces
            .into_iter()
            .flat_map(|surface| subtract_rect(surface, footprint))
            .collect();
    }
    surfaces
}

fn subtract_rect(surface: Rect, cut: Rect) -> Vec<Rect> {
    let min = surface.min.max(cut.min);
    let max = surface.max.min(cut.max);
    if min.x >= max.x || min.y >= max.y {
        return vec![surface];
    }
    [
        Rect {
            min: surface.min,
            max: Vec2::new(surface.max.x, min.y),
        },
        Rect {
            min: Vec2::new(surface.min.x, max.y),
            max: surface.max,
        },
        Rect {
            min: Vec2::new(surface.min.x, min.y),
            max: Vec2::new(min.x, max.y),
        },
        Rect {
            min: Vec2::new(max.x, min.y),
            max: Vec2::new(surface.max.x, max.y),
        },
    ]
    .into_iter()
    .filter(|rect| rect.width() > 0.0 && rect.height() > 0.0)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::protocol::{BridgeKindId, CarrierId};

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
}
