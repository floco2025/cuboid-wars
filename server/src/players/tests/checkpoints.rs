use bevy::prelude::*;
use common::{
    physics::{CharacterSupport, CollisionWorld},
    protocol::{
        BarrierKindId, CarrierId, Checkpoint, CheckpointKind, FaceYaw, Floor, Health, MapLayout, PlayerId, Position,
        ServerMessage,
    },
};

use super::{
    CheckpointEntry, CheckpointId, PlayerCheckpoint, PlayerMap, PowerUpState,
    checkpoints::apply_checkpoint_entries,
    players_checkpoints_system,
    respawn_tests::{add_player, advance, kill, respawn_app, start_checkpoint},
};
use crate::{
    config::{ActorRespawnScope, PlayerRespawnMode, ServerGameplayConfig},
    map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig},
    schedule::ServerSet,
    test_geometry::geometry,
};

const GRID_COLS: i32 = 12;

// Checkpoint `number` sits on this column of the one-row grid, past the
// respawn fixture's start and actor zones.
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

// The start, then checkpoints 1 and 2, each on its own floor.
fn app(mode: PlayerRespawnMode) -> App {
    let mut app = respawn_app(mode, ActorRespawnScope::Dead);
    let checkpoints = vec![start_checkpoint(&geometry(GRID_COLS, 1)), checkpoint(1), checkpoint(2)];
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

fn set_kind(app: &mut App, number: u32, kind: CheckpointKind) {
    for checkpoint in &mut app.world_mut().resource_mut::<MapLayout>().checkpoints {
        if checkpoint.number == number {
            checkpoint.kind = kind;
        }
    }
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
        .entry
        .map(|entry| entry.facing)
}

fn has_visit(app: &App, id: PlayerId, number: u32) -> bool {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .expect("player missing")
        .session
        .checkpoint_visits
        .contains_key(&number)
}

fn saved(app: &App, id: PlayerId) -> u32 {
    app.world()
        .resource::<PlayerMap>()
        .get(&id)
        .expect("player missing")
        .session
        .checkpoint
        .number
}

fn body_position(app: &App, id: PlayerId) -> Position {
    let player = app.world().resource::<PlayerMap>().get(&id).expect("player missing");
    *app.world()
        .get::<Position>(player.entity().expect("respawn missing"))
        .expect("position missing")
}

fn cues(receiver: &crossbeam_channel::Receiver<ServerMessage>) -> Vec<u32> {
    std::iter::from_fn(|| receiver.try_recv().ok())
        .filter_map(|message| match message {
            ServerMessage::CheckpointReached(cue) => Some(cue.checkpoint),
            _ => None,
        })
        .collect()
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
        assert_eq!(saved(&app, id), 0);
    }
    for (number, expected) in [(1, 1), (1, 1), (2, 2), (1, 2)] {
        stand(&mut app, id, inside(number), CharacterSupport::Ground);
        advance(&mut app, 0.0);
        assert_eq!(saved(&app, id), expected);
    }
    assert_eq!(saved(&app, PlayerId(2)), 0);
    assert_eq!(cues(&receiver), [1, 2], "a passed checkpoint is no news");
    let delay = app.world().resource::<ServerGameplayConfig>().player.respawn_secs;
    app.world_mut().resource_mut::<PlayerMap>().disconnect(&id, delay);
    add_player(&mut app, id);
    assert_eq!(saved(&app, id), 0);
}

#[test]
fn standing_at_the_start_saves_nothing_and_stays_silent() {
    let mut app = app(PlayerRespawnMode::Individual);
    let id = PlayerId(1);
    let (_, receiver) = add_player(&mut app, id);
    let geometry = geometry(GRID_COLS, 1);
    let start = Position {
        x: geometry.cell_center_x(1),
        y: 0.0,
        z: geometry.cell_center_z(0),
    };
    stand(&mut app, id, start, CharacterSupport::Ground);
    advance(&mut app, 0.0);
    let player = app.world().resource::<PlayerMap>().get(&id).expect("player missing");
    assert_eq!(
        player.life.checkpoint_contact,
        Some(CheckpointId(0)),
        "the start is entered like any rectangle"
    );
    assert_eq!(player.session.checkpoint, PlayerCheckpoint::START);
    assert!(cues(&receiver).is_empty());
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
        assert_eq!(saved(&app, id), 1);
        stand(&mut app, id, inside(1), CharacterSupport::Ground);
        advance(&mut app, 0.0);
        assert_eq!(cues(&receiver), [1]);
    }
}

#[test]
fn a_respawn_lands_in_any_rectangle_of_its_number_and_keeps_its_facing_only_in_the_one_entered() {
    let mut app = app(PlayerRespawnMode::Individual);
    let geometry = geometry(GRID_COLS, 1);
    let twin = Checkpoint {
        cols: [col_of(3), col_of(3) + 1],
        min_x: geometry.cell_to_world_x(col_of(3)),
        max_x: geometry.cell_to_world_x(col_of(3) + 1),
        ..checkpoint(1)
    };
    {
        let mut layout = app.world_mut().resource_mut::<MapLayout>();
        layout.floors.push(floor(&twin));
        layout.checkpoints.push(twin);
    }
    let layout = app.world().resource::<MapLayout>().clone();
    app.insert_resource(CollisionWorld::from_map_layout(&layout));
    let id = PlayerId(1);
    add_player(&mut app, id);
    stand_facing(&mut app, id, inside(1), CharacterSupport::Ground, 0.7);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, id), 1);
    let mut landed_in_entered = 0;
    let mut landed_in_twin = 0;
    for _ in 0..40 {
        kill(&mut app, id);
        advance(&mut app, 2.1);
        let pos = body_position(&app, id);
        let entity = app
            .world()
            .resource::<PlayerMap>()
            .get(&id)
            .expect("player missing")
            .entity()
            .expect("respawn missing");
        let yaw = app.world().get::<FaceYaw>(entity).expect("facing missing").0;
        if in_checkpoint(&pos, 1) {
            landed_in_entered += 1;
            assert!((yaw - 0.7).abs() < 1e-5, "the entered rectangle keeps the saved facing");
        } else {
            assert!(in_checkpoint(&pos, 3), "{pos:?} is in neither rectangle of number 1");
            landed_in_twin += 1;
            assert!((yaw - 0.7).abs() > 1e-3, "the other rectangle faces the origin");
        }
        assert_eq!(saved(&app, id), 1);
    }
    assert!(landed_in_entered > 0 && landed_in_twin > 0);
}

#[test]
fn a_player_killed_at_a_checkpoint_does_not_activate_it() {
    let mut app = app(PlayerRespawnMode::Individual);
    let id = PlayerId(1);
    add_player(&mut app, id);
    stand(&mut app, id, inside(1), CharacterSupport::Ground);
    kill(&mut app, id);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, id), 0);
}

// `entries` pairs a player with the number it enters, through that number's
// first rectangle.
fn entries(app: &mut App, entries: &[(u32, u32)]) {
    let checkpoints = app.world().resource::<MapLayout>().checkpoints.clone();
    let entered = entries
        .iter()
        .map(|&(player, number)| {
            let index = checkpoints
                .iter()
                .position(|checkpoint| checkpoint.number == number)
                .expect("entered number has no rectangle");
            (
                PlayerId(player),
                PlayerCheckpoint {
                    number,
                    entry: Some(CheckpointEntry {
                        id: CheckpointId(index),
                        facing: Vec3::new(player as f32, 0.0, 1.0).normalize(),
                    }),
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
            set_kind(&mut app, 2, kind);
            add_player(&mut app, PlayerId(1));
            add_player(&mut app, PlayerId(2));
            entries(&mut app, &[(1, 2), (2, 2)]);
            for id in [PlayerId(1), PlayerId(2)] {
                assert_eq!(saved(&app, id), 2);
            }
            kill(&mut app, PlayerId(1));
            advance(&mut app, 2.1);
            for id in [PlayerId(1), PlayerId(2)] {
                assert_eq!(saved(&app, id), 2);
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
    set_kind(&mut app, 2, CheckpointKind::GroupAll);
    let (_, first) = add_player(&mut app, PlayerId(1));
    let (_, second) = add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 2)]);
    assert_eq!(saved(&app, PlayerId(1)), 0);
    assert!(first.try_recv().is_err());
    kill(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(3));
    entries(&mut app, &[(2, 2)]);
    assert_eq!(saved(&app, PlayerId(2)), 0);
    disconnect(&mut app, 3);
    entries(&mut app, &[]);
    for id in [PlayerId(1), PlayerId(2)] {
        assert_eq!(saved(&app, id), 2);
        let player = app.world().resource::<PlayerMap>().get(&id).expect("player missing");
        assert!(player.session.checkpoint_visits.is_empty());
        assert_eq!(
            player.session.checkpoint.entry.expect("saved entry missing").facing,
            Vec3::new(1.0, 0.0, 1.0).normalize()
        );
    }
    assert_eq!(cues(&second), [2]);
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
    set_kind(&mut app, 1, CheckpointKind::GroupAny);
    set_kind(&mut app, 2, CheckpointKind::GroupAll);
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 2)]);
    entries(&mut app, &[(2, 1)]);
    entries(&mut app, &[(2, 2)]);
    assert_eq!(saved(&app, PlayerId(1)), 1, "the activation cleared the earlier visit");
    entries(&mut app, &[(1, 2)]);
    assert_eq!(saved(&app, PlayerId(2)), 2);
    disconnect(&mut app, 1);
    assert_eq!(app.world().resource::<PlayerMap>().shared_checkpoint.number, 2);
    disconnect(&mut app, 2);
    assert_eq!(
        app.world().resource::<PlayerMap>().shared_checkpoint,
        PlayerCheckpoint::START
    );
}

#[test]
fn a_shared_activation_raises_only_players_below_it_and_defers_higher_entries() {
    let mut app = app(PlayerRespawnMode::Individual);
    let mut third = checkpoint(3);
    third.kind = CheckpointKind::GroupAny;
    app.world_mut().resource_mut::<MapLayout>().checkpoints.push(third);
    set_kind(&mut app, 1, CheckpointKind::GroupAny);
    let (_, rx) = add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 2), (2, 3), (2, 1)]);
    assert_eq!(
        saved(&app, PlayerId(1)),
        2,
        "an individual save past the activation stands"
    );
    assert_eq!(saved(&app, PlayerId(2)), 1, "the lowest shared entry activates first");
    assert_eq!(cues(&rx), [2]);
    entries(&mut app, &[(2, 3)]);
    assert_eq!(saved(&app, PlayerId(1)), 3);
    assert_eq!(saved(&app, PlayerId(2)), 3);
}

#[test]
fn stationary_or_respawning_players_do_not_overwrite_teammates_individual_progress() {
    let mut app = app(PlayerRespawnMode::Individual);
    set_kind(&mut app, 1, CheckpointKind::GroupAny);
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    stand(&mut app, PlayerId(1), inside(1), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    stand(&mut app, PlayerId(2), inside(2), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(2)), 2);
    kill(&mut app, PlayerId(1));
    advance(&mut app, 2.1);
    stand(&mut app, PlayerId(1), inside(1), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(2)), 2);
    stand(&mut app, PlayerId(1), between(1), CharacterSupport::Airborne);
    advance(&mut app, 0.0);
    stand(&mut app, PlayerId(1), inside(1), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert_eq!(
        saved(&app, PlayerId(2)),
        2,
        "returning to the active shared checkpoint does not either"
    );
}

#[test]
fn re_entering_the_active_shared_checkpoint_changes_nothing() {
    let mut app = app(PlayerRespawnMode::Individual);
    set_kind(&mut app, 1, CheckpointKind::GroupAny);
    set_kind(&mut app, 2, CheckpointKind::GroupAll);
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    let shared = inside(1);
    stand_facing(&mut app, PlayerId(1), shared, CharacterSupport::Ground, 0.7);
    advance(&mut app, 0.0);
    let facing = shared_facing(&app).expect("shared checkpoint not activated");
    stand(&mut app, PlayerId(2), inside(2), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert!(has_visit(&app, PlayerId(2), 2), "a partial group visit");

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
    assert!(has_visit(&app, PlayerId(2), 2), "the partial visit stands");
    assert_eq!(saved(&app, PlayerId(2)), 1);

    // Leaving and returning is the same re-entry.
    stand_facing(&mut app, PlayerId(1), between(1), CharacterSupport::Ground, 2.0);
    advance(&mut app, 0.0);
    stand_facing(&mut app, PlayerId(1), shared, CharacterSupport::Ground, 2.0);
    advance(&mut app, 0.0);
    assert_eq!(shared_facing(&app), Some(facing));
    assert!(has_visit(&app, PlayerId(2), 2));

    // The shared checkpoint further along still activates once everyone has visited it.
    stand(&mut app, PlayerId(1), inside(2), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(1)), 2);
    assert_eq!(saved(&app, PlayerId(2)), 2);
}

#[test]
fn simultaneous_shared_entries_activate_on_consecutive_ticks() {
    let mut app = app(PlayerRespawnMode::Individual);
    set_kind(&mut app, 1, CheckpointKind::GroupAny);
    set_kind(&mut app, 2, CheckpointKind::GroupAny);
    let (_, rx) = add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    stand(&mut app, PlayerId(1), inside(1), CharacterSupport::Ground);
    stand(&mut app, PlayerId(2), inside(2), CharacterSupport::Ground);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(1)), 1, "the lower number wins the tick");
    assert_eq!(saved(&app, PlayerId(2)), 1);

    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(1)), 2, "the other entry follows");
    assert_eq!(saved(&app, PlayerId(2)), 2);
    assert_eq!(cues(&rx), [1, 2]);
    advance(&mut app, 0.0);
    assert_eq!(saved(&app, PlayerId(2)), 2, "and nothing flips back");
}

#[test]
fn the_lowest_shared_number_wins_the_tick_whatever_the_compiled_order() {
    let mut app = app(PlayerRespawnMode::Individual);
    {
        let mut layout = app.world_mut().resource_mut::<MapLayout>();
        layout.checkpoints.reverse();
    }
    set_kind(&mut app, 1, CheckpointKind::GroupAny);
    set_kind(&mut app, 2, CheckpointKind::GroupAny);
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    entries(&mut app, &[(1, 1), (2, 2)]);
    assert_eq!(saved(&app, PlayerId(1)), 1, "number 1 sits at the higher index");
    assert_eq!(saved(&app, PlayerId(2)), 1);
    entries(&mut app, &[(1, 2)]);
    assert_eq!(saved(&app, PlayerId(2)), 2, "number 2 follows");
}

#[test]
fn a_shared_number_activates_through_any_of_its_rectangles() {
    let mut app = app(PlayerRespawnMode::Individual);
    set_kind(&mut app, 2, CheckpointKind::GroupAll);
    let geometry = geometry(GRID_COLS, 1);
    let twin = Checkpoint {
        kind: CheckpointKind::GroupAll,
        cols: [col_of(3), col_of(3) + 1],
        min_x: geometry.cell_to_world_x(col_of(3)),
        max_x: geometry.cell_to_world_x(col_of(3) + 1),
        ..checkpoint(2)
    };
    app.world_mut().resource_mut::<MapLayout>().checkpoints.push(twin);
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    let checkpoints = app.world().resource::<MapLayout>().checkpoints.clone();
    let entered = [(PlayerId(1), 2usize), (PlayerId(2), 3)]
        .map(|(player, index)| {
            (
                player,
                PlayerCheckpoint {
                    number: 2,
                    entry: Some(CheckpointEntry {
                        id: CheckpointId(index),
                        facing: Vec3::Z,
                    }),
                },
            )
        })
        .to_vec();
    apply_checkpoint_entries(
        &mut app.world_mut().resource_mut::<PlayerMap>(),
        &checkpoints,
        entered,
        42,
    );
    assert_eq!(
        saved(&app, PlayerId(1)),
        2,
        "one visit per rectangle completes the number"
    );
    assert_eq!(saved(&app, PlayerId(2)), 2);
}

#[test]
fn checkpoint_progress_is_the_furthest_logged_in_players_number() {
    let mut app = app(PlayerRespawnMode::Individual);
    let progress = |app: &App| super::checkpoint_progress(app.world().resource::<PlayerMap>());
    add_player(&mut app, PlayerId(1));
    add_player(&mut app, PlayerId(2));
    assert_eq!(progress(&app), 0);
    entries(&mut app, &[(1, 2), (2, 1)]);
    assert_eq!(progress(&app), 2);
    kill(&mut app, PlayerId(1));
    assert_eq!(progress(&app), 2, "a dead player's checkpoint still counts");
    disconnect(&mut app, 1);
    assert_eq!(progress(&app), 1, "a departure can move the course back");
    disconnect(&mut app, 2);
    assert_eq!(progress(&app), 0, "an empty server sits at the start");
}

#[test]
fn checkpoint_numbers_name_any_placed_checkpoint() {
    let first = checkpoint(1);
    let mut second = first.clone();
    second.carrier = CarrierId(1);
    assert_eq!(
        super::checkpoint_numbered(&[first.clone(), second], 1),
        Ok(PlayerCheckpoint::numbered(1))
    );
    let error = super::checkpoint_numbered(&[first, checkpoint(3)], 2).expect_err("unknown number accepted");
    assert!(error.contains("1, 3"), "{error}");
}
