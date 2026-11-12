//! Portable Workflow projection classification.
//!
//! Exact port of `packages/server-workflow/src/domain/job-projector.ts`:
//! `EventMessage`, `eventWork` and `projectionWork`. It contains no SQL and no
//! owner reads; the adapter resolves source facts inside the caller's unit.
use serde::{Deserialize, Serialize};

/// One durable outbox event delivered to the Workflow projector.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventMessage {
    pub holding_id: String,
    pub id: String,
    pub event_type: String,
    pub aggregate_id: String,
    pub aggregate_version: i64,
    pub payload: EventPayload,
}

/// The owner event payload snapshot. `state` is absent for tasks and for People
/// events that only expose a version; the projector then re-reads the owner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPayload {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub version: i64,
    pub company_id: String,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub department_id: Option<String>,
}

/// Which native Workflow projection family an event belongs to.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventWork {
    pub is_leave: bool,
    pub is_overtime: bool,
    pub is_payroll: bool,
    pub is_task: bool,
}

impl EventWork {
    pub const fn any(self) -> bool {
        self.is_leave || self.is_overtime || self.is_payroll || self.is_task
    }
}

/// The retained work item queue states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkQueue {
    Todo,
    Waiting,
    Completed,
}

impl WorkQueue {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Waiting => "waiting",
            Self::Completed => "completed",
        }
    }
}

/// One `work_items` projection row body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionWork {
    pub source_type: &'static str,
    pub title: &'static str,
    pub queue: WorkQueue,
}

/// Exact `eventWork` port. Policy and eligibility configuration must never
/// masquerade as an employee request.
pub fn event_work(message: &EventMessage) -> EventWork {
    EventWork {
        is_leave: message.event_type.starts_with("leave.")
            && !message.event_type.starts_with("leave.policy."),
        is_overtime: message.event_type.starts_with("overtime.")
            && message.event_type != "overtime.eligibility",
        is_payroll: message.event_type.starts_with("people.payroll."),
        is_task: message.event_type.starts_with("task.") || message.event_type == "routine.run",
    }
}

/// Exact `projectionWork` port. A non-projection family yields no work row.
pub fn projection_work(flags: EventWork, state: Option<&str>) -> Option<ProjectionWork> {
    if !flags.any() {
        return None;
    }
    let state = state.unwrap_or_default();
    let completed = matches!(
        state,
        "approved" | "posted" | "rejected" | "cancelled" | "completed" | "withdrawn"
    );
    let queue = if completed {
        WorkQueue::Completed
    } else if state == "pending_approval" || state == "preview" {
        WorkQueue::Todo
    } else {
        WorkQueue::Waiting
    };
    Some(ProjectionWork {
        source_type: if flags.is_overtime {
            "overtime_request"
        } else if flags.is_task {
            "staff_task"
        } else if flags.is_payroll {
            "hr_payroll_run"
        } else {
            "leave_request"
        },
        title: if flags.is_overtime {
            "Overtime request"
        } else if flags.is_task {
            "Staff task"
        } else if flags.is_payroll {
            "Payroll preview"
        } else {
            "Leave request"
        },
        queue,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(event_type: &str, state: Option<&str>) -> EventMessage {
        EventMessage {
            holding_id: "holding".into(),
            id: "event".into(),
            event_type: event_type.into(),
            aggregate_id: "source".into(),
            aggregate_version: 2,
            payload: EventPayload {
                id: "source".into(),
                state: state.map(str::to_owned),
                version: 2,
                company_id: "company".into(),
                branch_id: None,
                department_id: None,
            },
        }
    }

    #[test]
    fn classification_excludes_policy_and_eligibility_configuration() {
        assert!(event_work(&message("task.start", None)).is_task);
        assert!(event_work(&message("routine.run", None)).is_task);
        assert!(!event_work(&message("leave.policy.update", None)).is_leave);
        assert!(!event_work(&message("overtime.eligibility", None)).is_overtime);
        assert!(event_work(&message("leave.submit", None)).is_leave);
        assert!(event_work(&message("overtime.authorize", None)).is_overtime);
        assert!(event_work(&message("people.payroll.preview", None)).is_payroll);
        assert!(!event_work(&message("finance.journal.post", None)).any());
    }

    #[test]
    fn work_queue_and_titles_match_the_retained_projection() {
        let task = projection_work(event_work(&message("task.start", None)), Some("completed"))
            .expect("task projects work");
        assert_eq!(task.source_type, "staff_task");
        assert_eq!(task.title, "Staff task");
        assert_eq!(task.queue, WorkQueue::Completed);
        assert_eq!(task.queue.as_str(), "completed");

        let leave = projection_work(
            event_work(&message("leave.submit", None)),
            Some("pending_approval"),
        )
        .expect("leave projects work");
        assert_eq!(leave.source_type, "leave_request");
        assert_eq!(leave.title, "Leave request");
        assert_eq!(leave.queue, WorkQueue::Todo);

        let overtime = projection_work(
            event_work(&message("overtime.authorize", None)),
            Some("authorized"),
        )
        .expect("overtime projects work");
        assert_eq!(overtime.source_type, "overtime_request");
        assert_eq!(overtime.title, "Overtime request");
        assert_eq!(overtime.queue, WorkQueue::Waiting);

        let payroll = projection_work(
            event_work(&message("people.payroll.preview", None)),
            Some("preview"),
        )
        .expect("payroll projects work");
        assert_eq!(payroll.source_type, "hr_payroll_run");
        assert_eq!(payroll.title, "Payroll preview");
        assert_eq!(payroll.queue, WorkQueue::Todo);

        assert!(
            projection_work(event_work(&message("finance.journal.post", None)), Some("posted"))
                .is_none()
        );
    }

    #[test]
    fn event_message_round_trips_its_camel_case_contract() {
        let value = serde_json::json!({
            "holdingId": "holding",
            "id": "event",
            "eventType": "people.payroll.preview",
            "aggregateId": "run",
            "aggregateVersion": 4,
            "payload": {
                "id": "run",
                "version": 4,
                "companyId": "company",
                "branchId": null,
                "departmentId": null
            }
        });
        let message: EventMessage = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(message.payload.state, None);
        assert_eq!(message.aggregate_version, 4);
        let encoded = serde_json::to_value(&message).unwrap();
        assert_eq!(encoded["eventType"], "people.payroll.preview");
        assert_eq!(encoded["payload"]["companyId"], "company");
    }
}
