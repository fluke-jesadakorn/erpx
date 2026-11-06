//! Native Customer Authority duplicate-review policy, command and read tests.
use erpx_server_customer_authority::public::access::{AccessActor, AccessReceipt, ReceiptSnapshot};
use erpx_server_customer_authority::public::duplicates::{
    DuplicateReviewCommandRepository, DuplicateReviewReadRepository, DuplicateReviewCustomer,
    DuplicateReviewEventRow, DuplicateReviewRequest, DuplicateReviewRow, RequestPlan,
    duplicate_review_detail, duplicate_review_workspace, request_duplicate_review,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, UserId};
use serde_json::Value;
use std::sync::Mutex;

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "customer-duplicate-test-0001";

fn actor(permissions: &[&str]) -> AccessActor {
    AccessActor {
        user_id: UserId::new("requester").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: erpx_server_shared::public::AssignmentId::new("assignment-requester").unwrap(),
        role: Some("sales_manager".into()),
        authority_revision: Some(3),
        assignment_version: Some(2),
        permissions: permissions.iter().map(|v| (*v).to_owned()).collect(),
        expires_at: NOW + 60_000,
    }
}

fn row() -> DuplicateReviewRow {
    DuplicateReviewRow {
        id: "customer-duplicate-KEY".replace("KEY", KEY),
        company_id: "company".into(),
        left_customer_id: "customer-a".into(),
        right_customer_id: "customer-b".into(),
        left_name: "Customer A".into(),
        right_name: "Customer B".into(),
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
    commits: Mutex<Vec<String>>,
    workspace: Vec<DuplicateReviewRow>,
    customers: Vec<DuplicateReviewCustomer>,
    events: Vec<DuplicateReviewEventRow>,
    deny_digest: bool,
}

impl Fake {
    fn ready() -> Self {
        Self {
            customers_active: 2,
            workspace: vec![row()],
            customers: vec![DuplicateReviewCustomer { id: "customer-a".into(), name: "Customer A".into(), code: "A".into() }],
            events: vec![DuplicateReviewEventRow { version: 1, action: "request".into(), actor_id: "requester".into(), reason: "Matching records".into(), canonical_customer_id: None, created_at: NOW - 1_000 }],
            ..Default::default()
        }
    }
}

// Implement command repo
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
    fn commit_request<'a>(&'a mut self, _actor: &'a AccessActor, key: &'a str, plan: &'a RequestPlan, _hash: &'a str) -> erpx_server_customer_authority::public::duplicates::DuplicateAccessFuture<'a, Value, Self::Error> {
        self.commits.lock().unwrap().push(key.to_owned());
        let result = serde_json::json!({"id": plan.id, "status": "pending", "version": 1});
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


#[tokio::test]
async fn request_duplicate_review_success() {
    let mut repo = Fake::ready();
    let actor = actor(&["commerce.read", "commerce.sales"]);
    let key = CommandKey::new(KEY).unwrap();
    let input = DuplicateReviewRequest {
        left_customer_id: "customer-a".into(),
        right_customer_id: "customer-b".into(),
        evidence: "  Matching records  ".into(),
    };
    let result = request_duplicate_review(&mut repo, &actor, &key, &input, NOW).await.unwrap();
    assert_eq!(result["status"], "pending");
    assert_eq!(result["version"], 1);
    assert_eq!(repo.commits.lock().unwrap().len(), 1);
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
    // build a prior receipt
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
    // digest must match
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
    let detail = duplicate_review_detail(&mut repo, &actor, &format!("customer-duplicate-{KEY}"), NOW).await.unwrap();
    assert_eq!(detail["row"]["status"], "pending");
    let history = detail["history"].as_array().unwrap();
    assert_eq!(history.len(), 1);
}

#[tokio::test]
async fn duplicate_review_workspace_rejects_permission() {
    let mut repo = Fake::ready();
    let actor = actor(&["commerce.read"]); // no sales
    let err = duplicate_review_workspace(&mut repo, &actor, NOW).await.unwrap_err();
    assert_eq!(err.kind, FailureKind::Forbidden);
}
