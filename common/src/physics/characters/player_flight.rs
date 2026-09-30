use bevy_math::Vec3;

use super::player_control::accelerate_player;
use crate::{
    config::{CharacterPhysicsConfig, PlayerMovementConfig, PortalFunnelConfig},
    constants::CHARACTER_TERMINAL_VELOCITY,
    physics::{
        CharacterHopBody, CharacterPortalHop, CollisionWorld, FunnelPrediction, PortalSet, floor_funnel_prediction,
        portals::FunnelStep,
    },
};

// A body in open air: feet position, horizontal velocity, vertical velocity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerFlightState {
    pub position: Vec3,
    pub horizontal_velocity: Vec3,
    pub vertical_velocity: f32,
}

// The gates a flight can be funnelled into and hop through.
#[derive(Clone, Copy)]
pub struct PlayerFlightPortals<'a> {
    pub set: &'a PortalSet,
    pub world: &'a CollisionWorld,
    pub funnel: PortalFunnelConfig,
    pub body: CharacterPhysicsConfig,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayerFlightTick {
    // After the hop when one happened.
    pub state: PlayerFlightState,
    // Feet at the end of the move, before any hop.
    pub arrived: Vec3,
    // What a landing during this tick reports.
    pub impact_speed: f32,
    pub funnelled: bool,
    pub hop: Option<CharacterPortalHop>,
}

// What `step_player_movement` and the owner's hop do when nothing is within
// reach: the same acceleration rule, gravity half-steps, funnel, and crossing
// without a world, for previews. A zero wish is released input, the only
// time the funnel acts. A hop that tilts the body crouches it about its
// centre, which is all this step reads, so the state stays the standing
// feet. `tests/player_flight.rs` holds it to the full step tick for tick.
#[must_use]
pub fn step_player_flight(
    state: PlayerFlightState,
    wish: Vec3,
    player: &PlayerMovementConfig,
    gravity: f32,
    delta: f32,
    portals: Option<&PlayerFlightPortals<'_>>,
) -> PlayerFlightTick {
    let horizontal = released_velocity(state, wish, player, delta);
    let correction = portals.filter(|_| wish == Vec3::ZERO).and_then(|portals| {
        portals.set.funnel_correction(FunnelStep {
            origin: state.position,
            physics: portals.body,
            velocity: horizontal.with_y(state.vertical_velocity),
            gravity,
            delta,
            config: portals.funnel,
            world: portals.world,
            passable_fields: &[],
        })
    });
    let falling = (state.vertical_velocity - gravity * delta * 0.5).max(-CHARACTER_TERMINAL_VELOCITY);
    let arrived =
        state.position + Vec3::new(horizontal.x, falling, horizontal.z) * delta + correction.unwrap_or_default();
    let vertical = (falling - gravity * delta * 0.5).max(-CHARACTER_TERMINAL_VELOCITY);
    let hop = portals.and_then(|portals| {
        portals.set.character_hop(
            state.position,
            arrived,
            portals.body,
            CharacterHopBody {
                knockback: Vec3::ZERO,
                horizontal_velocity: horizontal,
                vertical_velocity: vertical,
                yaw: 0.0,
            },
            0.0,
        )
    });
    PlayerFlightTick {
        state: hop.map_or(
            PlayerFlightState {
                position: arrived,
                horizontal_velocity: horizontal,
                vertical_velocity: vertical,
            },
            |hop| PlayerFlightState {
                position: hop.origin,
                horizontal_velocity: hop.horizontal_velocity,
                vertical_velocity: hop.vertical_velocity,
            },
        ),
        arrived,
        impact_speed: (-state.vertical_velocity.min(falling)).max(0.0),
        funnelled: correction.is_some(),
        hop,
    }
}

// The prediction the funnel would make this tick, with input released, for a
// floor portal centred on `plane_point`.
#[must_use]
pub fn flight_funnel_prediction(
    state: PlayerFlightState,
    player: &PlayerMovementConfig,
    gravity: f32,
    delta: f32,
    body: CharacterPhysicsConfig,
    plane_point: Vec3,
) -> Option<FunnelPrediction> {
    let velocity = released_velocity(state, Vec3::ZERO, player, delta).with_y(state.vertical_velocity);
    floor_funnel_prediction(plane_point, state.position, body, velocity, gravity)
}

fn released_velocity(state: PlayerFlightState, wish: Vec3, player: &PlayerMovementConfig, delta: f32) -> Vec3 {
    accelerate_player(
        state.horizontal_velocity,
        Vec3::ZERO,
        wish,
        state.vertical_velocity,
        false,
        player,
        delta,
    )
}
