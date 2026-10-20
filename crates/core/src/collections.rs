//! Smart collections (saved filter recipes) and keyword sets (named groups of
//! keywords for one-click tagging).

use serde::{Deserialize, Serialize};

use crate::{filters::FilterSet, FilterMode, RatingFilter};

/// A saved query: the facets, flag filter and rating filter that were active
/// when the collection was made. Opening it re-applies them to the catalog,
/// so its contents stay live as photos are culled and edited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartCollection {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub filters: FilterSet,
    #[serde(default)]
    pub filter_mode: FilterMode,
    #[serde(default)]
    pub rating: RatingFilter,
}

/// Every smart collection in a catalog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartCollections {
    pub items: Vec<SmartCollection>,
    next_id: u64,
}

impl SmartCollections {
    pub fn create(
        &mut self,
        name: &str,
        filters: FilterSet,
        filter_mode: FilterMode,
        rating: RatingFilter,
    ) -> Option<u64> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(SmartCollection {
            id,
            name: name.to_string(),
            filters,
            filter_mode,
            rating,
        });
        Some(id)
    }

    pub fn get(&self, id: u64) -> Option<&SmartCollection> {
        self.items.iter().find(|c| c.id == id)
    }

    pub fn delete(&mut self, id: u64) {
        self.items.retain(|c| c.id != id);
    }
}

/// A named group of keywords (Lightroom's keyword sets): clicking the set
/// applies all of its keywords at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeywordSet {
    pub name: String,
    pub keywords: Vec<String>,
}

/// The keyword sets a fresh catalog starts with.
pub fn default_keyword_sets() -> Vec<KeywordSet> {
    vec![
        KeywordSet {
            name: "People".into(),
            keywords: ["Portrait", "Family", "Friends", "Kids"]
                .map(String::from)
                .to_vec(),
        },
        KeywordSet {
            name: "Travel".into(),
            keywords: ["Travel", "Landscape", "Cityscape", "Architecture"]
                .map(String::from)
                .to_vec(),
        },
        KeywordSet {
            name: "Work".into(),
            keywords: ["Client", "Event", "Product", "Editorial"]
                .map(String::from)
                .to_vec(),
        },
        KeywordSet {
            name: "Output".into(),
            keywords: ["Portfolio", "Favourite", "To Print", "Retouch"]
                .map(String::from)
                .to_vec(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_collections_create_and_delete() {
        let mut collection_list = SmartCollections::default();
        let filters = FilterSet {
            keyword: Some("Travel".into()),
            ..Default::default()
        };
        let id = collection_list
            .create(
                "Travel keepers",
                filters.clone(),
                FilterMode::Picked,
                RatingFilter::AtLeast(3),
            )
            .unwrap();
        let saved = collection_list.get(id).unwrap();
        assert_eq!(saved.filters, filters);
        assert_eq!(saved.filter_mode, FilterMode::Picked);
        assert_eq!(saved.rating, RatingFilter::AtLeast(3));
        assert_eq!(
            collection_list.create("  ", filters, FilterMode::All, RatingFilter::Any),
            None
        );
        collection_list.delete(id);
        assert!(collection_list.items.is_empty());
    }
}
