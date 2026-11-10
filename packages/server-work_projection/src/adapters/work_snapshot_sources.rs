//! Native Work Projection snapshot source adapters.
//!
//! Owner crates already produce their bounded work-item facts. These mappings
//! only translate the existing public owner contracts into the shared
//! availability fact; they perform no SQL, authority or command work.

use crate::domain::work_card_availability::{WorkItemFact, WorkQueue};
use erpx_server_field_service::public::work::FieldServiceWorkItem;
use erpx_server_support::public::incident::IncidentWorkItem;
use erpx_server_support::public::session::SessionWorkItem;
use erpx_server_support::public::ticket::TicketWorkItem;
use erpx_server_workflow::public::approval_oversight::ApprovalQueueRow;

fn queue(value: &str) -> WorkQueue {
    match value {
        "todo" => WorkQueue::Todo,
        "completed" => WorkQueue::Completed,
        _ => WorkQueue::Waiting,
    }
}

#[allow(clippy::too_many_arguments)]
fn item(
    prefix: &str,
    kind: &str,
    id: &str,
    href: &str,
    queue_value: &str,
    can_act: bool,
    updated_at: i64,
) -> WorkItemFact {
    WorkItemFact {
        id: format!("{prefix}:{id}"),
        kind: kind.to_owned(),
        queue: queue(queue_value),
        can_act: Some(can_act),
        action_kind: None,
        relationships: Vec::new(),
        updated_at,
        source_id: Some(id.to_owned()),
        href: if href.is_empty() {
            format!("/{prefix}/{id}")
        } else {
            href.to_owned()
        },
    }
}

pub fn support_ticket_items(items: &[TicketWorkItem]) -> Vec<WorkItemFact> {
    items
        .iter()
        .map(|value| {
            let mut fact = item(
                "support_ticket",
                "support_ticket",
                &value.id,
                &value.href,
                &value.queue,
                value.can_act,
                0,
            );
            fact.relationships = value.relationships.clone();
            fact
        })
        .collect()
}

pub fn support_incident_items(items: &[IncidentWorkItem]) -> Vec<WorkItemFact> {
    items
        .iter()
        .map(|value| {
            let mut fact = item(
                "support_incident",
                "support_incident",
                &value.id,
                &value.href,
                &value.queue,
                value.can_act,
                0,
            );
            fact.relationships = value.relationships.clone();
            fact
        })
        .collect()
}

pub fn support_session_items(items: &[SessionWorkItem]) -> Vec<WorkItemFact> {
    items
        .iter()
        .map(|value| {
            item(
                "support_session",
                "support_session",
                &value.id,
                &value.href,
                &value.queue,
                value.can_act,
                0,
            )
        })
        .collect()
}

pub fn field_service_items(items: &[FieldServiceWorkItem]) -> Vec<WorkItemFact> {
    items
        .iter()
        .map(|value| {
            let mut fact = item(
                "field_service",
                &value.kind,
                &value.id,
                &value.href,
                &value.queue,
                value.can_act,
                value.updated_at,
            );
            fact.relationships = value.relationships.clone();
            fact
        })
        .collect()
}

pub fn approval_items(rows: &[ApprovalQueueRow]) -> Vec<WorkItemFact> {
    rows.iter()
        .map(|row| {
            let mut fact = item(
                "workflow_approval",
                &row.record_type,
                &row.id,
                "",
                if matches!(row.state.as_str(), "pending" | "open") {
                    "todo"
                } else {
                    "waiting"
                },
                matches!(row.state.as_str(), "pending" | "open"),
                row.created_at,
            );
            fact.source_id = Some(row.record_id.clone());
            fact.action_kind = Some("approval".into());
            fact.relationships = vec!["approval".into()];
            fact
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_field_service::public::work::FieldServiceWorkItem;
    use erpx_server_support::public::incident::IncidentWorkItem;
    use erpx_server_support::public::session::SessionWorkItem;
    use erpx_server_support::public::ticket::TicketWorkItem;

    #[test]
    fn approval_rows_become_actionable_approval_items() {
        let rows = vec![
            erpx_server_workflow::public::approval_oversight::ApprovalQueueRow {
                id: "approval-1".into(),
                company_id: "company".into(),
                branch_id: None,
                department_id: None,
                record_type: "expense".into(),
                record_id: "expense-1".into(),
                state: "pending".into(),
                blocked_reason: None,
                version: 1,
                created_at: 7,
            },
            erpx_server_workflow::public::approval_oversight::ApprovalQueueRow {
                id: "approval-2".into(),
                company_id: "company".into(),
                branch_id: None,
                department_id: None,
                record_type: "leave_request".into(),
                record_id: "leave-1".into(),
                state: "blocked".into(),
                blocked_reason: Some("Evidence".into()),
                version: 1,
                created_at: 8,
            },
        ];
        let items = approval_items(&rows);
        assert_eq!(items[0].queue, WorkQueue::Todo);
        assert_eq!(items[0].action_kind.as_deref(), Some("approval"));
        assert_eq!(items[0].source_id.as_deref(), Some("expense-1"));
        assert_eq!(items[1].queue, WorkQueue::Waiting);
        assert_eq!(items[1].can_act, Some(false));
        assert_eq!(items[1].relationships, ["approval"]);
    }

    #[test]
    fn approval_items_make_the_approvals_card_available() {
        use crate::domain::navigation_destinations::NavigationShortcutFacts;
        use crate::domain::work_card_availability::{WorkCapabilitiesFact, WorkSnapshotFact};
        use crate::domain::work_card_selection::available_work_cards;
        let rows = [
            erpx_server_workflow::public::approval_oversight::ApprovalQueueRow {
                id: "approval-1".into(),
                company_id: "company".into(),
                branch_id: None,
                department_id: None,
                record_type: "expense".into(),
                record_id: "expense-1".into(),
                state: "pending".into(),
                blocked_reason: None,
                version: 1,
                created_at: 7,
            },
        ];
        let available = available_work_cards(
            &WorkSnapshotFact {
                items: approval_items(&rows),
            },
            &WorkCapabilitiesFact::default(),
            &NavigationShortcutFacts {
                beta: false,
                scope_is_department: false,
                executive_visible: false,
                commerce_read: false,
                inventory_read: false,
                finance_read: false,
                support_read: false,
                quality_read: false,
                records_read: false,
                active_modules: &[],
            },
        );
        assert!(available.contains("approvals"));
        assert!(available.contains("expenses"));
    }

    #[test]
    fn preserves_owner_queue_action_relationships_and_href() {
        let ticket = TicketWorkItem {
            id: "ticket-1".into(),
            title: "Ticket".into(),
            detail: "Detail".into(),
            href: "/support/ticket-1".into(),
            status: "open".into(),
            queue: "todo".into(),
            can_act: true,
            responsible_queue: "Support agent".into(),
            relationships: vec!["assigned".into()],
        };
        let incident = IncidentWorkItem {
            id: "incident-1".into(),
            title: "Incident".into(),
            detail: "Detail".into(),
            href: "/support/incidents/incident-1".into(),
            status: "open".into(),
            queue: "waiting".into(),
            can_act: false,
            responsible_queue: "Incident owner".into(),
            relationships: vec!["requested".into()],
        };
        let session = SessionWorkItem {
            id: "session-1".into(),
            title: "Session".into(),
            href: "/support/sessions/session-1".into(),
            status: "pending".into(),
            queue: "completed".into(),
            can_act: false,
        };
        let field = FieldServiceWorkItem {
            id: "order-1".into(),
            kind: "field_service".into(),
            title: "Order".into(),
            detail: "Detail".into(),
            href: "/field-service/order-1".into(),
            status: "open".into(),
            updated_at: 42,
            queue: "todo".into(),
            can_act: true,
            responsible_queue: "Dispatcher".into(),
            relationships: vec!["team".into()],
            next_action: Some("dispatch".into()),
            due_at: None,
            blocker: None,
        };
        assert_eq!(
            support_ticket_items(&[ticket])[0],
            WorkItemFact {
                id: "support_ticket:ticket-1".into(),
                kind: "support_ticket".into(),
                queue: WorkQueue::Todo,
                can_act: Some(true),
                action_kind: None,
                relationships: vec!["assigned".into()],
                updated_at: 0,
                source_id: Some("ticket-1".into()),
                href: "/support/ticket-1".into(),
            }
        );
        assert_eq!(
            support_incident_items(&[incident])[0].queue,
            WorkQueue::Waiting
        );
        assert_eq!(
            support_session_items(&[session])[0].queue,
            WorkQueue::Completed
        );
        let mapped = &field_service_items(&[field])[0];
        assert_eq!(mapped.id, "field_service:order-1");
        assert_eq!(mapped.updated_at, 42);
        assert_eq!(mapped.relationships, ["team"]);
        assert_eq!(mapped.queue, WorkQueue::Todo);
    }
}
