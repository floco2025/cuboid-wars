use bincode::{Decode, Encode};

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct BarrierId(pub u32);

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct BridgeId(pub u32);

// One barrier or light bridge instance: what a switch turns off and a
// collision query lets a body through.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub enum FieldId {
    Barrier(BarrierId),
    Bridge(BridgeId),
}
