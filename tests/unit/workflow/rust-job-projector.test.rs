//! Retained Workflow projection classification over portable facts.
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::job_projector::{
    EventMessage, EventPayload, ProjectionSideEffect, ProjectionSideEffects, SideEffectFuture,
    WorkQueue, event_work, projection_side_effects, projection_work, run_side_effects,
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

fn effect_name(effect: ProjectionSideEffect) -> &'static str {
    match effect {
        ProjectionSideEffect::CustomerNotificationQueue => "customer.queue",
        ProjectionSideEffect::CustomerNotificationRetry => "customer.retry",
        ProjectionSideEffect::LegalAiQueue => "legal.ai",
        ProjectionSideEffect::ExecutiveGenerationQueue => "executive.generation",
        ProjectionSideEffect::SocialAgent => "social.agent",
        ProjectionSideEffect::ExpenseAiPrepare => "expense.ai",
        ProjectionSideEffect::FinanceAiPrepare => "finance.ai",
        ProjectionSideEffect::RequestAiPrepare => "request.ai",
    }
}

#[test]
fn owner_side_effect_routing_matches_the_retained_projector_order() {
    let effects = |event_type: &str, production: bool| {
        projection_side_effects(event_type, production)
            .into_iter()
            .map(effect_name)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        effects("commerce.customer-notification.queue", true),
        ["customer.queue"]
    );
    assert_eq!(
        effects("commerce.customer-notification.retry", true),
        ["customer.retry"]
    );
    assert_eq!(effects("legal.ai.queue", true), Vec::<&str>::new());
    assert_eq!(effects("executive.generation.queue", true), Vec::<&str>::new());
    assert_eq!(effects("social.post.publish", true), Vec::<&str>::new());
    assert_eq!(effects("legal.ai.queue", false), ["legal.ai"]);
    assert_eq!(
        effects("executive.generation.queue", false),
        ["executive.generation"]
    );
    for event_type in ["social.post.publish", "social.post.comment", "social.agent.retry"] {
        assert_eq!(effects(event_type, false), ["social.agent"], "{event_type}");
    }
    assert_eq!(effects("expense.ai.prepare", true), ["expense.ai"]);
    assert_eq!(effects("finance.ai.prepare", true), ["finance.ai"]);
    assert_eq!(effects("request.ai.prepare", true), ["request.ai"]);
    // Families outside the retained list never trigger an owner drain.
    for event_type in [
        "task.start",
        "leave.submit",
        "people.payroll.preview",
        "finance.journal.post",
        "pms.booking.created",
    ] {
        assert!(effects(event_type, false).is_empty(), "{event_type}");
    }
}

#[derive(Default)]
struct RecordingEffects {
    calls: Vec<(String, String)>,
    fail_at: Option<&'static str>,
}

impl RecordingEffects {
    fn record(&mut self, name: &'static str, id: String) -> Result<(), BusinessError> {
        self.calls.push((name.to_owned(), id));
        if self.fail_at == Some(name) {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "TEST_EFFECT_FAILED",
                "The owner drain failed.",
            ));
        }
        Ok(())
    }
}

impl ProjectionSideEffects for RecordingEffects {
    fn customer_notification_queue<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> SideEffectFuture<'a> {
        let id = fulfillment_id.to_owned();
        Box::pin(async move { self.record("customer.queue", id) })
    }

    fn customer_notification_retry<'a>(
        &'a mut self,
        notification_id: &'a str,
    ) -> SideEffectFuture<'a> {
        let id = notification_id.to_owned();
        Box::pin(async move { self.record("customer.retry", id) })
    }

    fn legal_ai<'a>(&'a mut self) -> SideEffectFuture<'a> {
        Box::pin(async move { self.record("legal.ai", String::new()) })
    }

    fn executive_generation<'a>(&'a mut self) -> SideEffectFuture<'a> {
        Box::pin(async move { self.record("executive.generation", String::new()) })
    }

    fn social_agent<'a>(&'a mut self, post_id: &'a str) -> SideEffectFuture<'a> {
        let id = post_id.to_owned();
        Box::pin(async move { self.record("social.agent", id) })
    }

    fn expense_ai<'a>(&'a mut self, claim_id: &'a str) -> SideEffectFuture<'a> {
        let id = claim_id.to_owned();
        Box::pin(async move { self.record("expense.ai", id) })
    }

    fn finance_ai<'a>(&'a mut self) -> SideEffectFuture<'a> {
        Box::pin(async move { self.record("finance.ai", String::new()) })
    }

    fn request_ai<'a>(&'a mut self, request_id: &'a str) -> SideEffectFuture<'a> {
        let id = request_id.to_owned();
        Box::pin(async move { self.record("request.ai", id) })
    }
}

#[tokio::test]
async fn side_effects_run_in_the_retained_order_with_the_durable_aggregate_id() {
    let mut effects = RecordingEffects::default();
    run_side_effects(&mut effects, &message("expense.ai.prepare", None), false)
        .await
        .unwrap();
    assert_eq!(effects.calls, [("expense.ai".to_owned(), "source".to_owned())]);

    let mut effects = RecordingEffects::default();
    run_side_effects(&mut effects, &message("social.post.publish", None), false)
        .await
        .unwrap();
    assert_eq!(effects.calls, [("social.agent".to_owned(), "source".to_owned())]);

    // Production suppresses the development drains exactly like the retained
    // `!ports.production` guards.
    let mut effects = RecordingEffects::default();
    run_side_effects(&mut effects, &message("social.post.publish", None), true)
        .await
        .unwrap();
    assert!(effects.calls.is_empty());
}

#[tokio::test]
async fn an_owner_side_effect_failure_stops_before_later_effects() {
    let mut effects = RecordingEffects {
        fail_at: Some("customer.queue"),
        ..RecordingEffects::default()
    };
    let error = run_side_effects(
        &mut effects,
        &message("commerce.customer-notification.queue", None),
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TEST_EFFECT_FAILED");
    assert_eq!(effects.calls.len(), 1);
}
