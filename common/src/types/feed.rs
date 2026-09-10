use bincode::{Decode, Encode};

use super::barrier_kind::BarrierKindId;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FeedStyle {
    Default,
    Dim,
    Chat,
    Console,
    Barrier(BarrierKindId),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct FeedSpan {
    pub text: String,
    pub style: FeedStyle,
}
