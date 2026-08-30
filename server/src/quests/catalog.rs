use std::{collections::HashMap, ops::Deref};

use bevy::prelude::Resource;
use common::protocol::QuestId;

use crate::config::{Quest, ServerGameplayConfig};

#[derive(Debug, Clone)]
pub struct CatalogQuest {
    pub definition: Quest,
    pub order: u32,
    pub points: i32,
}

impl Deref for CatalogQuest {
    type Target = Quest;

    fn deref(&self) -> &Self::Target {
        &self.definition
    }
}

#[derive(Resource, Debug)]
pub struct QuestCatalog {
    quests: Vec<CatalogQuest>,
    by_id: HashMap<QuestId, usize>,
    dependents: HashMap<QuestId, Vec<usize>>,
}

impl QuestCatalog {
    #[must_use]
    pub fn from_config(config: &ServerGameplayConfig) -> Self {
        let quests: Vec<_> = config
            .quests
            .iter()
            .enumerate()
            .map(|(order, quest)| CatalogQuest {
                definition: quest.clone(),
                order: u32::try_from(order).expect("quest catalog order exceeds u32"),
                points: config.scoring.quest_completed[&quest.id],
            })
            .collect();
        let by_id = quests
            .iter()
            .enumerate()
            .map(|(index, quest)| (quest.id.clone(), index))
            .collect();
        let mut dependents: HashMap<QuestId, Vec<usize>> = HashMap::new();
        for (index, quest) in quests.iter().enumerate() {
            if let Some(required) = &quest.requires {
                dependents.entry(required.clone()).or_default().push(index);
            }
        }
        Self {
            quests,
            by_id,
            dependents,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &CatalogQuest> {
        self.quests.iter()
    }

    #[must_use]
    pub fn get(&self, id: &QuestId) -> Option<&CatalogQuest> {
        self.by_id.get(id).map(|index| &self.quests[*index])
    }

    pub fn dependents(&self, id: &QuestId) -> impl Iterator<Item = &CatalogQuest> {
        self.dependents
            .get(id)
            .into_iter()
            .flatten()
            .map(|index| &self.quests[*index])
    }
}
