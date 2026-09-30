use bevy_math::{Vec2, Vec3, Vec3Swizzles};
use common::{
    constants::{CHARACTER_CONTACT_OFFSET, PORTAL_STANDABLE_NORMAL_Y},
    physics::{
        PlayerFlightPortals, PlayerFlightState, PlayerFlightTick, PortalFrame, in_character_aperture, landing_damage,
        step_player_flight,
    },
};
use serde::Serialize;

use super::{
    jump::PREVIEW_MAX_SECS,
    physics::{PreviewPhysics, Scenario},
};

// Levels and gate planes this close count as the same height.
const SAME_HEIGHT: f32 = 0.01;

// The open air one scenario flies through. `heights` are the floor tops a
// flight reports crossing, indexed by level.
pub(super) struct Air<'a> {
    pub physics: &'a PreviewPhysics,
    pub scenario: Scenario,
    pub heights: &'a [f32],
}

// The placed pair, as the game's own funnel and hop see it.
pub(super) struct Gates<'a> {
    pub portals: PlayerFlightPortals<'a>,
    pub entry: PortalFrame,
    // `None` pairs the entry with an unreachable stand-in: the flight ends at the entrance.
    pub exit: Option<PortalFrame>,
}

// Input over a flight. After a hop every flight is released.
#[derive(Debug, Clone, Copy)]
pub(super) enum Steering {
    Released,
    Constant(Vec3),
    // Released whenever the funnel engages, otherwise held toward the entry's centre.
    Aperture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    BeforeEntry,
    AfterExit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum End {
    Below,
    TimeCap,
    Entered,
    Reentered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    // Flown without gates.
    Free,
    Entered,
    // Met the exit gate first.
    Reversed,
    Missed,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Origin {
    pub state: PlayerFlightState,
    pub time: f32,
    pub phase: Phase,
}

// The feet descending through a level's height, interpolated inside the tick.
#[derive(Debug, Clone, Copy)]
pub(super) struct Crossing {
    pub level: usize,
    pub phase: Phase,
    pub point: Vec2,
    pub time: f32,
    // Fraction of full health.
    pub damage: f32,
    // The tick it happened in: the state it began with and where it moved the body.
    pub from: Origin,
    pub arrived: Vec2,
}

#[derive(Debug)]
pub(super) struct Flight {
    // Feet, one per tick; a hop adds its entrance and its exit.
    pub points: Vec<Vec3>,
    // Index of the entrance in `points`, and when it was reached.
    pub hop: Option<(usize, f32)>,
    pub end: End,
    pub crossings: Vec<Crossing>,
    // Where the body centre passed each level's height before any entry.
    pub centres: Vec<(usize, Vec2)>,
    pub outcome: Outcome,
    pub exit: Option<Origin>,
}

impl Flight {
    fn crossed(&self, level: usize, phase: Phase) -> bool {
        self.crossings
            .iter()
            .any(|crossing| crossing.level == level && crossing.phase == phase)
    }

    // A body a wall gate lets out at its rim is standing on that level's
    // floor: the hop lifts a low body into the aperture, which leaves its
    // feet a contact offset under the floor top, never to descend through it.
    fn record_emergence(&mut self, air: &Air<'_>, origin: Origin) {
        let feet = origin.state.position;
        if origin.state.vertical_velocity > 0.0 {
            return;
        }
        for (level, &height) in air.heights.iter().enumerate() {
            if feet.y <= height && feet.y >= height - CHARACTER_CONTACT_OFFSET - 1e-4 {
                self.crossings.push(Crossing {
                    level,
                    phase: origin.phase,
                    point: feet.xz(),
                    time: origin.time,
                    damage: air.damage(-origin.state.vertical_velocity),
                    from: origin,
                    arrived: feet.xz(),
                });
            }
        }
    }
}

impl Air<'_> {
    pub fn gravity(&self) -> f32 {
        self.physics.gravity_for(self.scenario)
    }

    pub fn speed(&self) -> f32 {
        self.physics.speed_for(self.scenario)
    }

    fn damage(&self, impact_speed: f32) -> f32 {
        landing_damage(
            impact_speed,
            self.physics.gravity,
            &self.physics.player_fall,
            self.physics.max_health,
        )
        .map_or(0.0, |damage| damage / self.physics.max_health)
    }

    fn step(&self, state: PlayerFlightState, steering: Steering, gates: Option<&Gates<'_>>) -> PlayerFlightTick {
        let portals = gates.map(|gates| &gates.portals);
        let step = |wish| {
            step_player_flight(
                state,
                wish,
                &self.physics.player,
                self.gravity(),
                self.physics.tick(),
                portals,
            )
        };
        match (steering, gates) {
            (Steering::Constant(wish), _) => step(wish),
            (Steering::Aperture, Some(gates)) => {
                let released = step(Vec3::ZERO);
                if released.funnelled || released.hop.is_some() {
                    return released;
                }
                self.aim(state, &gates.entry)
                    .try_normalize()
                    .map_or(released, |direction| step(direction * self.speed()))
            }
            _ => step(Vec3::ZERO),
        }
    }

    // Where input should push to bring the body into the aperture: straight
    // at a wall gate, and for a floor gate by the gap between its centre and
    // the point the current drift comes down on its plane, so a body that
    // would overshoot brakes instead of flying past.
    fn aim(&self, state: PlayerFlightState, entry: &PortalFrame) -> Vec3 {
        let to_centre = (entry.center - state.position).with_y(0.0);
        let height = state.position.y - entry.center.y;
        if entry.normal.y <= PORTAL_STANDABLE_NORMAL_Y || height <= 0.0 {
            return to_centre;
        }
        let (gravity, rising) = (self.gravity(), state.vertical_velocity);
        let time = if gravity > 0.0 {
            (rising + (rising * rising + 2.0 * gravity * height).sqrt()) / gravity
        } else if rising < 0.0 {
            height / -rising
        } else {
            return to_centre;
        };
        to_centre - state.horizontal_velocity * time
    }

    // An empty world never stops a body, so a flight ends once it is falling
    // below everything it was asked about.
    fn lowest(&self, gates: Option<&Gates<'_>>) -> Option<f32> {
        let centre = self.physics.centre_offset().y;
        let size = self.physics.portal_size;
        let reach = size.half_width().max(size.half_height());
        self.heights
            .iter()
            .copied()
            .chain(
                gates
                    .into_iter()
                    .flat_map(|gates| [Some(gates.entry), gates.exit])
                    .flatten()
                    .map(|frame| frame.center.y - reach),
            )
            .reduce(f32::min)
            .map(|lowest| lowest - centre - SAME_HEIGHT)
    }

    pub fn fly(&self, origin: Origin, steering: Steering, gates: Option<&Gates<'_>>) -> Flight {
        let dt = self.physics.tick();
        let centre = self.physics.centre_offset();
        let lowest = self.lowest(gates);
        let mut flight = Flight {
            points: vec![origin.state.position],
            hop: None,
            end: End::TimeCap,
            crossings: Vec::new(),
            centres: Vec::new(),
            outcome: if gates.is_some() {
                Outcome::Missed
            } else {
                Outcome::Free
            },
            exit: None,
        };
        let Origin {
            mut state,
            mut time,
            mut phase,
        } = origin;
        if phase == Phase::AfterExit {
            flight.record_emergence(self, origin);
        }
        // Where the feet last rose through each level's height since the takeoff or the exit.
        let mut rises: Vec<Option<Vec2>> = vec![None; self.heights.len()];
        while time < PREVIEW_MAX_SECS {
            let steering = if flight.hop.is_some() {
                Steering::Released
            } else {
                steering
            };
            let tick = self.step(state, steering, gates);
            let (from, to) = (state.position, tick.arrived);
            for (level, &height) in self.heights.iter().enumerate() {
                // Launching from a floor is not passing through its underside.
                if from.y <= height && to.y > height && !(time == origin.time && from.y == height) {
                    rises[level] = Some(from.lerp(to, (height - from.y) / (to.y - from.y)).xz());
                }
                if from.y > height && to.y <= height && !flight.crossed(level, phase) {
                    let fraction = (from.y - height) / (from.y - to.y);
                    let point = from.lerp(to, fraction).xz();
                    // Coming down where it went up is no landing: a floor
                    // there would have stopped the body from below.
                    if rises[level].is_none_or(|rise| rise.distance(point) >= self.physics.body.diameter) {
                        flight.crossings.push(Crossing {
                            level,
                            phase,
                            point,
                            time: time + dt * fraction,
                            damage: self.damage(tick.impact_speed),
                            from: Origin { state, time, phase },
                            arrived: to.xz(),
                        });
                    }
                }
                let (above, below) = (from.y + centre.y, to.y + centre.y);
                if phase == Phase::BeforeEntry
                    && above > height
                    && below <= height
                    && flight.centres.iter().all(|(crossed, _)| *crossed != level)
                {
                    let fraction = (above - height) / (above - below);
                    flight.centres.push((level, from.lerp(to, fraction).xz()));
                }
            }
            time += dt;
            flight.points.push(to);
            if let Some(hop) = tick.hop {
                // Sinking into a floor gate is inside the portal, not a landing beside it.
                if hop.entry.normal.y > PORTAL_STANDABLE_NORMAL_Y {
                    let plane = hop.entry.center.y + SAME_HEIGHT;
                    flight
                        .crossings
                        .retain(|crossing| crossing.phase != phase || self.heights[crossing.level] > plane);
                }
                if flight.hop.is_some() {
                    flight.end = End::Reentered;
                    return flight;
                }
                flight.hop = Some((flight.points.len() - 1, time));
                let entered = gates.is_some_and(|gates| same_gate(&hop.entry, &gates.entry));
                flight.outcome = if entered { Outcome::Entered } else { Outcome::Reversed };
                if entered && gates.is_some_and(|gates| gates.exit.is_none()) {
                    flight.end = End::Entered;
                    return flight;
                }
                flight.points.push(tick.state.position);
                phase = Phase::AfterExit;
                let exit = Origin {
                    state: tick.state,
                    time,
                    phase,
                };
                flight.exit = Some(exit);
                flight.record_emergence(self, exit);
                rises.fill(None);
            } else if flight.hop.is_none() && gates.is_some_and(|gates| misses(gates, centre, from, to)) {
                return flight;
            }
            state = tick.state;
            if state.vertical_velocity < 0.0 && lowest.is_some_and(|lowest| state.position.y < lowest) {
                flight.end = End::Below;
                return flight;
            }
        }
        flight
    }
}

fn same_gate(a: &PortalFrame, b: &PortalFrame) -> bool {
    a.center.distance_squared(b.center) < 1e-6 && a.normal.dot(b.normal) > 0.5
}

// What the floor or wall around the entry would have done to a move that
// did not hop: the centre went through the plane beside the aperture, or the
// feet reached a floor gate's plane with the centre outside it, which is
// when the game stops treating the backing as a hole. A body over the exit
// gate is sinking into that one instead.
fn misses(gates: &Gates<'_>, centre: Vec3, from: Vec3, to: Vec3) -> bool {
    let entry = &gates.entry;
    let offset = |feet: Vec3| feet + centre - entry.center;
    let (before, after) = (offset(from).dot(entry.normal), offset(to).dot(entry.normal));
    if before > 0.0 && after <= 0.0 {
        return true;
    }
    entry.normal.y > PORTAL_STANDABLE_NORMAL_Y
        && from.y > entry.center.y
        && to.y <= entry.center.y
        && !in_character_aperture(offset(to), entry)
        && !gates
            .exit
            .is_some_and(|exit| in_character_aperture(to + centre - exit.center, &exit))
}
