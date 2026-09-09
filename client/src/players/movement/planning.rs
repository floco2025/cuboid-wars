use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity,
        PlayerMovementStep, PortalSet, momentum_displacement, player_control_velocity, step_player_movement,
    },
    protocol::{
        ActorMarker, BarrierKindId, MapSettings, PlateState, PlayerId, PlayerMarker, PlayerMoveIntent, Position,
        PowerUpKind,
    },
};

use crate::{
    characters::{CharacterReconciliationOutcome, reconcile_character},
    network::ServerReconciliation,
    players::{BumpFeedbackState, LocalPlayerMarker, PlayerAnimationMotion, PlayerMap},
};

pub(crate) fn plan_player_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: &CollisionWorld,
    map_settings: &MapSettings,
    gameplay_config: &GameplayConfig,
    players: &mut PlayerMap,
    plates: &PlateState,
    portal_set: &PortalSet,
    carriers: &Carriers,
    query: &mut PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_physics = gameplay_config.player.physics();
    for (
        entity,
        player_id,
        mut client_pos,
        move_intent,
        mut motion,
        _,
        mut recon_option,
        knockback,
        mut airborne_momentum,
        mut animation_motion,
        is_local,
    ) in query
    {
        let info = players.get(player_id);
        let has_speed_power_up = info.is_some_and(|i| i.power_up(PowerUpKind::Speed));
        let has_low_gravity = info.is_some_and(|i| i.power_up(PowerUpKind::LowGravity));
        let movement_disabled = info.is_some_and(|i| i.stunned);
        let held_keys: &[BarrierKindId] = info.map_or(&[], |i| i.held_keys.as_slice());
        let player_name = info.map(|i| i.name.as_str());

        let control_velocity = player_control_velocity(
            *move_intent,
            &map_settings.movement,
            has_speed_power_up,
            movement_disabled,
        );

        let correction_displacement = match recon_option.as_mut() {
            Some(recon) if !is_local => match reconcile_character(
                commands,
                entity,
                player_id.0,
                player_name.unwrap_or("player"),
                &mut client_pos,
                &mut motion,
                recon,
                delta,
            ) {
                CharacterReconciliationOutcome::Displacement(displacement) => displacement,
                CharacterReconciliationOutcome::Snapped => {
                    planned_moves.push(CharacterMovePlan::stationary(
                        entity,
                        *client_pos,
                        motion.0,
                        player_physics,
                    ));
                    animation_motion.block_horizontal();
                    continue;
                }
            },
            _ => Vec3::ZERO,
        };
        let external_displacement =
            correction_displacement + momentum_displacement(knockback, airborne_momentum.as_deref(), delta);
        let step = step_player_movement(PlayerMovementStep {
            start: *client_pos,
            vertical_velocity: motion.0,
            control_velocity,
            additional_displacement: correction_displacement,
            delta,
            has_low_gravity,
            held_keys,
            open_kinds: &plates.open_barrier_kinds,
            knockback,
            airborne_momentum: airborne_momentum.as_deref_mut(),
            collision_world,
            map_settings,
            gameplay_config,
            portal_set,
            carriers,
        });
        commands.entity(entity).insert((step.grounding, step.support));
        animation_motion.record_step(*client_pos, &step, control_velocity, external_displacement, delta);
        planned_moves.push(CharacterMovePlan::from_movement_result(
            entity,
            *client_pos,
            step,
            player_physics,
        ));
    }
}

pub(crate) type PlayerMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        &'static mut Position,
        &'static PlayerMoveIntent,
        &'static mut CharacterVerticalVelocity,
        Option<&'static mut BumpFeedbackState>,
        Option<&'static mut ServerReconciliation>,
        Option<&'static KnockbackVelocity>,
        Option<&'static mut AirborneMomentum>,
        &'static mut PlayerAnimationMotion,
        Has<LocalPlayerMarker>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{characters::PreviousTickPosition, network::RoundTripTime, test_fixtures};
    use bevy::ecs::system::SystemState;
    use common::{
        constants::TICK_SECS,
        protocol::{BarrierKindTable, MapLayout},
    };
    use std::time::Duration;

    #[test]
    fn only_remote_players_smooth_or_snap_during_movement_planning() {
        let gameplay = test_fixtures::gameplay_config();
        let settings = test_fixtures::map_settings();
        let collision = CollisionWorld::from_map_layout(&MapLayout::default(), &BarrierKindTable::default());
        let rtt = RoundTripTime {
            rtt: Duration::from_millis(200),
            ..default()
        };
        for is_local in [true, false] {
            for error in [1.0, 3.0] {
                let mut world = World::new();
                let server_pos = Position {
                    x: error,
                    y: 0.0,
                    z: 0.0,
                };
                let entity = world
                    .spawn((
                        PlayerMarker,
                        PlayerId(1),
                        Position::default(),
                        PreviousTickPosition(Position::default()),
                        PlayerMoveIntent::Idle,
                        CharacterVerticalVelocity(0.0),
                        PlayerAnimationMotion::default(),
                        ServerReconciliation::new(Vec3::X * error, server_pos, Vec3::NEG_Y * 2.0, &rtt),
                    ))
                    .id();
                if is_local {
                    world.entity_mut(entity).insert(LocalPlayerMarker);
                }
                let mut state = SystemState::<(Commands, PlayerMovementQuery)>::new(&mut world);
                let mut plans = Vec::new();
                let (mut commands, mut query) = state.get_mut(&mut world).expect("movement query invalid");
                plan_player_moves(
                    &mut commands,
                    TICK_SECS,
                    &collision,
                    &settings,
                    &gameplay,
                    &mut PlayerMap::default(),
                    &PlateState::default(),
                    &PortalSet::default(),
                    &Carriers::default(),
                    &mut query,
                    &mut plans,
                );
                state.apply(&mut world);
                let plan = plans.first().expect("player movement plan missing");
                let expected_x = if is_local {
                    0.0
                } else if error < 3.0 {
                    error * TICK_SECS / 0.8
                } else {
                    error
                };
                assert!(
                    (plan.target.x - expected_x).abs() < 1e-5,
                    "local={is_local}, error={error}"
                );
                if !is_local && error == 3.0 {
                    assert!(world.get::<ServerReconciliation>(entity).is_none());
                    assert_eq!(
                        world
                            .get::<PreviousTickPosition>(entity)
                            .expect("previous position missing")
                            .0,
                        server_pos
                    );
                    assert_eq!(plan.target_vertical_velocity, -2.0);
                } else {
                    assert!(world.get::<ServerReconciliation>(entity).is_some());
                    assert_eq!(
                        *world.get::<Position>(entity).expect("position missing"),
                        Position::default()
                    );
                }
            }
        }
    }
}
