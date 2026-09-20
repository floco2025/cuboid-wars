use common::protocol::Position;

use super::GroundNavigation;

impl GroundNavigation<'_> {
    pub(crate) fn has_goal_near(
        &self,
        start: Position,
        target: Position,
        radius: f32,
        goal: impl Fn(Position, f32) -> Option<Position>,
    ) -> bool {
        if goal(start, self.graphs.get(self.carrier).cell_size()).is_some() {
            return true;
        }
        self.graphs.iter().any(|(carrier, graph)| {
            let pose = self.carriers.pose(carrier);
            graph
                .nodes_near(pose.inverse_transform_position(&target), radius)
                .any(|node| goal(pose.transform_position(&graph.node_center(node)), graph.cell_size()).is_some())
        })
    }
}
