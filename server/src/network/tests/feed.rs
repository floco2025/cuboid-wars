use bevy::prelude::Entity;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::*;
use crate::players::PlayerInfo;

fn players() -> (PlayerMap, UnboundedReceiver<ServerMessage>) {
    let (tx, rx) = unbounded_channel();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    info.connection.logged_in = true;
    let mut players = PlayerMap::default();
    players.insert(PlayerId(1), info);
    (players, rx)
}

fn chat() -> FeedEvent {
    FeedEvent::Chat {
        name: "Alex".to_owned(),
        text: "hi".to_owned(),
    }
}

fn receive(rx: &mut UnboundedReceiver<ServerMessage>) -> SFeed {
    match rx.try_recv().expect("feed line missing") {
        ServerMessage::Feed(line) => line,
        other => panic!("unexpected envelope: {other:?}"),
    }
}

fn text(line: &SFeed) -> String {
    line.spans.iter().map(|span| span.text.as_str()).collect()
}

#[test]
fn public_delivery_obeys_the_event_switch() {
    let (players, mut rx) = players();
    let mut config = FeedConfig::all(true, &[]);
    config.chat = false;

    emit_feed(&players, &config, FeedAudience::Everyone, chat());
    assert!(rx.try_recv().is_err());

    config.chat = true;
    emit_feed(&players, &config, FeedAudience::Everyone, chat());
    assert_eq!(text(&receive(&mut rx)), "Alex: hi");
}

#[test]
fn private_delivery_bypasses_public_switches() {
    let (players, mut rx) = players();
    let config = FeedConfig::all(false, &[]);

    emit_feed(
        &players,
        &config,
        FeedAudience::Player(PlayerId(1)),
        FeedEvent::AdminReply {
            text: "not authorized".to_owned(),
        },
    );

    assert_eq!(text(&receive(&mut rx)), "not authorized");
}

#[test]
fn everyone_except_skips_only_the_named_player() {
    let (mut players, mut first) = players();
    let (tx, mut second) = unbounded_channel();
    let mut info = PlayerInfo::new(Entity::PLACEHOLDER, tx);
    info.connection.logged_in = true;
    players.insert(PlayerId(2), info);

    emit_feed(
        &players,
        &FeedConfig::all(true, &[]),
        FeedAudience::EveryoneExcept(PlayerId(1)),
        chat(),
    );

    assert!(first.try_recv().is_err());
    assert_eq!(text(&receive(&mut second)), "Alex: hi");
}

#[test]
fn actor_switch_is_selected_by_kind() {
    let mut config = FeedConfig::all(false, &["scuttler", "bruiser"]);
    config.actor_destroyed.insert("bruiser".to_owned(), true);

    assert!(announces(
        &config,
        &FeedEvent::ActorDestroyed {
            name: "Alex".to_owned(),
            kind: "bruiser".to_owned(),
        }
    ));
    assert!(!announces(
        &config,
        &FeedEvent::ActorDestroyed {
            name: "Alex".to_owned(),
            kind: "scuttler".to_owned(),
        }
    ));
}

#[test]
fn death_wording_is_resolved_before_the_wire() {
    let line = render(FeedEvent::PlayerDied {
        name: "Alex".to_owned(),
        cause: DeathCause::Shot { by: "Bob".to_owned() },
    });

    assert_eq!(text(&line), "Bob shot Alex");
    assert_eq!(line.spans[0].style, FeedStyle::Default);
}

#[test]
fn switch_lines_name_the_switch_and_dim_the_release() {
    let on = render(FeedEvent::SwitchOn {
        name: "Alex".to_owned(),
        switch_name: "treasure".to_owned(),
    });
    assert_eq!(text(&on), "Alex turned on the treasure switch");
    assert_eq!(on.spans[0].style, FeedStyle::Default);

    let off = render(FeedEvent::SwitchOff {
        switch_name: "treasure".to_owned(),
    });
    assert_eq!(text(&off), "The treasure switch turned off");
    assert_eq!(off.spans[0].style, FeedStyle::Dim);
}

#[test]
fn every_death_cause_has_its_wording() {
    let bob = || "Bob".to_owned();
    let cases = [
        (DeathCause::SelfShot, "Alex shot themselves", FeedStyle::Default),
        (
            DeathCause::Missile { by: bob() },
            "Bob blew up Alex",
            FeedStyle::Default,
        ),
        (DeathCause::SelfMissile, "Alex blew themselves up", FeedStyle::Default),
        (
            DeathCause::Beam {
                kind: "zapper".to_owned(),
            },
            "Alex was zapped by a zapper",
            FeedStyle::Default,
        ),
        (
            DeathCause::ActorBlast {
                kind: "scuttler".to_owned(),
            },
            "Alex was blown up by a scuttler",
            FeedStyle::Default,
        ),
        (
            DeathCause::PlayerBlast { by: bob() },
            "Alex was caught in Bob's explosion",
            FeedStyle::Default,
        ),
        (DeathCause::Fall, "Alex fell", FeedStyle::Dim),
        (DeathCause::Crushed, "Alex was crushed", FeedStyle::Dim),
        (DeathCause::Admin, "Alex was killed by an admin", FeedStyle::Default),
    ];
    for (cause, expected, style) in cases {
        let line = render(FeedEvent::PlayerDied {
            name: "Alex".to_owned(),
            cause,
        });
        assert_eq!(text(&line), expected);
        assert_eq!(line.spans[0].style, style, "{expected}");
    }
}

#[test]
fn key_found_and_barrier_closed_color_only_the_kind_word() {
    let kind = BarrierKindId(1);
    let found = render(FeedEvent::KeyFound {
        name: "Alex".to_owned(),
        kind,
    });
    assert_eq!(text(&found), "Alex found a key");
    assert_eq!(found.spans[0].style, FeedStyle::Default);
    assert_eq!(found.spans[1].style, FeedStyle::Key(kind));
}

#[test]
fn console_chat_and_presence_lines_carry_their_styles() {
    let name = || "Alex".to_owned();
    let styled = |event: FeedEvent| {
        let line = render(event);
        (text(&line), line.spans[0].style)
    };

    assert_eq!(
        styled(FeedEvent::AdminReply {
            text: "/help".to_owned()
        }),
        ("/help".to_owned(), FeedStyle::Console)
    );
    assert_eq!(
        styled(FeedEvent::AdminAction {
            name: name(),
            text: "weather set to rain".to_owned(),
        }),
        ("Alex: weather set to rain".to_owned(), FeedStyle::Console)
    );
    assert_eq!(
        styled(FeedEvent::Chat {
            name: name(),
            text: "hi".to_owned(),
        }),
        ("Alex: hi".to_owned(), FeedStyle::Chat)
    );
    assert_eq!(
        styled(FeedEvent::PlayerJoined { name: name() }),
        ("Alex joined".to_owned(), FeedStyle::Dim)
    );
    assert_eq!(
        styled(FeedEvent::PlayerLeft { name: name() }),
        ("Alex left".to_owned(), FeedStyle::Dim)
    );
}
