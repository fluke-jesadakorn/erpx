//! Native Work Projection snapshot source adapters.
//!
//! Owner crates already produce their bounded work-item facts. These mappings
//! only translate the existing public owner contracts into the shared
//! availability fact; they perform no SQL, authority or command work.

use crate::domain::work_card_availability::{WorkItemFact, WorkQueue};
use erpx_server_collaboration::public::collaboration_work::ActionWork;
use erpx_server_field_service::public::work::FieldServiceWorkItem;
use erpx_server_operations::public::field_exceptions::ExceptionWork;
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

pub fn operation_exception_items(items: &[ExceptionWork]) -> Vec<WorkItemFact> {
    items
        .iter()
        .map(|value| {
            let mut fact = item(
                "operations_exception",
                value.kind,
                &value.id,
                "",
                value.queue,
                value.can_act,
                value.updated_at,
            );
            fact.relationships = value
                .relationships
                .iter()
                .map(|v| (*v).to_owned())
                .collect();
            fact
        })
        .collect()
}

pub fn collaboration_action_items(items: &[ActionWork]) -> Vec<WorkItemFact> {
    items
        .iter()
        .map(|value| {
            let mut fact = item(
                "collaboration_action",
                value.kind,
                &value.id,
                value.href,
                value.queue,
                value.can_act,
                value.updated_at,
            );
            fact.relationships = value
                .relationships
                .iter()
                .map(|v| (*v).to_owned())
                .collect();
            fact.action_kind = Some(value.next_action.to_owned());
            fact
        })
        .collect()
}

fn json_work_items(values: &[serde_json::Value], prefix: &str) -> Vec<WorkItemFact> {
    values
        .iter()
        .filter_map(|value| {
            let id = value.get("id")?.as_str()?;
            let kind = value.get("type")?.as_str()?;
            let queue_value = value.get("queue")?.as_str()?;
            let can_act = value.get("canAct")?.as_bool()?;
            let updated_at = value.get("updatedAt")?.as_i64()?;
            let href = value.get("href").and_then(|v| v.as_str()).unwrap_or("");
            let mut fact = item(prefix, kind, id, href, queue_value, can_act, updated_at);
            if let Some(action) = value.get("actionKind").and_then(|v| v.as_str()) {
                fact.action_kind = Some(action.to_owned());
            }
            if let Some(relationships) = value.get("relationships").and_then(|v| v.as_array()) {
                fact.relationships = relationships
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect();
            }
            Some(fact)
        })
        .collect()
}

pub fn field_financial_items(values: &[serde_json::Value]) -> Vec<WorkItemFact> {
    json_work_items(values, "field_financial")
}

pub fn customer_access_items(values: &[serde_json::Value]) -> Vec<WorkItemFact> {
    json_work_items(values, "customer_access")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedWorkRow {
    pub id: String,
    pub source_type: String,
    pub source_id: String,
    pub queue: String,
    pub updated_at: i64,
}

/// Maps the Workflow read-model rows without re-deriving their queue.
pub fn projected_work_items(rows: &[ProjectedWorkRow]) -> Vec<WorkItemFact> {
    rows.iter()
        .map(|row| WorkItemFact {
            id: format!("projected:{}", row.id),
            kind: row.source_type.clone(),
            queue: queue(&row.queue),
            can_act: None,
            action_kind: None,
            relationships: Vec::new(),
            updated_at: row.updated_at,
            source_id: Some(row.source_id.clone()),
            href: format!("/work/{}", row.id),
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
    fn projected_work_rows_preserve_source_type_and_queue() {
        let rows = [
            ProjectedWorkRow {
                id: "work-1".into(),
                source_type: "staff_task".into(),
                source_id: "task-1".into(),
                queue: "todo".into(),
                updated_at: 5,
            },
            ProjectedWorkRow {
                id: "work-2".into(),
                source_type: "leave_request".into(),
                source_id: "leave-1".into(),
                queue: "completed".into(),
                updated_at: 6,
            },
        ];
        let items = projected_work_items(&rows);
        assert_eq!(items[0].id, "projected:work-1");
        assert_eq!(items[0].kind, "staff_task");
        assert_eq!(items[0].queue, WorkQueue::Todo);
        assert_eq!(items[0].updated_at, 5);
        assert_eq!(items[1].queue, WorkQueue::Completed);
        assert_eq!(items[1].source_id.as_deref(), Some("leave-1"));
    }

    #[test]
    fn maps_operations_collaboration_and_field_financial_values() {
        let exception = erpx_server_operations::public::field_exceptions::ExceptionWork {
            id: "exception-1".into(),
            kind: "operational_exception",
            title: "Exception".into(),
            detail: "Issue".into(),
            status: "open".into(),
            updated_at: 11,
            due_at: 12,
            can_act: true,
            queue: "todo",
            responsible_queue: "Assigned exception owner",
            next_action: "Acknowledge the exception",
            relationships: vec!["assigned"],
        };
        let action = erpx_server_collaboration::public::collaboration_work::ActionWork {
            id: "action-1".into(),
            kind: "collaboration_action",
            title: "Action".into(),
            detail: "Detail".into(),
            href: "/field-service/collaboration",
            status: "open".into(),
            updated_at: 21,
            due_at: 22,
            queue: "todo",
            can_act: true,
            responsible_queue: "Assigned action owner",
            next_action: "Complete with evidence",
            blocker: None,
            relationships: vec!["assigned"],
        };
        let field = serde_json::json!({
            "id":"invoice-1","type":"invoice","actionKind":"work",
            "title":"INV-1","href":"/finance/invoices/invoice-1","status":"draft",
            "updatedAt":31,"queue":"todo","canAct":true,
            "relationships":["requested","assigned"]
        });
        let operation = operation_exception_items(&[exception]);
        assert_eq!(operation[0].queue, WorkQueue::Todo);
        assert_eq!(operation[0].updated_at, 11);
        let collaboration = collaboration_action_items(&[action]);
        assert_eq!(
            collaboration[0].action_kind.as_deref(),
            Some("Complete with evidence")
        );
        assert_eq!(collaboration[0].relationships, ["assigned"]);
        let financial = field_financial_items(&[field]);
        assert_eq!(financial[0].id, "field_financial:invoice-1");
        assert_eq!(financial[0].kind, "invoice");
        assert_eq!(financial[0].action_kind.as_deref(), Some("work"));
        assert_eq!(financial[0].relationships, ["requested", "assigned"]);
        let access = customer_access_items(&[serde_json::json!({
            "id":"access-1","type":"customer_access","title":"Customer access",
            "href":"/customer-access/access-1","status":"pending","updatedAt":41,
            "queue":"todo","canAct":true,"relationships":["approval"]
        })]);
        assert_eq!(access[0].id, "customer_access:access-1");
        assert_eq!(access[0].relationships, ["approval"]);
        assert_eq!(access[0].queue, WorkQueue::Todo);
    }

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
