//! Native Procurement supplier promise detail policy and retained-shape tests.
//! These prove owner projection facts only; purchase admission, Account session
//! and PostgreSQL remain separate.
use erpx_server_procurement::public::supplier_promise_history::{
    OpenPromiseRow, SupplierPromiseHistoryFuture, SupplierPromiseHistoryRepository,
    SupplierPromiseLineRow, SupplierPromiseOrderRow, SupplierPromisePurchaseAccess,
    SupplierPromiseReadActor, SupplierPromiseRow, SupplierPromiseSourceLineRow,
    supplier_promise_detail,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

fn actor(permissions: &[&str], modules: &[&str]) -> SupplierPromiseReadActor {
    SupplierPromiseReadActor {
        user_id: "owner".into(),
        company_id: company(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: permissions.iter().map(|value| (*value).to_owned()).collect(),
        modules: modules.iter().map(|value| (*value).to_owned()).collect(),
        expires_at: NOW.get() + 60_000,
    }
}

fn purchase(settle_existing: bool) -> SupplierPromisePurchaseAccess {
    SupplierPromisePurchaseAccess {
        new_work: true,
        settle_existing,
        history: true,
        revision: "sha256:7".into(),
    }
}

fn order(state: &str) -> SupplierPromiseOrderRow {
    SupplierPromiseOrderRow {
        id: "order".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: None,
        kind: "purchase".into(),
        number: "PO-1".into(),
        party_id: "supplier".into(),
        warehouse_id: Some("warehouse".into()),
        currency: "THB".into(),
        order_date: "2026-10-20".into(),
        due_date: "2026-11-10".into(),
        note: String::new(),
        state: state.into(),
        subtotal_minor: 15_000,
        tax_minor: 1_050,
        source_id: None,
        version: 7,
        created_by: "owner".into(),
        approved_by: Some("approver".into()),
        created_at: 10,
        updated_at: 20,
        price_list_id: None,
        price_list_version: None,
        discount_reason: None,
        discount_approval_required: 0,
        discount_approved_by: None,
        discount_approved_at: None,
        supplier_name: "Bounded supplier".into(),
    }
}

fn header(id: &str, version: i64, kind: &str, date: &str) -> SupplierPromiseRow {
    SupplierPromiseRow {
        id: id.into(),
        order_id: "order".into(),
        company_id: "company".into(),
        order_version: version,
        supplier_id: "supplier".into(),
        currency: "THB".into(),
        required_date: "2026-11-10".into(),
        kind: kind.into(),
        confirmation_date: date.into(),
        supplier_reference: "SUP-REF-1".into(),
        evidence: "Supplier confirmation evidence".into(),
        recorded_by: "owner".into(),
        created_at: 20,
    }
}

fn schedule(
    id: &str,
    order_line: &str,
    ordinal: i64,
    quantity: i64,
    snapshot: i64,
    date: &str,
) -> SupplierPromiseLineRow {
    SupplierPromiseLineRow {
        id: id.into(),
        promise_id: "promise-2".into(),
        order_line_id: order_line.into(),
        ordinal,
        quantity_milli: quantity,
        fulfilled_snapshot_milli: snapshot,
        promised_date: date.into(),
    }
}

fn open(line: &SupplierPromiseLineRow, prior: i64, remaining: i64) -> OpenPromiseRow {
    OpenPromiseRow {
        id: line.id.clone(),
        promise_id: line.promise_id.clone(),
        order_id: "order".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: None,
        order_number: "PO-1".into(),
        order_state: "confirmed".into(),
        order_line_id: line.order_line_id.clone(),
        ordinal: line.ordinal,
        quantity_milli: line.quantity_milli,
        fulfilled_snapshot_milli: line.fulfilled_snapshot_milli,
        promised_date: line.promised_date.clone(),
        product_id: "product".into(),
        description: "Bounded part".into(),
        fulfilled_milli: 4_000,
        prior_quantity: prior,
        remaining_milli: remaining,
    }
}

fn source(id: &str, ordinal: i64, quantity: i64, fulfilled: i64) -> SupplierPromiseSourceLineRow {
    SupplierPromiseSourceLineRow {
        id: id.into(),
        order_id: "order".into(),
        product_id: "product".into(),
        description: "Bounded part".into(),
        quantity_milli: quantity,
        fulfilled_milli: fulfilled,
        price_minor: 100,
        tax_bps: 700,
        subtotal_minor: 10_000,
        tax_minor: 700,
        ordinal,
        list_price_minor: None,
        discount_bps: 0,
        item_type: "stock".into(),
        sku: "SKU-A".into(),
    }
}

struct Repo {
    order: Option<SupplierPromiseOrderRow>,
    source: Vec<SupplierPromiseSourceLineRow>,
    history: Vec<SupplierPromiseRow>,
    schedule: Vec<SupplierPromiseLineRow>,
    open: Vec<OpenPromiseRow>,
    order_calls: usize,
    source_calls: usize,
    promise_calls: usize,
}

impl Repo {
    fn valid() -> Self {
        let schedule = vec![
            schedule("line-a1", "order-line-a", 0, 4_000, 4_000, "2026-11-08"),
            schedule("line-b1", "order-line-b", 1, 5_000, 0, "2026-11-09"),
        ];
        let open = vec![open(&schedule[0], 0, 4_000), open(&schedule[1], 0, 2_000)];
        Self {
            order: Some(order("confirmed")),
            source: vec![
                source("order-line-a", 0, 10_000, 4_000),
                source("order-line-b", 1, 5_000, 0),
            ],
            history: vec![
                header("promise-2", 2, "confirmed", "2026-11-01"),
                header("promise-1", 1, "withdrawn", "2026-10-30"),
            ],
            schedule,
            open,
            order_calls: 0,
            source_calls: 0,
            promise_calls: 0,
        }
    }
}

impl SupplierPromiseHistoryRepository for Repo {
    type Error = BusinessError;

    fn order<'a>(
        &'a mut self,
        _: &'a SupplierPromiseReadActor,
        _: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Option<SupplierPromiseOrderRow>, Self::Error> {
        self.order_calls += 1;
        Box::pin(async move { Ok(self.order.clone()) })
    }

    fn source_lines<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseSourceLineRow>, Self::Error> {
        self.source_calls += 1;
        Box::pin(async move { Ok(self.source.clone()) })
    }

    fn history<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseRow>, Self::Error> {
        self.promise_calls += 1;
        Box::pin(async move { Ok(self.history.clone()) })
    }

    fn lines<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseLineRow>, Self::Error> {
        self.promise_calls += 1;
        Box::pin(async move { Ok(self.schedule.clone()) })
    }

    fn open<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<OpenPromiseRow>, Self::Error> {
        self.promise_calls += 1;
        Box::pin(async move { Ok(self.open.clone()) })
    }
}

fn readable() -> SupplierPromiseReadActor {
    actor(&["commerce.read", "commerce.purchase"], &["procurement"])
}

#[tokio::test]
async fn detail_returns_the_retained_supplier_promise_shape() {
    let mut repository = Repo::valid();
    let detail = supplier_promise_detail(
        &readable(),
        &mut repository,
        "order",
        Some(purchase(true)),
        NOW,
    )
    .await
    .unwrap();

    assert_eq!(detail.order.id, "order");
    assert_eq!(detail.order.number, "PO-1");
    assert_eq!(detail.order.state, "confirmed");
    assert_eq!(detail.order.version, 7);
    assert_eq!(detail.order.supplier_name, "Bounded supplier");
    assert_eq!(detail.lines.len(), 2);
    assert_eq!(detail.lines[0].sku, "SKU-A");
    assert_eq!(detail.lines[0].outstanding_milli, 6_000);
    assert_eq!(detail.lines[0].unconfirmed_milli, 2_000);
    assert_eq!(detail.lines[1].outstanding_milli, 5_000);
    assert_eq!(detail.lines[1].unconfirmed_milli, 3_000);
    assert_eq!(detail.history.len(), 2);
    assert_eq!(detail.schedule.len(), 2);
    assert_eq!(detail.open.len(), 2);
    assert!(detail.can_record);
    let purchase = detail.purchase.as_ref().unwrap();
    assert!(purchase.new_work && purchase.settle_existing && purchase.history);
    assert_eq!(purchase.revision, "sha256:7");
    assert_eq!(repository.order_calls, 1);
    assert_eq!(repository.source_calls, 1);
    assert_eq!(repository.promise_calls, 3);
}

#[tokio::test]
async fn serialized_detail_uses_the_exact_retained_keys() {
    let mut repository = Repo::valid();
    let detail = supplier_promise_detail(
        &readable(),
        &mut repository,
        "order",
        Some(purchase(true)),
        NOW,
    )
    .await
    .unwrap();
    let value = serde_json::to_value(&detail).unwrap();
    let mut keys = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["canRecord", "history", "lines", "open", "order", "purchase", "schedule"]
    );
    assert_eq!(value["canRecord"], serde_json::json!(true));
    assert_eq!(value["order"]["supplier_name"], "Bounded supplier");
    assert!(value["lines"][0].get("outstanding_milli").is_some());
    assert!(value["lines"][0].get("unconfirmed_milli").is_some());
    assert_eq!(value["purchase"]["newWork"], serde_json::json!(true));
    assert_eq!(value["purchase"]["settleExisting"], serde_json::json!(true));
    assert_eq!(value["purchase"]["revision"], "sha256:7");
    assert!(value["purchase"].get("codes").is_none());
}

#[tokio::test]
async fn purchase_is_omitted_when_composition_supplies_no_package_overlay() {
    let mut repository = Repo::valid();
    let detail = supplier_promise_detail(&readable(), &mut repository, "order", None, NOW)
        .await
        .unwrap();
    assert!(detail.can_record);
    let value = serde_json::to_value(&detail).unwrap();
    assert!(value.get("purchase").is_none());
}

#[tokio::test]
async fn missing_or_out_of_scope_order_is_the_retained_not_found_before_promise_reads() {
    let mut repository = Repo::valid();
    repository.order = None;
    let error = supplier_promise_detail(&readable(), &mut repository, "order", None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotFound);
    assert_eq!(error.code, "PURCHASE_ORDER_NOT_FOUND");
    assert_eq!(repository.order_calls, 1);
    assert_eq!(repository.promise_calls, 0);
    assert_eq!(repository.source_calls, 0);
}

#[tokio::test]
async fn can_record_intersects_owner_iam_state_and_purchase_settlement() {
    let mut repository = Repo::valid();
    let detail = supplier_promise_detail(
        &actor(&["commerce.read"], &["procurement"]),
        &mut repository,
        "order",
        Some(purchase(true)),
        NOW,
    )
    .await
    .unwrap();
    assert!(!detail.can_record, "commerce.purchase remains owner IAM");

    let mut repository = Repo::valid();
    repository.order = Some(order("fulfilled"));
    let detail = supplier_promise_detail(
        &readable(),
        &mut repository,
        "order",
        Some(purchase(true)),
        NOW,
    )
    .await
    .unwrap();
    assert!(!detail.can_record, "fulfilled orders cannot be re-confirmed");

    let mut repository = Repo::valid();
    let detail = supplier_promise_detail(
        &readable(),
        &mut repository,
        "order",
        Some(purchase(false)),
        NOW,
    )
    .await
    .unwrap();
    assert!(!detail.can_record, "settleExisting remains composition-owned");
    assert_eq!(detail.purchase.as_ref().unwrap().settle_existing, false);
}

#[tokio::test]
async fn unconfirmed_quantities_never_go_negative() {
    let mut repository = Repo::valid();
    repository.open[0].remaining_milli = 4_000;
    repository.source[0].fulfilled_milli = 0;
    repository.open[0].fulfilled_milli = 0;
    repository.open[0].fulfilled_snapshot_milli = 0;
    repository.schedule[0].fulfilled_snapshot_milli = 0;
    let detail = supplier_promise_detail(&readable(), &mut repository, "order", None, NOW)
        .await
        .unwrap();
    assert_eq!(detail.lines[0].outstanding_milli, 10_000);
    assert_eq!(detail.lines[0].unconfirmed_milli, 6_000);
}

#[tokio::test]
async fn malformed_order_or_source_lines_fail_closed() {
    let mut foreign = Repo::valid();
    foreign.order.as_mut().unwrap().company_id = "foreign".into();
    let error = supplier_promise_detail(&readable(), &mut foreign, "order", None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut sales = Repo::valid();
    sales.order.as_mut().unwrap().kind = "sales".into();
    let error = supplier_promise_detail(&readable(), &mut sales, "order", None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut state = Repo::valid();
    state.order.as_mut().unwrap().state = "posted".into();
    let error = supplier_promise_detail(&readable(), &mut state, "order", None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut duplicate = Repo::valid();
    duplicate.source[1].id = duplicate.source[0].id.clone();
    let error = supplier_promise_detail(&readable(), &mut duplicate, "order", None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut over = Repo::valid();
    over.source[0].fulfilled_milli = over.source[0].quantity_milli + 1;
    let error = supplier_promise_detail(&readable(), &mut over, "order", None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut foreign_line = Repo::valid();
    foreign_line.source[0].order_id = "other-order".into();
    let error = supplier_promise_detail(&readable(), &mut foreign_line, "order", None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SUPPLIER_PROMISE_HISTORY_INVALID");
}

#[tokio::test]
async fn detail_read_policy_precedes_every_repository_read() {
    let mut repository = Repo::valid();
    let error = supplier_promise_detail(
        &actor(&[], &["procurement"]),
        &mut repository,
        "order",
        None,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    assert_eq!(repository.order_calls, 0);

    let mut repository = Repo::valid();
    let error = supplier_promise_detail(&readable(), &mut repository, "", None, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "PURCHASE_ORDER_NOT_FOUND");
    assert_eq!(repository.order_calls, 0);
}
