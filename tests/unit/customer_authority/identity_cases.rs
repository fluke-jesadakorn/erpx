//! Native Customer Authority customer identity-change policy tests.
use erpx_server_customer_authority::public::access::{AccessActor, AccessReceipt, ReceiptSnapshot};
use erpx_server_customer_authority::public::identity::{
    ChangePlan, CustomerAddress, CustomerIdentity, CustomerSnapshot, IdentityChange,
    IdentityChangeRow, IdentityCommandRepository, IdentityCustomer, IdentityEventRow,
    IdentityReadRepository, IdentityRequest, NotificationConsent, RequestPlan,
    change_customer_identity, identity_detail, identity_workspace,
    request_customer_identity_change,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, UserId,
};
use serde_json::Value;
use std::sync::Mutex;

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "customer-identity-test-0001";

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

fn requester() -> AccessActor {
    actor_with(
        "requester",
        &["commerce.read", "commerce.sales", "commerce.approve"],
    )
}

fn reviewer() -> AccessActor {
    actor_with(
        "reviewer",
        &["commerce.read", "commerce.sales", "commerce.approve"],
    )
}

fn proposed() -> CustomerIdentity {
    CustomerIdentity {
        name: "Reviewed customer name".into(),
        tax_registration: Some("TAX-NEW".into()),
        phone: Some("+66 2 555 0101".into()),
        address: Some(CustomerAddress {
            line1: "99 Demo Road".into(),
            line2: None,
            city: "Bangkok".into(),
            region: Some("Bangkok".into()),
            postal_code: "10110".into(),
            country_code: "TH".into(),
        }),
        notification_email: Some("billing@example.test".into()),
        notification_consent: NotificationConsent::Granted,
    }
}

fn previous() -> CustomerIdentity {
    CustomerIdentity {
        name: "Old customer name".into(),
        tax_registration: Some("TAX-OLD".into()),
        phone: None,
        address: None,
        notification_email: None,
        notification_consent: NotificationConsent::Unknown,
    }
}

fn row() -> IdentityChangeRow {
    IdentityChangeRow {
        id: format!("customer-identity-{KEY}"),
        company_id: "company".into(),
        customer_id: "customer-1".into(),
        customer_name: "Old customer name".into(),
        customer_code: "C-001".into(),
        current_customer_version: 1,
        expected_customer_version: 1,
        previous: serde_json::to_value(previous()).unwrap(),
        proposed: serde_json::to_value(proposed()).unwrap(),
        status: "pending".into(),
        evidence: "Customer supplied registration evidence".into(),
        created_by: "requester".into(),
        reviewed_by: None,
        version: 1,
        created_at: NOW - 1_000,
        updated_at: NOW - 1_000,
        actor_eligible: true,
    }
}

fn request() -> IdentityRequest {
    IdentityRequest {
        customer_id: "customer-1".into(),
        expected_customer_version: 1,
        proposed: proposed(),
        evidence: "  Customer supplied registration evidence  ".into(),
    }
}

fn change(action: &str) -> IdentityChange {
    match action {
        "resubmit" => IdentityChange {
            action: serde_json::from_value(serde_json::json!(action)).unwrap(),
            expected_version: 2,
            expected_customer_version: Some(1),
            proposed: Some(CustomerIdentity {
                name: "Corrected customer name".into(),
                tax_registration: Some("TAX-FINAL".into()),
                phone: None,
                address: None,
                notification_email: None,
                notification_consent: NotificationConsent::Revoked,
            }),
            reason: "Attached corrected evidence".into(),
        },
        _ => IdentityChange {
            action: serde_json::from_value(serde_json::json!(action)).unwrap(),
            expected_version: 1,
            expected_customer_version: None,
            proposed: None,
            reason: "Independent evidence review completed".into(),
        },
    }
}

#[derive(Default)]
struct Fake {
    receipt: Option<ReceiptSnapshot>,
    snapshot: Option<CustomerSnapshot>,
    version: Option<i64>,
    change: Option<IdentityChangeRow>,
    workspace: Vec<IdentityChangeRow>,
    events: Vec<IdentityEventRow>,
    eligible: bool,
    requests: Mutex<Vec<String>>,
    changes: Mutex<Vec<String>>,
}

impl Fake {
    fn ready() -> Self {
        Self {
            snapshot: Some(CustomerSnapshot {
                identity: previous(),
                version: 1,
                code: "C-001".into(),
            }),
            version: Some(1),
            change: Some(row()),
            workspace: vec![row()],
            events: vec![IdentityEventRow {
                id: format!("customer-identity-event-{KEY}"),
                change_id: format!("customer-identity-{KEY}"),
                version: 1,
                action: "request".into(),
                actor_id: "requester".into(),
                reason: "Customer supplied registration evidence".into(),
                customer_version: 1,
                previous: serde_json::to_value(previous()).unwrap(),
                proposed: serde_json::to_value(proposed()).unwrap(),
                created_at: NOW - 1_000,
            }],
            eligible: true,
            ..Default::default()
        }
    }
}

impl IdentityCommandRepository for Fake {
    type Error = BusinessError;
    fn digest<'a>(&'a mut self, _value: &'a str) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, String, Self::Error> {
        Box::pin(async move { Ok("a".repeat(64)) })
    }
    fn receipt<'a>(&'a mut self, _key: &'a str) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }
    fn source_customer<'a>(&'a mut self, _company: &'a str, _customer: &'a str) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, Option<CustomerSnapshot>, Self::Error> {
        let snapshot = self.snapshot.clone();
        Box::pin(async move { Ok(snapshot) })
    }
    fn customer_version<'a>(&'a mut self, _company: &'a str, _customer: &'a str) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, Option<i64>, Self::Error> {
        let version = self.version;
        Box::pin(async move { Ok(version) })
    }
    fn change_for_update<'a>(&'a mut self, _actor: &'a AccessActor, _id: &'a str) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, Option<IdentityChangeRow>, Self::Error> {
        let row = self.change.clone();
        Box::pin(async move { Ok(row) })
    }
    fn reviewer_eligible<'a>(&'a mut self, _actor: &'a AccessActor, _id: &'a str) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, bool, Self::Error> {
        let eligible = self.eligible;
        Box::pin(async move { Ok(eligible) })
    }
    fn commit_request<'a>(&'a mut self, _actor: &'a AccessActor, key: &'a str, plan: &'a RequestPlan, _hash: &'a str) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, Value, Self::Error> {
        self.requests.lock().unwrap().push(key.to_owned());
        let result = serde_json::json!({"id": plan.id, "status": "pending", "version": 1});
        Box::pin(async move { Ok(result) })
    }
    fn commit_change<'a>(&'a mut self, _actor: &'a AccessActor, key: &'a str, plan: &'a ChangePlan, _hash: &'a str) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, Value, Self::Error> {
        self.changes.lock().unwrap().push(key.to_owned());
        let result = serde_json::json!({
            "id": plan.id,
            "status": plan.status,
            "version": plan.version,
        });
        Box::pin(async move { Ok(result) })
    }
}

impl IdentityReadRepository for Fake {
    type Error = BusinessError;
    fn workspace<'a>(&'a mut self, _actor: &'a AccessActor) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, Vec<IdentityChangeRow>, Self::Error> {
        let rows = self.workspace.clone();
        Box::pin(async move { Ok(rows) })
    }
    fn customers<'a>(&'a mut self, _actor: &'a AccessActor) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, Vec<IdentityCustomer>, Self::Error> {
        let customers = vec![IdentityCustomer {
            id: "customer-1".into(),
            name: "Old customer name".into(),
            code: "C-001".into(),
            tax_registration: Some("TAX-OLD".into()),
            phone: None,
            address: serde_json::json!({}),
            version: 1,
        }];
        Box::pin(async move { Ok(customers) })
    }
    fn history<'a>(&'a mut self, _id: &'a str) -> erpx_server_customer_authority::public::identity::IdentityFuture<'a, Vec<IdentityEventRow>, Self::Error> {
        let events = self.events.clone();
        Box::pin(async move { Ok(events) })
    }
}

#[tokio::test]
async fn request_identity_change_success() {
    let mut repo = Fake::ready();
    let actor = requester();
    let key = CommandKey::new(KEY).unwrap();
    let result = request_customer_identity_change(&mut repo, &actor, &key, &request(), NOW)
        .await
        .unwrap();
    assert_eq!(result["status"], "pending");
    assert_eq!(result["version"], 1);
    assert_eq!(repo.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn request_identity_change_rejects_an_unchanged_snapshot() {
    let mut repo = Fake::ready();
    repo.snapshot = Some(CustomerSnapshot {
        identity: proposed(),
        version: 1,
        code: "C-001".into(),
    });
    let actor = requester();
    let key = CommandKey::new(KEY).unwrap();
    let err = request_customer_identity_change(&mut repo, &actor, &key, &request(), NOW)
        .await
        .unwrap_err();
    assert_eq!(err.code, "CUSTOMER_IDENTITY_UNCHANGED");
    assert_eq!(err.kind, FailureKind::InvalidInput);
}

#[tokio::test]
async fn request_identity_change_rejects_a_stale_customer_version() {
    let mut repo = Fake::ready();
    repo.snapshot = Some(CustomerSnapshot {
        identity: previous(),
        version: 2,
        code: "C-001".into(),
    });
    let actor = requester();
    let key = CommandKey::new(KEY).unwrap();
    let err = request_customer_identity_change(&mut repo, &actor, &key, &request(), NOW)
        .await
        .unwrap_err();
    assert_eq!(err.code, "CUSTOMER_IDENTITY_SOURCE_CHANGED");
    assert_eq!(err.kind, FailureKind::Conflict);
}

#[tokio::test]
async fn request_identity_change_is_idempotent() {
    let mut repo = Fake::ready();
    let receipt = AccessReceipt::new(format!("customer-identity-{KEY}"), "pending", 1);
    repo.receipt = Some(ReceiptSnapshot {
        request_hash: "a".repeat(64),
        result_json: receipt.to_wire().to_string(),
        operation: "customer.identity.request".into(),
        actor_id: "requester".into(),
        record_id: format!("customer-identity-{KEY}"),
        authority_json: serde_json::json!({
            "companyId": "company",
            "assignmentId": "assignment-requester"
        })
        .to_string(),
    });
    let actor = requester();
    let key = CommandKey::new(KEY).unwrap();
    let result = request_customer_identity_change(&mut repo, &actor, &key, &request(), NOW)
        .await
        .unwrap();
    assert_eq!(result["status"], "pending");
    assert!(repo.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn apply_requires_an_independent_manager() {
    let mut repo = Fake::ready();
    let actor = requester();
    let key = CommandKey::new(KEY).unwrap();
    let err = change_customer_identity(
        &mut repo,
        &actor,
        &key,
        &format!("customer-identity-{KEY}"),
        &change("apply"),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "INDEPENDENT_APPROVER_REQUIRED");
    assert_eq!(err.kind, FailureKind::Forbidden);
    assert!(repo.changes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn apply_rejects_an_ineligible_reviewer() {
    let mut repo = Fake::ready();
    repo.eligible = false;
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let err = change_customer_identity(
        &mut repo,
        &actor,
        &key,
        &format!("customer-identity-{KEY}"),
        &change("apply"),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "CUSTOMER_IDENTITY_REVIEWER_UNAVAILABLE");
}

#[tokio::test]
async fn apply_rejects_a_changed_customer_master() {
    let mut repo = Fake::ready();
    repo.version = Some(2);
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let err = change_customer_identity(
        &mut repo,
        &actor,
        &key,
        &format!("customer-identity-{KEY}"),
        &change("apply"),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, "CUSTOMER_IDENTITY_SOURCE_CHANGED");
}

#[tokio::test]
async fn apply_records_the_identity_decision() {
    let mut repo = Fake::ready();
    let actor = reviewer();
    let key = CommandKey::new(KEY).unwrap();
    let result = change_customer_identity(
        &mut repo,
        &actor,
        &key,
        &format!("customer-identity-{KEY}"),
        &change("apply"),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "applied");
    assert_eq!(result["version"], 2);
}

#[tokio::test]
async fn rejected_change_can_be_resubmitted_by_the_requester() {
    let mut repo = Fake::ready();
    repo.change = Some(IdentityChangeRow {
        status: "rejected".into(),
        version: 2,
        reviewed_by: Some("reviewer".into()),
        ..row()
    });
    let actor = requester();
    let key = CommandKey::new(KEY).unwrap();
    let result = change_customer_identity(
        &mut repo,
        &actor,
        &key,
        &format!("customer-identity-{KEY}"),
        &change("resubmit"),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "pending");
    assert_eq!(result["version"], 3);
}

#[tokio::test]
async fn workspace_and_detail_expose_review_capability() {
    let mut repo = Fake::ready();
    let actor = reviewer();
    let value = identity_workspace(&mut repo, &actor, NOW).await.unwrap();
    assert_eq!(value["rows"][0]["canReview"], true);
    assert_eq!(value["rows"][0]["canResubmit"], false);
    assert_eq!(value["canCreate"], true);
    let detail = identity_detail(
        &mut repo,
        &actor,
        &format!("customer-identity-{KEY}"),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(detail["history"].as_array().unwrap().len(), 1);
}
