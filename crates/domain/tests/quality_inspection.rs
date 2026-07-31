use chrono::{DateTime, Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::quality::{
    MAX_QUALITY_TEXT_BYTES, QualityError, QualityInspection, QualityInspectionKind,
    QualityInspectionStatus, deterministic_idempotency_key, deterministic_reference,
};
use uuid::Uuid;

const HOLDING_ID: Uuid = Uuid::from_u128(10);
const COMPANY_ID: Uuid = Uuid::from_u128(20);
const BRANCH_ID: Uuid = Uuid::from_u128(30);
const SIBLING_BRANCH_ID: Uuid = Uuid::from_u128(31);
const PRODUCT_ID: Uuid = Uuid::from_u128(40);

fn company_scope(company_id: Uuid) -> TenantScope {
    TenantScope::Company {
        holding_id: HOLDING_ID,
        company_id,
    }
}

fn branch_scope(company_id: Uuid, branch_id: Uuid) -> TenantScope {
    TenantScope::Branch {
        holding_id: HOLDING_ID,
        company_id,
        branch_id,
    }
}

fn context(key: &str, scope: TenantScope) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "quality-integration-test".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "exercise quality inspection domain contract",
    )
    .expect("valid command context")
}

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-07-28T00:00:00Z")
        .expect("fixture timestamp")
        .with_timezone(&Utc)
        + Duration::seconds(seconds)
}

fn inspection(
    id: Uuid,
    scope: TenantScope,
    branch_id: Option<Uuid>,
    command: &CommandContext,
) -> Result<QualityInspection, QualityError> {
    QualityInspection::new(
        id,
        scope,
        HOLDING_ID,
        COMPANY_ID,
        branch_id,
        QualityInspectionKind::Final,
        PRODUCT_ID,
        Some("LOT-2026-07".into()),
        10,
        at(0),
        command,
    )
}

fn valid_inspection() -> QualityInspection {
    let scope = company_scope(COMPANY_ID);
    inspection(
        Uuid::from_u128(100),
        scope.clone(),
        None,
        &context("create-quality-100", scope),
    )
    .expect("valid inspection")
}

#[test]
fn scope_binding_and_command_containment_are_fail_closed() {
    let company = company_scope(COMPANY_ID);
    let branch = branch_scope(COMPANY_ID, BRANCH_ID);

    let company_record = inspection(
        Uuid::from_u128(101),
        company.clone(),
        None,
        &context("create-company-quality", company.clone()),
    )
    .expect("company-scoped inspection");
    assert_eq!(company_record.scope, company);

    let branch_record = inspection(
        Uuid::from_u128(102),
        branch.clone(),
        Some(BRANCH_ID),
        &context("create-branch-quality", company.clone()),
    )
    .expect("company authority contains its branch");
    assert_eq!(branch_record.branch_id, Some(BRANCH_ID));

    assert!(matches!(
        inspection(
            Uuid::from_u128(103),
            company_scope(COMPANY_ID),
            None,
            &context("create-company-from-branch", branch.clone()),
        ),
        Err(QualityError::CommandScopeOutside { .. })
    ));
    assert!(matches!(
        inspection(
            Uuid::from_u128(104),
            branch_scope(COMPANY_ID, BRANCH_ID),
            Some(BRANCH_ID),
            &context(
                "create-sibling-quality",
                branch_scope(COMPANY_ID, SIBLING_BRANCH_ID),
            ),
        ),
        Err(QualityError::CommandScopeOutside { .. })
    ));

    assert_eq!(
        inspection(
            Uuid::from_u128(105),
            company_scope(COMPANY_ID),
            Some(BRANCH_ID),
            &context("invalid-company-binding", company_scope(COMPANY_ID)),
        ),
        Err(QualityError::InvalidScope)
    );
    assert_eq!(
        inspection(
            Uuid::from_u128(106),
            branch_scope(COMPANY_ID, BRANCH_ID),
            Some(SIBLING_BRANCH_ID),
            &context("invalid-branch-binding", company.clone()),
        ),
        Err(QualityError::InvalidScope)
    );
    assert_eq!(
        inspection(
            Uuid::from_u128(107),
            TenantScope::Platform,
            None,
            &context("platform-record-is-invalid", TenantScope::Platform),
        ),
        Err(QualityError::InvalidScope)
    );
}

#[test]
fn cancellation_respects_sibling_and_parent_company_scope_boundaries() {
    let branch = branch_scope(COMPANY_ID, BRANCH_ID);
    let sibling = branch_scope(COMPANY_ID, SIBLING_BRANCH_ID);
    let company = company_scope(COMPANY_ID);
    let mut value = inspection(
        Uuid::from_u128(112),
        branch.clone(),
        Some(BRANCH_ID),
        &context("create-branch-cancellable-quality", branch),
    )
    .expect("branch inspection");

    let before_sibling_cancellation = value.clone();
    assert!(matches!(
        value.cancel(at(1), &context("cancel-from-sibling-branch", sibling),),
        Err(QualityError::CommandScopeOutside { .. })
    ));
    assert_eq!(value, before_sibling_cancellation);

    value
        .cancel(at(2), &context("cancel-from-parent-company", company))
        .expect("parent company can cancel branch inspection");
    assert_eq!(value.status, QualityInspectionStatus::Cancelled);
}

#[test]
fn lifecycle_is_ordered_and_terminal_states_remain_closed() {
    let scope = company_scope(COMPANY_ID);
    let mut value = valid_inspection();

    assert_eq!(
        value.release(at(1), &context("release-before-result", scope.clone())),
        Err(QualityError::InspectionNotPassed)
    );
    assert_eq!(value.status, QualityInspectionStatus::Draft);
    assert_eq!(
        value.record_result(
            10,
            0,
            None,
            at(2),
            &context("result-before-start", scope.clone()),
        ),
        Err(QualityError::ResultNotRecordable)
    );
    assert_eq!(value.status, QualityInspectionStatus::Draft);

    value
        .start(at(3), &context("start-quality-100", scope.clone()))
        .expect("start inspection");
    assert_eq!(value.status, QualityInspectionStatus::InProgress);

    assert_eq!(
        value.record_result(
            9,
            0,
            None,
            at(4),
            &context("partial-quality-result", scope.clone()),
        ),
        Err(QualityError::ResultQuantityMismatch)
    );
    assert_eq!(value.status, QualityInspectionStatus::InProgress);
    assert_eq!(value.passed_quantity, 0);
    assert_eq!(value.failed_quantity, 0);

    value
        .record_result(
            8,
            2,
            Some("two units failed final inspection".into()),
            at(5),
            &context("failed-quality-result", scope.clone()),
        )
        .expect("record failed result");
    assert_eq!(value.status, QualityInspectionStatus::Failed);
    assert_eq!(value.passed_quantity, 8);
    assert_eq!(value.failed_quantity, 2);
    assert_eq!(
        value.release(at(6), &context("release-failed-quality", scope.clone())),
        Err(QualityError::InspectionNotPassed)
    );

    value
        .start(at(7), &context("retry-quality-100", scope.clone()))
        .expect("retry failed inspection");
    value
        .record_result(
            10,
            0,
            Some("recheck passed".into()),
            at(8),
            &context("passed-quality-result", scope.clone()),
        )
        .expect("record passed result");
    value
        .release(at(9), &context("release-quality-100", scope.clone()))
        .expect("release passed inspection");
    assert_eq!(value.status, QualityInspectionStatus::Released);

    let released_before_cancel = value.clone();
    assert!(matches!(
        value.cancel(at(10), &context("cancel-released-quality", scope.clone())),
        Err(QualityError::InvalidTransition {
            aggregate_type: "quality_inspection",
            ..
        })
    ));
    assert_eq!(value, released_before_cancel);

    let mut cancelled = QualityInspection::new(
        Uuid::from_u128(108),
        scope.clone(),
        HOLDING_ID,
        COMPANY_ID,
        None,
        QualityInspectionKind::Incoming,
        PRODUCT_ID,
        None,
        1,
        at(0),
        &context("create-cancellable-quality", scope.clone()),
    )
    .expect("cancellable inspection");
    cancelled
        .cancel(at(1), &context("cancel-quality", scope.clone()))
        .expect("cancel draft inspection");
    assert_eq!(cancelled.status, QualityInspectionStatus::Cancelled);
    let cancelled_before_restart = cancelled.clone();
    assert!(matches!(
        cancelled.start(at(2), &context("restart-cancelled-quality", scope)),
        Err(QualityError::InvalidTransition { .. })
    ));
    assert_eq!(cancelled, cancelled_before_restart);
}

#[test]
fn identity_is_deterministic_for_replays_and_distinguishes_commands() {
    let scope = company_scope(COMPANY_ID);
    let inspection_id = Uuid::from_u128(109);
    let original_context = context("  receive-quality-109  ", scope.clone());
    let mut replay_context = context("receive-quality-109", scope.clone());
    replay_context.command_id = Uuid::from_u128(110);

    let original = inspection(inspection_id, scope.clone(), None, &original_context)
        .expect("original inspection");
    let replay = inspection(inspection_id, scope.clone(), None, &replay_context)
        .expect("replayed inspection identity");

    assert_eq!(deterministic_reference(inspection_id), original.reference);
    assert_eq!(
        deterministic_idempotency_key(&original_context).expect("idempotency key"),
        "quality/inspection/receive-quality-109"
    );
    assert_eq!(original.reference, replay.reference);
    assert_eq!(original.idempotency_key, replay.idempotency_key);
    assert_ne!(original.command_id, replay.command_id);
    assert_ne!(
        original.reference,
        deterministic_reference(Uuid::from_u128(111))
    );

    let changed_key = context("receive-quality-109-retry-with-new-key", scope.clone());
    let changed_key_value =
        inspection(inspection_id, scope, None, &changed_key).expect("different command identity");
    assert_ne!(original.idempotency_key, changed_key_value.idempotency_key);
    original.validate().expect("original identity validates");
    replay.validate().expect("replay identity validates");
}

#[test]
fn rejected_public_commands_leave_the_whole_aggregate_unchanged() {
    let scope = company_scope(COMPANY_ID);
    let mut value = valid_inspection();
    value
        .start(at(1), &context("start-for-rollback", scope.clone()))
        .expect("start inspection");

    let before_scope_failure = value.clone();
    assert!(matches!(
        value.record_result(
            10,
            0,
            None,
            at(2),
            &context("outside-scope-result", company_scope(Uuid::from_u128(999))),
        ),
        Err(QualityError::CommandScopeOutside { .. })
    ));
    assert_eq!(value, before_scope_failure);

    let before_quantity_failure = value.clone();
    assert_eq!(
        value.record_result(
            9,
            0,
            None,
            at(3),
            &context("incomplete-result", scope.clone()),
        ),
        Err(QualityError::ResultQuantityMismatch)
    );
    assert_eq!(value, before_quantity_failure);

    let before_text_failure = value.clone();
    assert_eq!(
        value.record_result(
            10,
            0,
            Some("x".repeat(MAX_QUALITY_TEXT_BYTES + 1)),
            at(4),
            &context("oversized-result-note", scope.clone()),
        ),
        Err(QualityError::TooLong {
            field: "disposition_note",
            max: MAX_QUALITY_TEXT_BYTES,
        })
    );
    assert_eq!(value, before_text_failure);

    let before_transition_failure = value.clone();
    assert_eq!(
        value.release(at(5), &context("release-before-valid-result", scope)),
        Err(QualityError::InspectionNotPassed)
    );
    assert_eq!(value, before_transition_failure);
}

#[test]
fn candidate_commit_preserves_identity_and_terminal_authorization_boundaries() {
    let scope = company_scope(COMPANY_ID);
    let outsider_scope = company_scope(Uuid::from_u128(999));
    let mut value = valid_inspection();
    let identity = (
        value.id,
        value.reference.clone(),
        value.idempotency_key.clone(),
    );

    assert_eq!(value.reference, deterministic_reference(value.id));
    assert_eq!(
        value.idempotency_key,
        deterministic_idempotency_key(&context("create-quality-100", scope.clone()))
            .expect("deterministic idempotency key")
    );

    value
        .start(at(1), &context("start-candidate-regression", scope.clone()))
        .expect("start inspection");
    let before_candidate_rejection = value.clone();
    assert_eq!(
        value.record_result(
            10,
            0,
            Some("x".repeat(MAX_QUALITY_TEXT_BYTES + 1)),
            at(2),
            &context("reject-candidate-quality-result", scope.clone()),
        ),
        Err(QualityError::TooLong {
            field: "disposition_note",
            max: MAX_QUALITY_TEXT_BYTES,
        })
    );
    assert_eq!(value, before_candidate_rejection);
    assert_eq!(
        (
            value.id,
            value.reference.clone(),
            value.idempotency_key.clone()
        ),
        identity
    );

    let before_unauthorized_result = value.clone();
    assert!(matches!(
        value.record_result(
            10,
            0,
            None,
            at(3),
            &context("outside-scope-quality-result", outsider_scope.clone()),
        ),
        Err(QualityError::CommandScopeOutside { .. })
    ));
    assert_eq!(value, before_unauthorized_result);

    value
        .record_result(
            10,
            0,
            Some("retry passed".into()),
            at(4),
            &context("commit-quality-result-retry", scope.clone()),
        )
        .expect("commit valid candidate");
    value
        .release(at(5), &context("release-quality-terminal", scope.clone()))
        .expect("release passed inspection");
    assert_eq!(
        (
            value.id,
            value.reference.clone(),
            value.idempotency_key.clone()
        ),
        identity
    );

    let released_before_rollback = value.clone();
    assert!(matches!(
        value.cancel(at(6), &context("rollback-released-quality", scope.clone())),
        Err(QualityError::InvalidTransition {
            aggregate_type: "quality_inspection",
            ..
        })
    ));
    assert_eq!(value, released_before_rollback);

    let released_before_unauthorized = value.clone();
    assert!(matches!(
        value.cancel(
            at(7),
            &context("outside-scope-released-quality", outsider_scope),
        ),
        Err(QualityError::CommandScopeOutside { .. })
    ));
    assert_eq!(value, released_before_unauthorized);
}

#[test]
fn version_overflow_rejects_transition_and_result_without_mutation() {
    let scope = company_scope(COMPANY_ID);
    let mut draft = valid_inspection();
    draft.version = u64::MAX;
    let before_draft = draft.clone();

    assert_eq!(
        draft.start(at(1), &context("overflow-start-quality", scope.clone()),),
        Err(QualityError::VersionOverflow)
    );
    assert_eq!(draft, before_draft);

    let mut in_progress = valid_inspection();
    in_progress
        .start(at(1), &context("overflow-result-start", scope.clone()))
        .expect("start inspection");
    in_progress.version = u64::MAX;
    let before_result = in_progress.clone();

    assert_eq!(
        in_progress.record_result(
            10,
            0,
            None,
            at(2),
            &context("overflow-result-quality", scope),
        ),
        Err(QualityError::VersionOverflow)
    );
    assert_eq!(in_progress, before_result);
}
