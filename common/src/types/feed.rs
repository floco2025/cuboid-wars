use bincode::{Decode, Encode};

use super::field_kind::FieldKindId;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Encode, Decode)]
pub enum FeedStyle {
    Default,
    Dim,
    Chat,
    Console,
    Key(FieldKindId),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct FeedSpan {
    pub text: String,
    pub style: FeedStyle,
}
