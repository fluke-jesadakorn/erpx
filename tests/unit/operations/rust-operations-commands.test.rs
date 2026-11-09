//! Native Operations command policy and application cases. The repository is a
//! recording fake: real PostgreSQL CAS/rollback cases live in the integration
//! target, while this target proves validation, authority, scope, replay and
//! no-fallback order before any owner write.
use erpx_server_operations::public::field_exceptions::Exception;
use erpx_server_operations::public::field_policy::QueuePolicy;
use erpx_server_operations::public::operations_commands::*;
use erpx_server_shared::public::record_source_contract::{
    SourceKind, SourceRecord, SourceReference,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::{HashMap, HashSet};

struct Repo {
    receipts: HashMap<String, StoredReceipt>,
    exception: Option<Exception>,
    policy: Option<QueuePolicy>,
    calls: Vec<String>,
}
impl Repo {
    fn new(exception: Exception, policy: QueuePolicy) -> Self {
        Self {
            receipts: HashMap::new(),
            exception: Some(exception),
            policy: Some(policy),
            calls: Vec::new(),
        }
    }
    fn store(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        fingerprint: &str,
        receipt: &CommandReceipt,
    ) {
        self.receipts.insert(
            key.into(),
            StoredReceipt {
                operation: operation.into(),
                record_id: record_id.into(),
                request_fingerprint: fingerprint.into(),
                result_json: Some(serde_json::to_string(receipt).unwrap()),
                company_id: "company".into(),
                branch_id: Some("branch".into()),
                department_id: None,
                actor_id: "owner".into(),
            },
        );
    }
}
impl OperationsCommandRepository for Repo {
    type Error = BusinessError;
    fn stored_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> OperationsCommandFuture<'a, Option<StoredReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.receipts.get(key).cloned()) })
    }
    fn exception<'a>(
        &'a mut self,
        _: &'a str,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: &'a str,
    ) -> OperationsCommandFuture<'a, Option<Exception>, Self::Error> {
        Box::pin(async move { Ok(self.exception.clone()) })
    }
    fn policy<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> OperationsCommandFuture<'a, Option<QueuePolicy>, Self::Error> {
        Box::pin(async move { Ok(self.policy.clone()) })
    }
    fn open<'a>(
        &'a mut self,
        plan: OpenPlan,
    ) -> OperationsCommandFuture<'a, CommandReceipt, Self::Error> {
        Box::pin(async move {
            self.calls.push(format!("open:{}", plan.key.as_str()));
            let receipt = CommandReceipt::new(&plan.record_id, "open", 1);
            self.store(
                plan.key.as_str(),
                "operations.exception.open",
                &plan.record_id,
                &plan.request_fingerprint,
                &receipt,
            );
            Ok(receipt)
        })
    }
    fn change<'a>(
        &'a mut self,
        plan: ChangePlan,
    ) -> OperationsCommandFuture<'a, CommandReceipt, Self::Error> {
        Box::pin(async move {
            self.calls.push(format!("change:{}", plan.key.as_str()));
            let receipt = CommandReceipt::new(
                &plan.row.id,
                &plan.transition.status,
                plan.transition.version,
            );
            self.store(
                plan.key.as_str(),
                &format!("operations.exception.{}", plan.input.action),
                &plan.row.id,
                &plan.request_fingerprint,
                &receipt,
            );
            Ok(receipt)
        })
    }
    fn draft_policy<'a>(
        &'a mut self,
        plan: PolicyDraftPlan,
    ) -> OperationsCommandFuture<'a, CommandReceipt, Self::Error> {
        Box::pin(async move {
            self.calls.push(format!("draft:{}", plan.key.as_str()));
            let receipt = CommandReceipt::new(&plan.record_id, "draft", 1);
            self.store(
                plan.key.as_str(),
                "operations.policy.draft",
                &plan.record_id,
                &plan.request_fingerprint,
                &receipt,
            );
            Ok(receipt)
        })
    }
    fn review_policy<'a>(
        &'a mut self,
        plan: PolicyReviewPlan,
    ) -> OperationsCommandFuture<'a, CommandReceipt, Self::Error> {
        Box::pin(async move {
            self.calls.push(format!("review:{}", plan.key.as_str()));
            let receipt =
                CommandReceipt::new(&plan.record_id, plan.action.status(), plan.next_version);
            self.store(
                plan.key.as_str(),
                &format!("operations.policy.{}", plan.action.as_str()),
                &plan.record_id,
                &plan.request_fingerprint,
                &receipt,
            );
            Ok(receipt)
        })
    }
}

struct Sources {
    source: SourceRecord,
    recipient_error: Option<BusinessError>,
    same: HashSet<String>,
    source_calls: usize,
    recipient_calls: Vec<(String, bool)>,
}
impl Sources {
    fn new(source: SourceRecord, same: &[&str]) -> Self {
        Self {
            source,
            recipient_error: None,
            same: same.iter().map(|v| (*v).into()).collect(),
            source_calls: 0,
            recipient_calls: Vec::new(),
        }
    }
}
impl OperationsCommandSources for Sources {
    type Error = BusinessError;
    fn source<'a>(
        &'a mut self,
        _: &'a CommandActor,
        _: &'a SourceReference,
    ) -> OperationsCommandFuture<'a, SourceRecord, Self::Error> {
        Box::pin(async move {
            self.source_calls += 1;
            Ok(self.source.clone())
        })
    }
    fn recipient<'a>(
        &'a mut self,
        _: &'a CommandActor,
        _: &'a CommandScope,
        user_id: &'a str,
        review: bool,
    ) -> OperationsCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.recipient_calls.push((user_id.into(), review));
            match &self.recipient_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        })
    }
    fn same_person<'a>(
        &'a mut self,
        _: &'a CommandActor,
        user_id: &'a str,
    ) -> OperationsCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.same.contains(user_id)) })
    }
}

fn actor() -> CommandActor {
    CommandActor {
        user_id: "owner".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        assignment_scope: "holding".into(),
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "records.review".into(),
        ],
        modules: vec!["operations".into()],
        expires_at: 1_000,
    }
}
fn source(kind: SourceKind) -> SourceRecord {
    SourceRecord {
        kind,
        id: "task-open".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        version: 1,
        title: "Current task".into(),
        status: "open".into(),
    }
}
fn policy(kinds: &str) -> QueuePolicy {
    QueuePolicy {
        id: "policy-service".into(),
        company_id: "company".into(),
        area: "service".into(),
        name: "Service queue".into(),
        revision: 1,
        source_kinds_json: kinds.into(),
        target_minutes: 60,
        status: "published".into(),
        created_by: "maker".into(),
        reviewed_by: Some("reviewer".into()),
        version: 2,
        created_at: 0,
        updated_at: 1,
    }
}
fn row(status: &str) -> Exception {
    Exception {
        id: "exception-open".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        area: "service".into(),
        source_kind: "staff_task".into(),
        source_id: "task-open".into(),
        source_version: 1,
        source_reference_json: "{}".into(),
        policy_id: "policy-service".into(),
        policy_snapshot_json: "{}".into(),
        title: "Open task".into(),
        issue: "Synthetic issue".into(),
        priority: "normal".into(),
        status: status.into(),
        owner_id: "owner".into(),
        escalation_owner_id: None,
        resolution: None,
        resolved_by: None,
        reviewed_by: None,
        version: 1,
        created_by: "maker".into(),
        created_at: 0,
        updated_at: 0,
        due_at: 100,
    }
}
fn open_input() -> OpenExceptionCommand {
    OpenExceptionCommand {
        source: SourceCommandReference {
            kind: SourceKind::StaffTask,
            id: "task-open".into(),
            version: 1,
        },
        policy_id: "policy-service".into(),
        policy_version: 2,
        title: "Open task".into(),
        issue: "Synthetic issue".into(),
        priority: "normal".into(),
        owner_id: "owner".into(),
    }
}
fn change_input(action: &str, version: i64) -> ChangeExceptionCommand {
    ChangeExceptionCommand {
        id: "exception-open".into(),
        action: action.into(),
        expected_version: version,
        reason: "Reviewed actual source".into(),
        owner_id: None,
        escalation_owner_id: None,
        resolution: None,
        priority: None,
    }
}
fn draft_input() -> DraftPolicyCommand {
    DraftPolicyCommand {
        area: "service".into(),
        name: "Service queue".into(),
        source_kinds: vec![
            SourceKind::StaffTask,
            SourceKind::Asset,
            SourceKind::StaffTask,
        ],
        target_minutes: 60,
    }
}
fn draft_row() -> QueuePolicy {
    QueuePolicy {
        status: "draft".into(),
        version: 1,
        reviewed_by: None,
        ..policy("[\"staff_task\"]")
    }
}
fn review_input(action: &str) -> ReviewPolicyCommand {
    ReviewPolicyCommand {
        id: "policy-service".into(),
        action: action.into(),
        expected_version: 1,
        reason: "Independent policy review".into(),
    }
}

#[tokio::test]
async fn opening_checks_current_module_scope_source_and_policy_before_commit() {
    let mut repo = Repo::new(row("open"), policy("[\"staff_task\"]"));
    let mut sources = Sources::new(source(SourceKind::StaffTask), &["owner"]);
    let mut expired = actor();
    expired.expires_at = 1;
    assert_eq!(
        open_operations_exception(
            &expired,
            &mut repo,
            &mut sources,
            &open_input(),
            "command-key-01",
            1
        )
        .await
        .unwrap_err()
        .code,
        "WORK_CONTEXT_CHANGED"
    );
    let mut no_module = actor();
    no_module.modules.clear();
    assert_eq!(
        open_operations_exception(
            &no_module,
            &mut repo,
            &mut sources,
            &open_input(),
            "command-key-01",
            1
        )
        .await
        .unwrap_err()
        .code,
        "MODULE_FORBIDDEN"
    );
    let mut foreign_source = actor();
    foreign_source.company_id = "other".into();
    assert_eq!(
        open_operations_exception(
            &foreign_source,
            &mut repo,
            &mut sources,
            &open_input(),
            "command-key-01",
            1
        )
        .await
        .unwrap_err()
        .code,
        "OPERATIONS_SCOPE_REQUIRED"
    );
    let mut wrong_kind = Sources::new(source(SourceKind::Asset), &["owner"]);
    assert_eq!(
        open_operations_exception(
            &actor(),
            &mut repo,
            &mut wrong_kind,
            &open_input(),
            "command-key-01",
            1
        )
        .await
        .unwrap_err()
        .code,
        "SOURCE_RECORD_CHANGED"
    );
    sources.recipient_error = Some(BusinessError::new(
        FailureKind::Conflict,
        "OPERATIONS_RECIPIENT_UNAVAILABLE",
        "Choose an active eligible recipient in this scope.",
    ));
    assert_eq!(
        open_operations_exception(
            &actor(),
            &mut repo,
            &mut sources,
            &open_input(),
            "command-key-01",
            1
        )
        .await
        .unwrap_err()
        .code,
        "OPERATIONS_RECIPIENT_UNAVAILABLE"
    );
    assert!(repo.calls.is_empty());
}

#[tokio::test]
async fn opening_uses_the_exact_published_policy_and_idempotent_receipt() {
    let mut repo = Repo::new(row("open"), policy("[\"staff_task\"]"));
    let mut sources = Sources::new(source(SourceKind::StaffTask), &["owner"]);
    let receipt = open_operations_exception(
        &actor(),
        &mut repo,
        &mut sources,
        &open_input(),
        "command-key-01",
        10,
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        CommandReceipt::new("operations-exception-command-key-01", "open", 1)
    );
    assert_eq!(repo.calls, ["open:command-key-01"]);
    let first = receipt.clone();
    let replayed = open_operations_exception(
        &actor(),
        &mut repo,
        &mut sources,
        &open_input(),
        "command-key-01",
        10,
    )
    .await
    .unwrap();
    assert_eq!(replayed, first);
    assert_eq!(repo.calls.len(), 1);
    let mut changed = open_input();
    changed.issue = "A different command under the same key".into();
    assert_eq!(
        open_operations_exception(
            &actor(),
            &mut repo,
            &mut sources,
            &changed,
            "command-key-01",
            10
        )
        .await
        .unwrap_err()
        .code,
        "IDEMPOTENCY_CONFLICT"
    );

    repo.receipts.get_mut("command-key-01").unwrap().result_json = None;
    assert_eq!(
        open_operations_exception(
            &actor(),
            &mut repo,
            &mut sources,
            &open_input(),
            "command-key-01",
            10
        )
        .await
        .unwrap_err()
        .code,
        "COMMAND_INCOMPLETE"
    );
    repo.receipts.get_mut("command-key-01").unwrap().result_json = Some(
        r#"{"id":"operations-exception-command-key-01","status":"closed","version":1}"#.into(),
    );
    assert_eq!(
        open_operations_exception(
            &actor(),
            &mut repo,
            &mut sources,
            &open_input(),
            "command-key-01",
            10
        )
        .await
        .unwrap_err()
        .code,
        "COMMAND_RECEIPT_INVALID"
    );

    let mut stale_policy = Repo::new(row("open"), policy("[\"staff_task\"]"));
    let mut changed_policy = open_input();
    changed_policy.policy_version = 1;
    assert_eq!(
        open_operations_exception(
            &actor(),
            &mut stale_policy,
            &mut sources,
            &changed_policy,
            "command-key-02",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "OPERATIONS_POLICY_UNAVAILABLE"
    );
    assert!(stale_policy.calls.is_empty());
}

#[tokio::test]
async fn change_requires_owner_or_independent_review_and_rejects_stale_versions() {
    let mut repo = Repo::new(row("open"), policy("[\"staff_task\"]"));
    let mut sources = Sources::new(source(SourceKind::StaffTask), &[]);
    let mut other = actor();
    other.user_id = "other".into();
    assert_eq!(
        change_operations_exception(
            &other,
            &mut repo,
            &mut sources,
            &change_input("acknowledge", 1),
            "command-key-01",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "OPERATIONS_OWNER_REQUIRED"
    );
    let mut no_review = actor();
    no_review.permissions = vec!["records.read".into(), "records.write".into()];
    let mut assign = change_input("assign", 1);
    assign.owner_id = Some("other".into());
    assert_eq!(
        change_operations_exception(
            &no_review,
            &mut repo,
            &mut sources,
            &assign,
            "command-key-01",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "PERMISSION_REQUIRED"
    );
    assert_eq!(
        change_operations_exception(
            &actor(),
            &mut repo,
            &mut sources,
            &change_input("acknowledge", 2),
            "command-key-01",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "OPERATIONS_STATE_CHANGED"
    );
    let mut review = row("awaiting_review");
    review.owner_id = "owner".into();
    review.resolved_by = Some("owner".into());
    let mut repo = Repo::new(review, policy("[\"staff_task\"]"));
    let mut sources = Sources::new(source(SourceKind::StaffTask), &["owner"]);
    assert_eq!(
        change_operations_exception(
            &actor(),
            &mut repo,
            &mut sources,
            &change_input("approve", 1),
            "command-key-01",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "OPERATIONS_INDEPENDENT_REVIEW"
    );
    assert!(repo.calls.is_empty());
}

#[tokio::test]
async fn acknowledged_change_is_replayed_without_duplicate_evidence_and_cross_company_is_hidden() {
    let mut repo = Repo::new(row("open"), policy("[\"staff_task\"]"));
    let mut sources = Sources::new(source(SourceKind::StaffTask), &["owner"]);
    let receipt = change_operations_exception(
        &actor(),
        &mut repo,
        &mut sources,
        &change_input("acknowledge", 1),
        "command-key-01",
        10,
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        CommandReceipt::new("exception-open", "acknowledged", 2)
    );
    assert_eq!(repo.calls, ["change:command-key-01"]);
    let again = change_operations_exception(
        &actor(),
        &mut repo,
        &mut sources,
        &change_input("acknowledge", 1),
        "command-key-01",
        10,
    )
    .await
    .unwrap();
    assert_eq!(again, receipt);
    assert_eq!(repo.calls.len(), 1);

    let mut foreign = row("open");
    foreign.company_id = "other".into();
    let mut repo = Repo::new(foreign, policy("[\"staff_task\"]"));
    assert_eq!(
        change_operations_exception(
            &actor(),
            &mut repo,
            &mut sources,
            &change_input("acknowledge", 1),
            "command-key-01",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "OPERATIONS_EXCEPTION_NOT_FOUND"
    );
    assert!(repo.calls.is_empty());
}

#[tokio::test]
async fn reassignment_and_cancellation_are_managed_reviewed_transitions() {
    let mut repo = Repo::new(row("acknowledged"), policy("[\"staff_task\"]"));
    let mut sources = Sources::new(source(SourceKind::StaffTask), &["owner"]);
    let mut assign = change_input("assign", 1);
    assign.owner_id = Some("new-owner".into());
    let receipt = change_operations_exception(
        &actor(),
        &mut repo,
        &mut sources,
        &assign,
        "command-key-01",
        10,
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        CommandReceipt::new("exception-open", "acknowledged", 2)
    );
    assert_eq!(
        sources.recipient_calls,
        [("owner".into(), true), ("new-owner".into(), false)]
    );

    let mut repo = Repo::new(row("open"), policy("[\"staff_task\"]"));
    let receipt = change_operations_exception(
        &actor(),
        &mut repo,
        &mut sources,
        &change_input("cancel", 1),
        "command-key-02",
        10,
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        CommandReceipt::new("exception-open", "cancelled", 2)
    );
}

#[tokio::test]
async fn policy_commands_require_company_wide_review_authority_and_independent_creator() {
    let mut repo = Repo::new(row("open"), policy("[\"staff_task\"]"));
    let mut sources = Sources::new(source(SourceKind::StaffTask), &["maker"]);
    let mut no_review = actor();
    no_review.permissions = vec!["records.read".into(), "records.write".into()];
    assert_eq!(
        draft_operations_policy(
            &no_review,
            &mut repo,
            &mut sources,
            &draft_input(),
            "policy-key-01",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "PERMISSION_REQUIRED"
    );
    let mut branch = actor();
    branch.assignment_scope = "branch".into();
    assert_eq!(
        draft_operations_policy(
            &branch,
            &mut repo,
            &mut sources,
            &draft_input(),
            "policy-key-01",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "CONFLICT"
    );
    let mut unsupported = draft_input();
    unsupported.source_kinds = vec![SourceKind::MaintenancePlan];
    assert_eq!(
        draft_operations_policy(
            &actor(),
            &mut repo,
            &mut sources,
            &unsupported,
            "policy-key-01",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "OPERATIONS_NATIVE_SOURCE_UNSUPPORTED"
    );
    let receipt = draft_operations_policy(
        &actor(),
        &mut repo,
        &mut sources,
        &draft_input(),
        "policy-key-01",
        10,
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        CommandReceipt::new("queue-policy-policy-key-01", "draft", 1)
    );
    assert_eq!(repo.calls, ["draft:policy-key-01"]);
    let mut self_creator = Repo::new(row("open"), draft_row());
    let mut maker = Sources::new(source(SourceKind::StaffTask), &["maker"]);
    assert_eq!(
        review_operations_policy(
            &actor(),
            &mut self_creator,
            &mut maker,
            &review_input("publish"),
            "policy-key-02",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "CONFLICT"
    );
    let mut fresh = Repo::new(row("open"), draft_row());
    let mut reviewer = Sources::new(source(SourceKind::StaffTask), &[]);
    let receipt = review_operations_policy(
        &actor(),
        &mut fresh,
        &mut reviewer,
        &review_input("publish"),
        "policy-key-03",
        10,
    )
    .await
    .unwrap();
    assert_eq!(
        receipt,
        CommandReceipt::new("policy-service", "published", 2)
    );
    assert_eq!(fresh.calls, ["review:policy-key-03"]);
    let mut current = draft_row();
    current.version = 2;
    let mut stale = Repo::new(row("open"), current);
    assert_eq!(
        review_operations_policy(
            &actor(),
            &mut stale,
            &mut reviewer,
            &review_input("publish"),
            "policy-key-04",
            10,
        )
        .await
        .unwrap_err()
        .code,
        "CONFLICT"
    );
    assert!(stale.calls.is_empty());
}

#[test]
fn operations_open_and_policy_commands_count_unicode_codepoints() {
    let mut open=open_input();
    open.source.id="😀".repeat(180);open.policy_id="😀".repeat(180);open.owner_id="😀".repeat(180);
    open.title="😀".repeat(160);open.issue="😀".repeat(4000);assert!(open.validate().is_ok());
    open.source.id="😀".repeat(181);assert!(open.validate().is_err());
    open.source.id="source\0id".into();assert!(open.validate().is_err());
    open.source.id="source".into();open.title="😀".repeat(161);assert!(open.validate().is_err());
    open.title="valid title".into();open.issue="😀".repeat(4001);assert!(open.validate().is_err());
    let mut draft=draft_input();draft.name="😀".repeat(160);assert!(draft.validate().is_ok());
    draft.name="😀".repeat(161);assert!(draft.validate().is_err());
    let mut change=change_input("resolve",1);change.reason="😀".repeat(4000);change.resolution=Some("😀".repeat(4000));assert!(change.validate().is_ok());
    change.resolution=Some("😀".repeat(4001));assert!(change.validate().is_err());
}
