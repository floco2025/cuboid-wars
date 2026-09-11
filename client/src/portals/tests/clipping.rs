use super::*;

fn frames() -> (PortalFrame, PortalFrame) {
    (
        PortalFrame::from_surface(Vec3::new(0.0, 1.6, 0.0), Vec3::Z, 0.0),
        PortalFrame::from_surface(Vec3::new(10.0, 1.0, 10.0), Vec3::X, 0.0),
    )
}

fn floor_pair() -> (PortalFrame, PortalFrame) {
    (
        PortalFrame::from_surface(Vec3::ZERO, Vec3::Y, 0.0),
        PortalFrame::from_surface(Vec3::new(10.0, 0.0, 0.0), Vec3::Y, 0.0),
    )
}

#[test]
fn clip_plane_keeps_the_front_of_the_gate() {
    let (entry, _) = frames();
    let plane = clip_plane(&entry);
    let signed_distance = |p: Vec3| p.dot(plane.xyz()) + plane.w;
    assert!(signed_distance(entry.center + entry.normal * 0.2) > 0.0);
    assert!(signed_distance(entry.center - entry.normal * 0.2) < 0.0);
    assert!(signed_distance(entry.center + entry.up * 0.7).abs() < 1e-5);
}

#[test]
fn twin_transform_places_the_model_pose_mapped_through_the_pair() {
    let (entry, exit) = frames();
    let player = Transform::from_xyz(0.1, 0.7, 0.15).with_rotation(Quat::from_rotation_y(0.4));
    let model = Transform::from_xyz(0.0, 0.05, -0.02)
        .with_rotation(Quat::from_rotation_x(0.1))
        .with_scale(Vec3::splat(1.2));
    let model_world = player.mul_transform(model);
    let twin = twin_transform_for(&player, &model_world, &entry, &exit);
    let twin_world = player.mul_transform(twin);
    let expected = traverse_point(&entry, &exit, model_world.translation);
    assert!((twin_world.translation - expected).length() < 1e-4);
    let expected_rotation = traverse_rotation(&entry, &exit) * model_world.rotation;
    assert!(twin_world.rotation.angle_between(expected_rotation) < 1e-4);
    assert!((twin_world.scale - model_world.scale).length() < 1e-5);
    // A model point behind the entry plane comes out in front of the exit plane.
    let heel = model_world.translation - entry.normal * 0.3;
    let heel_offset = traverse_point(&entry, &exit, heel) - exit.center;
    assert!(heel_offset.dot(exit.normal) > 0.0);
}

#[test]
fn a_floor_handoff_starts_the_body_inverted_about_its_centre() {
    let (entry, exit) = floor_pair();
    let before = Quat::from_rotation_y(0.3);
    let after = Quat::from_rotation_y(1.0);
    let turn = handoff_turn(&entry, &exit, before, after);
    let centre = Vec3::new(10.0, 0.9, 0.0);
    let upright = Transform::from_xyz(10.0, 0.0, 0.0).with_rotation(after);
    let visual = pose_about(centre, turn, upright);
    assert!(
        (visual.rotation * Vec3::Y).y < -0.99,
        "the body did not start head down"
    );
    // The feet mirror through the centre: where the twin's mapped feet were.
    assert!((visual.translation - Vec3::new(10.0, 1.8, 0.0)).length() < 1e-4);
    assert!(
        pose_about(centre, Quat::IDENTITY, upright)
            .translation
            .distance(upright.translation)
            < 1e-6
    );
}

#[test]
fn a_wall_handoff_needs_no_turn() {
    let entry = PortalFrame::from_surface(Vec3::new(0.0, 1.0, 0.0), Vec3::Z, 0.0);
    let exit = PortalFrame::from_surface(Vec3::new(0.0, 1.0, 10.0), Vec3::NEG_Z, 0.0);
    let before = Quat::from_rotation_y(0.3);
    let after = traverse_rotation(&entry, &exit) * before;
    assert!((after * Vec3::Y).y > 0.99, "a facing pair keeps the body upright");
    let turn = handoff_turn(&entry, &exit, before, after);
    assert!(turn.angle_between(Quat::IDENTITY) < 1e-4);
}
