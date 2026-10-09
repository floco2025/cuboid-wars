use bevy::{ecs::world::CommandQueue, prelude::*};
use crossbeam_channel::{Receiver, unbounded};

use super::{PendingExplosion, PendingExplosions, damage::*};
use crate::{
    actors::test_kinds::{self, BEAM},
    config::{ServerGameplayConfig, fixtures},
    players::{PlayerInfo, PlayerMap, PowerUpState},
};
use common::protocol::{
    FieldId, Health, PlayerDeathEffect, PlayerId, Position, PowerUpKind, SPlayerDeath, ServerMessage,
};

fn server_gameplay_config() -> ServerGameplayConfig {
    let mut config = fixtures::server_config();
    config.combat.damage.projectile = 25.0;
    config.scoring.player_kill = 1;
    config.scoring.player_death = -1;
    config
}

fn add_player(players: &mut PlayerMap, id: u32, logged_in: bool) -> Receiver<ServerMessage> {
    let (tx, rx) = unbounded();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    info.connection.logged_in = logged_in;
    players.insert(PlayerId(id), info);
    rx
}

fn score(players: &PlayerMap, id: u32) -> i32 {
    players.get(&PlayerId(id)).expect("player missing").session.score
}

fn next_player_death(receiver: &mut Receiver<ServerMessage>) -> SPlayerDeath {
    loop {
        match receiver.try_recv().expect("expected a PlayerDeath broadcast") {
            ServerMessage::PlayerDeath(msg) => return msg,
            _ => continue,
        }
    }
}

// Runs the death sequence for `victim` standing at `pos`; returns the explosions it queued.
fn kill_at(
    players: &mut PlayerMap,
    victim: PlayerId,
    pos: Position,
    source: DeathSource,
    config: &ServerGameplayConfig,
) -> PendingExplosions {
    let mut world = World::new();
    let entity = world.spawn_empty().id();
    let mut queue = CommandQueue::default();
    let mut pending = PendingExplosions::default();
    kill_player(
        &mut Commands::new(&mut queue, &world),
        players,
        victim,
        entity,
        pos,
        2.0,
        source,
        config,
        &mut pending,
    );
    queue.apply(&mut world);
    pending
}

fn kill_with(players: &mut PlayerMap, victim: PlayerId, source: DeathSource) -> PendingExplosions {
    kill_at(players, victim, Position::default(), source, &server_gameplay_config())
}

#[test]
fn repeated_hits_then_a_kill_charge_the_death_once() {
    let mut players = PlayerMap::default();
    add_player(&mut players, 1, true);
    add_player(&mut players, 2, true);
    let config = server_gameplay_config();
    let mut health = Health(100.0);
    for hit in 1..=4 {
        let lethal = apply_player_projectile_hit(&players, PlayerId(2), &mut health, &config, false);
        assert_eq!(lethal, hit == 4, "hit {hit}");
    }
    assert_eq!(health.0, 0.0);
    assert_eq!((score(&players, 1), score(&players, 2)), (0, 0));

    kill_with(&mut players, PlayerId(2), DeathSource::Shot(PlayerId(1)));

    assert_eq!(score(&players, 1), config.scoring.player_kill);
    assert_eq!(score(&players, 2), config.scoring.player_death);
}

#[test]
fn a_dead_player_takes_no_further_hit_or_beam_damage() {
    let mut players = PlayerMap::default();
    add_player(&mut players, 1, false);
    add_player(&mut players, 2, false);
    players.get_mut(&PlayerId(2)).expect("target").begin_respawn(2.0);
    let mut health = Health(50.0);

    assert!(!apply_player_projectile_hit(
        &players,
        PlayerId(2),
        &mut health,
        &server_gameplay_config(),
        false
    ));
    assert!(!apply_player_beam_damage(
        &players,
        PlayerId(2),
        &mut health,
        100.0,
        false
    ));

    assert_eq!(health.0, 50.0);
    assert_eq!((score(&players, 1), score(&players, 2)), (0, 0));
}

#[test]
fn dead_actor_takes_no_further_hits_or_score() {
    let config = test_kinds::server_config();
    let mut players = PlayerMap::default();
    add_player(&mut players, 1, false);
    let mut health = Health(1.0);

    let first_hit_lethal = apply_actor_projectile_hit(&mut players, &PlayerId(1), BEAM, &mut health, &config);
    assert!(first_hit_lethal);
    let score_after_kill = score(&players, 1);

    // The dying actor's entity stays queryable until removal runs later
    // in the tick; a same-tick second hit must not count as lethal again.
    let second_hit_lethal = apply_actor_projectile_hit(&mut players, &PlayerId(1), BEAM, &mut health, &config);
    assert!(!second_hit_lethal);
    assert_eq!(score(&players, 1), score_after_kill);
}

#[test]
fn crouched_death_keeps_the_blast_and_cue_at_the_body_center_after_life_reset() {
    let mut players = PlayerMap::default();
    let id = PlayerId(7);
    let mut receiver = add_player(&mut players, id.0, true);
    players
        .get_mut(&id)
        .expect("victim missing")
        .life
        .movement
        .stance
        .crouched = true;
    let mut config = server_gameplay_config();
    config.player.gameplay.hitbox.height = 2.0;
    config.player.gameplay.hitbox.bottom_offset = 0.1;
    let pos = Position { x: 2.0, y: 3.0, z: 4.0 };
    let expected = Vec3::new(2.0, 3.6, 4.0);

    let mut pending = kill_at(&mut players, id, pos, DeathSource::Admin, &config);

    let victim = players.get(&id).expect("victim missing");
    assert!(victim.is_dead());
    assert!(!victim.stance().crouched);
    let Some(PendingExplosion::Player { source_id, center }) = pending.0.pop_front() else {
        panic!("player death blast missing");
    };
    assert_eq!(source_id, id);
    assert!(center.distance(expected) < 1e-6);
    let death = next_player_death(&mut receiver);
    assert_eq!(death.pos, pos);
    let PlayerDeathEffect::Explosion { center } = death.effect else {
        panic!("death explosion effect missing");
    };
    assert!(Vec3::from(center).distance(expected) < 1e-6);
}

#[test]
fn kill_credit_ignores_departed_and_self_shooters() {
    let mut players = PlayerMap::default();
    add_player(&mut players, 1, true);
    add_player(&mut players, 2, true);

    assert_eq!(
        kill_credit(&DeathSource::Shot(PlayerId(9)), PlayerId(2), &players),
        None
    );
    assert_eq!(
        kill_credit(&DeathSource::Shot(PlayerId(2)), PlayerId(2), &players),
        None
    );
    assert_eq!(
        kill_credit(&DeathSource::Missile(PlayerId(1)), PlayerId(2), &players),
        Some(PlayerId(1))
    );
    assert_eq!(
        kill_credit(&DeathSource::PlayerBlast(PlayerId(1)), PlayerId(2), &players),
        None
    );
    assert_eq!(kill_credit(&DeathSource::Fall, PlayerId(2), &players), None);
}

#[test]
fn kill_player_clears_state_and_arms_timer() {
    let mut players = PlayerMap::default();
    add_player(&mut players, 7, false);
    let info = players.get_mut(&PlayerId(7)).expect("player missing");
    info.life.power_ups[PowerUpKind::Speed.index()] = PowerUpState::Timed(1.5);
    info.add_key(FieldId(0));

    kill_with(&mut players, PlayerId(7), DeathSource::Fall);

    let info = players.get(&PlayerId(7)).expect("player still tracked after death");
    assert_eq!(info.respawn_remaining_secs(), Some(2.0));
    assert_eq!(info.life.power_ups, [PowerUpState::Inactive; PowerUpKind::COUNT]);
    assert!(info.life.held_keys.is_empty());
    assert_eq!(info.entity(), None);
    assert!(info.is_dead());
}

#[test]
fn void_fall_queues_no_explosion() {
    let mut players = PlayerMap::default();
    add_player(&mut players, 7, false);

    let pending = kill_with(&mut players, PlayerId(7), DeathSource::Void);

    assert!(pending.0.is_empty(), "a void fall must not queue an explosion");
}
