use super::*;
use crate::protocol::CarrierId;

fn barrier(id: u32, kind: BarrierKindId) -> Barrier {
    Barrier {
        id: BarrierId(id),
        kind,
        switch: None,
        switch_inverted: false,
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

#[test]
fn matching_keys_open_every_barrier_of_their_kind_and_plates_open_individual_barriers() {
    let barriers = [
        barrier(0, BarrierKindId(1)),
        barrier(1, BarrierKindId(1)),
        barrier(2, BarrierKindId(2)),
    ];
    assert!(passable_barriers(&[], &[], &barriers).is_empty());
    assert_eq!(
        passable_barriers(&[BarrierKindId(1)], &[], &barriers),
        [BarrierId(0), BarrierId(1)]
    );
    assert_eq!(passable_barriers(&[], &[BarrierId(1)], &barriers), [BarrierId(1)]);
    assert_eq!(
        passable_barriers(&[BarrierKindId(1)], &[BarrierId(0), BarrierId(1)], &barriers),
        [BarrierId(0), BarrierId(1)]
    );
}
