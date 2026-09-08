use rapier3d::{
    parry::query::{
        ClosestPoints, Contact, NonlinearRigidMotion, QueryDispatcher, ShapeCastHit, ShapeCastOptions, Unsupported,
    },
    prelude::{Pose, Shape, Vector},
};

pub(super) struct CharacterQueryDispatcher<'a>(pub &'a dyn QueryDispatcher);

impl QueryDispatcher for CharacterQueryDispatcher<'_> {
    fn intersection_test(&self, pose: &Pose, a: &dyn Shape, b: &dyn Shape) -> Result<bool, Unsupported> {
        self.0.intersection_test(pose, a, b)
    }

    fn distance(&self, pose: &Pose, a: &dyn Shape, b: &dyn Shape) -> Result<f32, Unsupported> {
        self.0.distance(pose, a, b)
    }

    fn contact(
        &self,
        pose: &Pose,
        a: &dyn Shape,
        b: &dyn Shape,
        prediction: f32,
    ) -> Result<Option<Contact>, Unsupported> {
        self.0.contact(pose, a, b, prediction)
    }

    fn closest_points(
        &self,
        pose: &Pose,
        a: &dyn Shape,
        b: &dyn Shape,
        max_dist: f32,
    ) -> Result<ClosestPoints, Unsupported> {
        self.0.closest_points(pose, a, b, max_dist)
    }

    fn cast_shapes(
        &self,
        pose: &Pose,
        velocity: Vector,
        a: &dyn Shape,
        b: &dyn Shape,
        options: ShapeCastOptions,
    ) -> Result<Option<ShapeCastHit>, Unsupported> {
        let mut hit = self.0.cast_shapes(pose, velocity, a, b, options)?;
        if let Some(hit) = hit.as_mut() {
            let mut impact_pose = *pose;
            impact_pose.translation += velocity * hit.time_of_impact;
            // Rapier 0.32's slope decomposition can discard forward motion with imprecise capsule cast normals.
            if let Some(contact) = self.0.contact(&impact_pose, a, b, f32::MAX)? {
                hit.normal1 = contact.normal1.normalize_or_zero();
                hit.normal2 = contact.normal2.normalize_or_zero();
                hit.witness1 = contact.point1;
                hit.witness2 = contact.point2;
            }
        }
        Ok(hit)
    }

    fn cast_shapes_nonlinear(
        &self,
        motion1: &NonlinearRigidMotion,
        a: &dyn Shape,
        motion2: &NonlinearRigidMotion,
        b: &dyn Shape,
        start: f32,
        end: f32,
        stop: bool,
    ) -> Result<Option<ShapeCastHit>, Unsupported> {
        self.0.cast_shapes_nonlinear(motion1, a, motion2, b, start, end, stop)
    }
}
