//! Native Customer Authority duplicate-review policy, command and read tests.
//!
//! The identity-change and HTTP router cases live in sibling non-target modules
//! so this registered Cargo target keeps covering both owner capabilities
//! without a catalogue edit.
mod http_cases;
mod identity_cases;

use erpx_server_customer_authority::public::access::{AccessActor, AccessReceipt, ReceiptSnapshot};
use erpx_server_customer_authority::public::duplicates::{
    ChangePlan, DuplicateChange, DuplicateReviewCommandRepository, DuplicateReviewCustomer,
    DuplicateReviewEventRow, DuplicateReviewReadRepository, DuplicateReviewRequest,
    DuplicateReviewRow, MergeBlockers, MergePartyFacts, MergePlan, RequestPlan,
    change_duplicate_review, duplicate_review_detail, duplicate_review_workspace,
    request_duplicate_review,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde_json::Value;
use std::sync::Mutex;

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "customer-duplicate-test-0001";

fn actor_with(user: &str, permissions: &[&str]) -> AccessActor {
    AccessActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: erpx_server_shared::public::AssignmentId::new(format!(
            "assignment-{user}"
        ))
        .unwrap(),
        role: Some("sales_manager".into()),
        authority_revision: Some(3),
        assignment_version: Some(2),
        permissions: permissions.iter().map(|v| (*v).to_owned()).collect(),
        expires_at: NOW + 60_000,
    }
}

fn actor(permissions: &[&str]) -> AccessActor {
    actor_with("requester", permissions)
}

fn reviewer() -> AccessActor {
    actor_with(
        "reviewer",
        &["commerce.read", "commerce.sales", "commerce.approve"],
    )
}

fn row() -> DuplicateReviewRow {
    DuplicateReviewRow {
        id: "customer-duplicate-KEY".replace("KEY", KEY),
        company_id: "company".into(),
        left_customer_id: "customer-a".into(),
        right_customer_id: "customer-b".into(),
        left_name: "Customer A".into(),
        right_name: "Customer B".into(),
        left_code: "A".into(),
        right_code: "B".into(),
        left_tax_registration: Some("TAX-A".into()),
        right_tax_registration: Some("TAX-B".into()),
        left_status: "active".into(),
        right_status: "active".into(),
        left_merged_into_id: None,
        right_merged_into_id: None,
        status: "pending".into(),
        canonical_customer_id: None,
        evidence: "Matching records".into(),
        created_by: "requester".into(),
        reviewed_by: None,
        version: 1,
        created_at: NOW - 1_000,
        updated_at: NOW - 1_000,
        actor_eligible: true,
    }
}

#[derive(Default)]
struct Fake {
    receipt: Option<ReceiptSnapshot>,
    customers_active: i64,
    customer_active: i64,
    commits: Mutex<Vec<String>>,
    request_pairs: Mutex<Vec<(String, String)>>,
    changes: Mutex<Vec<String>>,
    merges: Mutex<Vec<String>>,
    workspace: Vec<DuplicateReviewRow>,
    review: Option<DuplicateReviewRow>,
    customers: Vec<DuplicateReviewCustomer>,
    events: Vec<DuplicateReviewEventRow>,
    eligible: bool,
    blockers: MergeBlockers,
    facts: Option<MergePartyFacts>,
    deny_digest: bool,
}

impl Fake {
    fn ready() -> Self {
        Self {
            customers_active: 2,
            customer_active: 1,
            workspace: vec![row()],
            review: Some(row()),
            customers: vec![DuplicateReviewCustomer {
                id: "customer-a".into(),
                name: "Customer A".into(),
                code: "A".into(),
                tax_registration: Some("TAX-A".into()),
                version: 1,
            }],
            events: vec![DuplicateReviewEventRow {
                id: format!("customer-duplicate-event-{KEY}"),
                version: 1,
                action: "request".into(),
                actor_id: "requester".into(),
                reason: "Matching records".into(),
                canonical_customer_id: None,
                created_at: NOW - 1_000,
            }],
            eligible: true,
            facts: Some(MergePartyFacts {
                canonical_status: "active".into(),
                merged_status: "active".into(),
                canonical_merged_into_id: None,
                merged_merged_into_id: None,
                canonical_currency: "THB".into(),
                merged_currency: "THB".into(),
            }),
            ..Default::default()
        }
    }
}

impl DuplicateReviewCommandRepository for Fake {
    type Error = BusinessError;
    fn digest<'a>(&'a mut self, _value: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, String, Self::Error> {
        Box::pin(async move {
            if self.deny_digest { return Err(BusinessError::invalid("X","Y")); }
            Ok("a".repeat(64))
        })
    }
    fn receipt<'a>(&'a mut self, _key: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        let r = self.receipt.clone();
        Box::pin(async move { Ok(r) })
    }
    fn customers_active<'a>(&'a mut self, _company: &'a str, _left: &'a str, _right: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, i64, Self::Error> {
        let c = self.customers_active;
        Box::pin(async move { Ok(c) })
    }
    fn customer_active<'a>(&'a mut self, _company: &'a str, _customer: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, i64, Self::Error> {
        let c = self.customer_active;
        Box::pin(async move { Ok(c) })
    }
    fn review_for_update<'a>(&'a mut self, _actor: &'a AccessActor, _id: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Option<DuplicateReviewRow>, Self::Error> {
        let row = self.review.clone();
        Box::pin(async move { Ok(row) })
    }
    fn reviewer_eligible<'a>(&'a mut self, _actor: &'a AccessActor, _id: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, bool, Self::Error> {
        let eligible = self.eligible;
        Box::pin(async move { Ok(eligible) })
    }
    fn merge_blockers<'a>(&'a mut self, _merged: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, MergeBlockers, Self::Error> {
        let blockers = self.blockers.clone();
        Box::pin(async move { Ok(blockers) })
    }
    fn merge_parties<'a>(&'a mut self, _company: &'a str, _canonical: &'a str, _merged: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Option<MergePartyFacts>, Self::Error> {
        let facts = self.facts.clone();
        Box::pin(async move { Ok(facts) })
    }
    fn commit_request<'a>(&'a mut self, _actor: &'a AccessActor, key: &'a str, plan: &'a RequestPlan, _hash: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Value, Self::Error> {
        self.commits.lock().unwrap().push(key.to_owned());
        self.request_pairs
            .lock()
            .unwrap()
            .push((plan.left_customer_id.clone(), plan.right_customer_id.clone()));
        let result = serde_json::json!({"id": plan.id, "status": "pending", "version": 1});
        Box::pin(async move { Ok(result) })
    }
    fn commit_change<'a>(&'a mut self, _actor: &'a AccessActor, key: &'a str, plan: &'a ChangePlan, _hash: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Value, Self::Error> {
        self.changes.lock().unwrap().push(key.to_owned());
        let result = serde_json::json!({
            "id": plan.id,
            "status": plan.status,
            "version": plan.version,
        });
        Box::pin(async move { Ok(result) })
    }
    fn commit_merge<'a>(&'a mut self, _actor: &'a AccessActor, key: &'a str, plan: &'a MergePlan, _hash: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Value, Self::Error> {
        self.merges.lock().unwrap().push(key.to_owned());
        let result = serde_json::json!({
            "id": plan.id,
            "status": "merged",
            "version": plan.version,
            "canonicalCustomerId": plan.canonical_customer_id,
            "mergedCustomerId": plan.merged_customer_id,
        });
        Box::pin(async move { Ok(result) })
    }
}

impl DuplicateReviewReadRepository for Fake {
    type Error = BusinessError;
    fn workspace<'a>(&'a mut self, _actor: &'a AccessActor) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Vec<DuplicateReviewRow>, Self::Error> {
        let rows = self.workspace.clone();
        Box::pin(async move { Ok(rows) })
    }
    fn customers<'a>(&'a mut self, _actor: &'a AccessActor) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Vec<DuplicateReviewCustomer>, Self::Error> {
        let c = self.customers.clone();
        Box::pin(async move { Ok(c) })
    }
    fn history<'a>(&'a mut self, _id: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Vec<DuplicateReviewEventRow>, Self::Error> {
        let e = self.events.clone();
        Box::pin(async move { Ok(e) })
    }
}

fn change(action: &str, expected: i64) -> DuplicateChange {
    DuplicateChange {
        action: serde_json::from_value(serde_json::json!(action)).unwrap(),
        expected_version: expected,
        canonical_customer_id: None,
        reason: "Independent duplicate review".into(),
    }
}

#[tokio::test]
async fn request_duplicate_review_success() {
    let mut repo = Fake::ready();
    let actor = actor(&["commerce.read", "commerce.sales"]);
    let key = CommandKey::new(KEY).unwrap();
    let input = DuplicateReviewRequest {
        left_customer_id: "customer-b".into(),
        right_customer_id: "customer-a".into(),
        evidence: "  Matching records  ".into(),
    };
    let result = request_duplicate_review(&mut repo, &actor, &key, &input, NOW).await.unwrap();
    assert_eq!(result["status"], "pending");
    assert_eq!(result["version"], 1);
    assert_eq!(repo.commits.lock().unwrap().len(), 1);
    assert_eq!(
        repo.request_pairs.lock().unwrap().as_slice(),
        [("customer-a".into(), "customer-b".into())]
    );
}

#[tokio::test]
async fn request_duplicate_review_requires_company_scope_before_receipt_lookup() {
    let mut repo = Fake::ready();
    let mut scoped = actor(&["commerce.read", "commerce.sales"]);
    scoped.branch_id = Some(RecordId::new("branch").unwrap());
    let key = CommandKey::new(KEY).unwrap();
    let input = DuplicateReviewRequest {
        left_customer_id: "customer-a".into(),
        right_customer_id: "customer-b".into(),
        evidence: "Branch-scoped duplicate proposal".into(),
    };
    let err = request_duplicate_review(&mut repo, &scoped, &key, &input, NOW)
        .await
        .unwrap_err();
    assert_eq!(err.kind, FailureKind::Forbidden);
    assert_eq!(err.code, "CUSTOMER_COMPANY_REVIEW_REQUIRED");
    assert!(repo.commits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn request_duplicate_review_invalid_input() {
    let mut repo = Fake::ready();
    let actor = actor(&["commerce.read", "commerce.sales"]);
    let key = CommandKey::new(KEY).unwrap();
    let input = DuplicateReviewRequest {
        left_customer_id: "customer-a".into(),
        right_customer_id: "customer-a".into(),
        evidence: "Matching records".into(),
    };
    let err = request_duplicate_review(&mut repo, &actor, &key, &input, NOW).await.unwrap_err();
    assert_eq!(err.kind, FailureKind::InvalidInput);
}

#[tokio::test]
async fn request_duplicate_review_missing_customers() {
    let mut repo = Fake::ready();
    repo.customers_active = 1;
    let actor = actor(&["commerce.read", "commerce.sales"]);
    let key = CommandKey::new(KEY).unwrap();
    let input = DuplicateReviewRequest {
        left_customer_id: "customer-a".into(),
        right_customer_id: "customer-b".into(),
        evidence: "Matching records".into(),
    };
    let err = request_duplicate_review(&mut repo, &actor, &key, &input, NOW).await.unwrap_err();
    assert_eq!(err.kind, FailureKind::NotFound);
}

#[tokio::test]
async fn request_duplicate_review_idempotent_replay() {
    let mut repo = Fake::ready();
    let parsed = AccessReceipt::new(
        format!("customer-duplicate-{KEY}"), "pending", 1,
    );
    repo.receipt = Some(ReceiptSnapshot {
        request_hash: "a".repeat(64),
        result_json: parsed.to_wire().to_string(),
        operation: "customer.duplicate.request".into(),
        actor_id: "requester".into(),
        record_id: format!("customer-duplicate-{KEY}"),
        authority_json: serde_json::json!({"companyId":"company","assignmentId":"assignment-requester"}).to_string(),
    });
    let actor = actor(&["commerce.read", "commerce.sales"]);
    let key = CommandKey::new(KEY).unwrap();
    let input = DuplicateReviewRequest {
        left_customer_id: "customer-a".into(),
        right_customer_id: "customer-b".into(),
        evidence: "Matching records".into(),
    };
    let result = request_duplicate_review(&mut repo, &actor, &key, &input, NOW).await.unwrap();
    assert_eq!(result["status"], "pending");
    assert!(repo.commits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn duplicate_review_workspace_and_detail() {
    let mut repo = Fake::ready();
    let actor = actor(&["commerce.read", "commerce.sales"]);
    let value = duplicate_review_workspace(&mut repo, &actor, NOW).await.unwrap();
    let rows = value["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(value["canCreate"], true);
    assert_eq!(rows[0]["canReview"], false);
    let detail = duplicate_review_detail(&mut repo, &actor, &format!("customer-duplicate-{KEY}"), NOW).await.unwrap();
    assert_eq!(detail["row"]["status"], "pending");
    let history = detail["history"].as_array().unwrap();
    assert_eq!(history.len(), 1);
}

#[tokio::test]
async fn duplicate_review_workspace_rejects_permission() {
    let mut repo = Fake::ready();
    let actor = actor(&["commerce.read"]);
    let err = duplicate_review_workspace(&mut repo, &actor, NOW).await.unwrap_err();
    assert_eq!(err.kind, FailureKind::Forbidden);
}

#[tokio::test]
async fn duplicate_workspace_exposes_independent_review_flags() {
    let mut repo = Fake::ready();
    let actor = reviewer();
    let value = duplicate_review_workspace(&mut repo, &actor, NOW).await.unwrap();
    assert_eq!(value["rows"][0]["canReview"], true);
    assert_eq!(value["rows"][0]["canMerge"], false);
}

#[tokio::test]
async fn duplicate_change_requires_independent_approver() {
    let mut repo = Fake::ready();
    let actor = actor(&["commerce.read", "commerce.sales", "commerce.approve"]);
    let key = CommandKey::new(KEY).unwrap();
    let err = change_duplicate_review(
        &mut repo,
        &actor,
        &key,
        &format!("customer-duplicate-{KEY}"),
        &change("distinct", 1),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "INDEPENDENT_APPROVER_REQUIRED");
    assert_eq!(err.kind, FailureKind::Forbidden);
    assert!(repo.changes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn duplicate_change_rejects_stale_version() {
    let mut repo = Fake::ready();
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let err = change_duplicate_review(
        &mut repo,
        &actor,
        &key,
        &format!("customer-duplicate-{KEY}"),
        &change("distinct", 2),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "CUSTOMER_DUPLICATE_CHANGED");
    assert_eq!(err.kind, FailureKind::Conflict);
}

#[tokio::test]
async fn duplicate_change_requires_canonical_pair_member() {
    let mut repo = Fake::ready();
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let mut input = change("duplicate", 1);
    input.canonical_customer_id = Some("other".into());
    let err = change_duplicate_review(
        &mut repo,
        &actor,
        &key,
        &format!("customer-duplicate-{KEY}"),
        &input,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "CUSTOMER_DUPLICATE_CHANGED");
}

#[tokio::test]
async fn duplicate_decision_records_the_canonical_customer() {
    let mut repo = Fake::ready();
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let mut input = change("duplicate", 1);
    input.canonical_customer_id = Some("customer-a".into());
    let result = change_duplicate_review(
        &mut repo,
        &actor,
        &key,
        &format!("customer-duplicate-{KEY}"),
        &input,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "duplicate");
    assert_eq!(result["version"], 2);
    assert_eq!(repo.changes.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn reopen_resets_the_review_to_pending() {
    let mut repo = Fake::ready();
    repo.review = Some(DuplicateReviewRow {
        status: "distinct".into(),
        version: 2,
        reviewed_by: Some("reviewer".into()),
        ..row()
    });
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let result = change_duplicate_review(
        &mut repo,
        &actor,
        &key,
        &format!("customer-duplicate-{KEY}"),
        &change("reopen", 2),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "pending");
    assert_eq!(result["version"], 3);
}

#[tokio::test]
async fn merge_requires_a_confirmed_duplicate_decision() {
    let mut repo = Fake::ready();
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let err = change_duplicate_review(
        &mut repo,
        &actor,
        &key,
        &format!("customer-duplicate-{KEY}"),
        &change("merge", 1),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "CUSTOMER_MERGE_REQUIRES_DUPLICATE");
    assert!(repo.merges.lock().unwrap().is_empty());
}

#[tokio::test]
async fn merge_blocks_when_a_source_retains_immutable_scope() {
    let mut repo = Fake::ready();
    repo.review = Some(DuplicateReviewRow {
        status: "duplicate".into(),
        canonical_customer_id: Some("customer-a".into()),
        version: 2,
        reviewed_by: Some("reviewer".into()),
        ..row()
    });
    repo.blockers = MergeBlockers {
        grants: true,
        ..Default::default()
    };
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let err = change_duplicate_review(
        &mut repo,
        &actor,
        &key,
        &format!("customer-duplicate-{KEY}"),
        &change("merge", 2),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "CUSTOMER_MERGE_BLOCKED");
    assert!(repo.merges.lock().unwrap().is_empty());
}

#[tokio::test]
async fn merge_repoints_the_loser_to_the_canonical_customer() {
    let mut repo = Fake::ready();
    repo.review = Some(DuplicateReviewRow {
        status: "duplicate".into(),
        canonical_customer_id: Some("customer-a".into()),
        version: 2,
        reviewed_by: Some("reviewer".into()),
        ..row()
    });
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let result = change_duplicate_review(
        &mut repo,
        &actor,
        &key,
        &format!("customer-duplicate-{KEY}"),
        &change("merge", 2),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "merged");
    assert_eq!(result["version"], 3);
    assert_eq!(result["canonicalCustomerId"], "customer-a");
    assert_eq!(result["mergedCustomerId"], "customer-b");
    assert_eq!(repo.merges.lock().unwrap().len(), 1);
}
