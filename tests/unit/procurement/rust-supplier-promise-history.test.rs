//! Native Procurement supplier promise history policy and row tests.
use erpx_server_procurement::public::supplier_promise_history::{
    OpenPromiseRow, SupplierPromiseHistoryFuture, SupplierPromiseHistoryRepository,
    SupplierPromiseLineRow, SupplierPromiseOrderRow, SupplierPromiseReadActor, SupplierPromiseRow,
    SupplierPromiseSourceLineRow, supplier_promise_history,
};
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};

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
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: modules.iter().map(|value| (*value).to_owned()).collect(),
        expires_at: NOW.get() + 60_000,
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
        created_at: 10,
    }
}

fn line(
    id: &str,
    promise: &str,
    order_line: &str,
    ordinal: i64,
    quantity: i64,
    snapshot: i64,
    date: &str,
) -> SupplierPromiseLineRow {
    SupplierPromiseLineRow {
        id: id.into(),
        promise_id: promise.into(),
        order_line_id: order_line.into(),
        ordinal,
        quantity_milli: quantity,
        fulfilled_snapshot_milli: snapshot,
        promised_date: date.into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn open(
    id: &str,
    promise: &str,
    order_line: &str,
    ordinal: i64,
    quantity: i64,
    snapshot: i64,
    date: &str,
    prior: i64,
    remaining: i64,
) -> OpenPromiseRow {
    OpenPromiseRow {
        id: id.into(),
        promise_id: promise.into(),
        order_id: "order".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: None,
        order_number: "PO-1".into(),
        order_state: "confirmed".into(),
        order_line_id: order_line.into(),
        ordinal,
        quantity_milli: quantity,
        fulfilled_snapshot_milli: snapshot,
        promised_date: date.into(),
        product_id: "product".into(),
        description: "Bounded part".into(),
        fulfilled_milli: 4_000,
        prior_quantity: prior,
        remaining_milli: remaining,
    }
}

fn history() -> Vec<SupplierPromiseRow> {
    vec![
        header("promise-2", 2, "confirmed", "2026-11-01"),
        header("promise-1", 1, "withdrawn", "2026-10-30"),
    ]
}

fn lines() -> Vec<SupplierPromiseLineRow> {
    vec![
        line(
            "line-a1",
            "promise-2",
            "order-a",
            0,
            4_000,
            4_000,
            "2026-11-08",
        ),
        line(
            "line-a2",
            "promise-2",
            "order-a",
            1,
            2_000,
            4_000,
            "2026-11-09",
        ),
        line("line-b1", "promise-2", "order-b", 2, 5_000, 0, "2026-11-09"),
    ]
}

fn open_rows() -> Vec<OpenPromiseRow> {
    vec![
        open(
            "line-a1",
            "promise-2",
            "order-a",
            0,
            4_000,
            4_000,
            "2026-11-08",
            0,
            4_000,
        ),
        open(
            "line-a2",
            "promise-2",
            "order-a",
            1,
            2_000,
            4_000,
            "2026-11-09",
            4_000,
            2_000,
        ),
        open(
            "line-b1",
            "promise-2",
            "order-b",
            2,
            5_000,
            0,
            "2026-11-09",
            0,
            5_000,
        ),
    ]
}

struct Repo {
    history: Vec<SupplierPromiseRow>,
    lines: Vec<SupplierPromiseLineRow>,
    open: Vec<OpenPromiseRow>,
    calls: usize,
}

impl Repo {
    fn valid() -> Self {
        Self {
            history: history(),
            lines: lines(),
            open: open_rows(),
            calls: 0,
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
        Box::pin(async { Ok(None) })
    }

    fn source_lines<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseSourceLineRow>, Self::Error> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn history<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseRow>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.history.clone()) })
    }

    fn lines<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<SupplierPromiseLineRow>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.lines.clone()) })
    }

    fn open<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> SupplierPromiseHistoryFuture<'a, Vec<OpenPromiseRow>, Self::Error> {
        self.calls += 1;
        Box::pin(async move { Ok(self.open.clone()) })
    }
}

#[tokio::test]
async fn policy_requires_permission_module_and_current_context() {
    let mut denied = Repo::valid();
    let error = supplier_promise_history(&actor(&[], &["procurement"]), &mut denied, "order", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    assert_eq!(denied.calls, 0);

    let mut module = Repo::valid();
    let error = supplier_promise_history(
        &actor(&["commerce.read"], &["finance"]),
        &mut module,
        "order",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "MODULE_FORBIDDEN");
    assert_eq!(module.calls, 0);

    let mut expired = actor(&["commerce.read"], &["procurement"]);
    expired.expires_at = NOW.get();
    let mut repository = Repo::valid();
    let error = supplier_promise_history(&expired, &mut repository, "order", NOW)
        .await
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(repository.calls, 0);
}

#[tokio::test]
async fn read_returns_company_scoped_history_lines_and_open_deliveries() {
    let mut repository = Repo::valid();
    let result = supplier_promise_history(
        &actor(&["commerce.read"], &["procurement"]),
        &mut repository,
        "order",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.order_id, "order");
    assert_eq!(result.history.len(), 2);
    assert_eq!(result.history[0].id, "promise-2");
    assert_eq!(result.history[1].kind, "withdrawn");
    assert_eq!(result.lines.len(), 3);
    assert_eq!(result.open.len(), 3);
    assert_eq!(result.open[0].remaining_milli, 4_000);
    assert_eq!(result.open[1].remaining_milli, 2_000);
    assert_eq!(result.open[2].remaining_milli, 5_000);
    assert_eq!(repository.calls, 3);
}

#[tokio::test]
async fn empty_history_is_valid_for_an_order_without_confirmations() {
    let mut repository = Repo {
        history: Vec::new(),
        lines: Vec::new(),
        open: Vec::new(),
        calls: 0,
    };
    let result = supplier_promise_history(
        &actor(&["commerce.read"], &["procurement"]),
        &mut repository,
        "order",
        NOW,
    )
    .await
    .unwrap();
    assert!(result.history.is_empty());
    assert!(result.lines.is_empty());
    assert!(result.open.is_empty());
}

fn invalid(repository: Repo) -> BusinessError {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async move {
        let mut repository = repository;
        supplier_promise_history(
            &actor(&["commerce.read"], &["procurement"]),
            &mut repository,
            "order",
            NOW,
        )
        .await
        .unwrap_err()
    })
}

#[test]
fn foreign_company_or_duplicate_sources_fail_closed() {
    let mut foreign = Repo::valid();
    foreign.history[0].company_id = "foreign".into();
    assert_eq!(invalid(foreign).code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut duplicate = Repo::valid();
    duplicate.history[1].id = duplicate.history[0].id.clone();
    assert_eq!(invalid(duplicate).code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut duplicate_line = Repo::valid();
    duplicate_line.lines[1].id = duplicate_line.lines[0].id.clone();
    assert_eq!(
        invalid(duplicate_line).code,
        "SUPPLIER_PROMISE_HISTORY_INVALID"
    );
}

#[test]
fn malformed_dates_quantities_and_open_rows_fail_closed() {
    let mut date = Repo::valid();
    date.lines[0].promised_date = "2026-10-31".into();
    assert_eq!(invalid(date).code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut bad_format = Repo::valid();
    bad_format.history[0].confirmation_date = "2026-13-01".into();
    assert_eq!(invalid(bad_format).code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut impossible_header = Repo::valid();
    impossible_header.history[0].confirmation_date = "2026-02-29".into();
    assert_eq!(invalid(impossible_header).code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut impossible_line = Repo::valid();
    impossible_line.lines[0].promised_date = "2026-11-31".into();
    assert_eq!(invalid(impossible_line).code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut zero = Repo::valid();
    zero.lines[0].quantity_milli = 0;
    assert_eq!(invalid(zero).code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut unknown_promise = Repo::valid();
    unknown_promise.lines[0].promise_id = "missing".into();
    assert_eq!(
        invalid(unknown_promise).code,
        "SUPPLIER_PROMISE_HISTORY_INVALID"
    );

    let mut unknown_line = Repo::valid();
    unknown_line.open[0].id = "missing".into();
    assert_eq!(
        invalid(unknown_line).code,
        "SUPPLIER_PROMISE_HISTORY_INVALID"
    );

    let mut overstated_open = Repo::valid();
    overstated_open.open[0].remaining_milli = 9_999;
    assert_eq!(
        invalid(overstated_open).code,
        "SUPPLIER_PROMISE_HISTORY_INVALID"
    );

    let mut mismatch = Repo::valid();
    mismatch.open[1].order_line_id = "order-b".into();
    assert_eq!(invalid(mismatch).code, "SUPPLIER_PROMISE_HISTORY_INVALID");

    let mut withdrawn_open = Repo::valid();
    withdrawn_open.open[0].promise_id = "promise-1".into();
    assert_eq!(
        invalid(withdrawn_open).code,
        "SUPPLIER_PROMISE_HISTORY_INVALID"
    );
}

#[test]
fn overlong_utf16_supplier_history_fails_closed() {
    let mut repository = Repo::valid();
    repository.history[0].supplier_reference = format!("{}x", "😀".repeat(80));
    assert_eq!(invalid(repository).code, "SUPPLIER_PROMISE_HISTORY_INVALID");
    let mut repository = Repo::valid();
    repository.history[0].evidence = format!("{}x", "😀".repeat(1000));
    assert_eq!(invalid(repository).code, "SUPPLIER_PROMISE_HISTORY_INVALID");
}

#[tokio::test]
async fn supplier_history_uses_javascript_trim_whitespace() {
    let mut repository = Repo::valid();
    repository.history[0].supplier_reference = "\u{0085}supplier".into();
    supplier_promise_history(&actor(&["commerce.read"], &["procurement"]), &mut repository, "order", NOW)
        .await.expect("JavaScript trim preserves U+0085");

    let mut repository = Repo::valid();
    repository.history[0].supplier_reference = "\u{feff}supplier".into();
    assert_eq!(supplier_promise_history(&actor(&["commerce.read"], &["procurement"]), &mut repository, "order", NOW)
        .await.unwrap_err().code, "SUPPLIER_PROMISE_HISTORY_INVALID");
}

#[test]
fn oversized_history_fails_before_presentation() {
    let mut repository = Repo::valid();
    repository.history = (0..1_001)
        .map(|index| {
            header(
                &format!("promise-{index}"),
                index + 1,
                "withdrawn",
                "2026-11-01",
            )
        })
        .collect();
    repository.lines.clear();
    repository.open.clear();
    assert_eq!(
        invalid(repository).code,
        "SUPPLIER_PROMISE_HISTORY_TOO_LARGE"
    );
}

#[test]
fn serialization_uses_the_exact_bounded_owner_keys() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(async {
        let mut repository = Repo::valid();
        supplier_promise_history(
            &actor(&["commerce.read"], &["procurement"]),
            &mut repository,
            "order",
            NOW,
        )
        .await
        .unwrap()
    });
    let value = serde_json::to_value(&result).unwrap();
    let mut keys = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(keys, vec!["history", "lines", "open", "order_id"]);
    let history = value["history"][0].as_object().unwrap();
    assert_eq!(history.len(), 13);
    assert!(history.contains_key("supplier_reference"));
    assert!(history.contains_key("order_version"));
    let line = value["lines"][0].as_object().unwrap();
    assert_eq!(line.len(), 7);
    assert!(line.contains_key("fulfilled_snapshot_milli"));
    let open = value["open"][0].as_object().unwrap();
    assert_eq!(open.len(), 18);
    assert!(open.contains_key("remaining_milli"));
    assert!(open.contains_key("order_state"));
}
