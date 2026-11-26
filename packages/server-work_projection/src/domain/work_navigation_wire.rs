//! Exact `NavigationSnapshot` serialization from native owner facts.
//!
//! `read_native_work_navigation` executes the owner SQL and returns compact
//! authorization facts. This module renders the wire value the retained Next
//! route produced. Presentation-only `WorkItem` fields are supplied by the
//! caller's owner read; this module never synthesizes them.

use crate::domain::work_card_availability::WorkQueue;
use crate::domain::work_navigation::{WorkAttentionFact, WorkNavigationFact};
use erpx_server_runtime::public::http_support::serde_json::{Map, Value, json};

fn queue_name(queue: WorkQueue) -> &'static str {
    match queue {
        WorkQueue::Todo => "todo",
        WorkQueue::Waiting => "waiting",
        WorkQueue::Completed => "completed",
    }
}

/// Renders the native attention rows with the retained `WorkItem` field names.
/// Presentation-only fields (`title`, `detail`, `status`, `flow`) are supplied
/// by the caller's owner presentation read, never synthesized here.
pub fn attention_items_json(attention: Option<&WorkAttentionFact>) -> Vec<Value> {
    attention
        .map(|value| {
            value
                .items
                .iter()
                .map(|item| {
                    json!({
                        "id": item.id,
                        "type": item.kind,
                        "queue": queue_name(item.queue),
                        "canAct": item.can_act,
                        "actionKind": item.action_kind,
                        "relationships": item.relationships,
                        "updatedAt": item.updated_at,
                        "sourceId": item.source_id,
                        "href": item.href,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Renders `{home,work,attention,workViewAvailability,snapshotAt}` exactly as
/// the browser `NavigationSnapshot` contract. `attention_items` are already
/// authorized full `WorkItem` values; `attention` facts control count, waiting
/// and incompleteness.
pub fn navigation_snapshot_json(
    fact: &WorkNavigationFact,
    now: i64,
    attention_items: Vec<Value>,
) -> Value {
    let mut map = Map::new();
    map.insert("home".to_owned(), json!(fact.home));
    map.insert("work".to_owned(), json!(fact.work));
    map.insert(
        "workViewAvailability".to_owned(),
        json!({
            "team": fact.work_view_availability.team,
            "routing": fact.work_view_availability.routing,
        }),
    );
    if let Some(value) = &fact.attention {
        let mut items = attention_items;
        items.truncate(3);
        map.insert(
            "attention".to_owned(),
            json!({
                "items": items,
                "total": value.total,
                "waiting": value.waiting,
                "incomplete": value.incomplete,
            }),
        );
        map.insert("snapshotAt".to_owned(), json!(now));
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::work_card_availability::{
        WorkCapabilitiesFact, WorkItemFact, WorkQueue, WorkSnapshotFact,
    };
    use crate::domain::work_navigation::{WorkAttentionFact, WorkNavigationFacts, work_navigation};

    #[test]
    fn renders_the_exact_browser_snapshot_keys() {
        let item = WorkItemFact {
            id: "task-1".into(),
            kind: "staff_task".into(),
            queue: WorkQueue::Todo,
            can_act: Some(true),
            action_kind: None,
            relationships: Vec::new(),
            updated_at: 1,
            source_id: None,
            href: "/work/task-1".into(),
        };
        let snapshot = WorkSnapshotFact {
            items: vec![item],
        };
        let fact = work_navigation(WorkNavigationFacts {
            snapshot: &snapshot,
            unavailable_sources: &[],
            capabilities: &WorkCapabilitiesFact::default(),
            home_unread: 4,
            include_attention: true,
            routing_attention: false,
        });
        assert_eq!(
            fact.attention,
            Some(WorkAttentionFact {
                items: vec![snapshot.items[0].clone()],
                total: 1,
                waiting: 0,
                incomplete: false,
            })
        );
        let wire = navigation_snapshot_json(
            &fact,
            1700000000000,
            vec![json!({"id": "task-1", "title": "Task", "href": "/work/task-1"})],
        );
        assert_eq!(
            wire,
            json!({
                "home": 4,
                "work": 1,
                "attention": {
                    "items": [{"id": "task-1", "title": "Task", "href": "/work/task-1"}],
                    "total": 1,
                    "waiting": 0,
                    "incomplete": false
                },
                "workViewAvailability": {"team": false, "routing": false},
                "snapshotAt": 1700000000000i64
            })
        );
    }

    #[test]
    fn compact_read_omits_attention_but_keeps_snapshot_at() {
        let snapshot = WorkSnapshotFact::default();
        let fact = work_navigation(WorkNavigationFacts {
            snapshot: &snapshot,
            unavailable_sources: &[],
            capabilities: &WorkCapabilitiesFact::default(),
            home_unread: 0,
            include_attention: false,
            routing_attention: false,
        });
        let wire = navigation_snapshot_json(&fact, 1, vec![]);
        assert_eq!(wire.as_object().unwrap().len(), 3);
        assert_eq!(wire["work"], 0);
        assert!(wire.get("attention").is_none());
        assert!(wire.get("snapshotAt").is_none());
    }
}
