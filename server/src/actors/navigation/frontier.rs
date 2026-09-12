use std::cmp::Ordering;

#[derive(Clone, Copy)]
pub(super) struct Frontier<N> {
    pub node: N,
    pub cost: f32,
    pub priority: f32,
    pub order: usize,
}

impl<N> PartialEq for Frontier<N> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl<N> Eq for Frontier<N> {}
impl<N> PartialOrd for Frontier<N> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<N> Ord for Frontier<N> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .total_cmp(&self.priority)
            .then_with(|| other.order.cmp(&self.order))
    }
}
