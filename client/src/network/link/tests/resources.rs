use super::*;
use common::protocol::{SFeed, SFirework};
use crossbeam_channel::unbounded;

#[test]
fn local_link_stops_where_the_sink_breaks_and_keeps_the_rest() {
    let (sender, receiver) = unbounded();
    let mut link = ServerLink::Local(receiver);
    sender
        .send(ServerMessage::Feed(SFeed { spans: Vec::new() }))
        .expect("send failed");
    sender
        .send(ServerMessage::Firework(SFirework { seed: 1 }))
        .expect("send failed");
    sender
        .send(ServerMessage::Firework(SFirework { seed: 2 }))
        .expect("send failed");

    let mut seen = Vec::new();
    link.receive(Instant::now(), |message| {
        let stop = matches!(message, ServerMessage::Firework(_));
        seen.push(message);
        if stop {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })
    .expect("link closed");
    assert!(matches!(
        seen.as_slice(),
        [ServerMessage::Feed(_), ServerMessage::Firework(SFirework { seed: 1 })]
    ));

    let mut rest = Vec::new();
    link.receive(Instant::now(), |message| {
        rest.push(message);
        ControlFlow::Continue(())
    })
    .expect("link closed");
    assert!(matches!(
        rest.as_slice(),
        [ServerMessage::Firework(SFirework { seed: 2 })]
    ));

    drop(sender);
    assert!(link.receive(Instant::now(), |_| ControlFlow::Continue(())).is_err());
}
