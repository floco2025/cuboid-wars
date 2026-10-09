use super::*;

fn quest(id: &str, scope: QuestScope) -> Quest {
    Quest {
        id: QuestId(id.to_owned()),
        kind: QuestKind::Gold,
        scope,
        requires: None,
        actor_kind: None,
        threshold: 1,
        points: 100,
        title: "Go".to_owned(),
        description: "go do the thing".to_owned(),
        completed_text: "done".to_owned(),
    }
}

fn requiring(id: &str, requires: &str) -> Quest {
    Quest {
        requires: Some(QuestId(requires.to_owned())),
        ..quest(id, QuestScope::Shared)
    }
}

fn validate<T>(quests: &[Quest], actors: &HashMap<String, T>) -> Result<()> {
    validate_quests(quests, actors, "quests")
}

#[test]
fn requires_names_an_earlier_group_quest_other_than_itself() {
    let no_actors = HashMap::<String, ()>::new();
    validate(
        &[quest("gold", QuestScope::Everyone), requiring("fireworks", "gold")],
        &no_actors,
    )
    .expect("valid chain rejected");
    for (quests, expected) in [
        (vec![requiring("a", "nope")], "unknown quest"),
        (
            vec![requiring("a", "b"), quest("b", QuestScope::Everyone)],
            "defined earlier",
        ),
        (vec![requiring("a", "a")], "itself"),
        (
            vec![quest("solo", QuestScope::Individual), requiring("b", "solo")],
            "shared or everyone",
        ),
    ] {
        let err = validate(&quests, &no_actors).expect_err("invalid prerequisite accepted");
        assert!(err.to_string().contains(expected), "{err}");
    }
}

#[test]
fn fireworks_quest_must_be_shared() {
    let fireworks = Quest {
        kind: QuestKind::Fireworks,
        ..quest("start_fireworks", QuestScope::Everyone)
    };
    let err = validate(&[fireworks], &HashMap::<String, ()>::new()).expect_err("non-shared fireworks quest accepted");
    assert!(err.to_string().contains("must have scope `shared`"));
}

#[test]
fn actor_kind_names_a_known_kind_and_only_on_an_actor_kills_quest() {
    let actors = HashMap::from([("bruiser".to_owned(), ())]);
    let hunt = |kind: &str| Quest {
        kind: QuestKind::ActorKills,
        actor_kind: Some(kind.to_owned()),
        ..quest("hunt", QuestScope::Individual)
    };
    validate(&[hunt("bruiser")], &actors).expect("known actor kind rejected");
    let err = validate(&[hunt("dragon")], &actors).expect_err("unknown actor kind accepted");
    assert!(err.to_string().contains("not a known actor kind"), "{err}");
    let gold = Quest {
        actor_kind: Some("bruiser".to_owned()),
        ..quest("oops", QuestScope::Individual)
    };
    let err = validate(&[gold], &actors).expect_err("actor_kind on a gold quest accepted");
    assert!(err.to_string().contains("only valid on an actor_kills quest"), "{err}");
}
