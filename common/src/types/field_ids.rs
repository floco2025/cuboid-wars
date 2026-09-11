use bincode::{Decode, Encode};

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct BarrierId(pub u32);

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct BridgeId(pub u32);
