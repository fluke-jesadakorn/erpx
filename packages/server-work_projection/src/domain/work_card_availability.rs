//! Pure Work Projection card availability.
//!
//! This is the native replacement for the visibility half of
//! `getWorkSummary(actor).cards`: it decides which catalogue and destination
//! cards a member may pin. It performs no I/O and grants no authority; the
//! caller supplies the already-authorized snapshot, capabilities and
//! navigation destinations. The TypeScript catalogue and shortcut registry are
//! mirrored exactly so the native save command cannot accept a hidden card.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkQueue {
    Todo,
    Waiting,
    Completed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkItemFact {
    pub id: String,
    pub kind: String,
    pub queue: WorkQueue,
    pub can_act: Option<bool>,
    pub action_kind: Option<String>,
    pub relationships: Vec<String>,
    pub updated_at: i64,
    pub source_id: Option<String>,
    pub href: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkSnapshotFact {
    pub items: Vec<WorkItemFact>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkCapabilitiesFact {
    pub creation: Vec<String>,
    pub team_queue: bool,
    pub roster_access: bool,
}

/// The exact catalogue IDs and the work-item type/creation kind each card maps
/// to. Cards without `new_kind` are driven by capabilities or existing items.
const CATALOGUE: &[(&str, Option<&str>, Option<&str>)] = &[
    ("tasks", Some("staff_task"), Some("task")),
    ("expenses", Some("expense"), Some("expense")),
    ("shifts", None, None),
    ("overtime", Some("overtime"), Some("overtime")),
    ("leave", Some("leave"), Some("leave")),
    ("services", Some("service_request"), Some("service_request")),
    ("equipment", Some("equipment_request"), Some("equipment_request")),
    ("purchases", Some("purchase_request"), Some("purchase_request")),
    ("approvals", None, None),
    ("team", None, None),
    ("counts", Some("stock_count"), Some("stock_count")),
    ("transfers", Some("transfer"), Some("transfer")),
];

/// Mirrors `shortcutDestinationIds` in the workplace catalogue.
const SHORTCUT_DESTINATIONS: &[&str] = &[
    "customer-followups",
    "crm-pipeline",
    "customer-timeline",
    "operations-quotations",
    "operations-sales",
    "operations-stock",
    "operations-purchasing",
    "procurement-rfqs",
    "operations-transit",
    "finance-payments",
    "finance-ledger",
    "finance-bank",
    "finance-payables",
    "finance-receivables",
    "projects-list",
    "projects-routines",
    "support-cases",
    "quality-holds",
    "assets",
    "maintenance",
    "field-service",
    "learning",
    "benefits",
    "talent",
    "marketing",
    "planning",
    "manufacturing",
    "executive-canvas",
];

fn is_my_action(item: &WorkItemFact) -> bool {
    item.queue == WorkQueue::Todo && item.can_act.unwrap_or(true)
}

fn is_approval_action(item: &WorkItemFact) -> bool {
    is_my_action(item)
        && (matches!(item.action_kind.as_deref(), Some("approval" | "override"))
            || (item.action_kind.is_none()
                && item.relationships.iter().any(|value| value == "approval")))
}

fn merge_key(item: &WorkItemFact) -> String {
    format!(
        "{}:{}:{}",
        item.kind,
        item.id,
        if item.action_kind.as_deref() == Some("override") {
            "override"
        } else {
            "record"
        }
    )
}

/// Mirrors `mergeWorkItems` for the fields that affect card visibility.
fn merged_items(items: &[WorkItemFact]) -> Vec<WorkItemFact> {
    let mut merged: BTreeMap<String, WorkItemFact> = BTreeMap::new();
    for item in items {
        let key = merge_key(item);
        match merged.get_mut(&key) {
            None => {
                merged.insert(key, item.clone());
            }
            Some(old) => {
                let actionable = if old.queue == WorkQueue::Todo {
                    old.clone()
                } else if item.queue == WorkQueue::Todo {
                    item.clone()
                } else if old.updated_at >= item.updated_at {
                    old.clone()
                } else {
                    item.clone()
                };
                let mut relationships = old.relationships.clone();
                for relationship in &item.relationships {
                    if !relationships.iter().any(|value| value == relationship) {
                        relationships.push(relationship.clone());
                    }
                }
                *old = WorkItemFact {
                    relationships,
                    ..actionable
                };
            }
        }
    }
    let mut values = merged.into_values().collect::<Vec<_>>();
    let holds = values
        .iter()
        .filter(|item| item.kind == "quality_hold")
        .map(|item| (item.id.clone(), item.clone()))
        .collect::<BTreeMap<_, _>>();
    for alert in values
        .iter()
        .filter(|item| item.kind == "quality_alert")
        .cloned()
        .collect::<Vec<_>>()
    {
        let hold_id = alert
            .source_id
            .clone()
            .or_else(|| alert.href.rsplit('/').next().map(str::to_owned));
        let Some(hold) = hold_id.as_ref().and_then(|id| holds.get(id)) else {
            continue;
        };
        if hold.queue == WorkQueue::Completed {
            continue;
        }
        if let Some(hold) = values.iter_mut().find(|item| item.kind == "quality_hold" && item.id == hold.id) {
            for relationship in &alert.relationships {
                if !hold.relationships.iter().any(|value| value == relationship) {
                    hold.relationships.push(relationship.clone());
                }
            }
            if is_my_action(&alert) && !is_my_action(hold) {
                hold.can_act = Some(true);
                hold.queue = WorkQueue::Todo;
                hold.action_kind = Some("work".into());
            }
        }
    }
    values.retain(|item| {
        if item.kind != "quality_alert" {
            return true;
        }
        let hold_id = item
            .source_id
            .clone()
            .or_else(|| item.href.rsplit('/').next().map(str::to_owned));
        hold_id
            .as_ref()
            .and_then(|id| holds.get(id))
            .is_none_or(|hold| hold.queue == WorkQueue::Completed)
    });
    values
}

fn card_matches(item: &WorkItemFact, id: &str, kind: Option<&str>) -> bool {
    if id == "shifts" {
        return false;
    }
    if let Some(kind) = kind {
        return item.kind == kind;
    }
    if id == "approvals" {
        return is_approval_action(item);
    }
    item.relationships.iter().any(|value| value == "team")
}

fn creation_enables(capabilities: &WorkCapabilitiesFact, id: &str, new_kind: Option<&str>) -> bool {
    capabilities.creation.iter().any(|kind| {
        Some(kind.as_str()) == new_kind || (id == "tasks" && kind == "conversation")
    })
}

/// Returns the exact card IDs the TypeScript summary would render for this
/// authorized snapshot. Destination IDs must come from the trusted navigation
/// projection; only registered shortcuts are accepted.
pub fn available_work_card_ids(
    snapshot: &WorkSnapshotFact,
    capabilities: &WorkCapabilitiesFact,
    destination_ids: &[String],
) -> BTreeSet<String> {
    let items = merged_items(&snapshot.items);
    let mut available = BTreeSet::new();
    for (id, kind, new_kind) in CATALOGUE {
        let matches = items
            .iter()
            .filter(|item| card_matches(item, id, *kind))
            .count();
        let create = creation_enables(capabilities, id, *new_kind);
        let visible = match *id {
            "shifts" => capabilities.roster_access,
            "team" => capabilities.team_queue,
            _ => create || matches > 0,
        };
        if visible {
            available.insert((*id).to_owned());
        }
    }
    for id in destination_ids {
        if SHORTCUT_DESTINATIONS.contains(&id.as_str()) {
            available.insert(id.clone());
        }
    }
    available
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, kind: &str, queue: WorkQueue) -> WorkItemFact {
        WorkItemFact {
            id: id.into(),
            kind: kind.into(),
            queue,
            can_act: None,
            action_kind: None,
            relationships: Vec::new(),
            updated_at: 1,
            source_id: None,
            href: format!("/work/{id}"),
        }
    }

    #[test]
    fn creation_and_capabilities_make_cards_available() {
        let capabilities = WorkCapabilitiesFact {
            creation: vec!["task".into(), "conversation".into()],
            team_queue: true,
            roster_access: true,
        };
        let available = available_work_card_ids(
            &WorkSnapshotFact::default(),
            &capabilities,
            &[],
        );
        assert!(available.contains("tasks"));
        assert!(available.contains("team"));
        assert!(available.contains("shifts"));
        assert!(!available.contains("approvals"));
        assert!(!available.contains("expenses"));
    }

    #[test]
    fn actionable_approval_items_make_approvals_visible_but_completed_do_not() {
        let capabilities = WorkCapabilitiesFact::default();
        let mut approval = item("approval-1", "expense", WorkQueue::Todo);
        approval.relationships = vec!["approval".into()];
        let available = available_work_card_ids(
            &WorkSnapshotFact { items: vec![approval.clone()] },
            &capabilities,
            &[],
        );
        assert!(available.contains("approvals"));
        approval.queue = WorkQueue::Completed;
        let available = available_work_card_ids(
            &WorkSnapshotFact { items: vec![approval] },
            &capabilities,
            &[],
        );
        assert!(!available.contains("approvals"));
    }

    #[test]
    fn team_requires_the_capability_while_type_cards_require_real_items() {
        let mut team_item = item("team-1", "service_request", WorkQueue::Waiting);
        team_item.relationships = vec!["team".into()];
        let snapshot = WorkSnapshotFact {
            items: vec![team_item, item("expense-1", "expense", WorkQueue::Waiting)],
        };
        let without_team = available_work_card_ids(
            &snapshot,
            &WorkCapabilitiesFact::default(),
            &[],
        );
        assert!(!without_team.contains("team"));
        assert!(without_team.contains("expenses"));
        assert!(!without_team.contains("tasks"));
        let with_team = available_work_card_ids(
            &snapshot,
            &WorkCapabilitiesFact {
                team_queue: true,
                ..Default::default()
            },
            &[],
        );
        assert!(with_team.contains("team"));
    }

    #[test]
    fn destination_cards_are_limited_to_the_shortcut_registry() {
        let available = available_work_card_ids(
            &WorkSnapshotFact::default(),
            &WorkCapabilitiesFact::default(),
            &[
                "finance-payments".into(),
                "not-a-shortcut".into(),
            ],
        );
        assert!(available.contains("finance-payments"));
        assert!(!available.contains("not-a-shortcut"));
    }

    #[test]
    fn duplicate_records_prefer_the_actionable_item_and_merge_relationships() {
        let mut waiting = item("record-1", "purchase_request", WorkQueue::Waiting);
        waiting.relationships = vec!["team".into()];
        let mut todo = item("record-1", "purchase_request", WorkQueue::Todo);
        todo.relationships = vec!["approval".into()];
        let available = available_work_card_ids(
            &WorkSnapshotFact {
                items: vec![waiting, todo],
            },
            &WorkCapabilitiesFact::default(),
            &[],
        );
        assert!(available.contains("purchases"));
        assert!(available.contains("approvals"));
        assert!(!available.contains("team"));
    }
}
