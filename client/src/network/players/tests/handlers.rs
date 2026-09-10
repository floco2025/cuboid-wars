use super::*;
use crate::players::PlayerInfo;
use bevy::ecs::world::CommandQueue;

#[test]
fn queued_impulses_add_to_current_motion_and_clamp_the_planar_sum() {
    let mut world = World::new();
    let entity = world
        .spawn((
            CharacterVerticalVelocity(-6.0),
            KnockbackVelocity(Vec3::X * 2.0),
            Health(100.0),
        ))
        .id();
    let mut queue = CommandQueue::default();
    {
        let mut commands = Commands::new(&mut queue, &world);
        for health in [90.0, 80.0] {
            commands.entity(entity).queue(move |entity: EntityWorldMut| {
                apply_player_impulse(
                    entity,
                    SPlayerKnockback {
                        id: PlayerId(1),
                        generation: PlayerGeneration(0),
                        health: Health(health),
                        impulse: [3.0, 4.0, 0.0],
                    },
                    10.0,
                );
            });
        }
    }
    queue.apply(&mut world);
    assert_eq!(
        world
            .get::<CharacterVerticalVelocity>(entity)
            .expect("vertical velocity missing")
            .0,
        2.0
    );
    assert_eq!(
        world.get::<KnockbackVelocity>(entity).expect("knockback missing").0,
        Vec3::X * 8.0
    );
    assert_eq!(world.get::<Health>(entity).expect("health missing").0, 80.0);
    apply_player_impulse(
        world.entity_mut(entity),
        SPlayerKnockback {
            id: PlayerId(1),
            generation: PlayerGeneration(0),
            health: Health(70.0),
            impulse: [100.0, 30.0, 0.0],
        },
        10.0,
    );
    assert_eq!(
        world
            .get::<CharacterVerticalVelocity>(entity)
            .expect("vertical velocity missing")
            .0,
        32.0
    );
    assert_eq!(
        world.get::<KnockbackVelocity>(entity).expect("knockback missing").0,
        Vec3::X * 10.0
    );
}

fn player_info(entity: Entity, name: &str) -> PlayerInfo {
    PlayerInfo {
        generation: PlayerGeneration(0),
        entity,
        score: 0,
        name: name.to_owned(),
        power_ups: [false; PowerUpKind::COUNT],
        stunned: false,
        held_keys: Vec::new(),
        missiles: 0,
        last_movement_tick: 0,
        spawn_tick: 0,
    }
}

#[test]
fn death_and_group_respawn_hide_the_player_with_the_matching_banner() {
    for snapshot_first in [false, true] {
        for effect in [
            PlayerDeathEffect::Explosion,
            PlayerDeathEffect::VoidFall,
            PlayerDeathEffect::GroupRespawn,
        ] {
            let my_id = PlayerId(7);
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            let entity = app.world_mut().spawn((Health(42.0), Visibility::Visible)).id();
            let world = app.world_mut();
            let mut players = PlayerMap::default();
            players.insert(my_id, player_info(entity, "Alice"));
            let mut local_player_info = LocalPlayerInfo::default();
            if snapshot_first {
                players.retire_body(my_id, PlayerGeneration(0));
                local_player_info.is_dead = true;
                world.entity_mut(entity).insert(Visibility::Hidden);
            }
            let mut banner = HudBanner::default();
            let mut commands_queue = bevy::ecs::world::CommandQueue::default();

            {
                let mut commands = bevy::ecs::system::Commands::new(&mut commands_queue, world);
                apply_player_death(
                    &mut commands,
                    &mut players,
                    &mut local_player_info,
                    &mut banner,
                    my_id,
                    SPlayerDeath {
                        id: my_id,
                        generation: PlayerGeneration(0),
                        pos: Position::default(),
                        killer: None,
                        victim_score: 0,
                        killer_score: None,
                        effect,
                    },
                );
            }
            commands_queue.apply(world);

            assert_eq!(world.entity(entity).get::<Health>(), Some(&Health(0.0)));
            assert_eq!(world.entity(entity).get::<Visibility>(), Some(&Visibility::Hidden));
            assert!(local_player_info.is_dead);
            assert_eq!(
                banner.pending_texts(),
                [if effect == PlayerDeathEffect::GroupRespawn {
                    "Group respawning"
                } else {
                    "You died!"
                }]
            );
        }
    }
}
