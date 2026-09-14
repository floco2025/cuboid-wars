use super::*;
use crate::{
    config::gameplay::load_test_gameplay,
    map::Carriers,
    physics::{
        CharacterEnvironment, CharacterStep, CharacterSupport, CollisionWorld, LadderMode, step_character_movement,
    },
    protocol::{MapLayout, Position, TERRAIN_MATERIAL},
};

fn grounds() -> Grounds {
    Grounds {
        center: [0.0, 0.0],
        half_size: [20.0, 30.0],
        y: 4.4,
        settings: GroundsSettings { level: 1 },
    }
}

#[test]
fn terrain_joins_the_map_leaves_the_basement_open_and_faces_up() {
    let grounds = grounds();
    let mesh = grounds.mesh();
    for &[a, b, c] in &mesh.triangles {
        let [a, b, c] = [a, b, c].map(|i| mesh.vertices[i as usize]);
        assert!((b - a).cross(c - a).y > 0.0);
        let center = (a + b + c) / 3.0;
        assert!(center.x.abs() >= 20.0 || center.z.abs() >= 30.0);
    }
    for vertex in mesh
        .vertices
        .iter()
        .filter(|v| grounds.distance_outside_footprint(v.x, v.z).abs() < 0.001)
    {
        assert!((vertex.y - grounds.y).abs() < 0.001);
    }
    let layout = MapLayout {
        grounds: Some(grounds),
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    assert!(world.ground_surface_below(Vec3::new(0.0, 15.0, 0.0), 30.0).is_none());
    let hit = world
        .ground_surface_below(Vec3::new(24.0, 15.0, 0.0), 30.0)
        .expect("surrounding ground missing");
    assert!((hit.point.y - 4.4).abs() < 0.001);
    assert_eq!(world.surface_material(&hit, &layout), Some(TERRAIN_MATERIAL));
}

#[test]
fn offset_footprints_keep_terrain_and_decorations_on_all_four_sides() {
    let grounds = Grounds {
        center: [1000.0, -800.0],
        ..grounds()
    };
    let mesh = grounds.mesh();
    for vertex in &mesh.vertices {
        assert!(grounds.distance_outside_footprint(vertex.x, vertex.z) >= -0.001);
        assert_eq!(vertex.y, grounds.height(vertex.x, vertex.z));
    }
    let decorations = grounds.decorations();
    for direction in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
        assert!(
            decorations.iter().any(|decoration| {
                let offset = Vec2::new(decoration.position.x, decoration.position.z) - Vec2::from(grounds.center);
                offset.dot(direction) > 600.0
            }),
            "decorations must reach every side of an offset base"
        );
    }
    assert!(decorations.iter().all(|decoration| {
        (1.5..=DECORATION_EXTENT)
            .contains(&grounds.distance_outside_footprint(decoration.position.x, decoration.position.z))
    }));
}

#[test]
fn an_empty_footprint_makes_a_filled_mesh_without_zero_area_triangles() {
    let grounds = Grounds {
        center: [-194.0, -34.0],
        half_size: [0.0, 0.0],
        ..grounds()
    };
    let mesh = grounds.mesh();
    for &[a, b, c] in &mesh.triangles {
        let [a, b, c] = [a, b, c].map(|i| mesh.vertices[i as usize]);
        assert!((b - a).cross(c - a).y > 0.0);
    }
    assert!(mesh.vertices.contains(&Vec3::new(-194.0, grounds.y, -34.0)));
}

#[test]
fn terrain_stays_flat_through_the_map_join() {
    let grounds = grounds();
    for (x, z) in [
        (0.0, 0.0),
        (20.0, 0.0),
        (-20.0 - HILL_BLEND_START, 4.0),
        (7.0, 30.0 + HILL_BLEND_START),
    ] {
        assert_eq!(grounds.height(x, z), grounds.y);
    }
}

#[test]
fn exterior_has_visible_rolling_height_variation() {
    let grounds = grounds();
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for z in -6..=6 {
        for x in -6..=6 {
            let x = x as f32 * 18.0;
            let z = z as f32 * 18.0;
            let height = grounds.height(x, z);
            min = min.min(height);
            max = max.max(height);
        }
    }
    assert!(max - min > 10.0, "terrain height range was only {} metres", max - min);
}

#[test]
fn terrain_reaches_past_the_decorations_and_is_closed_around_the_map() {
    let grounds = grounds();
    let mesh = grounds.mesh();
    let farthest = mesh
        .vertices
        .iter()
        .map(|v| grounds.distance_outside_footprint(v.x, v.z))
        .fold(0.0f32, f32::max);
    assert!((farthest - grounds.extent()).abs() < 0.01);
    assert!(grounds.extent() > 700.0, "the terrain reaches past the decorations");
    let decorations = grounds.decorations();
    assert!(
        decorations
            .iter()
            .all(|d| grounds.distance_outside_footprint(d.position.x, d.position.z) < grounds.extent())
    );
    assert_eq!(
        grounds.collidable_decorations().len(),
        decorations.iter().filter(|d| d.collides()).count(),
        "every solid decoration collides wherever it stands"
    );
}

#[test]
fn a_player_lands_on_the_terrain_and_walks_across_its_triangles() {
    let grounds = grounds();
    let world = CollisionWorld::from_map_layout(&MapLayout {
        grounds: Some(grounds),
        ..Default::default()
    });
    let carriers = Carriers::default();
    let environment = CharacterEnvironment {
        collision_world: &world,
        carriers: &carriers,
        physics: load_test_gameplay().expect("test gameplay rejected").player.physics(),
        gravity: 25.0,
        passable_kinds: &[],
        ladder_climb_ratio: 0.5,
        ladder_mode: LadderMode::Automatic,
        portals: None,
    };
    let mut pos = Position {
        x: 24.0,
        y: 9.0,
        z: -8.0,
    };
    let mut velocity = 0.0;
    let mut landed = false;
    for _ in 0..120 {
        let result = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity: velocity,
                control_velocity: Vec3::Z * 4.0,
                external_displacement: Vec3::ZERO,
                delta: 1.0 / 30.0,
            },
            &environment,
        );
        pos = result.position;
        velocity = result.vertical_velocity;
        if result.support == CharacterSupport::Ground {
            landed = true;
        }
        assert!(!result.crushed);
        assert!(pos.y >= 4.39);
    }
    assert!(landed);
    assert!(pos.z > 7.0);
}

#[test]
fn decorations_keep_off_the_map_and_only_reachable_ones_collide() {
    let grounds = grounds();
    let all = grounds.decorations();
    assert!(
        all.len() > 1000,
        "a jittered grid over the grounds places thousands of decorations"
    );
    let mut trees = Vec::new();
    let mut rocks = Vec::new();
    for decoration in &all {
        let outside = grounds.distance_outside_footprint(decoration.position.x, decoration.position.z);
        let clearance = match decoration.kind {
            DecorationKind::Tree | DecorationKind::Rock(RockClass::Boulder) => 8.0,
            DecorationKind::Rock(RockClass::Stone) => 4.0,
            DecorationKind::Rock(RockClass::Pebble) => 1.5,
        };
        assert!(outside >= clearance, "a decoration sits on the map seam");
        assert!(outside <= 700.0);
        let ground = grounds.height(decoration.position.x, decoration.position.z);
        match decoration.kind {
            DecorationKind::Tree => {
                assert!((decoration.position.y - (ground - 0.2)).abs() < 0.001);
                trees.push(decoration);
            }
            DecorationKind::Rock(class) => {
                let (min, max) = class.size_range();
                assert!(decoration.scale.x >= min && decoration.scale.x <= max);
                assert!(decoration.position.y < ground && decoration.position.y > ground - max);
                if class == RockClass::Pebble {
                    assert!(outside <= 160.0, "pebbles stop where nobody sees them");
                }
                rocks.push(decoration);
            }
        }
    }
    for class in RockClass::ALL {
        assert!(rocks.iter().any(|rock| rock.kind == DecorationKind::Rock(class)));
    }
    for rock in &rocks {
        let clear = rock.scale.x + 1.0 - 0.01;
        assert!(
            trees.iter().all(
                |tree| Vec2::new(tree.position.x - rock.position.x, tree.position.z - rock.position.z).length()
                    >= clear
            ),
            "a rock sits in a trunk"
        );
    }
    let collidable = grounds.collidable_decorations();
    assert!(!collidable.is_empty() && collidable.len() < all.len());
    for decoration in &collidable {
        assert_ne!(decoration.kind, DecorationKind::Rock(RockClass::Pebble));
    }
}

#[test]
fn rocks_within_finds_every_rock_that_reaches_a_cell() {
    let grounds = grounds();
    let rocks: Vec<_> = grounds
        .decorations()
        .into_iter()
        .filter(|d| matches!(d.kind, DecorationKind::Rock(_)))
        .collect();
    let min = Vec2::new(60.0, -40.0);
    let max = min + Vec2::splat(10.0);
    let expected: Vec<_> = rocks
        .iter()
        .filter(|rock| {
            let center = Vec2::new(rock.position.x, rock.position.z);
            let nearest = center.clamp(min, max);
            center.distance(nearest) <= rock.scale.x * 2.0
        })
        .collect();
    assert!(!expected.is_empty(), "the sample cell has rocks reaching into it");
    let found = grounds.rocks_within(min, max);
    for rock in expected {
        assert!(
            found.iter().any(|f| f.position == rock.position),
            "a rock reaching the cell is missing"
        );
    }
}

// The terrain is one trimesh of tens of thousands of triangles, so a step
// whose queries scale with the mesh instead of what the body touches is a
// hitch on every tick outdoors: this is the regression a coarse bound
// catches, not a benchmark.
#[test]
fn a_step_on_the_grounds_touches_only_the_terrain_under_the_body() {
    let grounds = grounds();
    let world = CollisionWorld::from_map_layout(&MapLayout {
        grounds: Some(grounds.clone()),
        ..Default::default()
    });
    let carriers = Carriers::default();
    let physics = load_test_gameplay().expect("test gameplay rejected").player.physics();
    let environment = CharacterEnvironment {
        collision_world: &world,
        carriers: &carriers,
        physics,
        gravity: 25.0,
        passable_kinds: &[],
        ladder_climb_ratio: 0.5,
        ladder_mode: LadderMode::Automatic,
        portals: None,
    };
    let (x, z) = (120.0, 40.0);
    let mut pos = Position {
        x,
        y: grounds.height(x, z) + 0.05,
        z,
    };
    let mut velocity = 0.0;
    let started = std::time::Instant::now();
    let steps = 100;
    for _ in 0..steps {
        let result = step_character_movement(
            CharacterStep {
                start: pos,
                vertical_velocity: velocity,
                control_velocity: Vec3::new(0.7, 0.0, 0.7) * 6.0,
                external_displacement: Vec3::ZERO,
                delta: 1.0 / 30.0,
            },
            &environment,
        );
        pos = result.position;
        velocity = result.vertical_velocity;
    }
    let elapsed = started.elapsed();
    assert!(pos.x > x + 10.0, "the body walked over the hills");
    // Under 100 µs a step when bounded; 15 ms and up when a contact query
    // scans the mesh.
    assert!(
        elapsed < std::time::Duration::from_millis(steps * 2),
        "{steps} steps took {elapsed:?}"
    );
}
