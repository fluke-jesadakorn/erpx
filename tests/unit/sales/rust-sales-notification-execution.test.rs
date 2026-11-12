//! Native Sales customer-notice delivery rules and orchestration.
//!
//! Pure owner rules (source/consent/provider invalidity, lease/attempt claim
//! decisions, queued identity binding) plus the bounded orchestration that
//! commits a claim before I/O and marks the same attempt on failure.
use erpx_server_sales::public::sales_notification_execution::{
    ClaimDecision, DeliveryStage, DispatchOutcome, DispatchPlan, DispatchTrace, EmailDispatchPort,
    ExecutionAttempt, ExecutionAuthorization, ExecutionFuture, NotificationExecutionUnit,
    NotificationSource, OutboundEmail, QueuedActor, authorization_current, claim_decision,
    deliver_notification, drain_notifications, invalid_reason, parse_queued_actor,
};
use erpx_server_shared::public::BusinessError;
use std::sync::{Arc, Mutex};

fn source(consent: &str, order_state: &str, adapter: &str) -> NotificationSource {
    NotificationSource {
        id: "notice".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        order_id: "order".into(),
        order_state: Some(order_state.into()),
        fulfillment_id: "fulfillment".into(),
        party_id: "customer".into(),
        recipient_email: "customer@example.invalid".into(),
        subject: "Delivery recorded".into(),
        body_text: "Immutable delivery snapshot".into(),
        adapter: adapter.into(),
        current_adapter: Some(adapter.into()),
        status: "pending".into(),
        version: 1,
        attempts: 0,
        next_attempt_at: 100,
        lease_until: None,
        provider_message_id: None,
        created_by: "creator".into(),
        source_company: Some("company".into()),
        customer_company: Some("company".into()),
        customer_status: Some("active".into()),
        notification_email: Some("customer@example.invalid".into()),
        notification_consent: Some(consent.into()),
        release_ready: true,
        source_hash: "a".repeat(64),
    }
}

fn authorization(hash: &str) -> ExecutionAuthorization {
    ExecutionAuthorization {
        command_id: "command".into(),
        notification_id: "notice".into(),
        company_id: "company".into(),
        kind: "queue".into(),
        actor_id: "creator".into(),
        assignment_id: "assignment".into(),
        authority_json: serde_json::json!({
            "userId": "creator",
            "assignmentId": "assignment",
            "role": "sales_manager",
            "companyId": "company",
            "branchId": "branch",
            "iam": {"queuedAssignmentVersion": 1},
        })
        .to_string(),
        source_hash: hash.into(),
        notification_version: 1,
    }
}

fn actor() -> QueuedActor {
    parse_queued_actor(&authorization(&"a".repeat(64)).authority_json).unwrap()
}

#[test]
fn current_consent_order_and_provider_changes_are_named_before_dispatch() {
    assert_eq!(
        invalid_reason(&source("granted", "confirmed", "resend")),
        None
    );
    assert_eq!(
        invalid_reason(&source("revoked", "confirmed", "resend")),
        Some("CUSTOMER_CONSENT_OR_EMAIL_CHANGED")
    );
    assert_eq!(
        invalid_reason(&source("granted", "draft", "resend")),
        Some("SALES_NOTIFICATION_SOURCE_CHANGED")
    );
    let mut changed = source("granted", "confirmed", "resend");
    changed.current_adapter = Some("sandbox".into());
    assert_eq!(
        invalid_reason(&changed),
        Some("CUSTOMER_NOTIFICATION_PROVIDER_CHANGED")
    );
    let mut foreign = source("granted", "confirmed", "resend");
    foreign.source_company = Some("foreign".into());
    assert_eq!(
        invalid_reason(&foreign),
        Some("SALES_NOTIFICATION_SOURCE_CHANGED")
    );
    let mut address = source("granted", "confirmed", "resend");
    address.notification_email = Some("other@example.invalid".into());
    assert_eq!(
        invalid_reason(&address),
        Some("CUSTOMER_CONSENT_OR_EMAIL_CHANGED")
    );
}

#[test]
fn claim_decisions_preserve_the_attempt_cap_and_lease_takeover() {
    let due = source("granted", "confirmed", "resend");
    assert_eq!(claim_decision(&due, 100), ClaimDecision::Ready);

    let mut exhausted = due.clone();
    exhausted.attempts = 8;
    assert_eq!(claim_decision(&exhausted, 100), ClaimDecision::Exhausted);

    let mut future = due.clone();
    future.next_attempt_at = 200;
    assert_eq!(claim_decision(&future, 100), ClaimDecision::Exhausted);

    let mut sending = due.clone();
    sending.status = "sending".into();
    sending.lease_until = Some(200);
    assert_eq!(claim_decision(&sending, 100), ClaimDecision::LeaseHeld);

    sending.lease_until = Some(99);
    assert_eq!(claim_decision(&sending, 100), ClaimDecision::Takeover);

    let mut sent = due;
    sent.status = "sent".into();
    assert_eq!(claim_decision(&sent, 100), ClaimDecision::NotQueued);
}

#[test]
fn queued_identity_must_bind_the_exact_source_hash_scope_and_actor() {
    let mail = source("granted", "confirmed", "resend");
    let bound = authorization(&mail.source_hash);
    assert!(authorization_current(&mail, &bound, &actor()));

    let mut stale = bound.clone();
    stale.source_hash = "b".repeat(64);
    assert!(!authorization_current(&mail, &stale, &actor()));

    let mut newer = bound;
    newer.notification_version = mail.version + 1;
    assert!(!authorization_current(&mail, &newer, &actor()));

    let mut department = actor();
    department.department_id = Some("department".into());
    assert!(!department.binds(&mail, &authorization(&mail.source_hash)));
}

#[derive(Default)]
struct FakeState {
    next: Vec<String>,
    delivered: Vec<String>,
    failures: Vec<bool>,
    deliver_error: bool,
}

struct FakeUnit {
    state: Mutex<FakeState>,
}

impl FakeUnit {
    fn new(next: Vec<String>, deliver_error: bool) -> Self {
        Self {
            state: Mutex::new(FakeState {
                next,
                deliver_error,
                ..FakeState::default()
            }),
        }
    }
}

struct NoopDispatch;

impl EmailDispatchPort for NoopDispatch {
    fn prepare(&self, _plan: DispatchPlan) -> Result<(), BusinessError> {
        Ok(())
    }

    fn dispatch<'a>(
        &'a self,
        _email: OutboundEmail<'a>,
    ) -> ExecutionFuture<'a, DispatchOutcome, BusinessError> {
        Box::pin(async {
            Ok(DispatchOutcome::Accepted {
                provider_id: "provider".into(),
            })
        })
    }
}

impl NotificationExecutionUnit for FakeUnit {
    type Error = BusinessError;

    fn company_of<'a>(
        &'a self,
        notification_id: &'a str,
    ) -> ExecutionFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move {
            if notification_id.is_empty() {
                Ok(None)
            } else {
                Ok(Some("company".into()))
            }
        })
    }

    fn claim<'a>(
        &'a self,
        _company_id: &'a str,
        notification_id: &'a str,
        _now: i64,
    ) -> ExecutionFuture<'a, Option<DeliveryStage>, Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            if !state.next.iter().any(|id| id == notification_id) {
                return Ok(None);
            }
            state.next.retain(|id| id != notification_id);
            Ok(Some(DeliveryStage {
                source: source("granted", "confirmed", "sandbox"),
                authorization: Some(authorization(&"a".repeat(64))),
                attempt: ExecutionAttempt {
                    id: "attempt".into(),
                    notification_id: notification_id.into(),
                    company_id: "company".into(),
                    attempt: 1,
                    lease_until: 90_100,
                    source_hash: "a".repeat(64),
                    state: "sending".into(),
                },
                plan: DispatchPlan::Sandbox,
            }))
        })
    }

    fn deliver<'a>(
        &'a self,
        stage: &'a DeliveryStage,
        _dispatch: Arc<dyn EmailDispatchPort>,
        _trace: Arc<DispatchTrace>,
        _now: i64,
    ) -> ExecutionFuture<'a, (), Self::Error> {
        Box::pin(async move {
            let mut state = self.state.lock().unwrap();
            if state.deliver_error {
                return Err(BusinessError::new(
                    erpx_server_shared::public::FailureKind::Conflict,
                    "CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN",
                    "declared provider uncertainty",
                ));
            }
            state.delivered.push(stage.source.id.clone());
            Ok(())
        })
    }

    fn fail<'a>(
        &'a self,
        _stage: &'a DeliveryStage,
        invoked: bool,
        _provider_id: Option<&'a str>,
        _now: i64,
    ) -> ExecutionFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.state.lock().unwrap().failures.push(invoked);
            Ok(())
        })
    }

    fn next_due<'a>(&'a self, _now: i64) -> ExecutionFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move { Ok(self.state.lock().unwrap().next.first().cloned()) })
    }
}

#[tokio::test]
async fn one_delivery_commits_the_notice_and_never_fails_it() {
    let unit = FakeUnit::new(vec!["notice".into()], false);
    let delivered = deliver_notification(&unit, Arc::new(NoopDispatch), "notice", 100)
        .await
        .unwrap();
    assert_eq!(delivered.as_deref(), Some("notice"));
    assert_eq!(
        unit.state.lock().unwrap().delivered,
        vec!["notice".to_owned()]
    );
    assert!(unit.state.lock().unwrap().failures.is_empty());
}

#[tokio::test]
async fn a_failed_delivery_marks_the_same_attempt_before_rethrowing() {
    let unit = FakeUnit::new(vec!["notice".into()], true);
    let error = deliver_notification(&unit, Arc::new(NoopDispatch), "notice", 100)
        .await
        .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN");
    assert_eq!(unit.state.lock().unwrap().failures, vec![false]);
}

#[tokio::test]
async fn drain_is_bounded_and_skips_unclaimable_notices() {
    let unit = FakeUnit::new(
        vec!["one".into(), "two".into(), "three".into(), "four".into()],
        false,
    );
    assert_eq!(
        drain_notifications(&unit, Arc::new(NoopDispatch), 100, 2)
            .await
            .unwrap(),
        2
    );
    assert_eq!(unit.state.lock().unwrap().delivered.len(), 2);
    assert_eq!(unit.state.lock().unwrap().next.len(), 2);
}
