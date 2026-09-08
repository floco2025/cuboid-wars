use bevy::prelude::*;
use rand::{
    RngExt, rng,
    rngs::ThreadRng,
    seq::{IndexedRandom, SliceRandom},
};

use crate::map::{ActorSpawnZone, CarrierGrid, MapConfig};
use common::{
    config::{ActorGameplayConfig, CharacterPhysicsConfig},
    map::{Carriers, MapGeometry},
    physics::{CollisionWorld, character_paths_intersect},
    protocol::{CarrierId, Position},
};

const SPAWN_MAX_ATTEMPTS: usize = 100;

// Initial facing for a freshly spawned character: toward the map origin (0,0).
// The negation + atan2 argument order is the non-obvious part, so the rule
// lives in one place shared by login and respawn.
#[must_use]
pub fn spawn_face_yaw(pos: &Position) -> f32 {
    (-pos.x).atan2(-pos.z)
}

// Pick a random clear position from any player spawn zone, on the map or
// on a nested map. All cells across all player zones are pooled and one is
// picked uniformly at random; no per-zone capacity tracking, no fallback.
// Used by login and player fall recovery.
//
// Returns the world origin if no player zone has any spawnable cells.
#[must_use]
pub fn generate_player_spawn_position(
    map_config: &MapConfig,
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> Position {
    let mut valid_cells = Vec::new();
    for zone in &map_config.player_spawn_zones {
        valid_cells.extend(collect_valid_cells(
            map_config.grid(zone.carrier),
            zone.level,
            zone.cells(),
        ));
    }
    pick_clear_position(
        &valid_cells,
        map_config,
        carriers,
        collision_world,
        occupied_positions,
        character_physics,
    )
    .unwrap_or_else(|| {
        warn!(
            "no clear player spawn position among {} spawnable cells, spawning at center",
            valid_cells.len()
        );
        Position::default()
    })
}

// Pick a clear position from a single actor spawn zone, on the zone's
// carrier. Used by the actor quota spawner — when topping a specific zone
// up, we never want to spill into other zones. `None` when the zone has no
// clear spot right now; the caller leaves the slot empty rather than spawn
// somewhere the actor does not belong.
#[must_use]
pub fn generate_actor_spawn_position_in_zone(
    map_config: &MapConfig,
    carriers: &Carriers,
    zone: &ActorSpawnZone,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    actor_config: &ActorGameplayConfig,
) -> Option<Position> {
    let character_physics = actor_config.physics();
    if actor_config.immovable {
        let grid = map_config.grid(zone.carrier);
        let mut cells: Vec<_> = zone.immovable_cells(grid).collect();
        cells.shuffle(&mut rng());
        return cells.into_iter().find_map(|(col, row)| {
            let local = Position {
                x: grid.geometry.cell_center_x(col),
                y: grid.geometry.level_y(zone.level),
                z: grid.geometry.cell_center_z(row),
            };
            let pos = carriers.pose(zone.carrier).transform_position(&local);
            character_spawn_position_is_clear(&pos, collision_world, occupied_positions, character_physics)
                .then_some(pos)
        });
    }
    let valid_cells = collect_valid_cells(map_config.grid(zone.carrier), zone.level, zone.cells());
    pick_clear_position(
        &valid_cells,
        map_config,
        carriers,
        collision_world,
        occupied_positions,
        character_physics,
    )
}

// (carrier, level, col, row) — the cell in its carrier's grid, same axis
// order as the file format's `cols`/`rows` arrays and the editor's drag
// tool. Internally the cell grid is indexed `[row][col]`, but that's local
// to the bounds checks below.
type SpawnCell = (CarrierId, u8, i32, i32);

fn collect_valid_cells(grid: &CarrierGrid, level: u8, cells: impl Iterator<Item = (i32, i32)>) -> Vec<SpawnCell> {
    let Some(level_grid) = grid.levels.get(level as usize) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let grid_cells = &level_grid.cells.rows;
    for (col, row) in cells {
        if row < 0 || row >= grid_cells.len() as i32 {
            continue;
        }
        if col < 0 || col >= grid_cells[row as usize].len() as i32 {
            continue;
        }
        let cell = &grid_cells[row as usize][col as usize];
        if cell.is_spawnable() {
            out.push((grid.carrier, level, col, row));
        }
    }
    out
}

// The cell's position is in its carrier's frame; the carrier's pose at this
// tick puts it in the world, where the colliders are. `None` when no cell
// is spawnable or no random spot came up clear.
fn pick_clear_position(
    valid_cells: &[SpawnCell],
    map_config: &MapConfig,
    carriers: &Carriers,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> Option<Position> {
    let mut rng = rng();
    for _ in 0..SPAWN_MAX_ATTEMPTS {
        let &(carrier, level, col, row) = valid_cells.choose(&mut rng)?;
        let geometry = &map_config.grid(carrier).geometry;
        let Some(local) = random_position_in_spawn_cell(&mut rng, geometry, level, col, row, character_physics) else {
            continue;
        };
        let pos = carriers.pose(carrier).transform_position(&local);

        if character_spawn_position_is_clear(&pos, collision_world, occupied_positions, character_physics) {
            return Some(pos);
        }
    }
    None
}

fn random_position_in_spawn_cell(
    rng: &mut ThreadRng,
    geometry: &MapGeometry,
    level: u8,
    col: i32,
    row: i32,
    character_physics: CharacterPhysicsConfig,
) -> Option<Position> {
    if character_physics.movement_collider.diameter > geometry.cell_size() {
        return None;
    }
    let cell_min_x = geometry.cell_to_world_x(col);
    let cell_max_x = cell_min_x + geometry.cell_size();
    let cell_min_z = geometry.cell_to_world_z(row);
    let cell_max_z = cell_min_z + geometry.cell_size();

    Some(Position {
        x: rng.random_range(
            (cell_min_x + character_physics.movement_collider.radius())
                ..=(cell_max_x - character_physics.movement_collider.radius()),
        ),
        y: geometry.level_y(level),
        z: rng.random_range(
            (cell_min_z + character_physics.movement_collider.radius())
                ..=(cell_max_z - character_physics.movement_collider.radius()),
        ),
    })
}

fn character_spawn_position_is_clear(
    pos: &Position,
    collision_world: &CollisionWorld,
    occupied_positions: &[Position],
    character_physics: CharacterPhysicsConfig,
) -> bool {
    !occupied_positions
        .iter()
        .any(|other| character_position_intersects_character(pos, other, character_physics))
        && !collision_world.character_overlaps_wall(pos, character_physics)
}

fn character_position_intersects_character(
    pos: &Position,
    other: &Position,
    character_physics: CharacterPhysicsConfig,
) -> bool {
    character_paths_intersect(pos, pos, character_physics, other, other, character_physics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        map::{CarrierGrid, CellGrid, EdgeGrid, LevelGrid, MapConfig, PlayerSpawnZone},
        test_geometry::{LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS, geometry},
    };
    use common::protocol::{Carrier, CarrierId, MapLayout, Wall};

    fn empty_layout() -> MapLayout {
        MapLayout::default()
    }

    fn collision_world(layout: &MapLayout) -> CollisionWorld {
        CollisionWorld::from_map_layout(layout, &common::protocol::BarrierKindTable::default())
    }

    fn character_physics() -> CharacterPhysicsConfig {
        crate::config::ServerGameplayConfig::load_default()
            .expect("default server gameplay config should load")
            .gameplay_config()
            .player
            .physics()
    }

    fn actor_config(kind: &str) -> ActorGameplayConfig {
        crate::config::ServerGameplayConfig::load_default()
            .expect("gameplay config rejected")
            .expect_actor(kind)
            .character
            .clone()
    }

    fn map_config_with_player_spawn(level: u8, col: i32, row: i32) -> MapConfig {
        let mut levels = (0..=level)
            .map(|_| LevelGrid {
                cells: CellGrid::new(2, 2),
                edges: EdgeGrid::new(2, 2),
                barrier_edges: EdgeGrid::new(2, 2),
            })
            .collect::<Vec<_>>();
        levels[usize::from(level)].cells.rows[row as usize][col as usize].has_floor = true;
        MapConfig {
            player_spawn_zones: vec![PlayerSpawnZone {
                carrier: CarrierId::WORLD,
                level,
                cols: [col, col + 1],
                rows: [row, row + 1],
            }],
            ..MapConfig::for_grid(levels, geometry(2, 2))
        }
    }

    #[test]
    fn spawn_position_rejects_other_player_overlap() {
        let layout = empty_layout();
        let collision_world = collision_world(&layout);
        let pos = Position::default();

        assert!(!character_spawn_position_is_clear(
            &pos,
            &collision_world,
            &[pos],
            character_physics()
        ));
    }

    #[test]
    fn spawn_position_rejects_wall_overlap() {
        let mut layout = empty_layout();
        layout.walls.push(Wall {
            x1: -1.0,
            z1: 0.0,
            x2: 1.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        });
        let collision_world = collision_world(&layout);

        assert!(!character_spawn_position_is_clear(
            &Position::default(),
            &collision_world,
            &[],
            character_physics()
        ));
    }

    #[test]
    fn spawn_position_ignores_wall_on_other_level() {
        let mut layout = empty_layout();
        layout.walls.push(Wall {
            x1: -1.0,
            z1: 0.0,
            x2: 1.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 1,
            y: LEVEL_HEIGHT,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        });
        let collision_world = collision_world(&layout);

        assert!(character_spawn_position_is_clear(
            &Position::default(),
            &collision_world,
            &[],
            character_physics()
        ));
    }

    fn floor_level(cols: i32, rows: i32, floored: &[(i32, i32)]) -> LevelGrid {
        let mut level = LevelGrid {
            cells: CellGrid::new(cols, rows),
            edges: EdgeGrid::new(cols, rows),
            barrier_edges: EdgeGrid::new(cols, rows),
        };
        for &(col, row) in floored {
            level.cells.rows[row as usize][col as usize].has_floor = true;
        }
        level
    }

    // A 2x2 nested grid resting at `rest`, holding one scuttler zone on its
    // (1, 1) cell: a floor, or a ramp, which is never spawnable.
    fn nested_zone_fixture(rest: Position, floored: bool) -> (MapConfig, Carriers, ActorSpawnZone) {
        let mut map_config = MapConfig::for_grid(vec![floor_level(2, 2, &[])], geometry(2, 2));
        let mut nested = floor_level(2, 2, &[(1, 1)]);
        if !floored {
            nested.cells.rows[1][1].has_ramp = true;
        }
        map_config
            .grids
            .push(CarrierGrid::new(CarrierId(1), geometry(2, 2), vec![nested]));
        let carriers = Carriers::from_layout(&MapLayout {
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: rest,
                to: rest,
                travel_ticks: 1,
                pause_ticks: 0,
                phase_ticks: 0,
            }],
            ..MapLayout::default()
        });
        let zone = ActorSpawnZone {
            carrier: CarrierId(1),
            level: 0,
            cols: [1, 2],
            rows: [1, 2],
            kind: "scuttler".to_owned(),
            count: 1,
        };
        (map_config, carriers, zone)
    }

    #[test]
    fn actor_spawn_in_a_nested_zone_goes_through_the_carriers_pose() {
        let collision_world = collision_world(&empty_layout());
        let rest = Position {
            x: 30.0,
            y: LEVEL_HEIGHT,
            z: -10.0,
        };
        let (map_config, carriers, zone) = nested_zone_fixture(rest, true);

        let pos = generate_actor_spawn_position_in_zone(
            &map_config,
            &carriers,
            &zone,
            &collision_world,
            &[],
            &actor_config("scuttler"),
        )
        .expect("floored cell rejected");

        let local = carriers.pose(CarrierId(1)).inverse_transform_position(&pos);
        let geometry = geometry(2, 2);
        assert_eq!(pos.y, LEVEL_HEIGHT);
        assert_eq!(local.y, 0.0);
        assert!(local.x >= geometry.cell_to_world_x(1) && local.x <= geometry.cell_to_world_x(2));
        assert!(local.z >= geometry.cell_to_world_z(1) && local.z <= geometry.cell_to_world_z(2));
    }

    #[test]
    fn actor_spawn_in_a_zone_without_a_spawnable_cell_yields_nothing() {
        let collision_world = collision_world(&empty_layout());
        let (map_config, carriers, zone) = nested_zone_fixture(Position::default(), false);

        assert!(
            generate_actor_spawn_position_in_zone(
                &map_config,
                &carriers,
                &zone,
                &collision_world,
                &[],
                &actor_config("scuttler"),
            )
            .is_none()
        );
    }

    #[test]
    fn immovable_spawn_uses_the_cell_center_in_its_carriers_frame() {
        let world = collision_world(&empty_layout());
        let (map, carriers, zone) = nested_zone_fixture(
            Position {
                x: 30.0,
                y: 4.0,
                z: -10.0,
            },
            true,
        );
        let geometry = map.grid(zone.carrier).geometry;
        let center = Position {
            x: geometry.cell_center_x(1),
            y: 0.0,
            z: geometry.cell_center_z(1),
        };
        let expected = carriers.pose(zone.carrier).transform_position(&center);
        let turret = actor_config("turret");
        for _ in 0..10 {
            assert_eq!(
                generate_actor_spawn_position_in_zone(&map, &carriers, &zone, &world, &[], &turret),
                Some(expected)
            );
        }
        assert!(generate_actor_spawn_position_in_zone(&map, &carriers, &zone, &world, &[expected], &turret).is_none());
    }

    #[test]
    fn immovable_spawn_checks_every_cell_before_reporting_a_full_zone() {
        let world = collision_world(&empty_layout());
        let map = MapConfig::for_grid(
            vec![floor_level(120, 1, &(0..120).map(|c| (c, 0)).collect::<Vec<_>>())],
            geometry(120, 1),
        );
        let zone = ActorSpawnZone {
            carrier: CarrierId::WORLD,
            level: 0,
            cols: [0, 120],
            rows: [0, 1],
            kind: "turret".into(),
            count: 120,
        };
        let geometry = map.root_grid().geometry;
        let centers: Vec<_> = (0..120)
            .map(|col| Position {
                x: geometry.cell_center_x(col),
                y: 0.0,
                z: geometry.cell_center_z(0),
            })
            .collect();
        let turret = actor_config("turret");
        let carriers = Carriers::default();
        assert_eq!(
            generate_actor_spawn_position_in_zone(&map, &carriers, &zone, &world, &centers[..119], &turret),
            Some(centers[119])
        );
        assert!(generate_actor_spawn_position_in_zone(&map, &carriers, &zone, &world, &centers, &turret).is_none());
    }

    #[test]
    fn immovable_spawn_waits_instead_of_shifting_away_from_an_obstructed_center() {
        let geometry = geometry(2, 2);
        let layout = MapLayout {
            walls: vec![Wall {
                x1: geometry.cell_to_world_x(1),
                z1: geometry.cell_center_z(1),
                x2: geometry.cell_to_world_x(2),
                z2: geometry.cell_center_z(1),
                width: WALL_THICKNESS,
                level: 0,
                y: 0.0,
                height: WALL_HEIGHT,
                carrier: CarrierId::WORLD,
            }],
            ..Default::default()
        };
        let world = collision_world(&layout);
        let map = MapConfig::for_grid(vec![floor_level(2, 2, &[(1, 1)])], geometry);
        let zone = ActorSpawnZone {
            carrier: CarrierId::WORLD,
            level: 0,
            cols: [1, 2],
            rows: [1, 2],
            kind: "turret".into(),
            count: 1,
        };
        assert!(
            generate_actor_spawn_position_in_zone(
                &map,
                &Carriers::default(),
                &zone,
                &world,
                &[],
                &actor_config("turret")
            )
            .is_none()
        );
    }

    #[test]
    fn player_spawn_position_uses_configured_spawn_level() {
        let layout = empty_layout();
        let collision_world = collision_world(&layout);
        let map_config = map_config_with_player_spawn(1, 0, 0);

        let pos = generate_player_spawn_position(
            &map_config,
            &Carriers::default(),
            &collision_world,
            &[],
            character_physics(),
        );

        assert_eq!(pos.y, LEVEL_HEIGHT);
    }
}
