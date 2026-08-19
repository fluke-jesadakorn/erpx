use chrono::{DateTime, Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::pos::{Sale, SaleLine};
use erpx_domain::pos_refund::{
    CompletedPosSaleFact, CompletedPosSaleReference, PosRefund, PosRefundCommandResult,
    PosRefundError, PosRefundLineInput, PosRefundRequestInput, RefundCommandKind, RefundStatus,
    RefundableSaleLineFact,
};
use serde_json::to_string;
use uuid::Uuid;

fn scope(company: u128, branch: Option<u128>) -> AuthorityScope {
    let holding_id = Uuid::from_u128(10);
    let company_id = Uuid::from_u128(company);
    match branch {
        Some(branch_id) => AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id: Uuid::from_u128(branch_id),
        },
        None => AuthorityScope::Company {
            holding_id,
            company_id,
        },
    }
}

fn context(
    key: &str,
    principal: &str,
    command_scope: AuthorityScope,
    expected_version: Option<u64>,
) -> CommandContext {
    let actor = ActorAttribution {
        principal_ref: principal.to_owned(),
        kind: ActorKind::HoldingUser,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    };
    let mut context = CommandContext::try_new(key, actor, command_scope, "pos refund review")
        .expect("valid command context");
    context.expected_aggregate_version = expected_version;
    context
}

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .expect("fixture timestamp")
        .with_timezone(&Utc)
        + Duration::seconds(seconds)
}

fn input(requester_context: CommandContext) -> PosRefundRequestInput {
    let record_scope = scope(20, Some(30));
    let sale =
        CompletedPosSaleReference::new(Uuid::from_u128(100), "POS-SALE-100", record_scope.clone())
            .expect("valid sale reference");
    let fact = RefundableSaleLineFact::new(Uuid::from_u128(200), "POS-LINE-200", 3, 1_000)
        .expect("valid refundable fact");
    PosRefundRequestInput {
        scope: record_scope,
        sale: CompletedPosSaleFact::new(sale, "USD", vec![fact]).expect("valid sale facts"),
        lines: vec![
            PosRefundLineInput::new(Uuid::from_u128(200), "POS-LINE-200", 2, 600)
                .expect("valid refund line"),
        ],
        reason: "customer returned the item".to_owned(),
        context: requester_context,
        requested_at: at(0),
    }
}

fn refund() -> PosRefund {
    PosRefund::request(
        Uuid::from_u128(300),
        input(context("request-1", "requester", scope(20, None), None)),
    )
    .expect("valid refund request")
}

#[test]
fn lifecycle_is_closed_and_terminal_states_reject_further_commands() {
    let mut rejected = refund();
    rejected
        .reject(
            &context("reject-1", "reviewer", scope(20, None), Some(1)),
            at(1),
        )
        .expect("reject request");
    assert_eq!(rejected.status, RefundStatus::Rejected);
    assert!(rejected.status.is_terminal());
    assert!(matches!(
        rejected.approve(
            &context("approve-after-reject", "reviewer", scope(20, None), Some(2)),
            at(2),
        ),
        Err(PosRefundError::InvalidTransition {
            from: RefundStatus::Rejected,
            to: RefundStatus::Approved
        })
    ));

    let mut posted = refund();
    posted
        .approve(
            &context("approve-1", "reviewer", scope(20, None), Some(1)),
            at(1),
        )
        .expect("approve request");
    posted
        .post(
            &context("post-1", "finance-handoff", scope(20, None), Some(2)),
            at(2),
        )
        .expect("mark ready for handoff");
    assert_eq!(posted.status, RefundStatus::Posted);
    assert!(posted.status.is_terminal());
    assert!(matches!(
        posted.cancel(
            &context(
                "cancel-after-post",
                "finance-handoff",
                scope(20, None),
                Some(3)
            ),
            at(3),
        ),
        Err(PosRefundError::InvalidTransition {
            from: RefundStatus::Posted,
            to: RefundStatus::Cancelled
        })
    ));

    let mut cancelled = refund();
    cancelled
        .approve(
            &context("approve-for-cancel", "reviewer", scope(20, None), Some(1)),
            at(1),
        )
        .expect("approve request for cancellation");
    cancelled
        .cancel(
            &context("cancel-1", "operator", scope(20, None), Some(2)),
            at(2),
        )
        .expect("cancel approved request");
    assert_eq!(cancelled.status, RefundStatus::Cancelled);
    assert!(cancelled.status.is_terminal());
}

#[test]
fn history_replays_only_valid_transitions_and_monotonic_times() {
    let mut skipped_approval = refund();
    let mut skipped_record = skipped_approval.command_history[0].clone();
    skipped_record.command_id = Uuid::from_u128(999_010);
    skipped_record.idempotency_key = "post-without-approval".to_owned();
    skipped_record.command_kind = RefundCommandKind::Post;
    skipped_record.aggregate_version = 2;
    skipped_record.resulting_status = RefundStatus::Posted;
    skipped_record.occurred_at = at(1);
    skipped_approval.command_history.push(skipped_record);
    skipped_approval.version = 2;
    skipped_approval.status = RefundStatus::Posted;
    skipped_approval.reviewer_principal_ref = Some("reviewer".to_owned());
    skipped_approval.completion_principal_ref = Some("finance-handoff".to_owned());
    assert_eq!(
        skipped_approval.validate(),
        Err(PosRefundError::InvalidAggregate)
    );

    let mut reordered = refund();
    reordered
        .approve(
            &context("approve-for-history", "reviewer", scope(20, None), Some(1)),
            at(2),
        )
        .expect("approve request");
    reordered
        .post(
            &context(
                "post-for-history",
                "finance-handoff",
                scope(20, None),
                Some(2),
            ),
            at(3),
        )
        .expect("post approved request");
    reordered.command_history[2].occurred_at = at(1);
    assert_eq!(reordered.validate(), Err(PosRefundError::InvalidAggregate));
}

#[test]
fn finance_handoff_requires_scope_and_failed_commands_do_not_mutate() {
    let mut value = refund();
    let before_approval = value.clone();
    assert_eq!(
        value.approve(
            &context(
                "approve-wrong-branch",
                "reviewer",
                scope(20, Some(31)),
                Some(1)
            ),
            at(1),
        ),
        Err(PosRefundError::CommandScopeOutside)
    );
    assert_eq!(value, before_approval);

    value
        .approve(
            &context("approve-in-scope", "reviewer", scope(20, Some(30)), Some(1)),
            at(1),
        )
        .expect("approve in exact branch scope");
    let before_post = value.clone();
    assert_eq!(
        value.post(
            &context(
                "post-wrong-branch",
                "finance-handoff",
                scope(20, Some(31)),
                Some(2),
            ),
            at(2),
        ),
        Err(PosRefundError::CommandScopeOutside)
    );
    assert_eq!(value, before_post);

    let handoff = context("post-in-scope", "finance-handoff", scope(20, None), Some(2));
    value.post(&handoff, at(2)).expect("post in company scope");
    let replay = value.post(&handoff, at(2)).expect("exact handoff replay");
    assert!(matches!(replay, PosRefundCommandResult::Duplicate { .. }));
    assert_eq!(value.status, RefundStatus::Posted);
}

#[test]
fn scope_escape_and_sale_scope_mismatch_are_rejected() {
    let result = PosRefund::request(
        Uuid::from_u128(301),
        input(context(
            "request-outside",
            "requester",
            scope(21, None),
            None,
        )),
    );
    assert_eq!(result, Err(PosRefundError::CommandScopeOutside));

    let mut request = input(context(
        "request-mismatch",
        "requester",
        scope(20, None),
        None,
    ));
    request.scope = scope(20, None);
    request.sale.sale.scope = scope(20, Some(999));
    assert_eq!(
        PosRefund::request(Uuid::from_u128(302), request),
        Err(PosRefundError::SaleScopeMismatch)
    );
}

#[test]
fn non_completed_pos_sale_cannot_be_adapted_as_a_refund_fact() {
    let sale = Sale::new(
        Uuid::from_u128(400),
        scope(20, Some(30)),
        "USD",
        vec![
            SaleLine::new(Uuid::from_u128(401), "ITEM-401", "item", 1, 100, 0)
                .expect("valid sale line"),
        ],
        at(0),
        &context("sale-create", "requester", scope(20, None), None),
    )
    .expect("valid draft sale");
    assert_eq!(
        CompletedPosSaleFact::from_completed_sale(&sale, Vec::new()),
        Err(PosRefundError::SaleNotCompleted)
    );
}

#[test]
fn over_refund_quantity_and_amount_are_rejected_against_supplied_facts() {
    let mut too_many = input(context(
        "request-too-many",
        "requester",
        scope(20, None),
        None,
    ));
    too_many.lines[0].quantity = 4;
    assert_eq!(
        PosRefund::request(Uuid::from_u128(303), too_many),
        Err(PosRefundError::QuantityExceedsRefundable)
    );

    let mut too_expensive = input(context(
        "request-too-expensive",
        "requester",
        scope(20, None),
        None,
    ));
    too_expensive.lines[0].amount_minor = 1_001;
    assert_eq!(
        PosRefund::request(Uuid::from_u128(304), too_expensive),
        Err(PosRefundError::AmountExceedsRefundable)
    );
}

#[test]
fn approval_requires_requester_reviewer_separation() {
    let mut value = refund();
    assert_eq!(
        value.approve(
            &context("approve-self", "requester", scope(20, None), Some(1)),
            at(1),
        ),
        Err(PosRefundError::SelfApproval)
    );
    assert_eq!(value.status, RefundStatus::Requested);
}

#[test]
fn exact_replay_requires_both_command_identity_and_payload_binding() {
    let mut value = refund();
    let approval = context("approve-exact", "reviewer", scope(20, None), Some(1));
    value.approve(&approval, at(1)).expect("approve request");
    let replay = value
        .approve(&approval, at(1))
        .expect("exact replay is deterministic");
    assert!(matches!(replay, PosRefundCommandResult::Duplicate { .. }));
    assert_eq!(replay.version(), 2);

    assert_eq!(
        value.approve(&approval, at(99)),
        Err(PosRefundError::IdempotencyConflict)
    );

    let mut changed_key = approval.clone();
    changed_key.idempotency_key = "approve-different-key".to_owned();
    assert_eq!(
        value.approve(&changed_key, at(2)),
        Err(PosRefundError::IdempotencyConflict)
    );

    let mut changed_command = approval;
    changed_command.command_id = Uuid::from_u128(999_001);
    assert_eq!(
        value.approve(&changed_command, at(2)),
        Err(PosRefundError::IdempotencyConflict)
    );
}

#[test]
fn invalid_shape_and_secret_like_text_are_rejected_without_secret_fields() {
    assert_eq!(
        PosRefundLineInput::new(Uuid::from_u128(1), "provider_token=secret", 1, 1),
        Err(PosRefundError::SensitiveText {
            field: "refund_line_reference"
        })
    );
    assert_eq!(
        PosRefundLineInput::new(Uuid::from_u128(1), "line", 0, 1),
        Err(PosRefundError::NonPositive {
            field: "refund_quantity"
        })
    );
    assert_eq!(
        PosRefundLineInput::new(Uuid::from_u128(1), "line", 1, 0),
        Err(PosRefundError::NonPositive {
            field: "refund_amount_minor"
        })
    );

    let mut request = input(context(
        "request-sensitive",
        "requester",
        scope(20, None),
        None,
    ));
    request.reason = "card_number=4111111111111111".to_owned();
    assert_eq!(
        PosRefund::request(Uuid::from_u128(305), request),
        Err(PosRefundError::SensitiveText { field: "reason" })
    );

    let encoded = to_string(&refund()).expect("secret-free aggregate serializes");
    for forbidden in ["payment", "provider_token", "card_number", "accounting"] {
        assert!(!encoded.to_ascii_lowercase().contains(forbidden));
    }
}

#[test]
fn history_version_gaps_are_rejected() {
    let mut value = refund();
    let mut synthetic_last = value.command_history[0].clone();
    synthetic_last.command_id = Uuid::from_u128(999_002);
    synthetic_last.idempotency_key = "synthetic-gap-version".to_owned();
    synthetic_last.aggregate_version = 3;
    synthetic_last.command_kind = RefundCommandKind::Approve;
    synthetic_last.resulting_status = RefundStatus::Approved;
    value.command_history.push(synthetic_last);
    value.version = 3;
    value.status = RefundStatus::Approved;
    value.reviewer_principal_ref = Some("reviewer".to_owned());
    assert_eq!(
        value.approve(
            &context("approve-gap", "reviewer", scope(20, None), Some(3)),
            at(4),
        ),
        Err(PosRefundError::InvalidAggregate)
    );
}

#[test]
fn command_history_exposes_typed_replay_facts() {
    let value = refund();
    let record = value.command_history.first().expect("request fact");
    assert_eq!(record.command_kind, RefundCommandKind::Request);
    assert_eq!(record.aggregate_version, 1);
    assert_ne!(record.fingerprint, [0; 32]);
    assert_eq!(record.idempotency_key, "request-1");
    assert_eq!(record.resulting_status, RefundStatus::Requested);
}
