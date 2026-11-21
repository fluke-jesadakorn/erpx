//! Native Manufacturing BOM register domain/application tests.
use erpx_server_manufacturing::public::bom::{
    BomActor, BomLine, BomOperation, BomReadFuture, BomReadRepository, BomSpecification,
    MAX_LINES_PER_SPECIFICATION, MAX_OPERATIONS_PER_SPECIFICATION, MAX_SPECIFICATIONS,
    bom_register, validate_register,
};
use erpx_server_runtime::public::http_support::serde_json;
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> BomActor {
    BomActor {
        user_id: "user".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: vec!["records.read".into()],
        modules: vec!["manufacturing".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn line(id: &str, product: &str, ordinal: i64) -> BomLine {
    BomLine {
        id: id.into(),
        specification_id: "spec".into(),
        product_id: product.into(),
        quantity_milli: 1_500,
        substitution_group: None,
        ordinal,
    }
}

fn operation(id: &str, ordinal: i64) -> BomOperation {
    BomOperation {
        id: id.into(),
        specification_id: "spec".into(),
        resource_id: "resource".into(),
        resource_name: "Assembly line".into(),
        name: "Assemble".into(),
        setup_minutes: 5,
        run_minutes_per_batch: 30,
        ordinal,
    }
}

fn specification() -> BomSpecification {
    BomSpecification {
        id: "spec".into(),
        company_id: "company".into(),
        name: "Bounded assembly".into(),
        mode: "bom".into(),
        output_product_id: "output".into(),
        batch_quantity_milli: 10_000,
        expected_yield_bps: 9_800,
        status: "draft".into(),
        created_by: "maker".into(),
        reviewed_by: None,
        version: 1,
        created_at: 100,
        updated_at: 100,
        lines: vec![line("line-a", "material-a", 0)],
        operations: vec![operation("operation-a", 0)],
    }
}

fn approved() -> BomSpecification {
    BomSpecification {
        status: "approved".into(),
        reviewed_by: Some("reviewer".into()),
        version: 2,
        updated_at: 200,
        ..specification()
    }
}

struct Fake {
    rows: Vec<BomSpecification>,
    error: Option<BusinessError>,
    calls: Arc<AtomicUsize>,
}

impl BomReadRepository for Fake {
    type Error = BusinessError;

    fn specifications<'a>(
        &'a mut self,
        _: &'a CompanyId,
    ) -> BomReadFuture<'a, Vec<BomSpecification>, Self::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if let Some(error) = self.error.clone() {
                return Err(error);
            }
            Ok(self.rows.clone())
        })
    }
}

fn fake(rows: Vec<BomSpecification>) -> (Fake, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (
        Fake {
            rows,
            error: None,
            calls: calls.clone(),
        },
        calls,
    )
}

#[tokio::test]
async fn valid_register_reads_owner_rows_and_serializes_exact_fields() {
    let (mut repository, calls) = fake(vec![approved()]);
    let register = bom_register(&actor(), &mut repository, NOW).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(register.specifications.len(), 1);
    let value = serde_json::to_value(&register).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "specifications": [{
                "id": "spec",
                "company_id": "company",
                "name": "Bounded assembly",
                "mode": "bom",
                "output_product_id": "output",
                "batch_quantity_milli": 10000,
                "expected_yield_bps": 9800,
                "status": "approved",
                "created_by": "maker",
                "reviewed_by": "reviewer",
                "version": 2,
                "created_at": 100,
                "updated_at": 200,
                "lines": [{
                    "id": "line-a",
                    "specification_id": "spec",
                    "product_id": "material-a",
                    "quantity_milli": 1500,
                    "substitution_group": null,
                    "ordinal": 0
                }],
                "operations": [{
                    "id": "operation-a",
                    "specification_id": "spec",
                    "resource_id": "resource",
                    "resource_name": "Assembly line",
                    "name": "Assemble",
                    "setup_minutes": 5,
                    "run_minutes_per_batch": 30,
                    "ordinal": 0
                }]
            }]
        })
    );
}

#[tokio::test]
async fn policy_failures_never_read_the_owner_source() {
    let (mut repository, calls) = fake(vec![specification()]);

    let mut missing_permission = actor();
    missing_permission.permissions.clear();
    assert_eq!(
        bom_register(&missing_permission, &mut repository, NOW)
            .await
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_PERMISSION_REQUIRED"
    );

    let mut missing_module = actor();
    missing_module.modules.clear();
    assert_eq!(
        bom_register(&missing_module, &mut repository, NOW)
            .await
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_MODULE_FORBIDDEN"
    );

    let mut expired = actor();
    expired.expires_at = NOW.get();
    let error = bom_register(&expired, &mut repository, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");

    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn repository_failures_propagate_without_a_register() {
    let (mut repository, _) = fake(Vec::new());
    repository.error = Some(BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_BOM_SOURCE_INVALID",
        "Synthetic source failure.",
    ));
    let error = bom_register(&actor(), &mut repository, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "MANUFACTURING_BOM_SOURCE_INVALID");
}

#[test]
fn empty_register_is_valid() {
    assert!(
        validate_register(&actor(), Vec::new())
            .unwrap()
            .specifications
            .is_empty()
    );
}

#[test]
fn foreign_company_rows_are_rejected() {
    let mut foreign = specification();
    foreign.company_id = "foreign".into();
    assert_eq!(
        validate_register(&actor(), vec![foreign]).unwrap_err().code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );
}

#[test]
fn review_state_must_match_the_independent_review_contract() {
    let mut draft_with_reviewer = specification();
    draft_with_reviewer.reviewed_by = Some("reviewer".into());
    assert_eq!(
        validate_register(&actor(), vec![draft_with_reviewer])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut approved_without_reviewer = approved();
    approved_without_reviewer.reviewed_by = None;
    assert_eq!(
        validate_register(&actor(), vec![approved_without_reviewer])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut self_reviewed = approved();
    self_reviewed.reviewed_by = Some("maker".into());
    assert_eq!(
        validate_register(&actor(), vec![self_reviewed])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut rejected = approved();
    rejected.status = "rejected".into();
    assert!(
        validate_register(&actor(), vec![rejected])
            .unwrap()
            .specifications
            .len()
            == 1
    );
}

#[test]
fn duplicate_and_orphan_facts_are_rejected() {
    let mut duplicate_spec = specification();
    duplicate_spec.lines.push(line("line-b", "material-b", 1));
    assert_eq!(
        validate_register(&actor(), vec![duplicate_spec.clone(), duplicate_spec])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut duplicate_line_id = specification();
    duplicate_line_id
        .lines
        .push(line("line-a", "material-b", 1));
    assert_eq!(
        validate_register(&actor(), vec![duplicate_line_id])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut duplicate_product = specification();
    duplicate_product
        .lines
        .push(line("line-b", "material-a", 1));
    assert_eq!(
        validate_register(&actor(), vec![duplicate_product])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut duplicate_ordinal = specification();
    duplicate_ordinal
        .lines
        .push(line("line-b", "material-b", 0));
    assert_eq!(
        validate_register(&actor(), vec![duplicate_ordinal])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut material_is_output = specification();
    material_is_output.lines[0].product_id = "output".into();
    assert_eq!(
        validate_register(&actor(), vec![material_is_output])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut orphan_line = specification();
    orphan_line.lines[0].specification_id = "other".into();
    assert_eq!(
        validate_register(&actor(), vec![orphan_line])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut orphan_operation = specification();
    orphan_operation.operations[0].specification_id = "other".into();
    assert_eq!(
        validate_register(&actor(), vec![orphan_operation])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    let mut duplicate_operation = specification();
    duplicate_operation
        .operations
        .push(operation("operation-a", 1));
    assert_eq!(
        validate_register(&actor(), vec![duplicate_operation])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );
}

#[test]
fn malformed_scalar_facts_are_rejected() {
    for mutate in [
        |spec: &mut BomSpecification| spec.id = String::new(),
        |spec: &mut BomSpecification| spec.name = "x".into(),
        |spec: &mut BomSpecification| spec.name = " padded ".into(),
        |spec: &mut BomSpecification| spec.mode = "assembly".into(),
        |spec: &mut BomSpecification| spec.output_product_id = String::new(),
        |spec: &mut BomSpecification| spec.batch_quantity_milli = 0,
        |spec: &mut BomSpecification| spec.expected_yield_bps = 0,
        |spec: &mut BomSpecification| spec.expected_yield_bps = 20_001,
        |spec: &mut BomSpecification| spec.status = "archived".into(),
        |spec: &mut BomSpecification| spec.created_by = String::new(),
        |spec: &mut BomSpecification| spec.version = 0,
        |spec: &mut BomSpecification| spec.created_at = -1,
        |spec: &mut BomSpecification| spec.updated_at = 99,
        |spec: &mut BomSpecification| spec.updated_at = 9_007_199_254_740_992,
        |spec: &mut BomSpecification| spec.lines.clear(),
        |spec: &mut BomSpecification| spec.lines[0].quantity_milli = 0,
        |spec: &mut BomSpecification| spec.lines[0].ordinal = -1,
        |spec: &mut BomSpecification| spec.lines[0].substitution_group = Some(String::new()),
        |spec: &mut BomSpecification| spec.operations[0].resource_id = String::new(),
        |spec: &mut BomSpecification| spec.operations[0].resource_name = " ".into(),
        |spec: &mut BomSpecification| spec.operations[0].name = "x".into(),
        |spec: &mut BomSpecification| spec.operations[0].setup_minutes = -1,
        |spec: &mut BomSpecification| spec.operations[0].run_minutes_per_batch = 0,
        |spec: &mut BomSpecification| spec.operations[0].ordinal = -1,
    ] {
        let mut malformed = specification();
        mutate(&mut malformed);
        assert_eq!(
            validate_register(&actor(), vec![malformed])
                .unwrap_err()
                .code,
            "MANUFACTURING_BOM_SOURCE_INVALID"
        );
    }
}

#[test]
fn bounded_register_limits_are_enforced() {
    let mut too_many_specifications = Vec::new();
    for index in 0..=MAX_SPECIFICATIONS {
        let mut row = specification();
        row.id = format!("spec-{index}");
        row.lines[0].specification_id = row.id.clone();
        row.operations[0].specification_id = row.id.clone();
        too_many_specifications.push(row);
    }
    assert_eq!(
        validate_register(&actor(), too_many_specifications)
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_LIMIT"
    );

    let mut too_many_lines = specification();
    for index in 1..=MAX_LINES_PER_SPECIFICATION {
        too_many_lines.lines.push(line(
            &format!("line-{index}"),
            &format!("material-{index}"),
            index as i64,
        ));
    }
    assert_eq!(
        validate_register(&actor(), vec![too_many_lines])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_LIMIT"
    );

    let mut too_many_operations = specification();
    for index in 1..=MAX_OPERATIONS_PER_SPECIFICATION {
        too_many_operations
            .operations
            .push(operation(&format!("operation-{index}"), index as i64));
    }
    assert_eq!(
        validate_register(&actor(), vec![too_many_operations])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_LIMIT"
    );
}

#[test]
fn text_limits_use_chars_min_and_bytes_max_with_js_trim_edges() {
    // 40 non-BMP characters are 40 chars (>=2) and 160 bytes (== max),
    // so this passes the dual metric.
    let mut name_at_limit = specification();
    name_at_limit.name = "😀".repeat(40);
    assert!(validate_register(&actor(), vec![name_at_limit]).is_ok());

    // 41 emoji are 164 bytes, exceeding the 160-byte cap.
    let mut name_over_bytes = specification();
    name_over_bytes.name = "😀".repeat(41);
    assert_eq!(
        validate_register(&actor(), vec![name_over_bytes])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    // 81 emoji also exceed the 160-byte cap.
    let mut name_over_limit = specification();
    name_over_limit.name = "😀".repeat(81);
    assert_eq!(
        validate_register(&actor(), vec![name_over_limit])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    // U+FEFF is trimmed by ECMAScript trim but not by Rust trim; a value with
    // a leading zero-width no-break space must fail the edge check.
    let mut bom_prefix = specification();
    bom_prefix.name = "\u{feff}Bounded assembly".to_owned();
    assert_eq!(
        validate_register(&actor(), vec![bom_prefix])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    // A lone U+FEFF substitution group is trimmed to empty by ECMAScript
    // trim, so the minimum-length/edge checks must reject it.
    let mut feff_group = specification();
    feff_group.lines[0].substitution_group = Some("\u{feff}".to_owned());
    assert_eq!(
        validate_register(&actor(), vec![feff_group])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    // U+00A0 trailing whitespace still fails the JS edge check.
    let mut nbsp_trailing = specification();
    nbsp_trailing.name = "Bounded assembly\u{00a0}".to_owned();
    assert_eq!(
        validate_register(&actor(), vec![nbsp_trailing])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );

    // Interior C0 control fails closed.
    let mut control = specification();
    control.name = "Bounded\u{0007}assembly".to_owned();
    assert_eq!(
        validate_register(&actor(), vec![control])
            .unwrap_err()
            .code,
        "MANUFACTURING_BOM_SOURCE_INVALID"
    );
}
