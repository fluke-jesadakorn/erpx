use erpx_ui::{
    MAX_POS_COMMAND_ID_BYTES, MAX_POS_COMMAND_SCALAR_BYTES, POS_COMMANDS, PosCommandCurrencyCode,
    PosCommandInputError, PosCommandKind, PosCommandLocale, PosCommandMoney, PosCommandScalarText,
    PosCommandState, PosCommandStateKind, PosOpaqueId, ReceiptDeliveryMode, ReceiptIssueInput,
    ReceiptIssueStatus, ReceiptIssueSummary, RefundReviewInput, RefundReviewStatus,
    RefundReviewSummary, SaleEntryInput, SaleEntryLine, SaleEntryStatus, SaleEntrySummary,
    ShiftCloseApprovalInput, ShiftCloseStatus, ShiftCloseSummary,
};

type CurrencyCode = PosCommandCurrencyCode;
type Money = PosCommandMoney;
type ScalarText = PosCommandScalarText;

fn id(value: &str) -> PosOpaqueId {
    PosOpaqueId::new(value).expect("test identifier is bounded and opaque")
}

fn scalar(value: &str) -> ScalarText {
    ScalarText::new(value).expect("test value is scalar")
}

fn money(amount_minor: i64) -> Money {
    Money::new(CurrencyCode::new("THB").expect("currency"), amount_minor).expect("money")
}

#[test]
fn four_command_surfaces_have_stable_routes_and_bilingual_labels() {
    assert_eq!(POS_COMMANDS.len(), 4);
    assert_eq!(POS_COMMANDS[0], PosCommandKind::SaleEntry);
    assert_eq!(POS_COMMANDS[1], PosCommandKind::RefundReview);
    assert_eq!(POS_COMMANDS[2], PosCommandKind::ReceiptIssuance);
    assert_eq!(POS_COMMANDS[3], PosCommandKind::ShiftCloseApproval);

    for command in POS_COMMANDS {
        assert!(command.route().starts_with("/pos/"));
        assert!(!command.label(PosCommandLocale::En).is_empty());
        assert!(!command.label(PosCommandLocale::Th).is_empty());
        assert_ne!(
            command.label(PosCommandLocale::En),
            command.label(PosCommandLocale::Th)
        );
        assert_eq!(PosCommandKind::from_route(command.route()), Ok(*command));
        assert_eq!(
            PosCommandKind::try_from(format!("{}/", command.route()).as_str()),
            Ok(*command)
        );
    }

    for invalid in [
        "",
        "/pos",
        "/pos/sales",
        "/pos/sales/entry/extra",
        "/pos/sales/entry?token=hidden",
        "/pos/sales/entry#fragment",
        "/pos/sales/ entry",
    ] {
        let error = PosCommandKind::from_route(invalid).expect_err("accepted invalid route");
        assert!(!error.to_string().contains("token"));
    }
    assert!(matches!(
        PosCommandKind::from_route("/pos/sales/entry?token=hidden"),
        Err(PosCommandInputError::UnsafeCommandRoute)
    ));
}

#[test]
fn opaque_ids_and_scalar_values_are_bounded_and_redacted() {
    assert_eq!(id("sale-opaque-1").as_str(), "sale-opaque-1");
    for invalid in [
        "",
        ".",
        "..",
        "sale/id",
        "sale id",
        "../sale",
        "refresh-token",
        "api_key",
        "private_key",
    ] {
        assert!(PosOpaqueId::new(invalid).is_err(), "accepted {invalid:?}");
    }
    assert!(matches!(
        PosOpaqueId::new("x".repeat(MAX_POS_COMMAND_ID_BYTES + 1)),
        Err(PosCommandInputError::IdentifierTooLong)
    ));
    for invalid in ["{structured}", "[structured]", "provider token: hidden"] {
        assert!(ScalarText::new(invalid).is_err(), "accepted {invalid:?}");
    }
    assert!(matches!(
        ScalarText::new("x".repeat(MAX_POS_COMMAND_SCALAR_BYTES + 1)),
        Err(PosCommandInputError::ScalarTooLong)
    ));
}

#[test]
fn sale_entry_is_scalar_only_and_line_bounded() {
    let line =
        SaleEntryLine::new(scalar("sku-1"), scalar("Coffee"), 2, 100, 20).expect("valid line");
    assert_eq!(line.total_minor(), 220);
    let input = SaleEntryInput::new(
        id("command-sale-1"),
        id("register-1"),
        CurrencyCode::new("THB").expect("currency"),
        vec![line],
        Some(scalar("counter sale")),
    )
    .expect("valid sale input");
    assert_eq!(input.lines().len(), 1);
    assert_eq!(input.note().expect("note").as_str(), "counter sale");

    let summary = SaleEntrySummary::new(
        id("sale-1"),
        id("register-1"),
        input.lines().len(),
        money(220),
        SaleEntryStatus::Pending,
    )
    .expect("valid summary");
    assert_eq!(summary.total().amount_minor(), 220);
    assert_eq!(summary.status(), SaleEntryStatus::Pending);
    assert_eq!(SaleEntryStatus::Draft.label(PosCommandLocale::Th), "ฉบับร่าง");
}

#[test]
fn refund_receipt_and_shift_contracts_retain_only_scalar_summaries() {
    let refund = RefundReviewInput::new(
        id("command-refund-1"),
        id("sale-1"),
        Some(id("line-1")),
        money(100),
        scalar("customer request"),
    );
    assert_eq!(refund.amount().amount_minor(), 100);
    assert_eq!(refund.validate(), Ok(()));
    let refund_summary = RefundReviewSummary::new(
        id("refund-1"),
        id("sale-1"),
        money(100),
        RefundReviewStatus::Pending,
    );
    assert_eq!(refund_summary.status(), RefundReviewStatus::Pending);
    assert_eq!(refund_summary.validate(), Ok(()));

    let zero_refund = RefundReviewInput::new(
        id("command-refund-zero"),
        id("sale-1"),
        None,
        money(0),
        scalar("customer request"),
    );
    assert_eq!(
        zero_refund.validate(),
        Err(PosCommandInputError::InvalidAmount)
    );
    assert!(matches!(
        RefundReviewInput::try_new(
            id("command-refund-zero-try"),
            id("sale-1"),
            None,
            money(0),
            scalar("customer request"),
        ),
        Err(PosCommandInputError::InvalidAmount)
    ));
    assert!(matches!(
        RefundReviewSummary::try_new(
            id("refund-zero"),
            id("sale-1"),
            money(0),
            RefundReviewStatus::Pending,
        ),
        Err(PosCommandInputError::InvalidAmount)
    ));

    let receipt = ReceiptIssueInput::new(
        id("command-receipt-1"),
        id("sale-1"),
        ReceiptDeliveryMode::CustomerCopy,
    );
    let receipt_summary = ReceiptIssueSummary::new(
        id("receipt-1"),
        receipt.sale_id().clone(),
        receipt.delivery_mode(),
        ReceiptIssueStatus::Pending,
    );
    assert_eq!(
        receipt_summary.delivery_mode(),
        ReceiptDeliveryMode::CustomerCopy
    );
    assert_eq!(
        ReceiptDeliveryMode::Printed.label(PosCommandLocale::Th),
        "สำเนาพิมพ์"
    );

    let close = ShiftCloseApprovalInput::new(
        id("command-close-1"),
        id("shift-1"),
        7,
        money(1_000),
        Some(scalar("count reviewed")),
    );
    let close_summary = ShiftCloseSummary::new(
        close.shift_id().clone(),
        close.counted_total().clone(),
        ShiftCloseStatus::Pending,
    );
    assert_eq!(close.expected_version(), 7);
    assert_eq!(close_summary.status(), ShiftCloseStatus::Pending);
}

#[test]
fn every_explicit_state_is_renderable_without_implying_authorization() {
    let reason = || scalar("server response");
    let states = [
        PosCommandState::<()>::Loading,
        PosCommandState::Empty,
        PosCommandState::Ready(()),
        PosCommandState::Denied { reason: reason() },
        PosCommandState::Stale {
            data: Some(()),
            age_seconds: 60,
            reason: reason(),
        },
        PosCommandState::ReadOnly {
            data: Some(()),
            reason: reason(),
        },
        PosCommandState::Conflict {
            data: Some(()),
            reason: reason(),
        },
        PosCommandState::Offline {
            data: Some(()),
            reason: reason(),
        },
        PosCommandState::Error {
            data: None,
            reason: reason(),
        },
    ];
    let kinds: Vec<_> = states.iter().map(PosCommandState::kind).collect();
    assert_eq!(
        kinds,
        vec![
            PosCommandStateKind::Loading,
            PosCommandStateKind::Empty,
            PosCommandStateKind::Ready,
            PosCommandStateKind::Denied,
            PosCommandStateKind::Stale,
            PosCommandStateKind::ReadOnly,
            PosCommandStateKind::Conflict,
            PosCommandStateKind::Offline,
            PosCommandStateKind::Error,
        ]
    );
    assert_eq!(states[5].state_label(PosCommandLocale::En), "Read-only");
    assert_eq!(states[5].state_label(PosCommandLocale::Th), "อ่านอย่างเดียว");

    let cached_states = [
        PosCommandState::Stale {
            data: Some("cached"),
            age_seconds: 301,
            reason: scalar("refresh required"),
        },
        PosCommandState::ReadOnly {
            data: Some("read-only snapshot"),
            reason: scalar("command access is not enabled"),
        },
        PosCommandState::Conflict {
            data: Some("conflicted snapshot"),
            reason: scalar("version changed"),
        },
        PosCommandState::Offline {
            data: Some("offline snapshot"),
            reason: scalar("authoritative service unavailable"),
        },
        PosCommandState::Error {
            data: Some("last safe snapshot"),
            reason: scalar("safe retry required"),
        },
    ];
    for state in &cached_states {
        assert!(state.can_render_data());
        assert!(state.data().is_some());
        assert!(!state.reason().expect("safe reason").as_str().is_empty());
    }
    assert_eq!(cached_states[0].age_seconds(), Some(301));
    assert_eq!(cached_states[1].age_seconds(), None);
    assert_eq!(PosCommandState::<&str>::Loading.data(), None);
    assert_eq!(PosCommandState::<&str>::Empty.reason(), None);

    let all_kinds = [
        PosCommandStateKind::Loading,
        PosCommandStateKind::Empty,
        PosCommandStateKind::Ready,
        PosCommandStateKind::Denied,
        PosCommandStateKind::Stale,
        PosCommandStateKind::ReadOnly,
        PosCommandStateKind::Conflict,
        PosCommandStateKind::Offline,
        PosCommandStateKind::Error,
    ];
    for kind in all_kinds {
        assert!(!kind.label(PosCommandLocale::En).is_empty());
        assert!(!kind.label(PosCommandLocale::Th).is_empty());
    }
}
