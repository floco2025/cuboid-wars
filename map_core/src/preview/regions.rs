use std::f32::consts::{FRAC_PI_2, TAU};

use bevy_math::{Vec2, Vec3, Vec3Swizzles};
use common::physics::{PortalFrame, flight_funnel_prediction};

use super::{
    jump::{PREVIEW_MAX_SECS, PREVIEW_STEERING_DIRECTIONS},
    polygons::{Box2, Rectangle, area, boxes_union, clip, convex_hull, simplify, yaw_wedge},
    trajectory::{Air, Crossing, Origin, Steering},
};

// Slivers thinner than a drawn line are not regions.
const MIN_PIECE_AREA: f32 = 1e-4;
// Steps finer than this in a steered outline are chamfered away, in metres.
const STEERED_SMOOTHING: f32 = 0.02;

// The part of a region a floor portal shot from one quarter turn covers.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Piece {
    pub yaw: f32,
    pub polygon: Vec<Vec2>,
}

// What a flight is caught by on one level: either the funnel's catch
// rectangles, each about where a release would come down and grown by the
// margin for the time left, or, without a funnel, the plain aperture about
// the point the body comes down on cut by the aperture about the point its
// centre sinks through.
#[derive(Debug, Clone)]
pub(super) enum Catch {
    Funnel(Vec<(Vec2, f32)>),
    Direct { arrived: Vec2, centre: Vec2 },
}

// A floor portal's yaw snaps to the quarter turn its shooter faces, so each
// quarter has its own aperture axes and its own wedge of centres, seen from
// the shooter, that are placed with them. The catches of every flight are
// united per quarter, exactly; `smoothing` then chamfers fine staircases.
pub(super) fn pieces(air: &Air<'_>, shooter: Vec2, catches: &[Catch], smoothing: f32) -> Vec<Piece> {
    let size = air.physics.portal_size;
    (0..4)
        .flat_map(|quarter| {
            let yaw = quarter as f32 * FRAC_PI_2;
            let frame = PortalFrame::from_surface(Vec3::ZERO, Vec3::Y, yaw, size);
            let aperture = Rectangle {
                center: Vec2::ZERO,
                across: frame.right.xz(),
                half_across: size.half_width(),
                along: frame.up.xz(),
                half_along: size.half_height(),
            };
            let boxes: Vec<Box2> = catches
                .iter()
                .flat_map(|catch| match catch {
                    Catch::Funnel(rectangles) => rectangles
                        .iter()
                        .map(|&(centre, margin)| aperture.at(centre).grown(margin).bounds())
                        .collect::<Vec<_>>(),
                    Catch::Direct { arrived, centre } => {
                        let (a, b) = (aperture.at(*arrived).bounds(), aperture.at(*centre).bounds());
                        let overlap = Box2 {
                            low: a.low.max(b.low),
                            high: a.high.min(b.high),
                        };
                        (overlap.low.cmplt(overlap.high).all())
                            .then_some(overlap)
                            .into_iter()
                            .collect()
                    }
                })
                .collect();
            boxes_union(&boxes)
                .into_iter()
                .map(|outline| {
                    outline
                        .into_iter()
                        .map(|local| aperture.world(local))
                        .collect::<Vec<_>>()
                })
                .map(|polygon| simplify(clip(&polygon, &yaw_wedge(shooter, yaw)), smoothing))
                .filter(|polygon| polygon.len() >= 3 && area(polygon) > MIN_PIECE_AREA)
                .map(|polygon| Piece { yaw, polygon })
                .collect::<Vec<_>>()
        })
        .collect()
}

// What catches a flight from `origin` under `wish` on each level, by the
// game's own capture rule inverted: on every tick, a release now would come
// down within the margin for the time left of the predicted arrival, and
// the flight is caught the first time a centre is inside one of those
// rectangles, so their union is the region.
pub(super) fn catches(air: &Air<'_>, origin: Origin, wish: Vec3) -> Vec<(usize, Catch)> {
    let physics = air.physics;
    let steering = if wish == Vec3::ZERO {
        Steering::Released
    } else {
        Steering::Constant(wish)
    };
    let flight = air.fly(origin, steering, None);
    let crossings: Vec<&Crossing> = flight
        .crossings
        .iter()
        .filter(|crossing| crossing.phase == origin.phase)
        .collect();
    if !physics.funnel.assists() {
        return crossings
            .iter()
            .filter_map(|crossing| {
                let centre = flight
                    .centres
                    .iter()
                    .find_map(|(level, point)| (*level == crossing.level).then_some(*point))?;
                Some((
                    crossing.level,
                    Catch::Direct {
                        arrived: crossing.arrived,
                        centre,
                    },
                ))
            })
            .collect();
    }
    // Every level's catches, tick by tick until the feet reach it.
    let mut catches: Vec<(usize, Vec<(Vec2, f32)>)> = crossings.iter().map(|c| (c.level, Vec::new())).collect();
    let mut open: Vec<bool> = vec![true; catches.len()];
    let mut state = origin.state;
    let mut time = origin.time;
    while open.iter().any(|open| *open) && time < PREVIEW_MAX_SECS {
        let tick = air.step(state, steering, None);
        for (index, (level, rectangles)) in catches.iter_mut().enumerate() {
            if !open[index] {
                continue;
            }
            let height = air.heights[*level];
            if let Some(prediction) = flight_funnel_prediction(
                state,
                &physics.player,
                air.gravity(),
                physics.tick(),
                physics.character(),
                Vec3::Y * height,
            ) {
                rectangles.push((prediction.arrival.xz(), physics.funnel.margin_at(prediction.time)));
            }
            open[index] = !(state.position.y > height && tick.arrived.y <= height);
        }
        state = tick.state;
        time += physics.tick();
    }
    catches
        .into_iter()
        .filter(|(_, rectangles)| !rectangles.is_empty())
        .map(|(level, rectangles)| (level, Catch::Funnel(rectangles)))
        .collect()
}

// A released flight's capture regions by level, exact.
pub(super) fn captures(air: &Air<'_>, origin: Origin, shooter: Vec2) -> Vec<(usize, Vec<Piece>)> {
    catches(air, origin, Vec3::ZERO)
        .into_iter()
        .map(|(level, catch)| (level, pieces(air, shooter, &[catch], 0.0)))
        .collect()
}

pub(super) struct SteeringRegion {
    pub level: usize,
    pub landings: Vec<Vec2>,
    pub capture: Vec<Piece>,
}

// Where steering can bring a flight down on each level: the hull of the
// crossings of input held in each of a ring of fixed directions, and of
// released input. An inner estimate, since input may also change mid-flight.
// With a shooter, also the floor-portal centres those flights are caught
// by once released, whenever on the way that is.
pub(super) fn steering_regions(air: &Air<'_>, origin: Origin, shooter: Option<Vec2>) -> Vec<SteeringRegion> {
    let mut points: Vec<Vec<Vec2>> = vec![Vec::new(); air.heights.len()];
    let mut catches: Vec<Vec<Catch>> = vec![Vec::new(); air.heights.len()];
    let wishes = (0..PREVIEW_STEERING_DIRECTIONS).map(|index| {
        let (sin, cos) = (index as f32 * TAU / PREVIEW_STEERING_DIRECTIONS as f32).sin_cos();
        Vec3::new(sin, 0.0, cos) * air.speed()
    });
    for wish in wishes.chain([Vec3::ZERO]) {
        let steering = if wish == Vec3::ZERO {
            Steering::Released
        } else {
            Steering::Constant(wish)
        };
        for crossing in air.fly(origin, steering, None).crossings {
            if crossing.phase == origin.phase {
                points[crossing.level].push(crossing.point);
            }
        }
        if shooter.is_some() {
            for (level, catch) in self::catches(air, origin, wish) {
                catches[level].push(catch);
            }
        }
    }
    points
        .into_iter()
        .zip(catches)
        .enumerate()
        .filter(|(_, (points, _))| !points.is_empty())
        .map(|(level, (points, catches))| SteeringRegion {
            level,
            landings: convex_hull(&points),
            capture: shooter.map_or_else(Vec::new, |shooter| pieces(air, shooter, &catches, STEERED_SMOOTHING)),
        })
        .collect()
}
