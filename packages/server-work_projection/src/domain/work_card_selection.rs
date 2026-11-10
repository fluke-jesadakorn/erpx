//! Pure composition of Work Projection snapshot, capabilities and navigation.

use crate::domain::navigation_destinations::{NavigationShortcutFacts, shortcut_destination_ids};
use crate::domain::work_card_availability::{
    WorkCapabilitiesFact, WorkSnapshotFact, available_work_card_ids,
};
use std::collections::BTreeSet;

/// Returns the card IDs the native save path may accept for one actor.
pub fn available_work_cards(
    snapshot: &WorkSnapshotFact,
    capabilities: &WorkCapabilitiesFact,
    navigation: &NavigationShortcutFacts<'_>,
) -> BTreeSet<String> {
    let destinations = shortcut_destination_ids(navigation);
    available_work_card_ids(snapshot, capabilities, &destinations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::work_card_availability::{WorkItemFact, WorkQueue};

    fn navigation() -> NavigationShortcutFacts<'static> {
        NavigationShortcutFacts {
            beta: false,
            scope_is_department: false,
            executive_visible: false,
            commerce_read: true,
            inventory_read: true,
            finance_read: true,
            support_read: true,
            quality_read: true,
            records_read: true,
            active_modules: &[],
        }
    }

    #[test]
    fn composes_capability_creation_and_navigation_shortcuts() {
        let capabilities = WorkCapabilitiesFact {
            creation: vec!["expense".into(), "task".into()],
            team_queue: false,
            roster_access: false,
            unavailable: Vec::new(),
        };
        let available =
            available_work_cards(&WorkSnapshotFact::default(), &capabilities, &navigation());
        for id in [
            "expenses",
            "tasks",
            "customer-followups",
            "operations-stock",
            "operations-transit",
            "finance-payables",
            "support-cases",
            "quality-holds",
            "projects-list",
        ] {
            assert!(available.contains(id), "{id}");
        }
    }

    #[test]
    fn approval_items_and_team_capability_are_included() {
        let capabilities = WorkCapabilitiesFact {
            creation: Vec::new(),
            team_queue: true,
            roster_access: true,
            unavailable: Vec::new(),
        };
        let snapshot = WorkSnapshotFact {
            items: vec![WorkItemFact {
                id: "approval-1".into(),
                kind: "expense".into(),
                queue: WorkQueue::Todo,
                can_act: Some(true),
                action_kind: Some("approval".into()),
                relationships: Vec::new(),
                updated_at: 1,
                source_id: None,
                href: "/expenses/approval-1".into(),
            }],
        };
        let available = available_work_cards(&snapshot, &capabilities, &navigation());
        assert!(available.contains("approvals"));
        assert!(available.contains("expenses"));
        assert!(available.contains("team"));
        assert!(available.contains("shifts"));
    }

    #[test]
    fn beta_removes_business_shortcuts_but_keeps_capability_cards() {
        let capabilities = WorkCapabilitiesFact {
            creation: vec!["conversation".into()],
            team_queue: false,
            roster_access: false,
            unavailable: Vec::new(),
        };
        let mut facts = navigation();
        facts.beta = true;
        let available = available_work_cards(&WorkSnapshotFact::default(), &capabilities, &facts);
        assert!(available.contains("tasks"));
        assert!(!available.contains("customer-followups"));
    }
}
