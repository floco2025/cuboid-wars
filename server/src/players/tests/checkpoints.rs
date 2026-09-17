use bevy::prelude::*;
use common::{
    physics::{CharacterSupport, CollisionWorld},
    protocol::{
        BarrierKindId, CarrierId, Checkpoint, CheckpointKind, FaceYaw, Floor, Health, MapLayout, PlayerId, Position,
        ServerMessage,
    },
};

use super::{
    CheckpointId, PlayerCheckpoint, PlayerMap, PowerUpState,
    checkpoints::apply_checkpoint_entries,
    players_checkpoints_system,
    respawn_tests::{add_player, advance, kill, respawn_app},
};
use crate::{
    config::{ActorRespawnScope, PlayerRespawnMode, ServerGameplayConfig},
    map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    schedule::ServerSet,
    test_geometry::geometry,
};

const GRID_COLS: i32 = 12;

// Checkpoint `number` sits on this column of the one-row grid, past the
// respawn fixture's spawn and actor zones.
fn col_of(number: u32) -> i32 {
    4 + 2 * number as i32
}

fn checkpoint(number: u32) -> Checkpoint {
    let geometry = geometry(GRID_COLS, 1);
    let col = col_of(number);
    Checkpoint {
        kind: CheckpointKind::Individual,
        number,
        carrier: CarrierId::WORLD,
        level: 0,
        cols: [col, col + 1],
        rows: [0, 1],
        min_x: geometry.cell_to_world_x(col),
        max_x: geometry.cell_to_world_x(col + 1),
        min_z: geometry.cell_to_world_z(0),
        max_z: geometry.cell_to_world_z(1),
        y: 0.0,
    }
}

// The centre of checkpoint `number`.
fn inside(number: u32) -> Position {
    let geometry = geometry(GRID_COLS, 1);
    Position {
        x: geometry.cell_center_x(col_of(number)),
        y: 0.0,
        z: geometry.cell_center_z(0),
    }
}

// The cell after checkpoint `number`, in no checkpoint.
fn between(number: u32) -> Position {
    let geometry = geometry(GRID_COLS, 1);
    Position {
        x: geometry.cell_center_x(col_of(number) + 1),
        y: 0.0,
        z: geometry.cell_center_z(0),
    }
}

fn in_checkpoint(pos: &Position, number: u32) -> bool {
    let checkpoint = checkpoint(number);
    (checkpoint.min_x..checkpoint.max_x).contains(&pos.x) && (checkpoint.min_z..checkpoint.max_z).contains(&pos.z)
}

fn floor(c: &Checkpoint) -> Floor {
    Floor {
        x1: c.min_x,
        x2: c.max_x,
        z1: c.min_z,
        z2: c.max_z,
        y: c.y,
        thickness: 0.2,
        level: c.level,
        carrier: c.carrier,
    }
}

fn app(mode: PlayerRespawnMode) -> App {
    let mut app = respawn_app(mode, ActorRespawnScope::Dead);
    let checkpoints = vec![checkpoint(1), checkpoint(2)];
    let layout = MapLayout {
        floors: checkpoints.iter().map(floor).collect(),
        checkpoints,
        ..default()
    };
    app.insert_resource(CollisionWorld::from_map_layout(&layout));
    app.insert_resource(layout);
    let mut cells = CellGrid::new(GRID_COLS, 1);
    for cell in &mut cells.rows[0] {
        cell.has_floor = true;
    }
    app.world_mut().resource_mut::<MapConfig>().grids[0] = CarrierGrid::new(
        CarrierId::WORLD,
        geometry(GRID_COLS, 1),
        vec![LevelGrid {
            cells,
            edges: EdgeGrid::new(GRID_COLS, 1),
            barrier_edges: EdgeGrid::new(GRID_COLS, 1),
        }],
    );
    app.add_systems(Update, players_checkpoints_system.in_set(ServerSet::Maintenance));
    app
}

// A ramp flag on the checkpoint's cell blocks every spawn in it.
fn block(app: &mut App, number: u32, blocked: bool) {
    app.world_mut().resource_mut::<MapConfig>().grids[0].levels[0]
        .cells
        .rows[0][col_of(number) as usize]
        .has_ramp = blocked;
}

fn stand(app: &mut App, id: PlayerId, pos: Position, support: CharacterSupport) {
    stand_facing(app, id, pos, support, 0.7);
}

fn stand_facing(app: &mut App, id: PlayerId, pos: Position, support: CharacterSupport, yaw: f32) {
    let entity = {
        let mut players = app.world_mut().resource_mut::<PlayerMap>();
        let player = players.get_mut(&id).expect("player missing");
        player.life.movement.support = support;
        player.entity().expect("player body missing")
    };
    app.world_mut().entity_mut(entity).insert((pos, FaceYaw(yaw)));
}

fn shared_facing(app: &App) -> Option<Vec3> {
    app.world()
        .resource::<PlayerMap>()
        .shared_checkpoint
        .map(|checkpoint| checkpoint.facing)
}

fn has_visit(app: &App, id: PlayerId, checkpoint: usize) -> bool {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .expect("player missing")
        .session
        .checkpoint_visits
        .contains_key(&CheckpointId(checkpoint))
}

fn saved(app: &App, id: PlayerId) -> Option<CheckpointId> {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .expect("player missing")
        .session
        .checkpoint
        .map(|c| c.id)
}

fn body_position(app: &App, id: PlayerId) -> Position {
    let player = app.world().resource::<PlayerMap>().get(&id).expect("player missing");
    *app.world()
        .get::<Position>(player.entity().expect("respawn missing"))
        .expect("position missing")
}

#[test]
fn only_grounded_players_activate_and_lower_checkpoints_do_not_roll_back() {
    let mut app = app(PlayerRespawnMode::Individual);
    let id = PlayerId(1);
    let (_, receiver) = add_player(&mut app, id);
    add_player(&mut app, PlayerId(2));
    for (y, support) in [
        (1.0, CharacterSupport::Airborne),
        (0.0, CharacterSupport::Airborne),
        (4.0, CharacterSupport::Ground),
        (0.0, CharacterSupport::Ladder),
    ] {
        stand(&mut app, id, Position { y, ..inside(1) }, support);
        advance(&mut app, 0.0);
        assert!(saved(&app, id).is_none());
    }
    for (number, expected) in [(1, 0), (1, 0), (2, 1), (1, 1)] {
        stand(&mut app, id, inside(number), CharacterSupport::Ground);
        advance(&mut app, 0.0);
        assert_eq!(saved(&app, id), Some(CheckpointId(expected)));
    }
    assert!(saved(&app, PlayerId(2)).is_none());
    let mut notifications = 0;
    while let Ok(message) = receiver.try_recv() {
        if matches!(message, ServerMessage::CheckpointReached(_)) {
            notifications += 1;
        }
    }
    assert_eq!(notifications, 2, "a passed checkpoint is no news");
    let delay = app.world().resource::<ServerGameplayConfig>().player.respawn_secs;
    app.world_mut().resource_mut::<PlayerMap>().disconnect(&id, delay);
    add_player(&mut app, id);
    assert!(saved(&app, id).is_none());
}

#[test]
fn deaths_preserve_checkpoints_clear_equipment_and_retry_blocked_group_or_individual_respawns() {
    for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
        let mut app = app(mode);
        let id = PlayerId(1);
        let (_, receiver) = add_player(&mut app, id);
        stand(&mut app, id, inside(1), CharacterSupport::Ground);
        advance(&mut app, 0.0);
        let full_health = app.world().resource::<ServerGameplayConfig>().combat.health.player.max;
        {
            let mut players = app.world_mut().resource_mut::<PlayerMap>();
            let player = players.get_mut(&id).expect("player missing");
            player.life.held_keys = vec![BarrierKindId(0)];
            player.life.power_ups.fill(PowerUpState::Permanent);
        }
        kill(&mut app, id);
        block(&mut app, 1, true);
        advance(&mut app, 2.1);
        assert!(
            app.world()
                .resource::<PlayerMap>()
                .get(&id)
                .expect("player missing")
                .is_dead()
        );
        block(&mut app, 1, false);
        advance(&mut app, 0.1);
        let players = app.world().resource::<PlayerMap>();
        let player = players.get(&id).expect("player missing");
        let entity = player.entity().expect("blocked respawn did not retry");
        assert_eq!(player.life.missiles, 0);
        assert!(player.life.held_keys.is_empty());
        assert!(
            player
                .life
                .power_ups
                .iter()
                .all(|power| matches!(power, PowerUpState::Inactive))
        );
        assert_eq!(
            app.world().get::<Health>(entity).expect("health missing").0,
            full_health
        );
        assert!(in_checkpoint(
            app.world().get::<Position>(entity).expect("position missing"),
            1
        ));
        assert!((app.world().get::<FaceYaw>(entity).expect("facing missing").0 - 0.7).abs() < 1e-5);
        assert_eq!(saved(&app, id), Some(CheckpointId(0)));
        stand(&mut app, id, inside(1), CharacterSupport::Ground);
        advance(&mut app, 0.0);
        let mut notifications = 0;
        while let Ok(message) = receiver.try_recv() {
            if matches!(message, ServerMessage::CheckpointReached(_)) {
                notifications += 1;
            }
        }
        assert_eq!(notifications, 1);
    }
}

#[test]
fn a_player_killed_at_a_checkpoint_does_not_activate_it() {
    let mut app = app(PlayerRespawnMode::Individual);
    let id = PlayerId(1);
    add_player(&mut app, id);
    stand(&mut app, id, inside(1), CharacterSupport::Ground);
    kill(&mut app, id);
    advance(&mut app, 0.0);
    assert!(saved(&app, id).is_none());
}

fn entries(app: &mut App, entries: &[(u32, usize)]) {
    let checkpoints = app.world().resource::<MapLayout>().checkpoints.clone();
    let entered = entries
        .iter()
        .map(|&(player, checkpoint)| {
            (
                PlayerId(player),
                PlayerCheckpoint {
                    id: CheckpointId(checkpoint),
                    facing: Vec3::new(player as f32, 0.0, 1.0).normalize(),
                },
            )
        })
        .collect();
    apply_checkpoint_entries(
        &mut app.world_mut().resource_mut::<PlayerMap>(),
        &checkpoints,
        entered,
        42,
    );
}

fn disconnect(app: &mut App, id: u32) {
    let info = app
        .world_mut()
        .resource_mut::<PlayerMap>()
        .disconnect(&PlayerId(id), 2.0)
        .expect("departing player missing");
    if let Some(entity) = info.entity() {
        app.world_mut().despawn(entity);
    }
}

#[test]
fn shared_checkpoints_work_with_both_respawn_policies() {
    for mode in [PlayerRespawnMode::Individual, PlayerRespawnMode::Group] {
        for kind in [CheckpointKind::GroupAny, CheckpointKind::GroupAll] {
            let mut app = app(mode);
            app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = kind;
            add_player(&mut app, PlayerId(1));
            add_player(&mut app, PlayerId(2));
            entries(&mut app, &[(1, 1), (2, 1)]);
            for id in [PlayerId(1), PlayerId(2)] {
                assert_eq!(saved(&app, id), Some(CheckpointId(1)));
            }
            kill(&mut app, PlayerId(1));
            advance(&mut app, 2.1);
            for id in [PlayerId(1), PlayerId(2)] {
                assert_eq!(saved(&app, id), Some(CheckpointId(1)));
                if id == PlayerId(1) || mode == PlayerRespawnMode::Group {
                    assert!(in_checkpoint(&body_position(&app, id), 2));
                }
            }
        }
    }
}

#[test]
fn group_all_visits_survive_death_and_membership_changes() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAll;
    let (_, first) = add_player(&mut app, PlayerId(1));
    let (_, second) = add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 1)]);
    assert!(saved(&app, PlayerId(1)).is_none());
    assert!(first.try_recv().is_err());
    kill(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(3));
    entries(&mut app, &[(2, 1)]);
    assert!(saved(&app, PlayerId(2)).is_none());
    disconnect(&mut app, 3);
    entries(&mut app, &[]);
    for id in [PlayerId(1), PlayerId(2)] {
        assert_eq!(saved(&app, id), Some(CheckpointId(1)));
        let player = app.world().resource::<PlayerMap>().get(&id).expect("player missing");
        assert!(player.session.checkpoint_visits.is_empty());
        assert_eq!(
            player.session.checkpoint.expect("saved checkpoint missing").facing,
            Vec3::new(1.0, 0.0, 1.0).normalize()
        );
    }
    let mut cues = 0;
    while let Ok(message) = second.try_recv() {
        if matches!(message, ServerMessage::CheckpointReached(_)) {
            cues += 1;
        }
    }
    assert_eq!(cues, 1);
    assert!(
        app.world()
            .resource::<PlayerMap>()
            .get(&PlayerId(1))
            .expect("player missing")
            .is_dead()
    );
    advance(&mut app, 2.1);
    assert!(in_checkpoint(&body_position(&app, PlayerId(1)), 2));
}

#[test]
fn a_shared_activation_clears_other_partial_visits_and_empty_sessions_reset() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAny;
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAll;
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 1)]);
    entries(&mut app, &[(2, 0)]);
    entries(&mut app, &[(2, 1)]);
    assert_eq!(
        saved(&app, PlayerId(1)),
        Some(CheckpointId(0)),
        "the activation cleared the earlier visit"
    );
    entries(&mut app, &[(1, 1)]);
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(1)));
    disconnect(&mut app, 1);
    assert!(app.world().resource::<PlayerMap>().shared_checkpoint.is_some());
    disconnect(&mut app, 2);
    assert!(app.world().resource::<PlayerMap>().shared_checkpoint.is_none());
}

#[test]
fn a_shared_activation_raises_only_players_below_it_and_defers_higher_entries() {
    let mut app = app(PlayerRespawnMode::Individual);
    let mut third = checkpoint(3);
    third.kind = CheckpointKind::GroupAny;
    app.world_mut().resource_mut::<MapLayout>().checkpoints.push(third);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAny;
    let (_, rx) = add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 1), (2, 2), (2, 0)]);
    assert_eq!(
        saved(&app, PlayerId(1)),
        Some(CheckpointId(1)),
        "an individual save past the activation stands"
    );
    assert_eq!(
        saved(&app, PlayerId(2)),
        Some(CheckpointId(0)),
        "the lowest shared entry activates first"
    );
    assert!(matches!(rx.try_recv(), Ok(ServerMessage::CheckpointReached(_))));
    assert!(rx.try_recv().is_err());
    entries(&mut app, &[(2, 2)]);
    assert_eq!(saved(&app, PlayerId(1)), Some(CheckpointId(2)));
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(2)));
}

#[test]
fn stationary_or_respawning_players_do_not_overwrite_teammates_individual_progress() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAny;
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    stand(&mut app, PlayerId(1), inside(1), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    stand(&mut app, PlayerId(2), inside(2), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(1)));
    kill(&mut app, PlayerId(1));
    advance(&mut app, 2.1);
    stand(&mut app, PlayerId(1), inside(1), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(1)));
    stand(&mut app, PlayerId(1), between(1), CharacterSupport::Airborne);
    advance(&mut app, 0.0);
    stand(&mut app, PlayerId(1), inside(1), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert_eq!(
        saved(&app, PlayerId(2)),
        Some(CheckpointId(1)),
        "returning to the active shared checkpoint does not either"
    );
}

#[test]
fn re_entering_the_active_shared_checkpoint_changes_nothing() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAny;
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAll;
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    let shared = inside(1);
    stand_facing(&mut app, PlayerId(1), shared, CharacterSupport::Ground, 0.7);
    advance(&mut app, 0.0);
    let facing = shared_facing(&app).expect("shared checkpoint not activated");
    stand(&mut app, PlayerId(2), inside(2), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert!(has_visit(&app, PlayerId(2), 1), "a partial group visit");

    // A jump in place: above the seeded contact's band the airborne tick drops
    // it, and the landing re-enters, facing another way.
    stand_facing(
        &mut app,
        PlayerId(1),
        Position { y: 1.0, ..shared },
        CharacterSupport::Airborne,
        2.0,
    );
    advance(&mut app, 0.0);
    stand_facing(&mut app, PlayerId(1), shared, CharacterSupport::Ground, 2.0);
    advance(&mut app, 0.0);
    assert_eq!(shared_facing(&app), Some(facing), "the saved facing stands");
    assert!(has_visit(&app, PlayerId(2), 1), "the partial visit stands");
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(0)));

    // Leaving and returning is the same re-entry.
    stand_facing(&mut app, PlayerId(1), between(1), CharacterSupport::Ground, 2.0);
    advance(&mut app, 0.0);
    stand_facing(&mut app, PlayerId(1), shared, CharacterSupport::Ground, 2.0);
    advance(&mut app, 0.0);
    assert_eq!(shared_facing(&app), Some(facing));
    assert!(has_visit(&app, PlayerId(2), 1));

    // The shared checkpoint further along still activates once everyone has visited it.
    stand(&mut app, PlayerId(1), inside(2), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(1)), Some(CheckpointId(1)));
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(1)));
}

#[test]
fn simultaneous_shared_entries_activate_on_consecutive_ticks() {
    let mut app = app(PlayerRespawnMode::Individual);
    app.world_mut().resource_mut::<MapLayout>().checkpoints[0].kind = CheckpointKind::GroupAny;
    app.world_mut().resource_mut::<MapLayout>().checkpoints[1].kind = CheckpointKind::GroupAny;
    let (_, rx) = add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    stand(&mut app, PlayerId(1), inside(1), CharacterSupport::Ground);
    stand(&mut app, PlayerId(2), inside(2), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert_eq!(
        saved(&app, PlayerId(1)),
        Some(CheckpointId(0)),
        "the lower number wins the tick"
    );
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(0)));

    advance(&mut app, 0.0);
    assert_eq!(
        saved(&app, PlayerId(1)),
        Some(CheckpointId(1)),
        "the other entry follows"
    );
    assert_eq!(saved(&app, PlayerId(2)), Some(CheckpointId(1)));
    let mut cues = 0;
    while let Ok(message) = rx.try_recv() {
        if matches!(message, ServerMessage::CheckpointReached(_)) {
            cues += 1;
        }
    }
    assert_eq!(cues, 2);
    advance(&mut app, 0.0);
    assert_eq!(
        saved(&app, PlayerId(2)),
        Some(CheckpointId(1)),
        "and nothing flips back"
    );
}

#[test]
fn checkpoint_progress_is_the_furthest_logged_in_players_number() {
    let mut app = app(PlayerRespawnMode::Individual);
    let checkpoints = app.world().resource::<MapLayout>().checkpoints.clone();
    let progress = |app: &App| super::checkpoint_progress(app.world().resource::<PlayerMap>(), &checkpoints);
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    assert_eq!(progress(&app), None);
    entries(&mut app, &[(1, 1), (2, 0)]);
    assert_eq!(progress(&app), Some(2));
    kill(&mut app, PlayerId(1));
    assert_eq!(progress(&app), Some(2), "a dead player's checkpoint still counts");
    disconnect(&mut app, 1);
    assert_eq!(progress(&app), Some(1), "a departure can move the course back");
}

#[test]
fn checkpoint_numbers_must_identify_one_placed_checkpoint() {
    let first = checkpoint(1);
    let mut second = first.clone();
    second.carrier = CarrierId(1);
    assert_eq!(super::checkpoint_numbered(std::slice::from_ref(&first), 1), Ok(CheckpointId(0)));
    assert!(
        super::checkpoint_numbered(&[first.clone(), second], 1)
            .expect_err("ambiguous number accepted")
            .contains("ambiguous")
    );
    let error = super::checkpoint_numbered(&[first, checkpoint(3)], 2).expect_err("unknown number accepted");
    assert!(error.contains("1, 3"), "{error}");
}
