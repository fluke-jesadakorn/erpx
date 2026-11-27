//! Exact `NavigationSnapshot` serialization from native owner facts.
//!
//! Renders the browser contract from authorized Work facts and owner-provided
//! presentations.

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

/// Renders the browser field names from the owner fields carried by each item.
pub fn attention_items_json(attention: Option<&WorkAttentionFact>) -> Vec<Value> {
    attention
        .map(|value| {
            value
                .items
                .iter()
                .take(3)
                .filter_map(|item| {
                    let card = item.presentation.as_ref()?;
                    let mut view = json!({
                        "id": item.source_id.as_ref().unwrap_or(&item.id),
                        "type": item.kind,
                        "title": card.title,
                        "detail": card.detail,
                        "href": item.href,
                        "status": card.status,
                        "updatedAt": item.updated_at,
                        "queue": queue_name(item.queue),
                        "responsibleQueue": card.responsible_queue,
                        "nextAction": card.next_action,
                        "dueAt": card.due_at,
                        "blocker": card.blocker,
                        "relationships": item.relationships,
                    });
                    if let Some(can_act) = item.can_act {
                        view["canAct"] = json!(can_act);
                    }
                    if let Some(kind) = &item.action_kind {
                        view["actionKind"] = json!(kind);
                    }
                    Some(view)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Renders the browser envelope using the cards selected by the owner.
pub fn navigation_snapshot_json(fact: &WorkNavigationFact, now: i64) -> Value {
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
        map.insert(
            "attention".to_owned(),
            json!({
                "items": attention_items_json(Some(value)),
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
            source_id: Some("task-1".into()),
            presentation: Some(crate::domain::work_card_availability::WorkItemPresentation {
                title: "Task".into(),
                detail: "Task details".into(),
                status: "open".into(),
                due_at: Some(2000),
                responsible_queue: "Owner".into(),
                next_action: "Review".into(),
                blocker: None,
            }),
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
        let wire = navigation_snapshot_json(&fact, 1700000000000);
        assert_eq!(
            wire,
            json!({
                "home": 4,
                "work": 1,
                "attention": {
                    "items": [{
                        "id": "task-1", "title": "Task", "type": "staff_task", "href": "/work/task-1",
                        "detail": "Task details", "status": "open", "updatedAt": 1, "queue": "todo",
                        "responsibleQueue": "Owner", "nextAction": "Review", "dueAt": 2000,
                        "blocker": null, "relationships": [], "canAct": true
                    }],
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
        let wire = navigation_snapshot_json(&fact, 1);
        assert_eq!(wire.as_object().unwrap().len(), 3);
        assert_eq!(wire["work"], 0);
        assert!(wire.get("attention").is_none());
        assert!(wire.get("snapshotAt").is_none());
    }
}
