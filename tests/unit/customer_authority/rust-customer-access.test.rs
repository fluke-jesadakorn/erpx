//! Native Customer Authority customer-access policy, command and read tests.
use erpx_server_customer_authority::public::access::{
    AccessActor, AccessBindingRow, AccessChange, AccessEventRow, AccessFuture, AccessOrder,
    AccessReadRow, AccessReceipt, AccessRequest, AccessSourceRow, ChangePlan,
    CustomerAccessCommandRepository, CustomerAccessReadRepository, ReceiptSnapshot, RequestPlan,
    access_detail, access_workspace, change_access, request_access,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use serde_json::Value;
use std::sync::Mutex;

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "customer-access-test-0001";

fn actor(permissions: &[&str]) -> AccessActor {
    AccessActor {
        user_id: UserId::new("requester").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("assignment-requester").unwrap(),
        role: Some("sales_manager".into()),
        authority_revision: Some(3),
        assignment_version: Some(2),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW + 60_000,
    }
}

fn reviewer() -> AccessActor {
    AccessActor {
        user_id: UserId::new("reviewer").unwrap(),
        assignment_id: AssignmentId::new("assignment-reviewer").unwrap(),
        ..actor(&["commerce.read", "commerce.sales", "commerce.approve"])
    }
}

fn key() -> CommandKey {
    CommandKey::new(KEY).unwrap()
}

fn order() -> AccessOrder {
    AccessOrder {
        id: "order-1".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        customer_id: "customer-1".into(),
        number: "QT-100".into(),
        kind: "quote".into(),
        state: "confirmed".into(),
        version: 3,
    }
}

fn binding() -> AccessBindingRow {
    AccessBindingRow {
        id: "customer-access-review-1".into(),
        company_id: "company".into(),
        order_id: "order-1".into(),
        customer_id: "customer-1".into(),
        email: "customer@example.test".into(),
        expires_at: NOW + 86_400_000,
        status: "pending".into(),
        reason: "Verified customer authority source".into(),
        created_by: "requester".into(),
        approved_by: None,
        reviewed_at: None,
        version: 1,
        created_at: NOW - 1_000,
        updated_at: NOW - 1_000,
        order_number: "QT-100".into(),
        order_state: "confirmed".into(),
        order_kind: "quote".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        actor_eligible: true,
        reviewers: 1,
        identity_exists: false,
    }
}

fn request(email: &str) -> AccessRequest {
    AccessRequest {
        order_id: "order-1".into(),
        source_version: 3,
        email: email.into(),
        expires_at: NOW + 86_400_000,
        reason: "  Verified customer authority source  ".into(),
    }
}

#[derive(Default)]
struct Fake {
    receipt: Option<ReceiptSnapshot>,
    order: Option<AccessOrder>,
    binding: Option<AccessBindingRow>,
    reviewer: bool,
    commits: Mutex<Vec<String>>,
    plans: Mutex<Vec<String>>,
    workspace: Vec<AccessBindingRow>,
    sources: Vec<AccessSourceRow>,
    events: Vec<AccessEventRow>,
    reads: Vec<AccessReadRow>,
    deny_commit: bool,
    last_digest: Mutex<String>,
}

impl Fake {
    fn ready() -> Self {
        Self {
            order: Some(order()),
            binding: Some(binding()),
            reviewer: true,
            workspace: vec![binding()],
            sources: vec![AccessSourceRow {
                id: "order-1".into(),
                number: "QT-100".into(),
                name: "Synthetic customer".into(),
                version: 3,
            }],
            events: vec![AccessEventRow {
                version: 1,
                action: "request".into(),
                actor_id: "requester".into(),
                reason: "Verified customer authority source".into(),
                created_at: NOW - 1_000,
            }],
            reads: vec![AccessReadRow {
                binding_version: 1,
                user_id: "portal-user".into(),
                read_at: NOW - 500,
                projection_version: "v1".into(),
            }],
            ..Default::default()
        }
    }
    fn calls(&self) -> Vec<String> {
        self.commits.lock().unwrap().clone()
    }
}

impl CustomerAccessCommandRepository for Fake {
    type Error = BusinessError;

    fn digest<'a>(&'a mut self, value: &'a str) -> AccessFuture<'a, String, Self::Error> {
        let mut seed: u64 = 0xcbf29ce484222325;
        for byte in value.bytes() {
            seed ^= u64::from(byte);
            seed = seed.wrapping_mul(0x100000001b3);
        }
        let digest = (0..32)
            .map(|index| {
                let byte = ((seed >> ((index % 8) * 8)) ^ (seed >> (index % 13))) as u8;
                format!("{byte:02x}")
            })
            .collect::<String>();
        *self.last_digest.lock().unwrap() = digest.clone();
        Box::pin(async move { Ok(digest) })
    }
    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> AccessFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }
    fn order<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> AccessFuture<'a, Option<AccessOrder>, Self::Error> {
        let order = self.order.clone();
        Box::pin(async move { Ok(order) })
    }
    fn binding_for_update<'a>(
        &'a mut self,
        _: &'a AccessActor,
        _: &'a str,
    ) -> AccessFuture<'a, Option<AccessBindingRow>, Self::Error> {
        let binding = self.binding.clone();
        Box::pin(async move { Ok(binding) })
    }
    fn reviewer_eligible<'a>(
        &'a mut self,
        _: &'a AccessActor,
        _: &'a str,
    ) -> AccessFuture<'a, bool, Self::Error> {
        let reviewer = self.reviewer;
        Box::pin(async move { Ok(reviewer) })
    }
    fn commit_request<'a>(
        &'a mut self,
        actor: &'a AccessActor,
        _: &'a str,
        plan: &'a RequestPlan,
        _: &'a str,
    ) -> AccessFuture<'a, Value, Self::Error> {
        self.commits.lock().unwrap().push("request".into());
        self.plans.lock().unwrap().push(format!(
            "{}|{}|{}|{}|{}|{}",
            plan.id, plan.order_id, plan.customer_id, plan.email, plan.expires_at, plan.created_by
        ));
        let deny = self.deny_commit;
        let result = AccessReceipt::new(plan.id.clone(), "pending", 1).to_wire();
        let _ = actor;
        Box::pin(async move {
            if deny {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "SYNTHETIC_COMMIT_FAILED",
                    "Synthetic commit failure.",
                ));
            }
            Ok(result)
        })
    }
    fn commit_change<'a>(
        &'a mut self,
        _: &'a AccessActor,
        _: &'a str,
        plan: &'a ChangePlan,
        _: &'a str,
    ) -> AccessFuture<'a, Value, Self::Error> {
        self.commits.lock().unwrap().push("change".into());
        self.plans.lock().unwrap().push(format!(
            "{}|{}|{}|{}|{}",
            plan.id,
            plan.version,
            plan.status,
            plan.reason,
            plan.action.as_str()
        ));
        let deny = self.deny_commit;
        let result = AccessReceipt::new(plan.id.clone(), &plan.status, plan.version).to_wire();
        Box::pin(async move {
            if deny {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "SYNTHETIC_COMMIT_FAILED",
                    "Synthetic commit failure.",
                ));
            }
            Ok(result)
        })
    }
}

impl CustomerAccessReadRepository for Fake {
    type Error = BusinessError;

    fn binding<'a>(
        &'a mut self,
        _: &'a AccessActor,
        _: &'a str,
    ) -> AccessFuture<'a, Option<AccessBindingRow>, Self::Error> {
        let binding = self.binding.clone();
        Box::pin(async move { Ok(binding) })
    }
    fn workspace<'a>(
        &'a mut self,
        _: &'a AccessActor,
    ) -> AccessFuture<'a, Vec<AccessBindingRow>, Self::Error> {
        let rows = self.workspace.clone();
        Box::pin(async move { Ok(rows) })
    }
    fn sources<'a>(
        &'a mut self,
        _: &'a AccessActor,
    ) -> AccessFuture<'a, Vec<AccessSourceRow>, Self::Error> {
        let rows = self.sources.clone();
        Box::pin(async move { Ok(rows) })
    }
    fn events<'a>(&'a mut self, _: &'a str) -> AccessFuture<'a, Vec<AccessEventRow>, Self::Error> {
        let rows = self.events.clone();
        Box::pin(async move { Ok(rows) })
    }
    fn reads<'a>(&'a mut self, _: &'a str) -> AccessFuture<'a, Vec<AccessReadRow>, Self::Error> {
        let rows = self.reads.clone();
        Box::pin(async move { Ok(rows) })
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("test future unexpectedly pending"),
    }
}

#[test]
fn request_requires_current_read_and_sales_permissions_before_source() {
    for permissions in [vec!["commerce.read"], vec!["commerce.sales"]] {
        let mut fake = Fake::ready();
        let error = block_on(request_access(
            &mut fake,
            &actor(&permissions),
            &key(),
            &request("customer@example.test"),
            NOW,
        ))
        .unwrap_err();
        assert_eq!(error.code, "CUSTOMER_ACCESS_PERMISSION_REQUIRED");
        assert!(fake.calls().is_empty());
    }
    let mut expired = actor(&["commerce.read", "commerce.sales"]);
    expired.expires_at = NOW;
    let error = block_on(request_access(
        &mut Fake::ready(),
        &expired,
        &key(),
        &request("customer@example.test"),
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
}

#[test]
fn request_validates_expiry_email_and_reason_without_committing() {
    let mut fake = Fake::ready();
    for (input, code) in [
        (
            AccessRequest {
                expires_at: NOW,
                ..request("customer@example.test")
            },
            "CUSTOMER_ACCESS_EXPIRY",
        ),
        (
            AccessRequest {
                expires_at: NOW + 91 * 86_400_000,
                ..request("customer@example.test")
            },
            "CUSTOMER_ACCESS_EXPIRY",
        ),
        (request("not-an-email"), "CUSTOMER_ACCESS_REQUEST_INVALID"),
        (
            AccessRequest {
                reason: "  x  ".into(),
                ..request("customer@example.test")
            },
            "CUSTOMER_ACCESS_REQUEST_INVALID",
        ),
        (
            AccessRequest {
                source_version: 0,
                ..request("customer@example.test")
            },
            "CUSTOMER_ACCESS_REQUEST_INVALID",
        ),
    ] {
        let error = block_on(request_access(
            &mut fake,
            &actor(&["commerce.read", "commerce.sales"]),
            &key(),
            &input,
            NOW,
        ))
        .unwrap_err();
        assert_eq!(error.code, code);
    }
    assert!(fake.calls().is_empty());
}

#[test]
fn request_rejects_foreign_scope_stale_version_and_ineligible_source() {
    let mut foreign = Fake::ready();
    foreign.order.as_mut().unwrap().branch_id = Some("other-branch".into());
    let mut branch_actor = actor(&["commerce.read", "commerce.sales"]);
    branch_actor.branch_id = Some(RecordId::new("branch").unwrap());
    let error = block_on(request_access(
        &mut foreign,
        &branch_actor,
        &key(),
        &request("customer@example.test"),
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_ACCESS_SOURCE_NOT_FOUND");
    assert!(foreign.calls().is_empty());

    for mutate in [
        |order: &mut AccessOrder| order.version = 4,
        |order: &mut AccessOrder| order.state = "draft".into(),
        |order: &mut AccessOrder| order.kind = "purchase".into(),
        |order: &mut AccessOrder| order.company_id = "foreign".into(),
    ] {
        let mut fake = Fake::ready();
        mutate(fake.order.as_mut().unwrap());
        let error = block_on(request_access(
            &mut fake,
            &actor(&["commerce.read", "commerce.sales"]),
            &key(),
            &request("customer@example.test"),
            NOW,
        ))
        .unwrap_err();
        assert!(matches!(
            error.code,
            "CUSTOMER_ACCESS_SOURCE_CHANGED" | "CUSTOMER_ACCESS_SOURCE_NOT_FOUND"
        ));
        assert!(fake.calls().is_empty());
    }
}

#[test]
fn request_commits_the_exact_source_bound_plan_and_replays() {
    let mut fake = Fake::ready();
    let result = block_on(request_access(
        &mut fake,
        &actor(&["commerce.read", "commerce.sales"]),
        &key(),
        &request("  Customer@Example.Test "),
        NOW,
    ))
    .unwrap();
    assert_eq!(
        result,
        serde_json::json!({"id": format!("customer-access-{KEY}"), "status": "pending", "version": 1})
    );
    assert_eq!(fake.calls(), vec!["request"]);
    assert_eq!(
        fake.plans.lock().unwrap()[0],
        format!(
            "customer-access-{KEY}|order-1|customer-1|customer@example.test|{}|requester",
            NOW + 86_400_000
        )
    );
    let digest = fake.last_digest.lock().unwrap().clone();
    fake.receipt = Some(ReceiptSnapshot {
        request_hash: digest,
        result_json: result.to_string(),
        operation: "customer.access.request".into(),
        actor_id: "requester".into(),
        record_id: format!("customer-access-{KEY}"),
        authority_json: actor(&["commerce.read", "commerce.sales"]).authority_json(),
    });
    let replay = block_on(request_access(
        &mut fake,
        &actor(&["commerce.read", "commerce.sales"]),
        &key(),
        &request("customer@example.test"),
        NOW,
    ))
    .unwrap();
    assert_eq!(replay, result);
    assert_eq!(fake.calls(), vec!["request"]);
    let error = block_on(request_access(
        &mut fake,
        &actor(&["commerce.read", "commerce.sales"]),
        &key(),
        &request("other@example.test"),
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_ACCESS_IDEMPOTENCY_CONFLICT");
}

#[test]
fn approve_requires_an_independent_active_reviewer_and_current_source() {
    let mut fake = Fake::ready();
    let change = AccessChange {
        action: erpx_server_customer_authority::public::access::AccessAction::Approve,
        expected_version: 1,
        reason: "Independent source-bound approval".into(),
    };
    let error = block_on(change_access(
        &mut fake,
        &actor(&["commerce.read", "commerce.sales", "commerce.approve"]),
        &key(),
        "customer-access-review-1",
        &change,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "INDEPENDENT_APPROVER_REQUIRED");
    assert!(fake.calls().is_empty());

    fake.reviewer = false;
    let error = block_on(change_access(
        &mut fake,
        &reviewer(),
        &key(),
        "customer-access-review-1",
        &change,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_ACCESS_REVIEWER_UNAVAILABLE");
    assert!(fake.calls().is_empty());

    fake.reviewer = true;
    fake.binding.as_mut().unwrap().expires_at = NOW;
    let error = block_on(change_access(
        &mut fake,
        &reviewer(),
        &key(),
        "customer-access-review-1",
        &change,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_ACCESS_CHANGED");

    fake = Fake::ready();
    fake.order.as_mut().unwrap().state = "cancelled".into();
    let error = block_on(change_access(
        &mut fake,
        &reviewer(),
        &key(),
        "customer-access-review-1",
        &change,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_ACCESS_SOURCE_CHANGED");
    assert!(fake.calls().is_empty());

    fake = Fake::ready();
    let result = block_on(change_access(
        &mut fake,
        &reviewer(),
        &key(),
        "customer-access-review-1",
        &change,
        NOW,
    ))
    .unwrap();
    assert_eq!(
        result,
        serde_json::json!({"id": "customer-access-review-1", "status": "approved", "version": 2})
    );
    assert_eq!(fake.calls(), vec!["change"]);
    assert_eq!(
        fake.plans.lock().unwrap()[0],
        "customer-access-review-1|2|approved|Independent source-bound approval|approve"
    );
}

#[test]
fn reject_and_revoke_preserve_cas_and_requester_or_manager_authority() {
    let mut fake = Fake::ready();
    let stale = AccessChange {
        action: erpx_server_customer_authority::public::access::AccessAction::Reject,
        expected_version: 2,
        reason: "Stale decision".into(),
    };
    let error = block_on(change_access(
        &mut fake,
        &reviewer(),
        &key(),
        "customer-access-review-1",
        &stale,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_ACCESS_CHANGED");

    let reject = AccessChange {
        action: erpx_server_customer_authority::public::access::AccessAction::Reject,
        expected_version: 1,
        reason: "Independent rejection".into(),
    };
    let result = block_on(change_access(
        &mut fake,
        &reviewer(),
        &key(),
        "customer-access-review-1",
        &reject,
        NOW,
    ))
    .unwrap();
    assert_eq!(result["status"], "rejected");
    assert_eq!(result["version"], 2);

    let mut fake = Fake::ready();
    let revoke = AccessChange {
        action: erpx_server_customer_authority::public::access::AccessAction::Revoke,
        expected_version: 1,
        reason: "Requester withdrew access".into(),
    };
    let result = block_on(change_access(
        &mut fake,
        &actor(&["commerce.read", "commerce.sales"]),
        &key(),
        "customer-access-review-1",
        &revoke,
        NOW,
    ))
    .unwrap();
    assert_eq!(result["status"], "revoked");

    let mut fake = Fake::ready();
    let mut other = actor(&["commerce.read", "commerce.sales"]);
    other.user_id = UserId::new("other").unwrap();
    let error = block_on(change_access(
        &mut fake,
        &other,
        &key(),
        "customer-access-review-1",
        &revoke,
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_ACCESS_PERMISSION_REQUIRED");
}

#[test]
fn workspace_filters_scope_and_exposes_bounded_decision_flags() {
    let mut fake = Fake::ready();
    let reviewer_actor = reviewer();
    let mut foreign = binding();
    foreign.id = "foreign".into();
    foreign.company_id = "foreign".into();
    fake.workspace.push(foreign);
    let mut other_branch = binding();
    other_branch.id = "other-branch".into();
    other_branch.branch_id = Some("other-branch".into());
    fake.workspace.push(other_branch);
    let mut scoped = actor(&["commerce.read", "commerce.sales", "commerce.approve"]);
    scoped.branch_id = Some(RecordId::new("branch").unwrap());
    let workspace = block_on(access_workspace(&mut fake, &scoped, NOW)).unwrap();
    let rows = workspace["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], "customer-access-review-1");
    assert_eq!(rows[0]["canReview"], true);
    assert_eq!(rows[0]["canInvite"], false);
    assert_eq!(rows[0]["reviewers"], 1);
    assert_eq!(workspace["sources"][0]["number"], "QT-100");
    let _ = reviewer_actor;
}

#[test]
fn detail_returns_only_the_scoped_binding_history_and_read_audit() {
    let mut fake = Fake::ready();
    let detail = block_on(access_detail(
        &mut fake,
        &reviewer(),
        "customer-access-review-1",
        NOW,
    ))
    .unwrap();
    assert_eq!(detail["binding"]["id"], "customer-access-review-1");
    assert_eq!(detail["events"][0]["action"], "request");
    assert_eq!(detail["reads"][0]["projection_version"], "v1");
    assert_eq!(
        detail
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["binding", "events", "reads"]
    );
    fake.binding = None;
    let error = block_on(access_detail(
        &mut fake,
        &reviewer(),
        "customer-access-review-1",
        NOW,
    ))
    .unwrap_err();
    assert_eq!(error.code, "CUSTOMER_ACCESS_NOT_FOUND");
}
