//! Native Sales customer-notice reconciliation rules and orchestration.
//!
//! Pure input/provider/outcome rules plus the use case with recording fakes.
//! The provider port is read-only and must be consulted exactly once on the
//! first command and never on replay; the immutable attempt is never rewritten.
use erpx_server_sales::public::sales_notification_execution::NotificationSource;
use erpx_server_sales::public::sales_notification_reconciliation::{
    NotificationReconciliationRepository, ProviderEvent, ProviderObservation,
    ProviderObservationPort, ReconcileActor, ReconcileAttempt, ReconcileFuture, ReconcileInput,
    ReconcilePlan, ReconcileReceiptSnapshot, reconcile_customer_notification,
};
use erpx_server_shared::public::BusinessError;
use std::sync::{Arc, Mutex};

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "sales-notification-reconciliation-0001";

fn source() -> NotificationSource {
    NotificationSource {
        id: "notice".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        order_id: "order".into(),
        order_state: Some("fulfilled".into()),
        fulfillment_id: "fulfillment".into(),
        party_id: "customer".into(),
        recipient_email: "customer@example.invalid".into(),
        subject: "Delivery recorded".into(),
        body_text: "Immutable delivery snapshot".into(),
        adapter: "resend".into(),
        current_adapter: Some("resend".into()),
        status: "unknown".into(),
        version: 2,
        attempts: 1,
        next_attempt_at: NOW,
        lease_until: None,
        provider_message_id: Some("provider-message-0001".into()),
        created_by: "creator".into(),
        source_company: Some("company".into()),
        customer_company: Some("company".into()),
        customer_status: Some("active".into()),
        notification_email: Some("customer@example.invalid".into()),
        notification_consent: Some("granted".into()),
        release_ready: true,
        source_hash: "a".repeat(64),
    }
}

fn attempt() -> ReconcileAttempt {
    ReconcileAttempt {
        id: "attempt".into(),
        notification_id: "notice".into(),
        company_id: "company".into(),
        authorization_command_id: "queue-command".into(),
        actor_id: "creator".into(),
        source_hash: "a".repeat(64),
        state: "unknown".into(),
        provider_message_id: Some("provider-message-0001".into()),
        attempt: 1,
        created_at: NOW - 1_000,
        updated_at: NOW - 500,
    }
}

fn actor(user: &str) -> ReconcileActor {
    ReconcileActor {
        user_id: user.into(),
        person_user_ids: vec![user.into()],
        company_id: "company".into(),
        branch_id: None,
        department_id: None,
        assignment_id: format!("assignment-{user}"),
        role: Some("sales_manager".into()),
        session_id: Some("session".into()),
        assignment_version: Some(1),
        permissions: vec!["commerce.read".into(), "commerce.sales".into(), "commerce.approve".into()],
    }
}

fn input() -> ReconcileInput {
    ReconcileInput {
        expected_version: 2,
        attempt_id: "attempt".into(),
        reason: "Independent current approver verifies the original provider identity".into(),
    }
}

fn observation() -> ProviderObservation {
    ProviderObservation {
        id: "provider-message-0001".into(),
        to: vec!["customer@example.invalid".into()],
        subject: "Delivery recorded".into(),
        text: "Immutable delivery snapshot".into(),
        created_at: NOW - 750,
        last_event: ProviderEvent::Delivered,
    }
}

struct FakeProvider {
    observation: Result<ProviderObservation, BusinessError>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl ProviderObservationPort for FakeProvider {
    fn observe<'a>(
        &'a self,
        provider_message_id: &'a str,
    ) -> ReconcileFuture<'a, ProviderObservation, BusinessError> {
        self.calls
            .lock()
            .unwrap()
            .push(provider_message_id.to_owned());
        let result = self.observation.clone();
        Box::pin(async move { result })
    }
}

#[derive(Default)]
struct FakeRepository {
    source: Option<NotificationSource>,
    attempt: Option<ReconcileAttempt>,
    receipt: Option<ReconcileReceiptSnapshot>,
    witness: bool,
    authority: bool,
    capability: Vec<(String, bool)>,
    commits: Vec<(String, i64, bool)>,
    fail_authority_on_commit: bool,
    committed: bool,
}

impl NotificationReconciliationRepository for FakeRepository {
    type Error = BusinessError;

    fn lock_command<'a>(&'a mut self, _: &'a str) -> ReconcileFuture<'a, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> ReconcileFuture<'a, String, Self::Error> {
        Box::pin(async move {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            Ok(format!("{:064x}", hasher.finish()))
        })
    }

    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> ReconcileFuture<'a, Option<ReconcileReceiptSnapshot>, Self::Error> {
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn reconciliation_witness<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> ReconcileFuture<'a, bool, Self::Error> {
        let witness = self.witness;
        Box::pin(async move { Ok(witness) })
    }

    fn source<'a>(
        &'a mut self,
        _: &'a str,
    ) -> ReconcileFuture<'a, Option<NotificationSource>, Self::Error> {
        let source = self.source.clone();
        Box::pin(async move { Ok(source) })
    }

    fn attempt<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> ReconcileFuture<'a, Option<ReconcileAttempt>, Self::Error> {
        let attempt = self.attempt.clone();
        Box::pin(async move { Ok(attempt) })
    }

    fn settle_capability<'a>(
        &'a mut self,
        company_id: &'a str,
        history: bool,
        _: i64,
    ) -> ReconcileFuture<'a, (), Self::Error> {
        self.capability
            .push((company_id.to_owned(), history));
        Box::pin(async { Ok(()) })
    }

    fn current_authority<'a>(
        &'a mut self,
        _: &'a ReconcileActor,
        _: i64,
    ) -> ReconcileFuture<'a, bool, Self::Error> {
        let authority = self.authority;
        Box::pin(async move { Ok(authority) })
    }

    fn commit<'a>(
        &'a mut self,
        plan: &'a ReconcilePlan<'a>,
        _: i64,
    ) -> ReconcileFuture<'a, (), Self::Error> {
        if self.fail_authority_on_commit {
            return Box::pin(async move {
                Err(erpx_server_sales::public::sales_notification_reconciliation::authority_changed())
            });
        }
        self.commits.push((
            plan.outcome.to_owned(),
            plan.version,
            plan.observation.last_event == ProviderEvent::Delivered,
        ));
        self.committed = true;
        Box::pin(async { Ok(()) })
    }
}

fn repository() -> FakeRepository {
    FakeRepository {
        source: Some(source()),
        attempt: Some(attempt()),
        authority: true,
        ..Default::default()
    }
}

fn run<'a>(
    repo: &'a mut FakeRepository,
    provider: &'a FakeProvider,
    actor: &'a ReconcileActor,
) -> std::pin::Pin<
    Box<
        dyn std::future::Future<
                Output = Result<
                    erpx_server_sales::public::sales_notification_reconciliation::ReconcileReceipt,
                    BusinessError,
                >,
            > + 'a,
    >,
> {
    Box::pin(reconcile_customer_notification(
        actor, "notice", input(), KEY, repo, provider, NOW,
    ))
}

#[test]
fn input_requires_exact_version_attempt_and_reason_bounds() {
    assert!(input().validate().is_ok());
    assert!(ReconcileInput { expected_version: 0, ..input() }.validate().is_err());
    assert!(ReconcileInput { attempt_id: "  ".into(), ..input() }.validate().is_err());
    assert!(ReconcileInput { attempt_id: "a".repeat(257), ..input() }.validate().is_err());
    assert!(ReconcileInput { reason: "tiny".into(), ..input() }.validate().is_err());
    assert!(ReconcileInput { reason: "x".repeat(1001), ..input() }.validate().is_err());
    assert_eq!(ReconcileInput { reason: " padded ".into(), ..input() }.validate().unwrap().reason, "padded");
}

#[test]
fn provider_outcomes_and_exact_identity_are_enforced() {
    for event in ["sent", "delivered", "opened", "clicked"] {
        assert_eq!(ProviderEvent::parse(event).unwrap().outcome(), "sent");
    }
    for event in ["bounced", "canceled", "failed", "suppressed"] {
        assert_eq!(ProviderEvent::parse(event).unwrap().outcome(), "cancelled");
    }
    assert!(ProviderEvent::parse("scheduled").is_none());
    assert!(observation().validate(&source(), &attempt()).is_ok());
    for (field, value) in [
        ("id", "another-provider-message"),
        ("subject", "Changed subject"),
        ("text", "Changed immutable message"),
    ] {
        let mut observed = observation();
        match field {
            "id" => observed.id = value.into(),
            "subject" => observed.subject = value.into(),
            _ => observed.text = value.into(),
        }
        assert_eq!(
            observed.validate(&source(), &attempt()).unwrap_err().code,
            "CUSTOMER_NOTIFICATION_PROVIDER_MISMATCH"
        );
    }
    let mut recipient = observation();
    recipient.to = vec!["another@example.invalid".into()];
    assert_eq!(
        recipient.validate(&source(), &attempt()).unwrap_err().code,
        "CUSTOMER_NOTIFICATION_PROVIDER_MISMATCH"
    );
    let mut duplicate = observation();
    duplicate.to = vec!["customer@example.invalid".into(), "customer@example.invalid".into()];
    assert_eq!(
        duplicate.validate(&source(), &attempt()).unwrap_err().code,
        "CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED"
    );
    for created in [NOW - 5_000, NOW + 5_000] {
        let mut outside = observation();
        outside.created_at = created;
        assert_eq!(
            outside.validate(&source(), &attempt()).unwrap_err().code,
            "CUSTOMER_NOTIFICATION_PROVIDER_MISMATCH"
        );
    }
}

#[tokio::test]
async fn a_different_current_reviewer_is_required_before_provider_read() {
    let mut repo = repository();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = FakeProvider {
        observation: Ok(observation()),
        calls: calls.clone(),
    };
    let requester = actor("creator");
    let error = run(&mut repo, &provider, &requester).await.unwrap_err();
    assert_eq!(error.code, "INDEPENDENT_NOTIFICATION_REVIEWER_REQUIRED");
    assert!(calls.lock().unwrap().is_empty());
    assert!(repo.commits.is_empty());

    let mut reviewer = actor("reviewer");
    reviewer.person_user_ids = vec!["reviewer".into(), "creator".into()];
    let error = run(&mut repo, &provider, &reviewer).await.unwrap_err();
    assert_eq!(error.code, "INDEPENDENT_NOTIFICATION_REVIEWER_REQUIRED");
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn reconciles_once_and_replays_without_another_provider_read() {
    let mut repo = repository();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = FakeProvider {
        observation: Ok(observation()),
        calls: calls.clone(),
    };
    let reviewer = actor("reviewer");
    let receipt = run(&mut repo, &provider, &reviewer).await.unwrap();
    assert_eq!(receipt.status, "sent");
    assert_eq!(receipt.version, 3);
    assert_eq!(receipt.reconciliation_id, format!("sales-notification-reconciliation-{KEY}"));
    assert_eq!(calls.lock().unwrap().as_slice(), ["provider-message-0001"]);
    assert_eq!(repo.commits.len(), 1);
    assert_eq!(repo.capability, [("company".into(), false)]);

    repo.committed = true;
    repo.witness = true;
    repo.receipt = Some(ReconcileReceiptSnapshot {
        request_hash: format!(
            "{:064x}",
            {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                input().identity(&reviewer, "notice", KEY).hash(&mut hasher);
                hasher.finish()
            }
        ),
        result_json: receipt.wire().to_string(),
        operation: "commerce.customer-notification.reconcile".into(),
        actor_id: "reviewer".into(),
        record_id: "notice".into(),
    });
    assert_eq!(run(&mut repo, &provider, &reviewer).await.unwrap(), receipt);
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(repo.capability.last(), Some(&("company".into(), true)));
    assert_eq!(repo.commits.len(), 1);
}

#[tokio::test]
async fn missing_identity_unconfirmed_and_mismatch_never_commit() {
    let reviewer = actor("reviewer");
    let calls = Arc::new(Mutex::new(Vec::new()));

    let mut missing = repository();
    missing.source.as_mut().unwrap().provider_message_id = None;
    let provider = FakeProvider {
        observation: Ok(observation()),
        calls: calls.clone(),
    };
    assert_eq!(
        run(&mut missing, &provider, &reviewer).await.unwrap_err().code,
        "CUSTOMER_NOTIFICATION_PROVIDER_ID_REQUIRED"
    );
    assert!(calls.lock().unwrap().is_empty());
    assert!(missing.commits.is_empty());

    let mut mismatch = repository();
    let provider = FakeProvider {
        observation: Ok(ProviderObservation {
            id: "different-provider-message".into(),
            ..observation()
        }),
        calls: calls.clone(),
    };
    assert_eq!(
        run(&mut mismatch, &provider, &reviewer).await.unwrap_err().code,
        "CUSTOMER_NOTIFICATION_PROVIDER_MISMATCH"
    );
    assert!(mismatch.commits.is_empty());

    let mut unconfirmed = repository();
    let provider = FakeProvider {
        observation: Err(erpx_server_sales::public::sales_notification_reconciliation::provider_unconfirmed()),
        calls: calls.clone(),
    };
    assert_eq!(
        run(&mut unconfirmed, &provider, &reviewer).await.unwrap_err().code,
        "CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED"
    );
    assert!(unconfirmed.commits.is_empty());
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn version_and_attempt_state_guards_refuse_before_provider_read() {
    let reviewer = actor("reviewer");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = FakeProvider {
        observation: Ok(observation()),
        calls: calls.clone(),
    };
    for (label, mutate) in [
        (
            "version",
            Box::new(|repo: &mut FakeRepository| {
                repo.source.as_mut().unwrap().version = 3;
            }) as Box<dyn Fn(&mut FakeRepository)>,
        ),
        (
            "status",
            Box::new(|repo: &mut FakeRepository| {
                repo.source.as_mut().unwrap().status = "sent".into();
            }),
        ),
        (
            "attempt",
            Box::new(|repo: &mut FakeRepository| {
                repo.attempt.as_mut().unwrap().state = "sent".into();
            }),
        ),
        (
            "attempts",
            Box::new(|repo: &mut FakeRepository| {
                repo.source.as_mut().unwrap().attempts = 2;
            }),
        ),
        (
            "source-hash",
            Box::new(|repo: &mut FakeRepository| {
                repo.attempt.as_mut().unwrap().source_hash = "b".repeat(64);
            }),
        ),
    ] {
        let mut repo = repository();
        mutate(&mut repo);
        let error = run(&mut repo, &provider, &reviewer).await.unwrap_err();
        assert!(
            matches!(error.code, "VERSION_CONFLICT" | "CUSTOMER_NOTIFICATION_NOT_UNCERTAIN" | "SALES_NOTIFICATION_SOURCE_CHANGED"),
            "{label}: {}",
            error.code
        );
        assert!(repo.commits.is_empty(), "{label}");
    }
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn foreign_scope_and_release_are_refused() {
    let reviewer = actor("reviewer");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = FakeProvider {
        observation: Ok(observation()),
        calls: calls.clone(),
    };
    let mut foreign = repository();
    foreign.source.as_mut().unwrap().company_id = "other-company".into();
    assert_eq!(
        run(&mut foreign, &provider, &reviewer).await.unwrap_err().code,
        "CUSTOMER_NOTIFICATION_NOT_FOUND"
    );
    let mut branch = actor("reviewer");
    branch.branch_id = Some("other-branch".into());
    let mut scoped = repository();
    scoped.source.as_mut().unwrap().branch_id = Some("branch".into());
    assert_eq!(
        run(&mut scoped, &provider, &branch).await.unwrap_err().code,
        "CUSTOMER_NOTIFICATION_NOT_FOUND"
    );
    let mut release = repository();
    release.source.as_mut().unwrap().release_ready = false;
    assert_eq!(
        run(&mut release, &provider, &reviewer).await.unwrap_err().code,
        "PRODUCTION_MODULE_DISABLED"
    );
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn late_authority_change_leaves_no_commit_and_retries_with_the_same_truth() {
    let reviewer = actor("reviewer");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = FakeProvider {
        observation: Ok(observation()),
        calls: calls.clone(),
    };
    let mut repo = repository();
    repo.fail_authority_on_commit = true;
    assert_eq!(
        run(&mut repo, &provider, &reviewer).await.unwrap_err().code,
        "QUEUED_AUTHORITY_CHANGED"
    );
    assert!(repo.commits.is_empty());
    assert!(!repo.committed);

    repo.fail_authority_on_commit = false;
    assert_eq!(run(&mut repo, &provider, &reviewer).await.unwrap().status, "sent");
    assert_eq!(repo.commits.len(), 1);
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn cancelled_provider_outcome_is_persisted_as_cancelled() {
    let reviewer = actor("reviewer");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let provider = FakeProvider {
        observation: Ok(ProviderObservation {
            last_event: ProviderEvent::Bounced,
            ..observation()
        }),
        calls,
    };
    let mut repo = repository();
    let receipt = run(&mut repo, &provider, &reviewer).await.unwrap();
    assert_eq!(receipt.status, "cancelled");
    assert_eq!(repo.commits, [("cancelled".into(), 3, false)]);
}
