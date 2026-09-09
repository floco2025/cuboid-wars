use super::*;

#[test]
fn insufficient_portal_space_dry_clicks_regardless_of_material() {
    for alias in ["test", "blocked"] {
        for (width, height) in [(0.2, 4.0), (4.0, 0.2)] {
            let mut layout = textured_layout(&placement_layout());
            layout.walls[0].x1 = -width / 2.0;
            layout.walls[0].x2 = width / 2.0;
            layout.walls[0].height = height;
            layout.wall_materials[0] = FaceMaterials::uniform(alias);
            let result = material_shot(&layout, Vec3::new(0.0, height / 2.0, 3.0), Vec3::NEG_Z);
            assert!(
                matches!(result, Err(PortalPlacementFailure::InvalidPlacement)),
                "{width} by {height} wall with {alias:?} material did not dry-click: {result:?}"
            );
        }
    }
}

#[test]
fn incompatible_material_fizzles_when_a_geometric_nudge_finds_space() {
    let mut layout = textured_layout(&placement_layout());
    layout.wall_materials[0] = FaceMaterials::uniform("blocked");
    let origin = Vec3::new(0.0, 0.1, 3.0);
    let geometric_fit = place_on_geometry(
        origin,
        Vec3::NEG_Z,
        0.0,
        40.0,
        &CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()),
        &layout,
        &Carriers::default(),
        &[],
    )
    .expect("wall has no geometric fit after nudging");
    assert!(geometric_fit.pos.y > origin.y + 0.5);
    assert!(matches!(
        material_shot(&layout, origin, Vec3::NEG_Z),
        Err(PortalPlacementFailure::IncompatibleMaterial(_))
    ));
}

#[test]
fn portal_permissions_follow_the_hit_face_and_never_shoot_through_it() {
    let mut layout = textured_layout(&placement_layout());
    layout.wall_materials[0].south = "blocked".to_owned();
    let mut behind = layout.walls[0];
    behind.z1 = -2.0;
    behind.z2 = -2.0;
    layout.walls.push(behind);
    layout.wall_materials.push(FaceMaterials::uniform("test"));
    let result = material_shot(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::NEG_Z);
    let Err(PortalPlacementFailure::IncompatibleMaterial(impact)) = result else {
        panic!("blocked front face did not fizzle: {result:?}");
    };
    assert!((impact.pos.z - WALL_THICKNESS / 2.0).abs() < 1e-4);
    assert!(material_shot(&layout, Vec3::new(0.0, 1.6, -1.0), Vec3::Z).is_ok());
}

#[test]
fn portal_floor_and_ceiling_have_independent_permissions() {
    let mut layout = textured_layout(&placement_layout());
    layout.floor_materials[0].top = "blocked".to_owned();
    assert!(matches!(
        material_shot(&layout, Vec3::new(0.0, 3.0, 3.0), Vec3::NEG_Y),
        Err(PortalPlacementFailure::IncompatibleMaterial(_))
    ));
    assert!(material_shot(&layout, Vec3::new(0.0, -3.0, 3.0), Vec3::Y).is_ok());
}

#[test]
fn portal_ramp_slope_uses_the_top_material_even_on_a_steep_ramp() {
    let layout = MapLayout {
        ramps: vec![Ramp {
            x1: -3.0,
            x2: 3.0,
            y1: 0.0,
            y2: 10.0,
            z1: 0.0,
            z2: 5.0,
            carrier: CarrierId::WORLD,
        }],
        ramp_materials: vec![FaceMaterials {
            top: "blocked".to_owned(),
            ..FaceMaterials::uniform("test")
        }],
        ..Default::default()
    };
    let normal = Vec3::new(0.0, 1.0, -2.0).normalize();
    let target = Vec3::new(0.0, 5.0, 2.5);
    assert!(matches!(
        material_shot(&layout, target + normal * 3.0, -normal),
        Err(PortalPlacementFailure::IncompatibleMaterial(_))
    ));
}

#[test]
fn portal_fit_detects_a_narrow_forbidden_patch_between_backing_probes() {
    let mut layout = textured_layout(&placement_layout());
    let patch = Wall {
        x1: 0.1,
        x2: 0.12,
        y: 1.7,
        height: 0.02,
        ..layout.walls[0]
    };
    layout.walls.push(patch);
    layout.wall_materials.push(FaceMaterials::uniform("blocked"));
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let frame = PortalFrame::from_surface(Vec3::new(0.0, 1.6, WALL_THICKNESS / 2.0), Vec3::Z, 0.0);
    assert!(!world.portal_materials_allow(&frame, &layout, &test_textures()));
    let placement = material_shot(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::NEG_Z)
        .expect("shot did not nudge clear of the forbidden patch");
    assert!(placement.pos.distance(frame.center) > 0.1);
    assert!(world.portal_materials_allow(
        &PortalFrame::from_surface(placement.pos, placement.normal, placement.yaw),
        &layout,
        &test_textures()
    ));
}

#[test]
fn portal_cannot_nudge_off_a_direct_hit_on_incompatible_material() {
    let mut layout = textured_layout(&placement_layout());
    layout.walls[0].x2 = 0.0;
    layout.wall_materials[0] = FaceMaterials::uniform("blocked");
    let right = Wall {
        x1: 0.0,
        x2: 6.0,
        ..layout.walls[0]
    };
    layout.walls.push(right);
    layout.wall_materials.push(FaceMaterials::uniform("test"));
    assert!(matches!(
        material_shot(&layout, Vec3::new(-0.05, 1.6, 3.0), Vec3::NEG_Z),
        Err(PortalPlacementFailure::IncompatibleMaterial(_))
    ));
    let placement = material_shot(&layout, Vec3::new(0.05, 1.6, 3.0), Vec3::NEG_Z)
        .expect("permitted side did not nudge clear of the material boundary");
    assert!(placement.pos.x >= PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE);
}

#[test]
fn incompatible_material_impact_keeps_its_carrier_local_position() {
    let mut layout = textured_layout(&placement_layout());
    layout.walls[0].carrier = CarrierId(1);
    layout.wall_materials[0] = FaceMaterials::uniform("blocked");
    layout.carriers.push(Carrier {
        parent: CarrierId::WORLD,
        level: 0,
        levels: 0,
        from: Vec3::new(8.0, 2.0, 4.0).into(),
        to: Vec3::new(10.0, 2.0, 4.0).into(),
        travel_ticks: 30,
        pause_ticks: 0,
        phase_ticks: 15,
    });
    let carriers = Carriers::from_layout(&layout);
    let origin = carriers.pose(CarrierId(1)).transform_point(Vec3::new(0.0, 1.6, 3.0));
    let result = material_shot(&layout, origin, Vec3::NEG_Z);
    let Err(PortalPlacementFailure::IncompatibleMaterial(impact)) = result else {
        panic!("carried material did not reject: {result:?}");
    };
    assert_eq!(impact.carrier, CarrierId(1));
    let wire = impact.portal(PortalPairId(1), PortalEnd::B, &carriers);
    assert!((wire.pos.x).abs() < 1e-4);
    assert!((wire.pos.y - 1.6).abs() < 1e-4);
    assert!((wire.pos.z - WALL_THICKNESS / 2.0).abs() < 1e-4);
}
