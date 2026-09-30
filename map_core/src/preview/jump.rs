use anyhow::{Result, ensure};
use bevy_math::{Vec2, Vec3};
use common::{
    config::PortalFunnelConfig,
    map::Carriers,
    physics::{CollisionWorld, PlayerFlightPortals, PlayerFlightState, PortalFrame, PortalSet, portal_placement_yaw},
    protocol::{CarrierId, MapLayout, Portal, PortalEnd, PortalPairId, Position},
};
use serde::{Deserialize, Serialize};

use super::{
    physics::{PreviewPhysics, SCENARIOS, Scenario, SurfaceSpec},
    regions::{Piece, captures, steering_regions},
    trajectory::{Air, End, Flight, Gates, Origin, Outcome, Phase, Steering},
};

// Fixed input directions that stand in for steering when a range is drawn.
pub const PREVIEW_STEERING_DIRECTIONS: usize = 16;
// A flight gravity never brings down ends here.
pub const PREVIEW_MAX_SECS: f32 = 30.0;
pub const PREVIEW_MAX_HEIGHTS: usize = 64;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JumpRequest {
    pub takeoff: TakeoffSpec,
    // Floor tops by level.
    pub heights: Vec<f32>,
    pub air_control: bool,
    // Where portal 1 is shot from, `[x, z]`: it decides a floor portal's quarter turn.
    pub shooter: [f32; 2],
    pub portals: Option<PortalsSpec>,
}

// The edge point a flight leaves from, heading `direction` at full speed and
// with input released from then on. A step walks off it, a jump leaves the
// ground `margin` seconds before it, both from where the body's support ends;
// a negative margin, down to the coyote window, is a jump pressed that long
// after the edge, from where the fall has taken the body by then.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakeoffSpec {
    pub point: [f32; 3],
    pub direction: [f32; 2],
    pub jumping: bool,
    pub margin: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalsSpec {
    pub entry: SurfaceSpec,
    pub exit: Option<SurfaceSpec>,
}

#[derive(Debug, Serialize)]
pub struct JumpPreview {
    pub scenarios: Vec<ScenarioPreview>,
    // How far past a slab's edge the body still stands, in metres: a flight
    // leaves that far beyond the takeoff edge and lands that close to a slab.
    pub edge_reach: f64,
}

// How a flight met portal 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
    // Would enter even without the funnel.
    Direct,
    // Enters because the funnel draws it in.
    Funnel,
    // Enters only when steered at the aperture; that flight is the path.
    Steered,
    // The path is the free flight.
    Missed,
    // Met portal 2 first and left through portal 1.
    Reversed,
}

#[derive(Debug, Serialize)]
pub struct ScenarioPreview {
    // Feet per tick, `[x, y, z]`; `path[hop]` is the entrance and the next point the exit.
    pub path: Vec<[f64; 3]>,
    pub hop: Option<usize>,
    pub hop_time: Option<f64>,
    pub end: End,
    pub crossings: Vec<CrossingPreview>,
    pub entry: Option<Entry>,
    // Without portals: the floor-portal centres that take the flight in, by level.
    pub capture: Vec<LevelPieces>,
    // With air control, without portals: where steering lands, and the centres it reaches.
    pub range: Vec<LevelPolygon>,
    pub capture_steered: Vec<LevelPieces>,
    // After a hop, with air control or when the flight falls back into a portal: where steering lands.
    pub exit_range: Vec<LevelPolygon>,
}

#[derive(Debug, Serialize)]
pub struct CrossingPreview {
    pub level: usize,
    pub phase: Phase,
    pub point: [f64; 2],
    pub time: f64,
    pub damage: f64,
}

#[derive(Debug, Serialize)]
pub struct LevelPolygon {
    pub level: usize,
    pub polygon: Vec<[f64; 2]>,
}

#[derive(Debug, Serialize)]
pub struct LevelPieces {
    pub level: usize,
    pub pieces: Vec<PiecePreview>,
}

#[derive(Debug, Serialize)]
pub struct PiecePreview {
    pub yaw: f64,
    pub polygon: Vec<[f64; 2]>,
}

// The pair as the game builds it, on a world with nothing else in it.
struct Pair {
    set: PortalSet,
    world: CollisionWorld,
    entry: PortalFrame,
    exit: Option<PortalFrame>,
}

impl Pair {
    fn new(physics: &PreviewPhysics, portals: &PortalsSpec) -> Self {
        let record = |surface: &SurfaceSpec, end| {
            let normal = Vec3::from_array(surface.normal);
            Portal {
                pair: PortalPairId(0),
                end,
                pos: Position::from(Vec3::from_array(surface.center)),
                nx: normal.x,
                ny: normal.y,
                nz: normal.z,
                yaw: portal_placement_yaw(normal, surface.yaw),
                carrier: CarrierId::WORLD,
            }
        };
        let unreachable = SurfaceSpec {
            center: [1.0e6; 3],
            normal: [0.0, 1.0, 0.0],
            yaw: 0.0,
        };
        let records = [
            record(&portals.entry, PortalEnd::A),
            record(portals.exit.as_ref().unwrap_or(&unreachable), PortalEnd::B),
        ];
        let world = CollisionWorld::from_map_layout(&MapLayout::default());
        Self {
            set: PortalSet::rebuild(&records, &world, &Carriers::default(), physics.portal_size),
            world,
            entry: physics.frame(&portals.entry),
            exit: portals.exit.as_ref().map(|exit| physics.frame(exit)),
        }
    }

    fn gates(&self, physics: &PreviewPhysics, funnel: PortalFunnelConfig) -> Gates<'_> {
        Gates {
            portals: PlayerFlightPortals {
                set: &self.set,
                world: &self.world,
                funnel,
                body: physics.character(),
            },
            entry: self.entry,
            exit: self.exit,
        }
    }
}

pub fn jump_preview(physics: &PreviewPhysics, request: &JumpRequest) -> Result<JumpPreview> {
    let takeoff = &request.takeoff;
    ensure!(
        takeoff
            .point
            .iter()
            .chain(&takeoff.direction)
            .all(|value| value.is_finite()),
        "takeoff must be finite"
    );
    let direction = Vec2::from_array(takeoff.direction)
        .try_normalize()
        .ok_or_else(|| anyhow::anyhow!("takeoff.direction must not be zero"))?;
    ensure!(
        takeoff.margin.is_finite() && takeoff.margin >= -physics.coyote_secs,
        "takeoff.margin must be finite and at least -{}",
        physics.coyote_secs
    );
    ensure!(
        request.heights.len() <= PREVIEW_MAX_HEIGHTS && request.heights.iter().all(|height| height.is_finite()),
        "heights must be at most {PREVIEW_MAX_HEIGHTS} finite numbers"
    );
    ensure!(
        request.shooter.iter().all(|value| value.is_finite()),
        "shooter must be finite"
    );
    if let Some(portals) = &request.portals {
        portals.entry.validate("portals.entry")?;
        if let Some(exit) = &portals.exit {
            exit.validate("portals.exit")?;
        }
    }
    let pair = request.portals.as_ref().map(|portals| Pair::new(physics, portals));
    let direction = Vec3::new(direction.x, 0.0, direction.y);
    Ok(JumpPreview {
        scenarios: SCENARIOS
            .iter()
            .map(|&scenario| scenario_preview(physics, request, direction, scenario, pair.as_ref()))
            .collect(),
        edge_reach: rounded(physics.edge_reach()),
    })
}

fn scenario_preview(
    physics: &PreviewPhysics,
    request: &JumpRequest,
    direction: Vec3,
    scenario: Scenario,
    pair: Option<&Pair>,
) -> ScenarioPreview {
    let air = Air {
        physics,
        scenario,
        heights: &request.heights,
    };
    let takeoff = &request.takeoff;
    // Support ends a body's reach past the edge, and that is where the flight begins.
    let edge = Vec3::from_array(takeoff.point) + direction * physics.edge_reach();
    let velocity = direction * air.speed();
    let origin = Origin {
        state: if takeoff.jumping {
            let late = (-takeoff.margin).max(0.0);
            PlayerFlightState {
                position: edge - velocity * takeoff.margin - Vec3::Y * (0.5 * air.gravity() * late * late),
                horizontal_velocity: velocity,
                vertical_velocity: physics.player.jump_speed,
            }
        } else {
            PlayerFlightState {
                position: edge,
                horizontal_velocity: velocity,
                vertical_velocity: 0.0,
            }
        },
        time: 0.0,
        phase: Phase::BeforeEntry,
    };
    let shooter = Vec2::from_array(request.shooter);
    let Some(pair) = pair else {
        let flight = air.fly(origin, Steering::Released, None);
        let capture = captures(&air, origin, shooter)
            .into_iter()
            .map(|(level, pieces)| level_pieces(level, pieces))
            .filter(|level| !level.pieces.is_empty())
            .collect();
        let regions = if request.air_control {
            steering_regions(&air, origin, Some(shooter))
        } else {
            Vec::new()
        };
        return ScenarioPreview {
            capture,
            capture_steered: regions
                .iter()
                .map(|region| level_pieces(region.level, region.capture.clone()))
                .filter(|level| !level.pieces.is_empty())
                .collect(),
            range: level_polygons(regions.into_iter().map(|region| (region.level, region.landings))),
            ..scenario_path(&flight, None)
        };
    };
    let gates = pair.gates(physics, physics.funnel);
    let released = air.fly(origin, Steering::Released, Some(&gates));
    let (flight, entry) = match released.outcome {
        Outcome::Entered => {
            let plain = pair.gates(
                physics,
                PortalFunnelConfig {
                    capture_margin: 0.0,
                    capture_growth: 0.0,
                },
            );
            let unaided = air.fly(origin, Steering::Released, Some(&plain)).outcome == Outcome::Entered;
            (released, if unaided { Entry::Direct } else { Entry::Funnel })
        }
        Outcome::Reversed => (released, Entry::Reversed),
        Outcome::Missed | Outcome::Free => {
            let steered = request
                .air_control
                .then(|| air.fly(origin, Steering::Aperture, Some(&gates)))
                .filter(|flight| flight.outcome == Outcome::Entered);
            match steered {
                Some(flight) => (flight, Entry::Steered),
                None => (air.fly(origin, Steering::Released, None), Entry::Missed),
            }
        }
    };
    // A flight that falls back into a portal lands only where it is steered.
    let exit_range = match flight.exit {
        Some(exit) if request.air_control || flight.end == End::Reentered => level_polygons(
            steering_regions(&air, exit, None)
                .into_iter()
                .map(|region| (region.level, region.landings)),
        ),
        _ => Vec::new(),
    };
    ScenarioPreview {
        exit_range,
        ..scenario_path(&flight, Some(entry))
    }
}

fn scenario_path(flight: &Flight, entry: Option<Entry>) -> ScenarioPreview {
    ScenarioPreview {
        path: flight
            .points
            .iter()
            .map(|point| point.to_array().map(rounded))
            .collect(),
        hop: flight.hop.map(|(index, _)| index),
        hop_time: flight.hop.map(|(_, time)| rounded(time)),
        end: flight.end,
        crossings: flight
            .crossings
            .iter()
            .map(|crossing| CrossingPreview {
                level: crossing.level,
                phase: crossing.phase,
                point: crossing.point.to_array().map(rounded),
                time: rounded(crossing.time),
                damage: f64::from(crossing.damage),
            })
            .collect(),
        entry,
        capture: Vec::new(),
        range: Vec::new(),
        capture_steered: Vec::new(),
        exit_range: Vec::new(),
    }
}

fn level_polygons(hulls: impl IntoIterator<Item = (usize, Vec<Vec2>)>) -> Vec<LevelPolygon> {
    hulls
        .into_iter()
        .map(|(level, hull)| LevelPolygon {
            level,
            polygon: polygon(&hull),
        })
        .collect()
}

fn level_pieces(level: usize, pieces: Vec<Piece>) -> LevelPieces {
    LevelPieces {
        level,
        pieces: pieces
            .into_iter()
            .map(|piece| PiecePreview {
                yaw: rounded(piece.yaw),
                polygon: polygon(&piece.polygon),
            })
            .collect(),
    }
}

fn polygon(points: &[Vec2]) -> Vec<[f64; 2]> {
    points.iter().map(|point| point.to_array().map(rounded)).collect()
}

// Millimetres and milliseconds are finer than anything drawn and keep the reply small.
fn rounded(value: f32) -> f64 {
    (f64::from(value) * 1000.0).round() / 1000.0
}

#[cfg(test)]
#[path = "tests/jump.rs"]
mod tests;
