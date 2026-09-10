use bevy::prelude::*;

use super::{Invincibility, PlayerMap, PlayerSpawn, place_player_body};
use crate::{
    characters::generate_player_spawn_position,
    combat::{DeathSource, PendingExplosions, kill_player},
    config::{FallDamageConfig, ServerGameplayConfig},
    map::MapConfig,
    network::ServerToClient,
    portals::PortalAssignments,
};
use common::{
    config::GameplayConfig,
    constants::CHARACTER_FALL_DEATH_Y,
    health::apply_damage,
    map::Carriers,
    physics::CollisionWorld,
    protocol::{Health, MapSettings, PlayerId, PlayerMarker, Position, SPlayerFallDamage, ServerMessage, ServerTick},
};

// Crushing and void falls reported by the owner; an invincible void fall is rescued instead.
pub fn players_fatal_outcomes_system(
    mut commands: Commands,
    tick: Res<ServerTick>,
    portal_assignments: Res<PortalAssignments>,
    mut players: ResMut<PlayerMap>,
    mut pending_explosions: ResMut<PendingExplosions>,
    gameplay_config: Res<GameplayConfig>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    invincibility: Res<Invincibility>,
    map_config: Res<MapConfig>,
    carriers: Res<Carriers>,
    collision_world: Res<CollisionWorld>,
    player_query: Query<(Entity, &PlayerId, &Position, &Health), With<PlayerMarker>>,
) {
    for (entity, id, pos, health) in player_query.iter() {
        let Some(info) = players.get_mut(id).filter(|info| !info.is_dead()) else {
            continue;
        };
        let crushed = info.life.outcomes.crushed.take();
        let fell_out_of_world = std::mem::take(&mut info.life.outcomes.fell_out_of_world);
        if let Some(pos) = crushed.filter(|_| !invincibility.0) {
            info!("{} was crushed by moving geometry at {:?}", players.describe(id), pos);
            kill_player(
                &mut commands,
                &mut players,
                *id,
                entity,
                pos,
                server_gameplay_config.player.respawn_secs,
                DeathSource::Crushed,
                &server_gameplay_config,
                &mut pending_explosions,
            );
            continue;
        }
        if !fell_out_of_world {
            continue;
        }
        if invincibility.0 {
            // Void rescue preserves equipment and score.
            let occupied_positions: Vec<Position> = player_query
                .iter()
                .filter(|(other, _, other_pos, _)| *other != entity && other_pos.y >= CHARACTER_FALL_DEATH_Y)
                .map(|(_, _, other_pos, _)| *other_pos)
                .collect();
            let spawn_pos = generate_player_spawn_position(
                &map_config,
                &carriers,
                &collision_world,
                &occupied_positions,
                gameplay_config.player.physics(),
            );
            info!(
                "{} fell out of the world while invincible; teleporting to {:?}",
                players.describe(id),
                spawn_pos
            );
            if let Some(info) = players.get_mut(id) {
                info.advance_body();
            }
            place_player_body(
                &mut commands,
                &mut players,
                *id,
                entity,
                &PlayerSpawn::without_checkpoint(spawn_pos),
                *health,
                tick.0,
                portal_assignments.get(id),
            );
            continue;
        }
        info!("{} fell out of the world", players.describe(id));
        kill_player(
            &mut commands,
            &mut players,
            *id,
            entity,
            *pos,
            server_gameplay_config.player.respawn_secs,
            DeathSource::Void,
            &server_gameplay_config,
            &mut pending_explosions,
        );
    }
}

// ============================================================================
// Players Fall Damage System
// ============================================================================

// Below this damage, skip the impact effect entirely. The lerp produces
// near-zero damage just past `safe_distance` due to float / tick
// noise; without this gate the client would get a wiggle for every tiny
// step off a curb.
// Keep this cutoff in sync with tools/map_editor/jump_reach.py.
const FALL_DAMAGE_EMIT_THRESHOLD: f32 = 1.0;

pub fn players_fall_damage_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    mut pending_explosions: ResMut<PendingExplosions>,
    server_gameplay_config: Res<ServerGameplayConfig>,
    invincibility: Res<Invincibility>,
    map_settings: Res<MapSettings>,
    fall: Res<FallDamageConfig>,
    mut player_query: Query<(Entity, &PlayerId, &mut Health), With<PlayerMarker>>,
) {
    let invincible = invincibility.0;
    let max_health = server_gameplay_config.combat.health.player.max;
    let respawn_secs = server_gameplay_config.player.respawn_secs;

    for (entity, id, mut health) in player_query.iter_mut() {
        let Some(info) = players.get_mut(id) else { continue };
        if info.is_dead() {
            continue;
        }

        let landings = std::mem::take(&mut info.life.outcomes.landings);
        for impact in landings {
            if players.get(id).is_none_or(|info| info.is_dead()) {
                break;
            }
            let pos = &impact.pos;
            let fall_distance = fall_distance_for_speed(impact.impact_speed, map_settings.movement.gravity);
            if fall_distance <= fall.safe_distance {
                continue;
            }

            let damage = fall_damage_for_distance(fall_distance, fall.safe_distance, fall.lethal_distance, max_health);
            // Skip the entire emission path for negligible damage —
            // the safe-threshold lerp produces near-zero damage just
            // past `safe_distance` from floating-point slack and
            // discrete-tick noise. No HUD update or camera wiggle for
            // a fall the player barely registers.
            if damage < FALL_DAMAGE_EMIT_THRESHOLD {
                continue;
            }
            if !invincible {
                apply_damage(&mut health, damage);
            }
            // Unicast `SPlayerFallDamage` to the victim so the HUD health bar
            // and vertical camera wiggle land on the impact frame
            // instead of waiting for the next snapshot. The fatal-fall
            // case additionally surfaces `SPlayerDeath` via
            // `kill_player` below.
            if let Some(info) = players.get(id) {
                let _ = info
                    .connection
                    .channel
                    .send(ServerToClient::Send(ServerMessage::PlayerFallDamage(
                        SPlayerFallDamage {
                            id: *id,
                            generation: info.session.generation,
                            health: *health,
                        },
                    )));
            }
            if health.0 <= 0.0 {
                info!(
                    "{} died from fall (distance {:.1}m)",
                    players.describe(id),
                    fall_distance
                );
                kill_player(
                    &mut commands,
                    &mut players,
                    *id,
                    entity,
                    *pos,
                    respawn_secs,
                    DeathSource::Fall,
                    &server_gameplay_config,
                    &mut pending_explosions,
                );
            }
        }
    }
}

// Express impact energy as a normal-gravity drop so map distance thresholds retain their meaning.
fn fall_distance_for_speed(impact_speed: f32, normal_gravity: f32) -> f32 {
    impact_speed * impact_speed / (2.0 * normal_gravity)
}

// Lerp damage between `safe_distance` (0 dmg) and `lethal_distance`
// (full health), clamping the falloff beyond the lethal endpoint.
// Keep this curve in sync with tools/map_editor/jump_reach.py::FallSettings.damage_fraction.
fn fall_damage_for_distance(distance: f32, safe: f32, lethal: f32, max_health: f32) -> f32 {
    let t = ((distance - safe) / (lethal - safe)).clamp(0.0, 1.0);
    t * max_health
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        players::{PlayerInfo, PowerUpState, outcomes::Landing},
        test_geometry::geometry,
    };
    use common::protocol::{
        BarrierKindId, BarrierKindTable, Lane, MapLayout, PlayerGeneration, PortalMode, PowerUpKind,
    };
    use tokio::sync::mpsc::unbounded_channel;

    // Matches the shipping map's normal-gravity setting.
    const TEST_GRAVITY: f32 = 25.0;

    #[test]
    fn a_crushed_player_dies_at_the_reported_contact() {
        let server = ServerGameplayConfig::load_default().expect("default server gameplay config missing");
        let gameplay = server.gameplay_config();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(gameplay)
            .insert_resource(server)
            .insert_resource(MapConfig::for_grid(Vec::new(), geometry(1, 1)))
            .insert_resource(Carriers::default())
            .insert_resource(CollisionWorld::from_map_layout(
                &MapLayout::default(),
                &BarrierKindTable::default(),
            ))
            .insert_resource(PlayerMap::default())
            .insert_resource(Invincibility(false))
            .init_resource::<ServerTick>()
            .insert_resource(PortalAssignments::new(PortalMode::Both))
            .insert_resource(PendingExplosions::default())
            .add_systems(Update, players_fatal_outcomes_system);
        let id = PlayerId(1);
        let entity = app
            .world_mut()
            .spawn((PlayerMarker, id, Position::default(), Health(100.0)))
            .id();
        let (sender, mut receiver) = unbounded_channel();
        let mut info = PlayerInfo::new(entity, sender);
        info.connection.logged_in = true;
        let contact = Position { x: 20.0, ..default() };
        info.life.outcomes.crushed = Some(contact);
        app.world_mut().resource_mut::<PlayerMap>().insert(id, info);

        app.update();

        assert!(
            app.world()
                .resource::<PlayerMap>()
                .get(&id)
                .is_some_and(PlayerInfo::is_dead)
        );
        assert!(app.world().get_entity(entity).is_err());
        let death = loop {
            match receiver.try_recv().expect("no death message reached the player") {
                ServerToClient::Send(ServerMessage::PlayerDeath(death)) => break death,
                _ => continue,
            }
        };
        assert_eq!(death.id, id);
        assert_eq!(death.killer, None);
        assert_eq!(death.pos, contact);
    }

    #[test]
    fn fall_damage_zero_at_safe_distance() {
        assert_eq!(fall_damage_for_distance(4.0, 4.0, 12.0, 100.0), 0.0);
        assert_eq!(fall_damage_for_distance(3.0, 4.0, 12.0, 100.0), 0.0);
    }

    #[test]
    fn invincible_void_rescue_relocates_reliably_and_preserves_equipment() {
        let server = ServerGameplayConfig::load_default().expect("gameplay config missing");
        let mut app = App::new();
        app.insert_resource(server.gameplay_config())
            .insert_resource(server)
            .insert_resource(MapConfig::for_grid(Vec::new(), geometry(1, 1)))
            .init_resource::<Carriers>()
            .insert_resource(CollisionWorld::from_map_layout(
                &MapLayout::default(),
                &BarrierKindTable::default(),
            ))
            .init_resource::<PlayerMap>()
            .insert_resource(Invincibility(true))
            .insert_resource(ServerTick(42))
            .insert_resource(PortalAssignments::new(PortalMode::Both))
            .init_resource::<PendingExplosions>()
            .add_systems(Update, players_fatal_outcomes_system);
        let id = PlayerId(1);
        let pos = Position {
            y: CHARACTER_FALL_DEATH_Y - 1.0,
            ..default()
        };
        let entity = app.world_mut().spawn((PlayerMarker, id, pos, Health(37.0))).id();
        let (sender, mut receiver) = unbounded_channel();
        let mut info = PlayerInfo::new(entity, sender);
        info.connection.logged_in = true;
        info.session.score = 5;
        info.life.missiles = 3;
        info.life.held_keys.push(BarrierKindId(1));
        info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Permanent;
        app.world_mut().resource_mut::<PlayerMap>().insert(id, info);
        app.update();
        assert!(receiver.try_recv().is_err());
        assert_eq!(*app.world().get::<Position>(entity).expect("position missing"), pos);
        app.world_mut()
            .resource_mut::<PlayerMap>()
            .get_mut(&id)
            .expect("player missing")
            .life
            .outcomes
            .fell_out_of_world = true;
        app.update();
        let ServerToClient::Send(message @ ServerMessage::PlayerRelocated(_)) =
            receiver.try_recv().expect("relocation missing")
        else {
            panic!("void rescue did not send a relocation")
        };
        assert_eq!(message.lane(), Lane::Reliable);
        let ServerMessage::PlayerRelocated(relocation) = message else {
            unreachable!()
        };
        assert_eq!(relocation.id, id);
        assert_eq!(relocation.tick, 42);
        assert_eq!(relocation.player.generation, PlayerGeneration(1));
        assert_eq!(relocation.player.health.0, 37.0);
        assert_eq!(relocation.player.score, 5);
        assert_eq!(relocation.player.missiles, 3);
        assert_eq!(relocation.player.held_keys, [BarrierKindId(1)]);
        assert!(relocation.player.power_up(PowerUpKind::Speed));
        assert_eq!(
            *app.world().get::<Position>(entity).expect("position missing"),
            relocation.player.movement.pos
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn landing_damage_uses_impact_speed_and_map_thresholds() {
        for (safe, lethal, drop, low_gravity, max_health, initial_health, expected_health) in [
            (12.0, 16.0, 12.0, false, 100.0, 100.0, 100.0),
            (8.0, 16.0, 12.0, false, 100.0, 100.0, 50.0),
            (4.0, 12.0, 12.0, false, 100.0, 100.0, 0.0),
            (4.0, 12.0, 20.0, false, 100.0, 100.0, 0.0),
            (4.0, 12.0, 12.0, true, 100.0, 100.0, 75.0),
            (4.0, 12.0, 8.0, true, 100.0, 100.0, 100.0),
            (4.0, 12.0, 8.0, false, 100.0, 40.0, 0.0),
            (4.0, 12.0, 4.0625, false, 100.0, 100.0, 100.0),
            (4.0, 12.0, 4.0625, false, 1000.0, 1000.0, 992.1875),
            (0.0, 8.0, 1.0, false, 8.0, 8.0, 7.0),
            (0.0, 8.0, 100.0, false, 0.5, 0.5, 0.5),
        ] {
            let mut server = ServerGameplayConfig::load_default().expect("server gameplay config missing");
            server.combat.health.player.max = max_health;
            let mut settings = server.maps["hotel"].settings.clone();
            settings.movement.gravity = 2.0;
            settings.movement.low_gravity = 1.0;
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .insert_resource(server)
                .insert_resource(settings)
                .insert_resource(FallDamageConfig {
                    safe_distance: safe,
                    lethal_distance: lethal,
                })
                .insert_resource(PlayerMap::default())
                .insert_resource(Invincibility(false))
                .insert_resource(PendingExplosions::default())
                .add_systems(Update, players_fall_damage_system);
            let id = PlayerId(1);
            let entity = app
                .world_mut()
                .spawn((PlayerMarker, id, Position::default(), Health(initial_health)))
                .id();
            let (sender, mut receiver) = unbounded_channel();
            let mut info = PlayerInfo::new(entity, sender);
            info.connection.logged_in = true;
            info.life.outcomes.landings.push(Landing {
                pos: Position::default(),
                impact_speed: (2.0_f32 * if low_gravity { 1.0 } else { 2.0 } * drop).sqrt(),
            });
            app.world_mut().resource_mut::<PlayerMap>().insert(id, info);

            app.update();

            let dead = app
                .world()
                .resource::<PlayerMap>()
                .get(&id)
                .expect("player missing")
                .is_dead();
            assert_eq!(dead, expected_health == 0.0);
            if !dead {
                assert!(
                    (app.world().get::<Health>(entity).expect("player health missing").0 - expected_health).abs()
                        < 0.001
                );
            }
            app.update();
            if !dead {
                assert!(
                    (app.world().get::<Health>(entity).expect("player health missing").0 - expected_health).abs()
                        < 0.001
                );
            }
            let mut impact_health = None;
            while let Ok(message) = receiver.try_recv() {
                if let ServerToClient::Send(ServerMessage::PlayerFallDamage(impact)) = message {
                    impact_health = Some(impact.health.0);
                }
            }
            assert_eq!(impact_health.is_some(), expected_health < initial_health);
            if let Some(health) = impact_health {
                assert!((health - expected_health).abs() < 0.001);
            }
        }
    }

    #[test]
    fn fall_damage_lethal_at_lethal_distance() {
        assert_eq!(fall_damage_for_distance(12.0, 4.0, 12.0, 100.0), 100.0);
    }

    #[test]
    fn fall_damage_lerps_midpoint() {
        // (8 - 4) / (12 - 4) = 0.5 → 50 dmg
        assert_eq!(fall_damage_for_distance(8.0, 4.0, 12.0, 100.0), 50.0);
    }

    #[test]
    fn fall_damage_saturates_past_lethal() {
        assert_eq!(fall_damage_for_distance(100.0, 4.0, 12.0, 100.0), 100.0);
    }

    #[test]
    fn impact_energy_determines_the_equivalent_drop() {
        assert_eq!(fall_distance_for_speed(0.0, TEST_GRAVITY), 0.0);
        assert_eq!(fall_distance_for_speed(10.0, TEST_GRAVITY), 2.0);
        assert_eq!(fall_distance_for_speed(20.0, TEST_GRAVITY), 8.0);
        assert_eq!(fall_distance_for_speed(25.0, TEST_GRAVITY), 12.5);
    }
}
