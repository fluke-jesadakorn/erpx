//! Native Work Projection snapshot source adapters.
//!
//! Owner crates already produce their bounded work-item facts. These mappings
//! only translate the existing public owner contracts into the shared
//! availability fact; they perform no SQL, authority or command work.

use crate::domain::work_card_availability::{WorkItemFact, WorkQueue};
use erpx_server_benefits::public::workspace::BenefitElectionRow;
use erpx_server_collaboration::public::collaboration_work::ActionWork;
use erpx_server_customer_authority::public::duplicates::DuplicateReviewRow;
use erpx_server_field_service::public::work::FieldServiceWorkItem;
use erpx_server_learning::public::learning::AssignmentRow;
use erpx_server_maintenance::public::read::{MaintenancePlanRow, MaintenanceWorkRow};
use erpx_server_marketing::public::lead_candidate::MarketingLeadCandidateRow;
use erpx_server_operations::public::field_exceptions::ExceptionWork;
use erpx_server_organization::public::organization_nodes::{ChangeRow, NodeChangeState};
use erpx_server_quality::public::quality_read::{HoldStatus, InspectionResult, QualityHold};
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

pub fn maintenance_items(
    rows: &[MaintenanceWorkRow],
    actor_user_id: &str,
    can_review: bool,
) -> Vec<WorkItemFact> {
    rows.iter()
        .map(|row| {
            let terminal = matches!(row.status.as_str(), "completed" | "cancelled");
            let manage = can_review && row.can_review > 0;
            let own = row.owner_id == actor_user_id && row.owner_eligible > 0;
            let can_act = !terminal && (own || manage);
            let relationships = if row.owner_id == actor_user_id {
                vec!["assigned".to_owned()]
            } else if row.created_by == actor_user_id {
                vec!["requested".to_owned()]
            } else {
                vec!["team".to_owned()]
            };
            WorkItemFact {
                id: format!("maintenance:{}", row.id),
                kind: "maintenance_work".into(),
                queue: if terminal {
                    WorkQueue::Completed
                } else if can_act {
                    WorkQueue::Todo
                } else {
                    WorkQueue::Waiting
                },
                can_act: Some(can_act),
                action_kind: None,
                relationships,
                updated_at: row.updated_at,
                source_id: Some(row.id.clone()),
                href: format!("/maintenance/{}", row.id),
            }
        })
        .collect()
}

pub fn preventive_plan_items(
    rows: &[MaintenancePlanRow],
    actor_user_id: &str,
    can_write: bool,
    can_review: bool,
    now: i64,
) -> Vec<WorkItemFact> {
    rows.iter()
        .map(|row| {
            let terminal = row.status == "cancelled";
            let own = row.owner_id == actor_user_id && row.owner_eligible > 0;
            let due = row.next_due_at <= now + i64::from(row.lead_days) * 86_400_000;
            let can_act = can_write
                && !terminal
                && if matches!(row.status.as_str(), "draft" | "paused") {
                    can_review && row.edited_by != actor_user_id
                } else {
                    row.status == "active" && due && (own || can_review)
                };
            let relationships = if row.owner_id == actor_user_id {
                vec!["assigned".to_owned()]
            } else if row.created_by == actor_user_id {
                vec!["requested".to_owned()]
            } else {
                vec!["team".to_owned()]
            };
            WorkItemFact {
                id: format!("maintenance_plan:{}", row.id),
                kind: "maintenance_plan".into(),
                queue: if terminal {
                    WorkQueue::Completed
                } else if can_act {
                    WorkQueue::Todo
                } else {
                    WorkQueue::Waiting
                },
                can_act: Some(can_act),
                action_kind: None,
                relationships,
                updated_at: row.updated_at,
                source_id: Some(row.id.clone()),
                href: format!("/maintenance/plans/{}", row.id),
            }
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalWorkRow {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub state: String,
    pub owner_id: String,
    pub updated_at: i64,
}

/// Records branch of `legalWork` after the owner has applied its visibility
/// and purchase filters.
pub fn legal_work_items(
    rows: &[LegalWorkRow],
    actor_user_id: &str,
    can_edit: bool,
) -> Vec<WorkItemFact> {
    rows.iter()
        .map(|row| {
            let own = row.owner_id == actor_user_id;
            WorkItemFact {
                id: format!("legal:{}", row.id),
                kind: "legal".into(),
                queue: if own {
                    WorkQueue::Todo
                } else {
                    WorkQueue::Waiting
                },
                can_act: Some(own && can_edit),
                action_kind: None,
                relationships: vec![if own {
                    "assigned".to_owned()
                } else {
                    "requested".to_owned()
                }],
                updated_at: row.updated_at,
                source_id: Some(row.id.clone()),
                href: format!("/legal?record={}", row.id),
            }
        })
        .collect()
}

pub struct QualityWorkAuthority<'a> {
    pub can_read: bool,
    pub department_scope: bool,
    pub can_inspect: bool,
    pub can_release: bool,
    /// The selected actor's exact user id and reviewed person aliases.
    pub actor_user_ids: &'a [&'a str],
    /// The owner read's independent-recipient availability fact.
    pub eligible_independent: bool,
}

fn same_quality_person(authority: &QualityWorkAuthority<'_>, user: &str) -> bool {
    authority
        .actor_user_ids
        .iter()
        .any(|candidate| *candidate == user)
}

/// Mirrors the availability-affecting branches of `qualityWork`.
pub fn quality_hold_items(
    rows: &[QualityHold],
    authority: &QualityWorkAuthority<'_>,
) -> Vec<WorkItemFact> {
    if !authority.can_read || authority.department_scope {
        return Vec::new();
    }
    rows.iter()
        .map(|row| {
            let status = row.status.as_str();
            let completed = row.status.is_terminal();
            let quality_review = matches!(
                row.status,
                HoldStatus::AwaitingRelease | HoldStatus::CancelPending
            );
            let creator = same_quality_person(authority, &row.created_by);
            let inspector = row
                .last_inspector
                .as_deref()
                .is_some_and(|value| same_quality_person(authority, value));
            let cancel_requester = row
                .cancel_requested_by
                .as_deref()
                .is_some_and(|value| same_quality_person(authority, value));
            let can_disposition = row.status == HoldStatus::Held
                && row.last_result == Some(InspectionResult::Fail)
                && authority.can_release
                && !creator
                && !inspector;
            let can_act = !completed
                && row.status != HoldStatus::DispositionPending
                && !creator
                && (if quality_review {
                    authority.can_release
                        && !inspector
                        && (row.status != HoldStatus::CancelPending || !cancel_requester)
                } else {
                    can_disposition || authority.can_inspect
                });
            let relationships = if creator {
                vec!["requested".to_owned()]
            } else if can_act {
                vec!["approval".to_owned()]
            } else {
                vec!["team".to_owned()]
            };
            WorkItemFact {
                id: format!("quality_hold:{}", row.id),
                kind: "quality_hold".into(),
                queue: if completed {
                    WorkQueue::Completed
                } else if can_act {
                    WorkQueue::Todo
                } else {
                    WorkQueue::Waiting
                },
                can_act: Some(can_act),
                action_kind: if quality_review && can_act {
                    Some("approval".into())
                } else {
                    Some("work".into())
                },
                relationships,
                updated_at: row.updated_at,
                source_id: Some(row.id.clone()),
                href: format!("/quality/{}", row.id),
            }
        })
        .collect()
}

/// Mirrors the availability branches of `learningWorkItems`.
pub fn learning_assignment_items(
    rows: &[AssignmentRow],
    actor_user_id: &str,
    can_review: bool,
) -> Vec<WorkItemFact> {
    rows.iter()
        .map(|row| {
            let terminal = matches!(row.status.as_str(), "completed" | "waived" | "cancelled");
            let own = row.learner_id == actor_user_id && row.learner_eligible;
            let can_act = !terminal && (own || can_review);
            WorkItemFact {
                id: format!("learning_assignment:{}", row.id),
                kind: "learning_assignment".into(),
                queue: if terminal {
                    WorkQueue::Completed
                } else if can_act {
                    WorkQueue::Todo
                } else {
                    WorkQueue::Waiting
                },
                can_act: Some(can_act),
                action_kind: None,
                relationships: vec![if own {
                    "assigned".to_owned()
                } else if row.created_by == actor_user_id {
                    "requested".to_owned()
                } else {
                    "team".to_owned()
                }],
                updated_at: row.updated_at,
                source_id: Some(row.id.clone()),
                href: format!("/learning/{}", row.id),
            }
        })
        .collect()
}

/// Mirrors `marketingWorkItems`: a creator waits for another reviewer and a
/// non-creator receives the actionable consent/handoff review.
pub fn marketing_candidate_items(
    rows: &[MarketingLeadCandidateRow],
    actor_user_id: &str,
) -> Vec<WorkItemFact> {
    rows.iter()
        .map(|row| {
            let own = row.created_by == actor_user_id;
            WorkItemFact {
                id: format!("marketing_candidate:{}", row.id),
                kind: "marketing_candidate".into(),
                queue: if own {
                    WorkQueue::Waiting
                } else {
                    WorkQueue::Todo
                },
                can_act: Some(!own),
                action_kind: None,
                relationships: vec![if own {
                    "requested".to_owned()
                } else {
                    "approval".to_owned()
                }],
                updated_at: row.updated_at,
                source_id: Some(row.id.clone()),
                href: format!("/marketing/{}", row.id),
            }
        })
        .collect()
}

pub struct OrganizationChangeWorkRow {
    pub change: ChangeRow,
    pub company_id: String,
}

/// Mirrors `organizationChangeWorkItems`.
pub fn organization_change_items(
    rows: &[OrganizationChangeWorkRow],
    actor_user_id: &str,
    actor_user_ids: &[&str],
) -> Vec<WorkItemFact> {
    rows.iter()
        .map(|row| {
            let change = &row.change;
            let requested_by = change.requested_by.as_str();
            let own = requested_by == actor_user_id;
            let same = actor_user_ids.iter().any(|candidate| *candidate == requested_by);
            let review = change.state == NodeChangeState::Submitted && !same;
            let correct = own && matches!(change.state, NodeChangeState::Draft | NodeChangeState::Rejected);
            let completed = matches!(change.state, NodeChangeState::Applied | NodeChangeState::Cancelled);
            let can_act = review || correct;
            let href = format!(
                "/people/organization?view=legacy&company={}&tab=changes&changeId={}#organization-change-{}",
                row.company_id, change.id, change.id
            );
            WorkItemFact {
                id: format!("organization_change:{}", change.id),
                kind: "organization_change".into(),
                queue: if completed {
                    WorkQueue::Completed
                } else if review || correct {
                    WorkQueue::Todo
                } else {
                    WorkQueue::Waiting
                },
                can_act: Some(can_act),
                action_kind: None,
                relationships: vec![if own {
                    "requested".to_owned()
                } else if review {
                    "approval".to_owned()
                } else {
                    "team".to_owned()
                }],
                updated_at: change.updated_at,
                source_id: Some(change.id.clone()),
                href,
            }
        })
        .collect()
}

pub struct BenefitElectionWorkRow {
    pub election: BenefitElectionRow,
    /// Owner read fact; the workspace election row does not carry it.
    pub acknowledged_by: Option<String>,
}

/// Mirrors `benefitWorkItems` for the workspace election rows.
pub fn benefit_election_items(
    rows: &[BenefitElectionWorkRow],
    actor_person_id: &str,
    actor_user_id: &str,
    can_review: bool,
) -> Vec<WorkItemFact> {
    rows.iter()
        .map(|row| {
            let election = &row.election;
            let review = matches!(election.status.as_str(), "submitted" | "acknowledged");
            let own = election.person_id == actor_person_id;
            let can_act = if review {
                can_review
                    && (election.status != "acknowledged"
                        || row.acknowledged_by.as_deref() != Some(actor_user_id))
            } else {
                own
            };
            WorkItemFact {
                id: format!("benefit_enrollment:{}", election.id),
                kind: "benefit_enrollment".into(),
                queue: if can_act {
                    WorkQueue::Todo
                } else {
                    WorkQueue::Waiting
                },
                can_act: Some(can_act),
                action_kind: None,
                relationships: {
                    let mut values = vec![if own {
                        "requested".to_owned()
                    } else {
                        "team".to_owned()
                    }];
                    if review {
                        values.push("approval".to_owned());
                    }
                    values
                },
                updated_at: election.updated_at,
                source_id: Some(election.id.clone()),
                href: format!("/benefits/{}", election.id),
            }
        })
        .collect()
}

pub struct CustomerDuplicateAuthority {
    pub can_read: bool,
    pub can_sales: bool,
    pub can_review: bool,
    pub can_merge: bool,
}

/// Mirrors `customerDuplicateWork`.
pub fn customer_duplicate_items(
    rows: &[DuplicateReviewRow],
    actor_user_id: &str,
    authority: &CustomerDuplicateAuthority,
) -> Vec<WorkItemFact> {
    if !authority.can_read || !authority.can_sales {
        return Vec::new();
    }
    rows.iter()
        .map(|row| {
            let can_act = authority.can_review || authority.can_merge;
            let completed = row.status != "pending";
            WorkItemFact {
                id: format!("customer_duplicate:{}", row.id),
                kind: "customer_duplicate".into(),
                queue: if can_act {
                    WorkQueue::Todo
                } else if !completed {
                    WorkQueue::Waiting
                } else {
                    WorkQueue::Completed
                },
                can_act: Some(can_act),
                action_kind: Some(if row.status == "pending" && authority.can_review {
                    "approval".into()
                } else {
                    "work".into()
                }),
                relationships: vec![if row.created_by == actor_user_id {
                    "requested".to_owned()
                } else if can_act {
                    "approval".to_owned()
                } else {
                    "team".to_owned()
                }],
                updated_at: row.updated_at,
                source_id: Some(row.id.clone()),
                href: format!("/customer-duplicates/{}", row.id),
            }
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
    fn legal_records_follow_owner_and_edit_rules() {
        let row = LegalWorkRow {
            id: "legal-1".into(),
            title: "Contract".into(),
            kind: "contract".into(),
            state: "draft".into(),
            owner_id: "owner".into(),
            updated_at: 11,
        };
        let own = legal_work_items(&[row.clone()], "owner", true);
        assert_eq!(own[0].id, "legal:legal-1");
        assert_eq!(own[0].queue, WorkQueue::Todo);
        assert_eq!(own[0].can_act, Some(true));
        assert_eq!(own[0].relationships, ["assigned"]);
        assert_eq!(own[0].href, "/legal?record=legal-1");
        let other = legal_work_items(&[row], "someone", true);
        assert_eq!(other[0].queue, WorkQueue::Waiting);
        assert_eq!(other[0].can_act, Some(false));
        assert_eq!(other[0].relationships, ["requested"]);
    }

    #[test]
    fn preventive_plans_follow_due_review_and_edited_by_rules() {
        let plan = erpx_server_maintenance::public::read::MaintenancePlanRow {
            id: "plan-1".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            asset_id: "asset-1".into(),
            title: "Quarterly".into(),
            priority: "high".into(),
            owner_id: "owner".into(),
            interval_days: 90,
            time_zone: "Asia/Bangkok".into(),
            first_due_local: "2026-01-01".into(),
            next_due_local: "2026-04-01".into(),
            next_due_at: 1_000_000,
            lead_days: 0,
            status: "active".into(),
            version: 1,
            created_by: "creator".into(),
            edited_by: "editor".into(),
            approved_by: Some("approver".into()),
            approved_at: Some(1),
            created_at: 1,
            updated_at: 9,
            asset_code: "A-1".into(),
            asset_name: "Asset".into(),
            owner_name: None,
            owner_eligible: 1,
        };
        let items = preventive_plan_items(&[plan.clone()], "owner", true, false, 2_000_000);
        assert_eq!(items[0].queue, WorkQueue::Todo);
        assert_eq!(items[0].relationships, ["assigned"]);
        let not_due = preventive_plan_items(&[plan.clone()], "owner", true, false, 0);
        assert_eq!(not_due[0].queue, WorkQueue::Waiting);
        let draft = erpx_server_maintenance::public::read::MaintenancePlanRow {
            status: "draft".into(),
            edited_by: "owner".into(),
            ..plan
        };
        let self_edit = preventive_plan_items(&[draft], "owner", true, true, 10);
        assert_eq!(self_edit[0].queue, WorkQueue::Waiting);
    }

    #[test]
    fn maintenance_rows_follow_schedule_owner_and_reviewer_rules() {
        let row = erpx_server_maintenance::public::read::MaintenanceWorkRow {
            id: "work-1".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            asset_id: "asset-1".into(),
            title: "Service".into(),
            priority: "high".into(),
            status: "scheduled".into(),
            owner_id: "owner".into(),
            scheduled_at: Some(1),
            downtime_started_at: None,
            downtime_ended_at: None,
            blocked_reason: None,
            completion_evidence: None,
            resulting_condition: None,
            version: 1,
            created_by: "creator".into(),
            created_at: 1,
            updated_at: 9,
            asset_code: "A-1".into(),
            asset_name: "Asset".into(),
            owner_name: None,
            owner_eligible: 1,
            can_review: 0,
        };
        let items = maintenance_items(&[row.clone()], "owner", false);
        assert_eq!(items[0].id, "maintenance:work-1");
        assert_eq!(items[0].queue, WorkQueue::Todo);
        assert_eq!(items[0].relationships, ["assigned"]);
        assert_eq!(items[0].updated_at, 9);
        let waiting = maintenance_items(&[row], "someone", false);
        assert_eq!(waiting[0].queue, WorkQueue::Waiting);
        assert_eq!(waiting[0].relationships, ["team"]);
    }

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

    fn quality_hold(status: HoldStatus) -> erpx_server_quality::public::quality_read::QualityHold {
        erpx_server_quality::public::quality_read::QualityHold {
            id: "hold-1".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            warehouse_id: "warehouse".into(),
            product_id: "product".into(),
            lot_id: None,
            reason: "Reason".into(),
            due_date: "2026-11-10".into(),
            status,
            created_by: "creator".into(),
            last_inspector: None,
            last_result: None,
            released_by: None,
            release_reason: None,
            released_at: None,
            version: 1,
            created_at: 1,
            updated_at: 9,
            spec_id: None,
            spec_version: None,
            quantity_scope: "all".into(),
            held_quantity_milli: Some(1000),
            source_type: None,
            source_id: None,
            cancel_requested_by: None,
            cancel_reason: None,
            cancelled_by: None,
            cancelled_at: None,
            disposition_kind: None,
            disposition_reason: None,
            disposition_requested_by: None,
            disposition_count_id: None,
            disposition_quantity_milli: None,
            disposed_by: None,
            disposed_at: None,
            inspection_count: 0,
        }
    }

    fn learning_assignment(
        status: &str,
        learner_eligible: bool,
    ) -> erpx_server_learning::public::learning::AssignmentRow {
        erpx_server_learning::public::learning::AssignmentRow {
            id: "assignment-1".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            course_id: "course".into(),
            course_version: 1,
            learner_id: "learner".into(),
            due_date: "2026-11-10".into(),
            status: status.into(),
            attendance: None,
            score_bps: None,
            result_evidence: None,
            completion_reason: None,
            created_by: "creator".into(),
            version: 1,
            created_at: 1,
            updated_at: 9,
            course_title: "Safety".into(),
            required_for: None,
            person_id: None,
            learner_name: None,
            learner_eligible,
        }
    }

    fn marketing_candidate(
        created_by: &str,
    ) -> erpx_server_marketing::public::lead_candidate::MarketingLeadCandidateRow {
        erpx_server_marketing::public::lead_candidate::MarketingLeadCandidateRow {
            id: "candidate-1".into(),
            company_id: "company".into(),
            branch_id: "branch".into(),
            department_id: None,
            form_id: "form".into(),
            form_version: 1,
            form_name: "Contact".into(),
            contact_name: "Fictional contact".into(),
            contact_detail: "contact@example.test".into(),
            consent_evidence: "Consent recorded".into(),
            attribution_json: "{}".into(),
            brief: "Review consent".into(),
            status: "pending".into(),
            crm_lead_id: None,
            created_by: created_by.into(),
            reviewed_by: None,
            version: 1,
            created_at: 1,
            updated_at: 9,
        }
    }

    fn organization_change(
        state: erpx_server_organization::public::organization_nodes::NodeChangeState,
    ) -> erpx_server_organization::public::organization_nodes::ChangeRow {
        use erpx_server_organization::public::organization_nodes::{
            NodeChangeAction, NodeLifecycleKind,
        };
        erpx_server_organization::public::organization_nodes::ChangeRow {
            id: "change-1".into(),
            target_node_id: None,
            action: NodeChangeAction::Create,
            node_kind: NodeLifecycleKind::Branch,
            slug: Some("branch".into()),
            display_name: "Branch".into(),
            branch_id: None,
            source_branch_id: None,
            expected_node_version: None,
            state,
            version: 1,
            requested_by: erpx_server_shared::public::UserId::new("requester").unwrap(),
            reviewed_by: None,
            reason: "Reason".into(),
            review_reason: None,
            created_at: 1,
            updated_at: 9,
            reviewed_at: None,
            applied_at: None,
        }
    }

    fn benefit_election(
        status: &str,
        person_id: &str,
    ) -> erpx_server_benefits::public::workspace::BenefitElectionRow {
        erpx_server_benefits::public::workspace::BenefitElectionRow {
            id: "election-1".into(),
            company_id: "company".into(),
            person_id: person_id.into(),
            engagement_id: "engagement".into(),
            plan_id: "plan".into(),
            plan_name: "Health".into(),
            employee_name: "Fictional employee".into(),
            plan_version: 1,
            window_id: "window".into(),
            window_version: 1,
            coverage_type: "employee".into(),
            currency: "THB".into(),
            employee_deduction_minor: 100,
            employer_cost_minor: 200,
            effective_from: "2026-11-01".into(),
            effective_to: None,
            status: status.into(),
            version: 1,
            created_at: 1,
            updated_at: 9,
        }
    }

    fn duplicate_review(
        status: &str,
        created_by: &str,
    ) -> erpx_server_customer_authority::public::duplicates::DuplicateReviewRow {
        erpx_server_customer_authority::public::duplicates::DuplicateReviewRow {
            id: "duplicate-1".into(),
            company_id: "company".into(),
            left_customer_id: "left".into(),
            right_customer_id: "right".into(),
            left_name: "Left".into(),
            right_name: "Right".into(),
            status: status.into(),
            canonical_customer_id: None,
            evidence: "Evidence".into(),
            created_by: created_by.into(),
            reviewed_by: None,
            version: 1,
            created_at: 1,
            updated_at: 9,
            actor_eligible: true,
        }
    }

    #[test]
    fn customer_duplicates_follow_review_and_merge_authority() {
        let authority = CustomerDuplicateAuthority {
            can_read: true,
            can_sales: true,
            can_review: true,
            can_merge: false,
        };
        let reviewer = customer_duplicate_items(
            &[duplicate_review("pending", "creator")],
            "reviewer",
            &authority,
        );
        assert_eq!(reviewer[0].queue, WorkQueue::Todo);
        assert_eq!(reviewer[0].relationships, ["approval"]);
        assert_eq!(reviewer[0].action_kind.as_deref(), Some("approval"));
        let creator = customer_duplicate_items(
            &[duplicate_review("pending", "creator")],
            "creator",
            &authority,
        );
        assert_eq!(creator[0].relationships, ["requested"]);
        let closed = customer_duplicate_items(
            &[duplicate_review("duplicate", "creator")],
            "reviewer",
            &authority,
        );
        assert_eq!(closed[0].queue, WorkQueue::Todo);
        let denied = customer_duplicate_items(
            &[duplicate_review("pending", "creator")],
            "reviewer",
            &CustomerDuplicateAuthority {
                can_read: false,
                ..authority
            },
        );
        assert!(denied.is_empty());
    }

    #[test]
    fn benefit_elections_keep_employee_submit_and_independent_review() {
        let row = BenefitElectionWorkRow {
            election: benefit_election("elected", "person-1"),
            acknowledged_by: None,
        };
        let own = benefit_election_items(&[row], "person-1", "user-1", false);
        assert_eq!(own[0].queue, WorkQueue::Todo);
        assert_eq!(own[0].relationships, ["requested"]);
        let row = BenefitElectionWorkRow {
            election: benefit_election("submitted", "person-2"),
            acknowledged_by: None,
        };
        let reviewer = benefit_election_items(&[row], "person-1", "reviewer", true);
        assert_eq!(reviewer[0].queue, WorkQueue::Todo);
        assert_eq!(reviewer[0].relationships, ["team", "approval"]);
        let row = BenefitElectionWorkRow {
            election: benefit_election("acknowledged", "person-2"),
            acknowledged_by: Some("reviewer".into()),
        };
        let same_reviewer = benefit_election_items(&[row], "person-1", "reviewer", true);
        assert_eq!(same_reviewer[0].queue, WorkQueue::Waiting);
        assert_eq!(same_reviewer[0].can_act, Some(false));
    }

    #[test]
    fn organization_changes_follow_independent_review_rules() {
        use erpx_server_organization::public::organization_nodes::NodeChangeState;
        let row = OrganizationChangeWorkRow {
            change: organization_change(NodeChangeState::Submitted),
            company_id: "company".into(),
        };
        let reviewer = organization_change_items(&[row], "reviewer", &["reviewer"]);
        assert_eq!(reviewer[0].queue, WorkQueue::Todo);
        assert_eq!(reviewer[0].relationships, ["approval"]);
        assert!(reviewer[0].href.contains("changeId=change-1"));
        let row = OrganizationChangeWorkRow {
            change: organization_change(NodeChangeState::Submitted),
            company_id: "company".into(),
        };
        let requester = organization_change_items(&[row], "requester", &["requester"]);
        assert_eq!(requester[0].queue, WorkQueue::Waiting);
        assert_eq!(requester[0].can_act, Some(false));
        let row = OrganizationChangeWorkRow {
            change: organization_change(NodeChangeState::Applied),
            company_id: "company".into(),
        };
        let completed = organization_change_items(&[row], "reviewer", &["reviewer"]);
        assert_eq!(completed[0].queue, WorkQueue::Completed);
    }

    #[test]
    fn marketing_candidates_separate_creator_from_reviewer() {
        let own = marketing_candidate_items(&[marketing_candidate("creator")], "creator");
        assert_eq!(own[0].id, "marketing_candidate:candidate-1");
        assert_eq!(own[0].queue, WorkQueue::Waiting);
        assert_eq!(own[0].can_act, Some(false));
        assert_eq!(own[0].relationships, ["requested"]);
        let reviewer = marketing_candidate_items(&[marketing_candidate("creator")], "reviewer");
        assert_eq!(reviewer[0].queue, WorkQueue::Todo);
        assert_eq!(reviewer[0].can_act, Some(true));
        assert_eq!(reviewer[0].relationships, ["approval"]);
    }

    #[test]
    fn learning_assignments_follow_learner_and_reviewer_rules() {
        let own =
            learning_assignment_items(&[learning_assignment("assigned", true)], "learner", false);
        assert_eq!(own[0].id, "learning_assignment:assignment-1");
        assert_eq!(own[0].queue, WorkQueue::Todo);
        assert_eq!(own[0].relationships, ["assigned"]);
        let reviewer =
            learning_assignment_items(&[learning_assignment("assigned", false)], "reviewer", true);
        assert_eq!(reviewer[0].can_act, Some(true));
        assert_eq!(reviewer[0].relationships, ["team"]);
        let completed =
            learning_assignment_items(&[learning_assignment("completed", true)], "learner", false);
        assert_eq!(completed[0].queue, WorkQueue::Completed);
        assert_eq!(completed[0].can_act, Some(false));
    }

    #[test]
    fn quality_holds_follow_independence_and_terminal_rules() {
        let authority = QualityWorkAuthority {
            can_read: true,
            department_scope: false,
            can_inspect: true,
            can_release: true,
            actor_user_ids: &["inspector"],
            eligible_independent: true,
        };
        let items = quality_hold_items(&[quality_hold(HoldStatus::Held)], &authority);
        assert_eq!(items[0].id, "quality_hold:hold-1");
        assert_eq!(items[0].queue, WorkQueue::Todo);
        assert_eq!(items[0].relationships, ["approval"]);
        assert_eq!(items[0].action_kind.as_deref(), Some("work"));
        let unreviewable = QualityWorkAuthority {
            can_inspect: false,
            can_release: false,
            department_scope: true,
            ..authority
        };
        assert!(quality_hold_items(&[quality_hold(HoldStatus::Held)], &unreviewable).is_empty());
        let released = quality_hold_items(&[quality_hold(HoldStatus::Released)], &authority);
        assert_eq!(released[0].queue, WorkQueue::Completed);
        assert_eq!(released[0].can_act, Some(false));
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
