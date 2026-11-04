//! Native Commerce quotation/sales-order policy and sequencing.
use erpx_server_commerce::public::order::{
    AuditEntry, CommerceOrderActor, CommerceOrderCommandRepository, CommerceOrderFuture,
    CommerceOrderReadRepository, CommerceOrderReceipt, ConvertedReadback, LINES_LIMIT, OrderAction,
    OrderCommandReceipt, OutboxEntry, ReceiptEntry, SalesOrderLineRow, SalesOrderRow,
    TransitionCommand, TransitionPlan, order_detail, transition_order,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};
use serde_json::json;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const KEY: &str = "commerce-order-command-0001";
const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn actor() -> CommerceOrderActor {
    CommerceOrderActor {
        user_id: UserId::new("user").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new("assignment").unwrap(),
        role: Some("sales_manager".into()),
        authority_revision: Some(7),
        assignment_version: Some(3),
        permissions: vec![
            "commerce.read".into(),
            "commerce.sales".into(),
            "commerce.approve".into(),
        ],
        modules: vec!["sales".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn order(kind: &str, state: &str, version: i64, created_by: &str) -> SalesOrderRow {
    SalesOrderRow {
        id: "order-1".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        kind: kind.into(),
        number: if kind == "quote" { "QT-1" } else { "SO-1" }.into(),
        party_id: "party-1".into(),
        warehouse_id: Some("warehouse-1".into()),
        currency: "THB".into(),
        order_date: "2026-11-01".into(),
        due_date: "2026-11-08".into(),
        note: String::new(),
        state: state.into(),
        subtotal_minor: 10_000,
        tax_minor: 700,
        source_id: None,
        version,
        created_by: created_by.into(),
        approved_by: None,
        created_at: 1,
        updated_at: 1,
        price_list_id: None,
        price_list_version: None,
        discount_reason: None,
        discount_approval_required: 0,
        discount_approved_by: None,
        discount_approved_at: None,
    }
}

fn line() -> SalesOrderLineRow {
    SalesOrderLineRow {
        id: "line-1".into(),
        order_id: "order-1".into(),
        product_id: "product-1".into(),
        description: "Service".into(),
        quantity_milli: 1_000,
        fulfilled_milli: 0,
        price_minor: 10_000,
        tax_bps: 700,
        subtotal_minor: 10_000,
        tax_minor: 700,
        ordinal: 0,
        list_price_minor: None,
        discount_bps: 0,
        item_type: "service".into(),
        sku: "SVC-1".into(),
        unit: "each".into(),
        tracking_mode: "none".into(),
    }
}

fn submit() -> TransitionCommand {
    TransitionCommand {
        key: CommandKey::new(KEY).unwrap(),
        order_id: RecordId::new("order-1").unwrap(),
        action: OrderAction::Submit,
        expected_version: 1,
        reason: None,
    }
}

fn expected_receipt(state: &str, version: i64, sales_id: Option<&str>) -> CommerceOrderReceipt {
    CommerceOrderReceipt {
        id: "order-1".into(),
        state: state.into(),
        version,
        sales_id: sales_id.map(str::to_owned),
    }
}

#[derive(Default)]
struct ReadProbe {
    authority: bool,
    order: Option<SalesOrderRow>,
    lines: Vec<SalesOrderLineRow>,
    calls: Vec<&'static str>,
}

impl CommerceOrderReadRepository for ReadProbe {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: bool,
        _: bool,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("authority");
        let authority = self.authority;
        Box::pin(async move { Ok(authority) })
    }

    fn order<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        self.calls.push("order");
        let order = self.order.clone();
        Box::pin(async move { Ok(order) })
    }

    fn lines<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, Vec<SalesOrderLineRow>, Self::Error> {
        self.calls.push("lines");
        let lines = self.lines.clone();
        Box::pin(async move { Ok(lines) })
    }
}

impl ReadProbe {
    fn ready(order: SalesOrderRow) -> Self {
        Self {
            authority: true,
            order: Some(order),
            lines: vec![line()],
            calls: Vec::new(),
        }
    }
}

#[derive(Default)]
struct CommandProbe {
    authority: bool,
    approve_authority: bool,
    hash: String,
    receipt: Option<OrderCommandReceipt>,
    locked: Option<SalesOrderRow>,
    fulfillments: i64,
    transition_ok: bool,
    insert_converted: bool,
    copied: u64,
    converted: Option<ConvertedReadback>,
    readback: Option<SalesOrderRow>,
    receipt_insert: bool,
    calls: Vec<&'static str>,
    plans: Vec<TransitionPlan>,
    audited: Vec<(String, String, String, i64)>,
    outboxed: Vec<(String, String, String, i64, String, Option<String>)>,
}

impl CommandProbe {
    fn ready(locked: SalesOrderRow, readback: SalesOrderRow) -> Self {
        Self {
            authority: true,
            approve_authority: true,
            hash: HASH.into(),
            locked: Some(locked),
            transition_ok: true,
            insert_converted: true,
            copied: 1,
            readback: Some(readback),
            receipt_insert: true,
            ..Self::default()
        }
    }
}

impl CommerceOrderCommandRepository for CommandProbe {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        write: bool,
        approve: bool,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push(if !write {
            "authority-read"
        } else if approve {
            "authority-approve"
        } else {
            "authority-write"
        });
        let value = if approve {
            self.approve_authority
        } else {
            self.authority
        };
        Box::pin(async move { Ok(value) })
    }

    fn lock_command<'a>(&'a mut self, key: &'a str) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("lock-command");
        assert_eq!(key, KEY);
        Box::pin(async move { Ok(()) })
    }

    fn digest<'a>(&'a mut self, value: &'a str) -> CommerceOrderFuture<'a, String, Self::Error> {
        self.calls.push("digest");
        assert!(value.contains("\"operation\""), "{value}");
        assert!(value.contains("\"expectedVersion\""), "{value}");
        let hash = self.hash.clone();
        Box::pin(async move { Ok(hash) })
    }

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CommerceOrderFuture<'a, Option<OrderCommandReceipt>, Self::Error> {
        self.calls.push("receipt");
        assert_eq!(key, KEY);
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }

    fn lock_order<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        self.calls.push("lock");
        let order = self.locked.clone();
        Box::pin(async move { Ok(order) })
    }

    fn count_fulfillments<'a>(
        &'a mut self,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, i64, Self::Error> {
        self.calls.push("fulfillments");
        let count = self.fulfillments;
        Box::pin(async move { Ok(count) })
    }

    fn transition<'a>(
        &'a mut self,
        plan: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("transition");
        self.plans.push(plan.clone());
        let changed = self.transition_ok;
        Box::pin(async move { Ok(changed) })
    }

    fn insert_converted_order<'a>(
        &'a mut self,
        _: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("insert-converted");
        let changed = self.insert_converted;
        Box::pin(async move { Ok(changed) })
    }

    fn copy_converted_lines<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, u64, Self::Error> {
        self.calls.push("copy-lines");
        let copied = self.copied;
        Box::pin(async move { Ok(copied) })
    }

    fn converted_readback<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a CompanyId,
    ) -> CommerceOrderFuture<'a, Option<ConvertedReadback>, Self::Error> {
        self.calls.push("converted-readback");
        let converted = self.converted.clone();
        Box::pin(async move { Ok(converted) })
    }

    fn readback_order<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        _: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error> {
        self.calls.push("readback");
        let readback = self.readback.clone();
        Box::pin(async move { Ok(readback) })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        _: &'a ReceiptEntry<'a>,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error> {
        self.calls.push("receipt-insert");
        let changed = self.receipt_insert;
        Box::pin(async move { Ok(changed) })
    }

    fn audit<'a>(
        &'a mut self,
        _: &'a CommerceOrderActor,
        entry: &'a AuditEntry<'a>,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("audit");
        self.audited.push((
            entry.operation.into(),
            entry.record_type.into(),
            entry.record_id.into(),
            entry.version,
        ));
        Box::pin(async move { Ok(()) })
    }

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        _: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error> {
        self.calls.push("outbox");
        self.outboxed.push((
            entry.operation.into(),
            entry.record_type.into(),
            entry.record_id.into(),
            entry.version,
            entry.state.into(),
            entry.sales_id.map(str::to_owned),
        ));
        Box::pin(async move { Ok(()) })
    }
}

fn read(probe: &mut ReadProbe, actor: &CommerceOrderActor) -> Result<(), BusinessError> {
    sqlx::test_block_on(order_detail(actor, "order-1", probe, NOW)).map(|_| ())
}

fn run(
    probe: &mut CommandProbe,
    actor: &CommerceOrderActor,
    command: &TransitionCommand,
) -> Result<CommerceOrderReceipt, BusinessError> {
    sqlx::test_block_on(transition_order(actor, command, probe, NOW))
}

#[test]
fn read_policy_precedes_every_source_read() {
    let mut no_permission = actor();
    no_permission.permissions.clear();
    let mut probe = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let error = read(&mut probe, &no_permission).unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "COMMERCE_PERMISSION_REQUIRED");
    assert!(probe.calls.is_empty());

    let mut no_module = actor();
    no_module.modules.clear();
    let mut probe = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let error = read(&mut probe, &no_module).unwrap_err();
    assert_eq!(error.code, "COMMERCE_MODULE_FORBIDDEN");
    assert!(probe.calls.is_empty());

    let mut expired = actor();
    expired.expires_at = NOW.get();
    let mut probe = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let error = read(&mut probe, &expired).unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert!(probe.calls.is_empty());

    let mut revoked = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    revoked.authority = false;
    let error = read(&mut revoked, &actor()).unwrap_err();
    assert_eq!(error.code, "COMMERCE_PERMISSION_REQUIRED");
    assert_eq!(revoked.calls, ["authority"]);
}

#[test]
fn read_rejects_foreign_scope_purchase_kind_and_invalid_documents() {
    let mut foreign = ReadProbe::ready(SalesOrderRow {
        company_id: "foreign".into(),
        ..order("quote", "draft", 1, "creator")
    });
    assert_eq!(
        read(&mut foreign, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut branch = ReadProbe::ready(SalesOrderRow {
        branch_id: Some("other-branch".into()),
        ..order("quote", "draft", 1, "creator")
    });
    assert_eq!(
        read(&mut branch, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut purchase = ReadProbe::ready(SalesOrderRow {
        kind: "purchase".into(),
        number: "PO-1".into(),
        ..order("quote", "draft", 1, "creator")
    });
    assert_eq!(
        read(&mut purchase, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut totals = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    totals.lines[0].subtotal_minor = 9_999;
    assert_eq!(
        read(&mut totals, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut unordered = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let mut second = line();
    second.id = "line-2".into();
    second.product_id = "product-2".into();
    second.ordinal = 0;
    unordered.lines.push(second);
    assert_eq!(
        read(&mut unordered, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut missing_product = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    missing_product.lines[0].sku.clear();
    assert_eq!(
        read(&mut missing_product, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut oversized = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    oversized.lines = (0..=LINES_LIMIT)
        .map(|index| SalesOrderLineRow {
            id: format!("line-{index}"),
            product_id: format!("product-{index}"),
            ordinal: index as i64,
            subtotal_minor: 0,
            tax_minor: 0,
            ..line()
        })
        .collect();
    oversized.order.as_mut().unwrap().subtotal_minor = 0;
    oversized.order.as_mut().unwrap().tax_minor = 0;
    assert_eq!(
        read(&mut oversized, &actor()).unwrap_err().code,
        "COMMERCE_ORDER_TOO_LARGE"
    );
}

#[test]
fn read_returns_only_the_exact_owner_columns() {
    let mut probe = ReadProbe::ready(order("quote", "draft", 1, "creator"));
    let document = sqlx::test_block_on(order_detail(&actor(), "order-1", &mut probe, NOW)).unwrap();
    assert_eq!(probe.calls, ["authority", "order", "lines"]);
    let value = serde_json::to_value(document.to_wire()).unwrap();
    assert_eq!(value["order"]["id"], "order-1");
    assert_eq!(value["order"]["party_id"], "party-1");
    assert_eq!(
        value["order"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [
            "approved_by",
            "branch_id",
            "company_id",
            "created_at",
            "created_by",
            "currency",
            "department_id",
            "discount_approval_required",
            "discount_approved_at",
            "discount_approved_by",
            "discount_reason",
            "due_date",
            "id",
            "kind",
            "note",
            "number",
            "order_date",
            "party_id",
            "price_list_id",
            "price_list_version",
            "source_id",
            "state",
            "subtotal_minor",
            "tax_minor",
            "updated_at",
            "version",
            "warehouse_id",
        ]
    );
    assert_eq!(
        value["lines"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        [
            "description",
            "discount_bps",
            "fulfilled_milli",
            "id",
            "item_type",
            "list_price_minor",
            "order_id",
            "ordinal",
            "price_minor",
            "product_id",
            "quantity_milli",
            "sku",
            "subtotal_minor",
            "tax_bps",
            "tax_minor",
            "tracking_mode",
            "unit",
        ]
    );
}

#[test]
fn submit_writes_one_exact_transition_receipt_audit_and_outbox() {
    let mut probe = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    let receipt = run(&mut probe, &actor(), &submit()).unwrap();
    assert_eq!(receipt, expected_receipt("submitted", 2, None));
    assert_eq!(
        probe.calls,
        [
            "lock-command",
            "digest",
            "receipt",
            "authority-write",
            "lock",
            "transition",
            "receipt-insert",
            "audit",
            "outbox",
            "readback"
        ]
    );
    let plan = &probe.plans[0];
    assert_eq!(plan.expected_state, "draft");
    assert_eq!(plan.next_state, "submitted");
    assert_eq!(plan.approved_by, None);
    assert_eq!(
        probe.audited,
        [(
            "commerce.order.submit".into(),
            "commerce_order".into(),
            "order-1".into(),
            2
        )]
    );
    assert_eq!(
        probe.outboxed,
        [(
            "commerce.order.submit".into(),
            "commerce_order".into(),
            "order-1".into(),
            2,
            "submitted".into(),
            None
        )]
    );
}

#[test]
fn replay_returns_the_original_receipt_without_a_fresh_write() {
    let mut probe = CommandProbe {
        receipt: Some(OrderCommandReceipt {
            request_hash: HASH.into(),
            result_json: json!({
                "id":"order-1","state":"submitted","version":2,"salesId":null
            })
            .to_string(),
            operation: Some("commerce.order.submit".into()),
            actor_id: Some("user".into()),
            record_id: Some("order-1".into()),
            authority_json: None,
        }),
        ..CommandProbe::ready(
            order("quote", "draft", 1, "creator"),
            order("quote", "submitted", 2, "creator"),
        )
    };
    let receipt = run(&mut probe, &actor(), &submit()).unwrap();
    assert_eq!(receipt, expected_receipt("submitted", 2, None));
    assert_eq!(probe.calls, ["lock-command", "digest", "receipt"]);

    let mut conflict = CommandProbe {
        receipt: Some(OrderCommandReceipt {
            request_hash: "b".repeat(64),
            result_json: json!({
                "id":"order-1","state":"submitted","version":2,"salesId":null
            })
            .to_string(),
            operation: Some("commerce.order.submit".into()),
            actor_id: Some("user".into()),
            record_id: Some("order-1".into()),
            authority_json: None,
        }),
        ..CommandProbe::ready(
            order("quote", "draft", 1, "creator"),
            order("quote", "submitted", 2, "creator"),
        )
    };
    assert_eq!(
        run(&mut conflict, &actor(), &submit()).unwrap_err().code,
        "IDEMPOTENCY_CONFLICT"
    );
}

#[test]
fn command_denials_precede_effects() {
    let mut no_authority = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    no_authority.authority = false;
    assert_eq!(
        run(&mut no_authority, &actor(), &submit())
            .unwrap_err()
            .code,
        "COMMERCE_PERMISSION_REQUIRED"
    );
    assert_eq!(
        no_authority.calls,
        ["lock-command", "digest", "receipt", "authority-write"]
    );

    for command in [
        TransitionCommand {
            expected_version: 0,
            ..submit()
        },
        TransitionCommand {
            reason: Some("x".repeat(1_001)),
            ..submit()
        },
    ] {
        let mut probe = CommandProbe::ready(
            order("quote", "draft", 1, "creator"),
            order("quote", "submitted", 2, "creator"),
        );
        assert_eq!(
            run(&mut probe, &actor(), &command).unwrap_err().code,
            "COMMERCE_ORDER_REQUEST_INVALID"
        );
        assert!(probe.calls.is_empty());
    }

    let mut not_found = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    not_found.locked = None;
    assert_eq!(
        run(&mut not_found, &actor(), &submit()).unwrap_err().code,
        "ORDER_NOT_FOUND"
    );
}

#[test]
fn quotation_approval_requires_independence_discount_reason_and_approver_permission() {
    let mut discount = order("quote", "submitted", 2, "creator");
    discount.discount_approval_required = 1;
    let mut create_self =
        CommandProbe::ready(discount.clone(), order("quote", "confirmed", 3, "creator"));
    let command = TransitionCommand {
        action: OrderAction::Approve,
        expected_version: 2,
        ..submit()
    };
    let mut self_actor = actor();
    self_actor.user_id = UserId::new("creator").unwrap();
    assert_eq!(
        run(&mut create_self, &self_actor, &command)
            .unwrap_err()
            .code,
        "INDEPENDENT_APPROVER_REQUIRED"
    );

    let mut missing_reason =
        CommandProbe::ready(discount.clone(), order("quote", "confirmed", 3, "creator"));
    assert_eq!(
        run(&mut missing_reason, &actor(), &command)
            .unwrap_err()
            .code,
        "DISCOUNT_APPROVAL_REASON_REQUIRED"
    );

    let mut no_approve =
        CommandProbe::ready(discount.clone(), order("quote", "confirmed", 3, "creator"));
    no_approve.approve_authority = false;
    assert_eq!(
        run(&mut no_approve, &actor(), &command).unwrap_err().code,
        "COMMERCE_APPROVAL_REQUIRED"
    );

    let mut approved = CommandProbe::ready(
        discount,
        SalesOrderRow {
            state: "confirmed".into(),
            version: 3,
            approved_by: Some("user".into()),
            discount_approved_by: Some("user".into()),
            discount_approved_at: Some(NOW.get()),
            ..order("quote", "confirmed", 3, "creator")
        },
    );
    let receipt = run(
        &mut approved,
        &actor(),
        &TransitionCommand {
            reason: Some("Independent discount review".into()),
            ..command
        },
    )
    .unwrap();
    assert_eq!(receipt, expected_receipt("confirmed", 3, None));
    assert_eq!(approved.plans[0].approved_by.as_deref(), Some("user"));
    assert_eq!(
        approved.plans[0].discount_approved_by.as_deref(),
        Some("user")
    );
}

#[test]
fn sales_approval_and_confirmed_sales_cancellation_fail_closed() {
    let mut approve = CommandProbe::ready(
        order("sales", "submitted", 2, "creator"),
        order("sales", "confirmed", 3, "creator"),
    );
    assert_eq!(
        run(
            &mut approve,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Approve,
                expected_version: 2,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "COMMERCE_SALES_APPROVAL_FINANCE_GUARD"
    );

    let mut cancel = CommandProbe::ready(
        order("sales", "confirmed", 3, "creator"),
        order("sales", "cancelled", 4, "creator"),
    );
    assert_eq!(
        run(
            &mut cancel,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Cancel,
                expected_version: 3,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "ORDER_REVERSAL_REQUIRED"
    );
}

#[test]
fn confirmed_quotation_cancel_is_independent_and_requires_no_fulfillment() {
    let mut fulfilled = CommandProbe::ready(
        order("quote", "confirmed", 3, "creator"),
        order("quote", "cancelled", 4, "creator"),
    );
    fulfilled.fulfillments = 1;
    assert_eq!(
        run(
            &mut fulfilled,
            &actor(),
            &TransitionCommand {
                action: OrderAction::Cancel,
                expected_version: 3,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "ORDER_REVERSAL_REQUIRED"
    );

    let mut self_cancel = CommandProbe::ready(
        order("quote", "confirmed", 3, "creator"),
        order("quote", "cancelled", 4, "creator"),
    );
    let mut self_actor = actor();
    self_actor.user_id = UserId::new("creator").unwrap();
    assert_eq!(
        run(
            &mut self_cancel,
            &self_actor,
            &TransitionCommand {
                action: OrderAction::Cancel,
                expected_version: 3,
                ..submit()
            }
        )
        .unwrap_err()
        .code,
        "INDEPENDENT_CANCELLER_REQUIRED"
    );

    let mut cancelled = CommandProbe::ready(
        order("quote", "confirmed", 3, "creator"),
        order("quote", "cancelled", 4, "creator"),
    );
    let receipt = run(
        &mut cancelled,
        &actor(),
        &TransitionCommand {
            action: OrderAction::Cancel,
            expected_version: 3,
            ..submit()
        },
    )
    .unwrap();
    assert_eq!(receipt, expected_receipt("cancelled", 4, None));
    assert_eq!(
        cancelled.calls,
        [
            "lock-command",
            "digest",
            "receipt",
            "authority-write",
            "lock",
            "authority-approve",
            "fulfillments",
            "transition",
            "receipt-insert",
            "audit",
            "outbox",
            "readback"
        ]
    );
}

#[test]
fn conversion_copies_the_quote_into_a_draft_sales_order() {
    let quote = order("quote", "confirmed", 3, "creator");
    let converted_order = SalesOrderRow {
        id: format!("order-{KEY}"),
        kind: "sales".into(),
        number: format!("SO-{}", HASH[..24].to_ascii_uppercase()),
        state: "draft".into(),
        version: 1,
        source_id: Some("order-1".into()),
        approved_by: None,
        created_by: "user".into(),
        created_at: NOW.get(),
        updated_at: NOW.get(),
        ..quote.clone()
    };
    let mut probe = CommandProbe::ready(
        quote.clone(),
        SalesOrderRow {
            state: "converted".into(),
            version: 4,
            ..quote.clone()
        },
    );
    probe.converted = Some(ConvertedReadback {
        order: converted_order,
        line_count: 1,
        line_subtotal_minor: quote.subtotal_minor,
        line_tax_minor: quote.tax_minor,
    });
    let receipt = run(
        &mut probe,
        &actor(),
        &TransitionCommand {
            action: OrderAction::Convert,
            expected_version: 3,
            ..submit()
        },
    )
    .unwrap();
    assert_eq!(
        receipt,
        CommerceOrderReceipt {
            id: "order-1".into(),
            state: "converted".into(),
            version: 4,
            sales_id: Some(format!("order-{KEY}")),
        }
    );
    assert_eq!(
        probe.calls,
        [
            "lock-command",
            "digest",
            "receipt",
            "authority-write",
            "lock",
            "transition",
            "insert-converted",
            "copy-lines",
            "converted-readback",
            "receipt-insert",
            "audit",
            "outbox",
            "readback"
        ]
    );
    assert_eq!(
        probe.outboxed[0].5.as_deref(),
        Some(format!("order-{KEY}").as_str())
    );
}

#[test]
fn stale_cas_and_bad_readback_fail_without_a_receipt() {
    let mut stale = CommandProbe::ready(
        order("quote", "draft", 2, "creator"),
        order("quote", "submitted", 3, "creator"),
    );
    assert_eq!(
        run(&mut stale, &actor(), &submit()).unwrap_err().code,
        "COMMERCE_ORDER_CHANGED"
    );

    let mut changed = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    changed.transition_ok = false;
    assert_eq!(
        run(&mut changed, &actor(), &submit()).unwrap_err().code,
        "COMMERCE_ORDER_CHANGED"
    );

    let mut bad = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    bad.readback.as_mut().unwrap().version = 3;
    assert_eq!(
        run(&mut bad, &actor(), &submit()).unwrap_err().code,
        "COMMERCE_ORDER_SOURCE_INVALID"
    );

    let mut duplicate_receipt = CommandProbe::ready(
        order("quote", "draft", 1, "creator"),
        order("quote", "submitted", 2, "creator"),
    );
    duplicate_receipt.receipt_insert = false;
    assert_eq!(
        run(&mut duplicate_receipt, &actor(), &submit())
            .unwrap_err()
            .code,
        "IDEMPOTENCY_CONFLICT"
    );
}
