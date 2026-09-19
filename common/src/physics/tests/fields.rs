use super::*;
use crate::protocol::{BarrierId, BridgeId, CarrierId};

fn barrier(id: u32, kind: FieldKindId) -> Barrier {
    Barrier {
        id: BarrierId(id),
        kind,
        switch: None,
        initially_on: true,
        x1: 0.0,
        z1: 0.0,
        x2: 1.0,
        z2: 0.0,
        width: 0.1,
        y: 0.0,
        height: 2.0,
        level: 0,
        levels: 1,
        carrier: CarrierId::WORLD,
    }
}

fn bridge(id: u32, kind: FieldKindId) -> LightBridge {
    LightBridge {
        id: BridgeId(id),
        kind,
        switch: None,
        initially_on: true,
        x1: 0.0,
        z1: 0.0,
        x2: 1.0,
        z2: 1.0,
        y: 3.0,
        thickness: 0.1,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

#[test]
fn matching_keys_open_every_field_of_their_kind_and_switches_open_individual_fields() {
    let barriers = [
        barrier(0, FieldKindId(1)),
        barrier(1, FieldKindId(1)),
        barrier(2, FieldKindId(2)),
    ];
    let bridges = [bridge(0, FieldKindId(1)), bridge(1, FieldKindId(2))];
    assert!(passable_fields(&[], &[], &barriers, &bridges).is_empty());
    assert_eq!(
        passable_fields(&[FieldKindId(1)], &[], &barriers, &bridges),
        [
            FieldId::Barrier(BarrierId(0)),
            FieldId::Barrier(BarrierId(1)),
            FieldId::Bridge(BridgeId(0)),
        ]
    );
    assert_eq!(
        passable_fields(&[], &[FieldId::Bridge(BridgeId(1))], &barriers, &bridges),
        [FieldId::Bridge(BridgeId(1))]
    );
    assert_eq!(
        passable_fields(
            &[FieldKindId(1)],
            &[FieldId::Barrier(BarrierId(0)), FieldId::Bridge(BridgeId(0))],
            &barriers,
            &bridges
        ),
        [
            FieldId::Barrier(BarrierId(0)),
            FieldId::Barrier(BarrierId(1)),
            FieldId::Bridge(BridgeId(0)),
        ]
    );
}
