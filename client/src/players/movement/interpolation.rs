use std::{
    collections::VecDeque,
    f32::consts::{PI, TAU},
};

use bevy::prelude::*;
use common::{
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterSupport, CharacterVerticalVelocity, KnockbackVelocity, player_control_velocity,
    },
    protocol::{
        CarrierId, FaceYaw, MapSettings, PlayerId, PlayerMove, PlayerMoveIntent, PlayerMovementState, Position,
        PowerUpKind, sequence_is_newer,
    },
};

use crate::players::{PlayerAnimationMotion, PlayerMap};

const MAX_SAMPLES: usize = 64;

#[derive(Component, Default)]
pub(crate) struct RemotePlayerMotion {
    samples: VecDeque<MovementSample>,
    cursor: f64,
    initial: Option<PlayerMovementState>,
}

#[derive(Clone, Copy)]
struct MovementSample {
    seq: u32,
    at: f64,
    portal_crossing: u32,
    movement: PlayerMovementState,
}

impl RemotePlayerMotion {
    pub(crate) fn new(movement: PlayerMovementState) -> Self {
        Self {
            initial: Some(movement),
            ..default()
        }
    }

    pub(crate) fn push(&mut self, entry: PlayerMove, delay_ticks: f64) {
        let at = if let Some(last) = self.samples.back() {
            if !sequence_is_newer(entry.seq, last.seq) {
                return;
            }
            last.at + f64::from(entry.seq.wrapping_sub(last.seq))
        } else {
            self.cursor = -delay_ticks;
            0.0
        };
        self.samples.push_back(MovementSample {
            seq: entry.seq,
            at,
            portal_crossing: entry.portal_crossing,
            movement: entry.movement,
        });
        if self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
    }

    fn advance(
        &mut self,
        delta_ticks: f64,
        carriers: &Carriers,
        alpha: f32,
        tick_secs: f64,
    ) -> Option<(PlayerMovementState, Vec3)> {
        let Some(newest) = self.samples.back().map(|sample| sample.at) else {
            return self
                .initial
                .map(|movement| (render_movement(movement, carriers, alpha), Vec3::ZERO));
        };
        // Playback never runs past a reported position, even when a packet is late.
        self.cursor = (self.cursor + delta_ticks).min(newest);
        while self.samples.get(1).is_some_and(|sample| sample.at <= self.cursor) {
            self.samples.pop_front();
        }
        let left = self.samples.front()?;
        let mut movement = render_movement(left.movement, carriers, alpha);
        let Some(right) = self.samples.get(1) else {
            return Some((movement, Vec3::ZERO));
        };
        if self.cursor < left.at || left.portal_crossing != right.portal_crossing {
            return Some((movement, Vec3::ZERO));
        }
        let span = right.at - left.at;
        let start = Vec3::from(movement.pos);
        let end = Vec3::from(render_movement(right.movement, carriers, alpha).pos);
        let alpha = ((self.cursor - left.at) / span) as f32;
        movement.pos = start.lerp(end, alpha).into();
        let yaw_delta = (right.movement.face_yaw - movement.face_yaw + PI).rem_euclid(TAU) - PI;
        movement.face_yaw += yaw_delta * alpha;
        movement.vertical_velocity += (right.movement.vertical_velocity - movement.vertical_velocity) * alpha;
        let velocity = (end - start) * (1.0 / (span * tick_secs)) as f32;
        Some((movement, velocity))
    }
}

fn render_movement(mut movement: PlayerMovementState, carriers: &Carriers, alpha: f32) -> PlayerMovementState {
    // Riders use the same rendered carrier pose as the platform, independently of sample delay.
    movement.pos = carriers
        .pose_between(movement.carrier, alpha)
        .transform_position(&movement.pos);
    movement.carrier = CarrierId::WORLD;
    movement
}

pub(crate) fn interpolate_remote_players_system(
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    carriers: Res<Carriers>,
    settings: Res<MapSettings>,
    players: Res<PlayerMap>,
    mut query: Query<(
        &PlayerId,
        &mut RemotePlayerMotion,
        &mut Position,
        &mut FaceYaw,
        &mut PlayerMoveIntent,
        &mut CharacterVerticalVelocity,
        &mut AirborneMomentum,
        &mut KnockbackVelocity,
        &mut CharacterSupport,
        &mut PlayerAnimationMotion,
    )>,
) {
    for (
        id,
        mut buffer,
        mut pos,
        mut yaw,
        mut intent,
        mut vertical,
        mut momentum,
        mut knockback,
        mut support,
        mut animation,
    ) in &mut query
    {
        let Some((movement, velocity)) = buffer.advance(
            time.delta_secs_f64() / fixed_time.timestep().as_secs_f64(),
            &carriers,
            fixed_time.overstep_fraction(),
            fixed_time.timestep().as_secs_f64(),
        ) else {
            continue;
        };
        *pos = movement.pos;
        yaw.0 = movement.face_yaw;
        *intent = movement.move_intent;
        vertical.0 = movement.vertical_velocity;
        momentum.0 = Vec3::from_array(movement.airborne_momentum);
        knockback.0 = Vec3::from_array(movement.knockback);
        *support = movement.support;
        let info = players.get(id);
        let control = player_control_velocity(
            *intent,
            &settings.movement,
            info.is_some_and(|info| info.power_up(PowerUpKind::Speed)),
            info.is_some_and(|info| info.stunned),
        );
        let direction = control.normalize_or_zero();
        let travelled = velocity - momentum.0 - knockback.0;
        let speed = travelled.dot(direction).clamp(0.0, control.length());
        *animation = PlayerAnimationMotion {
            support: *support,
            velocity: (direction * speed).with_y(vertical.0),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClientSettings;
    use common::config::NetworkConfig;
    use common::protocol::{Carrier, MapLayout, PlayerGeneration, UpdateCadence};

    fn sample(seq: u32, x: f32, portal_crossing: u32) -> PlayerMove {
        PlayerMove {
            id: PlayerId(2),
            generation: PlayerGeneration(1),
            seq,
            portal_crossing,
            movement: PlayerMovementState::new(
                Position { x, ..default() },
                PlayerMoveIntent::Running { direction: 0.0 },
                0.0,
                0.0,
            ),
        }
    }

    fn moving_platforms() -> Carriers {
        let platform = Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 1,
            from: Position {
                x: 100.0,
                y: 0.0,
                z: 0.0,
            },
            to: Position {
                x: 116.0,
                y: 8.0,
                z: 0.0,
            },
            travel_ticks: 30,
            pause_ticks: 9,
            phase_ticks: 0,
        };
        Carriers::from_layout(&MapLayout {
            carriers: vec![
                platform,
                Carrier {
                    parent: CarrierId(1),
                    from: Position::default(),
                    to: Position { x: 0.0, y: 0.0, z: 4.0 },
                    phase_ticks: 5,
                    ..platform
                },
            ],
            ..default()
        })
    }

    #[test]
    fn delayed_riders_follow_platform_stops_reversals_and_nested_motion_between_ticks() {
        let settings = ClientSettings::load_default().expect("client settings are invalid");
        for carrier in [CarrierId(1), CarrierId(2)] {
            for hz in [10, 30] {
                let mut carriers = moving_platforms();
                let mut buffer = RemotePlayerMotion::default();
                let mut cadence = UpdateCadence::default();
                let delay = settings.interpolation.delay_ticks(&NetworkConfig {
                    update_hz: hz,
                    ..Default::default()
                });
                let local = Position {
                    x: 1.5,
                    y: 0.01,
                    z: -0.5,
                };
                for tick in 1..180 {
                    carriers.advance(tick);
                    if cadence.ready(hz, 30) && !(45..90).contains(&tick) {
                        let mut entry = sample(tick, local.x, 0);
                        entry.movement.carrier = carrier;
                        entry.movement.pos = local;
                        entry.movement.support = CharacterSupport::Ground;
                        buffer.push(entry, delay);
                    }
                    for frame in 0..4 {
                        let alpha = frame as f32 / 4.0;
                        let (movement, velocity) = buffer
                            .advance(0.25, &carriers, alpha, 1.0 / 30.0)
                            .expect("rider sample missing");
                        let expected = carriers.pose_between(carrier, alpha).transform_position(&local);
                        assert!(
                            (Vec3::from(movement.pos) - Vec3::from(expected)).length() < 1e-5,
                            "rider separated from platform at tick {tick}, rate {hz}"
                        );
                        assert_eq!(velocity, Vec3::ZERO, "platform travel must not animate walking");
                    }
                }
            }
        }
    }

    #[test]
    fn a_rider_from_a_snapshot_follows_the_platform_before_the_first_movement_report() {
        let mut carriers = moving_platforms();
        let mut movement = sample(1, 2.0, 0).movement;
        movement.carrier = CarrierId(2);
        let mut buffer = RemotePlayerMotion::new(movement);
        for tick in 1..70 {
            carriers.advance(tick);
            let shown = buffer
                .advance(1.0, &carriers, 0.5, 1.0 / 30.0)
                .expect("snapshot rider missing")
                .0;
            assert_eq!(
                shown.pos,
                carriers
                    .pose_between(CarrierId(2), 0.5)
                    .transform_position(&movement.pos)
            );
        }
    }

    #[test]
    fn boarding_and_leaving_interpolate_world_positions_across_coordinate_frames() {
        let mut carriers = moving_platforms();
        carriers.advance(30);
        for (from, to) in [(CarrierId::WORLD, CarrierId(1)), (CarrierId(1), CarrierId::WORLD)] {
            let mut buffer = RemotePlayerMotion::default();
            let mut left = sample(1, 117.0, 0);
            let mut right = sample(4, 118.0, 0);
            left.movement.carrier = from;
            left.movement.pos = carriers.pose(from).inverse_transform_position(&left.movement.pos);
            right.movement.carrier = to;
            right.movement.pos = carriers.pose(to).inverse_transform_position(&right.movement.pos);
            buffer.push(left, 0.0);
            buffer.push(right, 0.0);
            let mid = buffer
                .advance(1.5, &carriers, 1.0, 1.0 / 30.0)
                .expect("transition sample missing")
                .0;
            assert!((mid.pos.x - 117.5).abs() < 1e-5);
            let end = buffer
                .advance(1.5, &carriers, 1.0, 1.0 / 30.0)
                .expect("transition sample missing")
                .0;
            assert_eq!(end.pos.x, 118.0);
        }
    }

    #[test]
    fn a_portal_crossing_cuts_between_different_carrier_frames() {
        let mut carriers = moving_platforms();
        carriers.advance(30);
        let mut left = sample(1, 1.0, 0);
        left.movement.carrier = CarrierId(1);
        let mut right = sample(4, 2.0, 1);
        right.movement.carrier = CarrierId(2);
        let mut buffer = RemotePlayerMotion::default();
        buffer.push(left, 0.0);
        buffer.push(right, 0.0);
        assert_eq!(
            buffer
                .advance(2.0, &carriers, 1.0, 1.0 / 30.0)
                .expect("entry missing")
                .0
                .pos,
            carriers.pose(CarrierId(1)).transform_position(&left.movement.pos)
        );
        assert_eq!(
            buffer
                .advance(1.0, &carriers, 1.0, 1.0 / 30.0)
                .expect("exit missing")
                .0
                .pos,
            carriers.pose(CarrierId(2)).transform_position(&right.movement.pos)
        );
    }

    #[test]
    fn stopping_never_overshoots_or_settles_back_at_each_update_rate() {
        let settings = ClientSettings::load_default().expect("client settings are invalid");
        for hz in [30, 15, 10, 7, 1] {
            let mut buffer = RemotePlayerMotion::default();
            let mut cadence = UpdateCadence::default();
            let delay = settings.interpolation.delay_ticks(&NetworkConfig {
                update_hz: hz,
                ..Default::default()
            });
            let mut previous = 0.0;
            for tick in 0..300 {
                if cadence.ready(hz, 30) {
                    buffer.push(sample(tick, (tick as f32 / 30.0).min(2.0), 0), delay);
                }
                for _ in 0..4 {
                    let (movement, _) = buffer
                        .advance(0.25, &Carriers::default(), 1.0, 1.0 / 30.0)
                        .expect("sample missing");
                    assert!(movement.pos.x >= previous, "backward motion at {hz} Hz, tick {tick}");
                    assert!(movement.pos.x <= 2.0, "overshoot at {hz} Hz");
                    previous = movement.pos.x;
                }
            }
            assert_eq!(previous, 2.0);
        }
    }

    #[test]
    fn lost_crossing_report_still_cuts_at_the_next_sample() {
        let mut buffer = RemotePlayerMotion::default();
        buffer.push(sample(1, 1.0, 0), 0.0);
        buffer.push(sample(4, 100.0, 1), 0.0);
        for _ in 0..11 {
            assert_eq!(
                buffer
                    .advance(0.25, &Carriers::default(), 1.0, 1.0 / 30.0)
                    .expect("sample missing")
                    .0
                    .pos
                    .x,
                1.0
            );
        }
        assert_eq!(
            buffer
                .advance(0.25, &Carriers::default(), 1.0, 1.0 / 30.0)
                .expect("sample missing")
                .0
                .pos
                .x,
            100.0
        );
    }

    #[test]
    fn sequences_wrap_and_repeated_or_late_reports_do_not_rewind_playback() {
        let mut buffer = RemotePlayerMotion::default();
        buffer.push(sample(u32::MAX, 1.0, 0), 0.0);
        buffer.push(sample(1, 3.0, 0), 0.0);
        assert_eq!(
            buffer
                .advance(1.0, &Carriers::default(), 1.0, 1.0 / 30.0)
                .expect("sample missing")
                .0
                .pos
                .x,
            2.0
        );
        buffer.push(sample(u32::MAX, -10.0, 0), 0.0);
        buffer.push(sample(1, -10.0, 0), 0.0);
        assert_eq!(
            buffer
                .advance(1.0, &Carriers::default(), 1.0, 1.0 / 30.0)
                .expect("sample missing")
                .0
                .pos
                .x,
            3.0
        );
        assert_eq!(
            buffer
                .advance(100.0, &Carriers::default(), 1.0, 1.0 / 30.0)
                .expect("sample missing")
                .0
                .pos
                .x,
            3.0
        );
        buffer.push(sample(3, 5.0, 0), 0.0);
        assert_eq!(
            buffer
                .advance(1.0, &Carriers::default(), 1.0, 1.0 / 30.0)
                .expect("sample missing")
                .0
                .pos
                .x,
            4.0
        );
    }

    #[test]
    fn landing_and_facing_follow_the_buffered_timeline() {
        let mut buffer = RemotePlayerMotion::default();
        let mut air = sample(1, 0.0, 0);
        air.movement.pos.y = 1.0;
        air.movement.face_yaw = PI - 0.1;
        air.movement.vertical_velocity = -2.0;
        air.movement.support = CharacterSupport::Airborne;
        let mut ground = sample(3, 2.0, 0);
        ground.movement.face_yaw = -PI + 0.1;
        ground.movement.support = CharacterSupport::Ground;
        buffer.push(air, 0.0);
        buffer.push(ground, 0.0);
        let mid = buffer
            .advance(1.0, &Carriers::default(), 1.0, 1.0 / 30.0)
            .expect("sample missing")
            .0;
        assert_eq!(mid.pos.y, 0.5);
        assert_eq!(mid.support, CharacterSupport::Airborne);
        assert!((mid.face_yaw - PI).abs() < 1e-5);
        let landed = buffer
            .advance(1.0, &Carriers::default(), 1.0, 1.0 / 30.0)
            .expect("sample missing")
            .0;
        assert_eq!(landed.pos.y, 0.0);
        assert_eq!(landed.support, CharacterSupport::Ground);
    }
}
