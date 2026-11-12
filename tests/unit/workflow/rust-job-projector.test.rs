//! Retained Workflow projection classification over portable facts.
use erpx_server_workflow::public::job_projector::{
    EventMessage, EventPayload, WorkQueue, event_work, projection_work,
};

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
fn classification_owns_employee_requests_and_never_policy_or_eligibility() {
    let task = event_work(&message("task.start", None));
    assert!(task.is_task && task.any());

    let routine = event_work(&message("routine.run", None));
    assert!(routine.is_task);

    let leave_policy = event_work(&message("leave.policy.update", None));
    assert!(!leave_policy.is_leave && !leave_policy.any());

    let overtime_eligibility = event_work(&message("overtime.eligibility", None));
    assert!(!overtime_eligibility.is_overtime && !overtime_eligibility.any());

    assert!(event_work(&message("leave.submit", None)).is_leave);
    assert!(event_work(&message("overtime.authorize", None)).is_overtime);
    assert!(event_work(&message("people.payroll.preview", None)).is_payroll);
    assert!(!event_work(&message("finance.journal.post", None)).any());
    assert!(!event_work(&message("field.order.create", None)).any());
}

#[test]
fn work_queue_titles_and_source_types_match_the_retained_projection() {
    let cases = [
        (
            "task.start",
            "completed",
            "staff_task",
            "Staff task",
            WorkQueue::Completed,
        ),
        (
            "routine.run",
            "ready",
            "staff_task",
            "Staff task",
            WorkQueue::Waiting,
        ),
        (
            "leave.submit",
            "pending_approval",
            "leave_request",
            "Leave request",
            WorkQueue::Todo,
        ),
        (
            "overtime.authorize",
            "authorized",
            "overtime_request",
            "Overtime request",
            WorkQueue::Waiting,
        ),
        (
            "people.payroll.preview",
            "preview",
            "hr_payroll_run",
            "Payroll preview",
            WorkQueue::Todo,
        ),
        (
            "people.payroll.post",
            "posted",
            "hr_payroll_run",
            "Payroll preview",
            WorkQueue::Completed,
        ),
    ];
    for (event_type, state, source_type, title, queue) in cases {
        let work = projection_work(event_work(&message(event_type, Some(state))), Some(state))
            .expect("projection family yields work");
        assert_eq!(work.source_type, source_type, "{event_type}");
        assert_eq!(work.title, title, "{event_type}");
        assert_eq!(work.queue, queue, "{event_type}");
    }

    let completed_states = [
        "approved",
        "posted",
        "rejected",
        "cancelled",
        "completed",
        "withdrawn",
    ];
    for state in completed_states {
        assert_eq!(
            projection_work(event_work(&message("task.start", None)), Some(state))
                .expect("task work")
                .queue,
            WorkQueue::Completed,
            "{state}"
        );
    }
    assert_eq!(
        projection_work(event_work(&message("leave.submit", None)), Some("draft"))
            .expect("leave work")
            .queue,
        WorkQueue::Waiting
    );
    assert_eq!(WorkQueue::Todo.as_str(), "todo");
    assert_eq!(WorkQueue::Waiting.as_str(), "waiting");
    assert_eq!(WorkQueue::Completed.as_str(), "completed");
}

#[test]
fn unhandled_families_never_produce_work() {
    assert!(projection_work(event_work(&message("finance.journal.post", None)), Some("posted")).is_none());
    assert!(projection_work(event_work(&message("leave.policy.update", None)), Some("active")).is_none());
    assert!(
        projection_work(
            event_work(&message("overtime.eligibility", None)),
            Some("eligible")
        )
        .is_none()
    );
}
