//! Native Manufacturing specification command decisions and application policy.
//! Domain/application only: no adapter, HTTP, receipt persistence or browser.
use erpx_server_manufacturing::public::bom::BomActor;
use erpx_server_manufacturing::public::specification::{
    CREATE_OPERATION, CreatePlan, CreateRequest, ReceiptSnapshot, ReviewAction, ReviewPlan,
    ReviewRequest, SpecificationCommandRepository, SpecificationRow, create_specification,
    parse_quantity_milli, review_specification,
};
use erpx_server_runtime::public::http_support::serde_json;
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, RecordId, TimestampMillis};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

type Fut<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

fn actor() -> BomActor {
    BomActor {
        user_id: "maker".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: vec!["records.read".into(), "records.write".into()],
        modules: vec!["manufacturing".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn creator_reviewer() -> BomActor {
    BomActor {
        user_id: "maker".into(),
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "records.review".into(),
        ],
        ..actor()
    }
}

fn reviewer() -> BomActor {
    BomActor {
        user_id: "reviewer".into(),
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "records.review".into(),
        ],
        ..actor()
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn request() -> CreateRequest {
    serde_json::from_value(serde_json::json!({
        "name": "Bounded assembly",
        "mode": "bom",
        "outputProductId": "output",
        "batchQuantity": "10",
        "expectedYieldBps": 9800,
        "lines": [
            {"productId": "material-a", "quantity": "1.5", "substitutionGroup": "secondary"},
            {"productId": "material-b", "quantity": "2"}
        ],
        "operations": [
            {"resourceId": "resource", "name": "Assemble", "setupMinutes": 5, "runMinutesPerBatch": 30}
        ]
    }))
    .unwrap()
}

fn review(action: ReviewAction, expected_version: i64) -> ReviewRequest {
    serde_json::from_value(serde_json::json!({
        "action": action,
        "expectedVersion": expected_version,
        "reason": "Independent review of the current specification."
    }))
    .unwrap()
}

fn stored(status: &str, version: i64, created_by: &str) -> SpecificationRow {
    SpecificationRow {
        id: "spec".into(),
        status: status.into(),
        version,
        created_by: created_by.into(),
    }
}

#[derive(Default)]
struct Fake {
    output_active: bool,
    materials_active: bool,
    resource_active: bool,
    authority: bool,
    receipt: Option<ReceiptSnapshot>,
    row: Option<SpecificationRow>,
    calls: RefCell<Vec<&'static str>>,
    created: RefCell<Vec<CreatePlan>>,
    reviewed: RefCell<Vec<ReviewPlan>>,
    hashes: RefCell<Vec<String>>,
    result_version: i64,
}

fn good() -> Fake {
    Fake {
        output_active: true,
        materials_active: true,
        resource_active: true,
        authority: true,
        row: Some(stored("draft", 1, "maker")),
        result_version: 1,
        ..Fake::default()
    }
}

impl SpecificationCommandRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(&'a mut self, _: &'a BomActor, write: bool, _: i64) -> Fut<'a, bool> {
        self.calls.borrow_mut().push(if write {
            "authority:write"
        } else {
            "authority:read"
        });
        Box::pin(async move { Ok(self.authority) })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> Fut<'a, String> {
        self.hashes.borrow_mut().push(value.to_owned());
        Box::pin(async { Ok("a".repeat(64)) })
    }

    fn receipt<'a>(&'a mut self, _: &'a str) -> Fut<'a, Option<ReceiptSnapshot>> {
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn output_product<'a>(&'a mut self, _: &'a BomActor, _: &'a str) -> Fut<'a, bool> {
        self.calls.borrow_mut().push("output");
        Box::pin(async move { Ok(self.output_active) })
    }

    fn material_product<'a>(&'a mut self, _: &'a BomActor, _: &'a str) -> Fut<'a, bool> {
        self.calls.borrow_mut().push("material");
        Box::pin(async move { Ok(self.materials_active) })
    }

    fn routing_resource<'a>(&'a mut self, _: &'a BomActor, _: &'a str) -> Fut<'a, bool> {
        self.calls.borrow_mut().push("resource");
        Box::pin(async move { Ok(self.resource_active) })
    }

    fn specification<'a>(
        &'a mut self,
        _: &'a BomActor,
        _: &'a str,
    ) -> Fut<'a, Option<SpecificationRow>> {
        self.calls.borrow_mut().push("specification");
        let row = self.row.clone();
        Box::pin(async move { Ok(row) })
    }

    fn commit_create<'a>(&'a mut self, plan: CreatePlan) -> Fut<'a, serde_json::Value> {
        self.calls.borrow_mut().push("commit-create");
        let version = self.result_version;
        self.created.borrow_mut().push(plan.clone());
        Box::pin(async move {
            Ok(serde_json::json!({
                "id": plan.id,
                "status": "draft",
                "version": version
            }))
        })
    }

    fn commit_review<'a>(&'a mut self, plan: ReviewPlan) -> Fut<'a, serde_json::Value> {
        self.calls.borrow_mut().push("commit-review");
        let version = self.result_version;
        self.reviewed.borrow_mut().push(plan.clone());
        Box::pin(async move {
            Ok(serde_json::json!({
                "id": plan.specification.id,
                "status": plan.status,
                "version": version
            }))
        })
    }
}

#[test]
fn create_validation_preserves_the_retained_specification_contract() {
    let input = request().validate().unwrap();
    assert_eq!(input.name, "Bounded assembly");
    assert_eq!(input.mode, "bom");
    assert_eq!(input.output_product_id, "output");
    assert_eq!(input.batch_quantity_milli, 10_000);
    assert_eq!(input.expected_yield_bps, 9_800);
    assert_eq!(input.lines.len(), 2);
    assert_eq!(input.lines[0].quantity_milli, 1_500);
    assert_eq!(
        input.lines[0].substitution_group.as_deref(),
        Some("secondary")
    );
    assert_eq!(input.lines[1].quantity_milli, 2_000);
    assert_eq!(input.operations[0].setup_minutes, 5);
    assert_eq!(input.operations[0].run_minutes_per_batch, 30);
}

#[test]
fn quantities_parse_exactly_with_at_most_three_decimals() {
    assert_eq!(parse_quantity_milli("0"), Some(0));
    assert_eq!(parse_quantity_milli("1"), Some(1_000));
    assert_eq!(parse_quantity_milli("1.5"), Some(1_500));
    assert_eq!(parse_quantity_milli("1.05"), Some(1_050));
    assert_eq!(parse_quantity_milli("1.005"), Some(1_005));
    assert_eq!(parse_quantity_milli("0.000"), Some(0));
    for invalid in ["", ".5", "1.", "1.0001", "-1", "+1", "1,5", " 1", "1 "] {
        assert_eq!(parse_quantity_milli(invalid), None, "{invalid}");
    }
}

#[test]
fn create_rejects_material_duplicates_and_the_output_product() {
    let duplicate = serde_json::json!({
        "name": "Duplicate assembly",
        "mode": "bom",
        "outputProductId": "output",
        "batchQuantity": "1",
        "expectedYieldBps": 9800,
        "lines": [
            {"productId": "material", "quantity": "1"},
            {"productId": "material", "quantity": "2"}
        ]
    });
    let error = serde_json::from_value::<CreateRequest>(duplicate)
        .unwrap()
        .validate()
        .unwrap_err();
    assert_eq!(error.code, "INVALID_SPECIFICATION_LINES");

    let self_output = serde_json::json!({
        "name": "Self material",
        "mode": "bom",
        "outputProductId": "output",
        "batchQuantity": "1",
        "expectedYieldBps": 9800,
        "lines": [{"productId": "output", "quantity": "1"}]
    });
    let error = serde_json::from_value::<CreateRequest>(self_output)
        .unwrap()
        .validate()
        .unwrap_err();
    assert_eq!(error.code, "INVALID_SPECIFICATION_LINES");
}

#[test]
fn create_rejects_out_of_contract_values() {
    for patch in [
        serde_json::json!({"name": " x "}),
        serde_json::json!({"mode": "assembly"}),
        serde_json::json!({"batchQuantity": "0"}),
        serde_json::json!({"batchQuantity": "1.0001"}),
        serde_json::json!({"expectedYieldBps": 0}),
        serde_json::json!({"expectedYieldBps": 20_001}),
        serde_json::json!({"lines": []}),
        serde_json::json!({"lines": [
            {"productId": "material", "quantity": "1", "substitutionGroup": " padded"}
        ]}),
        serde_json::json!({"operations": [
            {"resourceId": "resource", "name": "Assemble", "setupMinutes": -1, "runMinutesPerBatch": 1}
        ]}),
        serde_json::json!({"operations": [
            {"resourceId": "resource", "name": "Assemble", "setupMinutes": 1, "runMinutesPerBatch": 0}
        ]}),
    ] {
        let mut body = serde_json::to_value(request()).unwrap();
        for (name, value) in patch.as_object().unwrap() {
            body[name.as_str()] = value.clone();
        }
        let error = serde_json::from_value::<CreateRequest>(body)
            .unwrap()
            .validate()
            .unwrap_err();
        assert_eq!(error.code, "MANUFACTURING_REQUEST_INVALID");
    }

    // Unknown retained transport fields never reach a native write.
    let mut body = serde_json::to_value(request()).unwrap();
    body["entity"] = serde_json::json!("specification");
    assert!(serde_json::from_value::<CreateRequest>(body).is_err());
}

#[test]
fn write_and_review_policy_require_the_current_manufacturing_role() {
    let mut denied = actor();
    denied.permissions.retain(|value| value != "records.write");
    assert_eq!(
        denied.require_write(NOW).unwrap_err().code,
        "MANUFACTURING_PERMISSION_REQUIRED"
    );

    let mut no_module = actor();
    no_module.modules.clear();
    assert_eq!(
        no_module.require_write(NOW).unwrap_err().code,
        "MANUFACTURING_MODULE_FORBIDDEN"
    );

    let expired = BomActor {
        expires_at: NOW.get(),
        ..actor()
    };
    assert_eq!(
        expired.require_write(NOW).unwrap_err().code,
        "WORK_CONTEXT_CHANGED"
    );

    assert_eq!(
        actor().require_review(NOW).unwrap_err().code,
        "MANUFACTURING_REVIEW_REQUIRED"
    );
    assert!(reviewer().require_review(NOW).is_ok());
}

#[tokio::test]
async fn create_commits_one_ordered_draft_after_current_source_guards() {
    let mut repository = good();
    let value = create_specification(
        &mut repository,
        &actor(),
        &key("manufacturing-key-0001"),
        request(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(value["id"], "mfg-spec-manufacturing-key-0001");
    assert_eq!(value["status"], "draft");
    assert_eq!(value["version"], 1);

    let calls = repository.calls.borrow().clone();
    assert_eq!(
        calls,
        vec![
            "authority:write",
            "output",
            "material",
            "material",
            "resource",
            "commit-create"
        ]
    );
    let plans = repository.created.borrow();
    assert_eq!(plans.len(), 1);
    let plan = &plans[0];
    assert_eq!(plan.id, "mfg-spec-manufacturing-key-0001");
    assert_eq!(plan.actor.user_id, "maker");
    assert_eq!(plan.lines.len(), 2);
    assert_eq!(plan.lines[0].id, "mfg-spec-line-manufacturing-key-0001-0");
    assert_eq!(plan.lines[1].ordinal, 1);
    assert_eq!(
        plan.operations[0].id,
        "mfg-operation-manufacturing-key-0001-0"
    );
    assert_eq!(plan.operations[0].ordinal, 0);
    assert_eq!(plan.now, NOW.get());
    assert_eq!(plan.request_hash, "a".repeat(64));

    // The identity digest is computed from the canonical command, not the
    // already-validated row only.
    let identity = repository.hashes.borrow()[0].clone();
    assert!(identity.contains(CREATE_OPERATION));
    assert!(identity.contains("manufacturing-key-0001"));
    assert!(identity.contains("\"input\""));
}

#[tokio::test]
async fn create_is_idempotent_and_rejects_a_reused_key() {
    let mut repository = good();
    let first = create_specification(
        &mut repository,
        &actor(),
        &key("manufacturing-key-0001"),
        request(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(first["version"], 1);

    let prior = serde_json::json!({"id": "mfg-spec-manufacturing-key-0001", "status": "draft", "version": 1});
    repository.receipt = Some(ReceiptSnapshot {
        request_hash: "a".repeat(64),
        result_json: prior.to_string(),
        operation: CREATE_OPERATION.into(),
        actor_id: "maker".into(),
        record_id: "mfg-spec-manufacturing-key-0001".into(),
        authority_json: actor().authority_json(),
    });
    let replay = create_specification(
        &mut repository,
        &actor(),
        &key("manufacturing-key-0001"),
        request(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(replay, prior);
    assert_eq!(repository.created.borrow().len(), 1);

    repository.receipt.as_mut().unwrap().request_hash = "b".repeat(64);
    let error = create_specification(
        &mut repository,
        &actor(),
        &key("manufacturing-key-0001"),
        request(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MANUFACTURING_IDEMPOTENCY_CONFLICT");

    repository.receipt.as_mut().unwrap().request_hash = "a".repeat(64);
    let mut other_branch = actor();
    other_branch.branch_id = None;
    repository.receipt.as_mut().unwrap().authority_json = other_branch.authority_json();
    let error = create_specification(
        &mut repository,
        &actor(),
        &key("manufacturing-key-0001"),
        request(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MANUFACTURING_IDEMPOTENCY_CONFLICT");
}

#[tokio::test]
async fn create_fails_closed_on_inactive_owner_sources_and_authority() {
    for (output, materials, resource) in [
        (false, true, true),
        (true, false, true),
        (true, true, false),
    ] {
        let mut repository = good();
        repository.output_active = output;
        repository.materials_active = materials;
        repository.resource_active = resource;
        let error = create_specification(
            &mut repository,
            &actor(),
            &key("manufacturing-key-0002"),
            request(),
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "MANUFACTURING_SOURCE_CHANGED");
        assert!(repository.created.borrow().is_empty());
    }

    let mut repository = good();
    repository.authority = false;
    let error = create_specification(
        &mut repository,
        &actor(),
        &key("manufacturing-key-0003"),
        request(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MANUFACTURING_ACCESS_CHANGED");
    assert!(repository.created.borrow().is_empty());
}

#[tokio::test]
async fn independent_review_approves_and_retirement_is_allowed_for_the_creator() {
    let mut repository = good();
    repository.row = Some(stored("draft", 1, "maker"));
    let approved = review_specification(
        &mut repository,
        &reviewer(),
        &key("manufacturing-review-0001"),
        "spec",
        review(ReviewAction::Approve, 1),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(approved["status"], "approved");
    let plans = repository.reviewed.borrow().clone();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].status, "approved");
    assert_eq!(plans[0].expected_version, 1);
    assert_eq!(plans[0].specification.id, "spec");

    let mut repository = good();
    repository.row = Some(stored("approved", 2, "maker"));
    let retired = review_specification(
        &mut repository,
        &creator_reviewer(),
        &key("manufacturing-retire-0001"),
        "spec",
        review(ReviewAction::Retire, 2),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(retired["status"], "retired");
    assert_eq!(repository.reviewed.borrow()[0].status, "retired");
}

#[tokio::test]
async fn review_rejects_self_approval_stale_versions_and_wrong_states() {
    let cases = [
        (
            reviewer(),
            stored("draft", 1, "reviewer"),
            ReviewAction::Approve,
            1,
        ),
        (
            reviewer(),
            stored("draft", 1, "maker"),
            ReviewAction::Approve,
            2,
        ),
        (
            reviewer(),
            stored("approved", 2, "maker"),
            ReviewAction::Approve,
            2,
        ),
        (
            creator_reviewer(),
            stored("draft", 1, "maker"),
            ReviewAction::Retire,
            1,
        ),
        (
            creator_reviewer(),
            stored("rejected", 2, "maker"),
            ReviewAction::Retire,
            2,
        ),
        (
            reviewer(),
            stored("approved", 2, "maker"),
            ReviewAction::Reject,
            2,
        ),
    ];
    for (reviewer_actor, row, action, expected_version) in cases {
        let mut repository = good();
        repository.row = Some(row);
        let error = review_specification(
            &mut repository,
            &reviewer_actor,
            &key("manufacturing-review-0002"),
            "spec",
            review(action, expected_version),
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "CONFLICT", "{action:?}");
        assert!(repository.reviewed.borrow().is_empty());
    }
}

#[tokio::test]
async fn review_requires_a_current_specification_and_review_permission() {
    let mut repository = good();
    repository.row = None;
    let error = review_specification(
        &mut repository,
        &reviewer(),
        &key("manufacturing-review-0003"),
        "spec",
        review(ReviewAction::Approve, 1),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SPECIFICATION_NOT_FOUND");

    let error = review_specification(
        &mut repository,
        &actor(),
        &key("manufacturing-review-0004"),
        "spec",
        review(ReviewAction::Approve, 1),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MANUFACTURING_REVIEW_REQUIRED");
    assert!(repository.reviewed.borrow().is_empty());
}
