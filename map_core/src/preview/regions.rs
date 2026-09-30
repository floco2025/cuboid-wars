use std::f32::consts::{FRAC_PI_2, TAU};

use bevy_math::{Vec2, Vec3, Vec3Swizzles};
use common::physics::{PortalFrame, flight_funnel_prediction};

use super::{
    jump::PREVIEW_STEERING_DIRECTIONS,
    polygons::{Rectangle, area, clip, convex_hull, yaw_wedge},
    trajectory::{Air, Crossing, Origin, Steering},
};

// Slivers thinner than a drawn line are not regions.
const MIN_PIECE_AREA: f32 = 1e-4;

// The part of a region a floor portal shot from one quarter turn covers.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Piece {
    pub yaw: f32,
    pub polygon: Vec<Vec2>,
}

// A floor portal's yaw snaps to the quarter turn its shooter faces, so each
// quarter has its own aperture axes and its own wedge of centres, seen from
// the shooter, that are placed with them.
fn quarter_pieces(air: &Air<'_>, grow: f32, shooter: Vec2, region: impl Fn(&Rectangle) -> Vec<Vec2>) -> Vec<Piece> {
    let size = air.physics.portal_size;
    (0..4)
        .filter_map(|quarter| {
            let yaw = quarter as f32 * FRAC_PI_2;
            let frame = PortalFrame::from_surface(Vec3::ZERO, Vec3::Y, yaw, size);
            let aperture = Rectangle {
                center: Vec2::ZERO,
                across: frame.right.xz(),
                half_across: size.half_width() + grow,
                along: frame.up.xz(),
                half_along: size.half_height() + grow,
            };
            let polygon = clip(&region(&aperture), &yaw_wedge(shooter, yaw));
            (polygon.len() >= 3 && area(&polygon) > MIN_PIECE_AREA).then_some(Piece { yaw, polygon })
        })
        .collect()
}

// The floor-portal centres on a crossed level that take a released flight
// in. With the funnel, that is the game's own capture rule inverted for the
// last airborne tick, which then carries the body onto the centre: the
// margin rectangle about the body, cut by the same rectangle about its
// predicted arrival. Earlier ticks add nothing, because the body and its
// arrival only close in on each other along a straight flight. Without a
// funnel it is the plain aperture about the point the body comes down on,
// cut by the aperture about the point its centre sinks through.
pub(super) fn capture(air: &Air<'_>, crossing: &Crossing, centre: Option<Vec2>, shooter: Vec2) -> Vec<Piece> {
    let physics = air.physics;
    let margin = physics.funnel.capture_margin;
    let (first, second) = if margin > 0.0 {
        let Some(prediction) = flight_funnel_prediction(
            crossing.from.state,
            &physics.player,
            air.gravity(),
            physics.tick(),
            physics.character(),
            Vec3::Y * air.heights[crossing.level],
        ) else {
            return Vec::new();
        };
        (prediction.offset.xz(), prediction.arrival.xz())
    } else {
        let Some(centre) = centre else {
            return Vec::new();
        };
        (crossing.arrived, centre)
    };
    quarter_pieces(air, margin, shooter, |aperture| {
        clip(&aperture.at(first).corners(), &aperture.at(second).sides())
    })
}

pub(super) struct SteeringRegion {
    pub level: usize,
    pub landings: Vec<Vec2>,
    pub capture: Vec<Piece>,
}

// Where steering can bring a flight down on each level: the hull of the
// crossings of input held in each of a ring of fixed directions, and of
// released input. An inner estimate, since input may also change mid-flight.
pub(super) fn steering_regions(air: &Air<'_>, origin: Origin, shooter: Option<Vec2>) -> Vec<SteeringRegion> {
    let mut points: Vec<Vec<Vec2>> = vec![Vec::new(); air.heights.len()];
    let mut captures: Vec<Vec<Piece>> = vec![Vec::new(); air.heights.len()];
    let wishes = (0..PREVIEW_STEERING_DIRECTIONS).map(|index| {
        let (sin, cos) = (index as f32 * TAU / PREVIEW_STEERING_DIRECTIONS as f32).sin_cos();
        Steering::Constant(Vec3::new(sin, 0.0, cos) * air.speed())
    });
    for steering in wishes.chain([Steering::Released]) {
        for crossing in air.fly(origin, steering, None).crossings {
            if crossing.phase == origin.phase {
                points[crossing.level].push(crossing.point);
                if let Some(shooter) = shooter {
                    let released = air.fly(crossing.from, Steering::Released, None);
                    if let Some(landing) = released
                        .crossings
                        .iter()
                        .find(|landing| landing.level == crossing.level)
                    {
                        let centre = released
                            .centres
                            .iter()
                            .find_map(|(level, point)| (*level == crossing.level).then_some(*point));
                        // Keep the union: a hull would invent captures between sampled flights.
                        for piece in capture(air, landing, centre, shooter) {
                            if !captures[crossing.level].contains(&piece) {
                                captures[crossing.level].push(piece);
                            }
                        }
                    }
                }
            }
        }
    }
    points
        .into_iter()
        .zip(captures)
        .enumerate()
        .filter(|(_, (points, _))| !points.is_empty())
        .map(|(level, (points, capture))| SteeringRegion {
            level,
            landings: convex_hull(&points),
            capture,
        })
        .collect()
}
