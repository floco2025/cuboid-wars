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
        half_size: [20.0, 30.0],
        y: 4.4,
        settings: GroundsSettings {
            level: 1,
            margin: 50.0,
            return_secs: 8.0,
        },
    }
}

#[test]
fn terrain_joins_the_map_leaves_the_basement_open_and_faces_up() {
    let grounds = grounds();
    let mesh = grounds.mesh(false);
    for &[a, b, c] in &mesh.triangles {
        let [a, b, c] = [a, b, c].map(|i| mesh.vertices[i as usize]);
        assert!((b - a).cross(c - a).y > 0.0);
        let center = (a + b + c) / 3.0;
        assert!(center.x.abs() >= 20.0 || center.z.abs() >= 30.0);
    }
    for vertex in mesh
        .vertices
        .iter()
        .filter(|v| grounds.distance_outside_map(v.x, v.z).abs() < 0.001)
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
fn collision_triangles_match_the_visible_inner_terrain() {
    let grounds = grounds();
    let collision = grounds.mesh(false);
    let visual = grounds.mesh(true);
    let collision_side = collision.vertices.len() / 4;
    let visual_side = visual.vertices.len() / 4;
    for side in 0..4 {
        assert_eq!(
            collision.vertices[side * collision_side..(side + 1) * collision_side],
            visual.vertices[side * visual_side..side * visual_side + collision_side]
        );
    }
}

#[test]
fn boundary_timer_cancels_on_reentry_and_counts_down_from_the_start() {
    let mut timer = BoundaryTimer::default();
    assert_eq!(timer.tick(true, 3.0, 8.0), Some(5.0));
    assert_eq!(timer.tick(false, 10.0, 8.0), None);
    assert_eq!(timer.tick(true, 1.0, 8.0), Some(7.0));
    assert_eq!(timer.tick(true, 10.0, 8.0), Some(0.0));
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
