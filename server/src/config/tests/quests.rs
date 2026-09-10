use super::*;
use crate::config::{ActorKindServerConfig, ServerGameplayConfig};

fn ok_quest(id: &str, threshold: u32) -> Quest {
    Quest {
        id: QuestId(id.to_owned()),
        kind: QuestKind::Gold,
        scope: QuestScope::Individual,
        requires: None,
        actor_kind: None,
        threshold,
        points: 100,
        title: "Go".to_owned(),
        description: "go do the thing".to_owned(),
        completed_text: "done".to_owned(),
    }
}

fn no_actors() -> HashMap<String, ()> {
    HashMap::new()
}

fn default_actors() -> HashMap<String, ActorKindServerConfig> {
    ServerGameplayConfig::load_default()
        .expect("default server gameplay config should load")
        .actors
        .kinds
}

fn validate<T>(quests: &[Quest], actors: &HashMap<String, T>) -> Result<()> {
    validate_quests(quests, actors, "quests")
}

#[test]
fn validate_quests_accepts_single_valid_entry() {
    validate(&[ok_quest("a", 10)], &no_actors()).expect("valid quest should pass");
}

fn requiring(id: &str, requires: &str, scope: QuestScope) -> Quest {
    let mut quest = ok_quest(id, 1);
    quest.scope = scope;
    quest.requires = Some(QuestId(requires.to_owned()));
    quest
}

fn group(id: &str, scope: QuestScope) -> Quest {
    let mut quest = ok_quest(id, 1);
    quest.scope = scope;
    quest
}

#[test]
fn requires_accepts_an_earlier_group_quest() {
    let quests = [
        group("gold", QuestScope::Everyone),
        requiring("fireworks", "gold", QuestScope::Shared),
    ];
    validate(&quests, &no_actors()).expect("valid chain should pass");
}

#[test]
fn requires_rejects_unknown_quest() {
    let err = validate(&[requiring("a", "nope", QuestScope::Shared)], &no_actors())
        .expect_err("unknown prerequisite must fail");
    assert!(err.to_string().contains("unknown quest"));
}

#[test]
fn requires_rejects_a_later_quest() {
    let quests = [
        requiring("a", "b", QuestScope::Shared),
        group("b", QuestScope::Everyone),
    ];
    let err = validate(&quests, &no_actors()).expect_err("forward reference must fail");
    assert!(err.to_string().contains("defined earlier"));
}

#[test]
fn requires_rejects_itself() {
    let err = validate(&[requiring("a", "a", QuestScope::Shared)], &no_actors()).expect_err("self reference must fail");
    assert!(err.to_string().contains("itself"));
}

#[test]
fn requires_rejects_an_individual_target() {
    let quests = [ok_quest("solo", 1), requiring("b", "solo", QuestScope::Shared)];
    let err = validate(&quests, &no_actors()).expect_err("individual prerequisite must fail");
    assert!(err.to_string().contains("shared or everyone"));
}

#[test]
fn fireworks_quest_must_be_shared() {
    let mut quest = ok_quest("start_fireworks", 1);
    quest.kind = QuestKind::Fireworks;
    quest.scope = QuestScope::Everyone;
    let err = validate(&[quest], &no_actors()).expect_err("non-shared fireworks quest must fail");
    assert!(err.to_string().contains("must have scope `shared`"));
}

#[test]
fn fireworks_quest_rejects_actor_kind() {
    let mut quest = ok_quest("start_fireworks", 1);
    quest.kind = QuestKind::Fireworks;
    quest.scope = QuestScope::Shared;
    quest.actor_kind = Some("scuttler".to_owned());
    let err = validate(&[quest], &default_actors()).expect_err("actor_kind on a fireworks quest must fail");
    assert!(err.to_string().contains("only valid on an actor_kills quest"));
}

#[test]
fn validate_quests_accepts_empty_list() {
    validate(&[], &no_actors()).expect("maps may have no quests");
}

#[test]
fn validate_quests_rejects_duplicate_ids() {
    let err = validate(&[ok_quest("dup", 5), ok_quest("dup", 7)], &no_actors()).expect_err("dup ids must be rejected");
    assert!(err.to_string().contains("duplicated"));
}

#[test]
fn validate_quests_rejects_zero_threshold() {
    let err = validate(&[ok_quest("z", 0)], &no_actors()).expect_err("zero threshold must be rejected");
    assert!(err.to_string().contains("threshold"));
}

#[test]
fn validate_quests_rejects_empty_text_fields() {
    for field in ["title", "description", "completed_text"] {
        let mut quest = ok_quest("a", 1);
        match field {
            "title" => quest.title.clear(),
            "description" => quest.description.clear(),
            "completed_text" => quest.completed_text.clear(),
            _ => unreachable!(),
        }
        let err = validate(&[quest], &no_actors()).expect_err("empty quest text accepted");
        assert!(err.to_string().contains(field), "{field}: {err}");
    }
}

#[test]
fn validate_quests_accepts_actor_kills_with_known_actor_kind() {
    let mut quest = ok_quest("hunt", 4);
    quest.kind = QuestKind::ActorKills;
    quest.actor_kind = Some("bruiser".to_owned());
    validate(&[quest], &default_actors()).expect("known actor kind should pass");
}

#[test]
fn validate_quests_rejects_actor_kills_with_unknown_actor_kind() {
    let mut quest = ok_quest("hunt", 4);
    quest.kind = QuestKind::ActorKills;
    quest.actor_kind = Some("dragon".to_owned());
    let err = validate(&[quest], &default_actors()).expect_err("unknown actor kind must be rejected");
    assert!(err.to_string().contains("actor_kind"));
}

#[test]
fn validate_quests_rejects_actor_kind_on_non_actor_kills_quest() {
    let mut quest = ok_quest("oops", 4);
    quest.actor_kind = Some("bruiser".to_owned());
    let err = validate(&[quest], &default_actors()).expect_err("actor_kind on gold must be rejected");
    assert!(err.to_string().contains("actor_kind"));
}
