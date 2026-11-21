//! Native POS workspace/day read policy, source validation and bounded payload.
use erpx_server_pos::public::read::{
    MAX_CASH_MOVEMENTS, MAX_REGISTERS, MAX_SALES, MAX_SHIFTS, POS_PERMISSION_NAMES,
    PosCashMovementRow, PosCashSummary, PosDayCloseRow, PosLineRow, PosPaymentRow, PosReadActor,
    PosReadFuture, PosRegisterRow, PosSaleRow, PosSettingsRow, PosShiftRow,
    PosWorkspaceEntitlements, PosWorkspaceQuery, PosWorkspaceRepository, PosWorkspaceSource,
    PosWorkspaceTab, WORKSPACE_KEYS, business_date, pos_workspace,
};
use erpx_server_runtime::public::http_support::serde_json;
use erpx_server_shared::public::{
    BusinessError, CompanyId, FailureKind, RecordId, TimestampMillis,
};
use std::collections::BTreeSet;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const BUSINESS_DATE: &str = "2027-01-15";

fn actor() -> PosReadActor {
    PosReadActor {
        user_id: "cashier".into(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        permissions: vec!["pos.read".into(), "pos.sell".into(), "pos.reports".into()],
        modules: vec!["pos".into()],
        expires_at: NOW.get() + 60_000,
    }
}

fn register() -> PosRegisterRow {
    PosRegisterRow {
        id: "register".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        warehouse_id: "warehouse".into(),
        name: "Retail".into(),
        mode: "retail".into(),
        active: true,
        version: 1,
        warehouse_name: "Warehouse".into(),
    }
}

fn settings() -> PosSettingsRow {
    PosSettingsRow {
        enabled: true,
        currency: "THB".into(),
        timezone: "Asia/Bangkok".into(),
        cashier_discount_bps: 500,
        version: 3,
    }
}

fn shift() -> PosShiftRow {
    PosShiftRow {
        id: "shift".into(),
        register_id: "register".into(),
        register_name: "Retail".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        business_date: BUSINESS_DATE.into(),
        opened_by: "cashier".into(),
        opened_at: 1_700_000_000_000,
        opening_minor: 100_000,
        state: "open".into(),
        counted_minor: None,
        expected_minor: None,
        variance_minor: None,
        closed_by: None,
        closed_at: None,
        version: 2,
        cash_expected_minor: 150_000,
    }
}

fn movement() -> PosCashMovementRow {
    PosCashMovementRow {
        id: "movement".into(),
        shift_id: "shift".into(),
        register_id: "register".into(),
        amount_minor: 50_000,
        reason: "Opening float top-up".into(),
        created_by: "cashier".into(),
        approved_by: None,
        created_at: 1_700_000_100_000,
    }
}

fn sale() -> PosSaleRow {
    PosSaleRow {
        id: "sale".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        register_id: "register".into(),
        shift_id: "shift".into(),
        number: "POS-0001".into(),
        business_date: BUSINESS_DATE.into(),
        state: "completed".into(),
        currency: "THB".into(),
        party_id: None,
        subtotal_minor: 100_000,
        tax_minor: 7_000,
        discount_minor: 0,
        total_minor: 107_000,
        created_by: "cashier".into(),
        created_at: 1_700_000_200_000,
        version: 4,
    }
}

fn line() -> PosLineRow {
    PosLineRow {
        id: "line".into(),
        sale_id: "sale".into(),
        fulfillment_id: "fulfillment".into(),
        fulfillment_state: "delivered".into(),
        product_id: "product".into(),
        name: "Coffee".into(),
        quantity_milli: 2_000,
        price_minor: 50_000,
        discount_minor: 0,
        subtotal_minor: 100_000,
        tax_minor: 7_000,
        returned_milli: 0,
        refunded_minor: 0,
    }
}

fn payment() -> PosPaymentRow {
    PosPaymentRow {
        id: "payment".into(),
        sale_id: "sale".into(),
        method: "cash".into(),
        state: "succeeded".into(),
        amount_minor: 107_000,
        tendered_minor: 120_000,
        reference: String::new(),
        created_by: "cashier".into(),
        created_at: 1_700_000_300_000,
        version: 1,
        provider_confirmed: false,
    }
}

fn day_close() -> PosDayCloseRow {
    PosDayCloseRow {
        id: "day".into(),
        state: "prepared".into(),
        prepared_by: "cashier".into(),
        prepared_at: 1_700_000_400_000,
        confirmed_by: None,
        confirmed_at: None,
        version: 1,
    }
}

fn source() -> PosWorkspaceSource {
    PosWorkspaceSource {
        registers: vec![register()],
        settings: Some(settings()),
        shifts: vec![shift()],
        cash_movements: vec![movement()],
        cash_summary: PosCashSummary {
            pending_count: 1,
            pending_minor: 50_000,
            approved_count: 0,
            approved_minor: 0,
        },
        day_close: Some(day_close()),
        sales: vec![sale()],
        sale: Some(sale()),
        lines: vec![line()],
        payments: vec![payment()],
    }
}

fn query() -> PosWorkspaceQuery {
    PosWorkspaceQuery {
        sale_id: Some("sale".into()),
        ..PosWorkspaceQuery::default()
    }
}

#[derive(Default)]
struct Probe {
    result: Option<Result<PosWorkspaceSource, BusinessError>>,
    calls: usize,
}

impl PosWorkspaceRepository for Probe {
    type Error = BusinessError;

    fn workspace<'a>(
        &'a mut self,
        _: &'a PosReadActor,
        _: &'a PosWorkspaceQuery,
        _: &'a str,
    ) -> PosReadFuture<'a, PosWorkspaceSource, Self::Error> {
        self.calls += 1;
        let result = self.result.take().unwrap_or_else(|| Ok(source()));
        Box::pin(async move { result })
    }
}

fn keys(value: &serde_json::Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("workspace object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn policy_denies_before_any_owner_read() {
    sqlx::test_block_on(async {
        let mut probe = Probe::default();
        for (current, kind, code) in [
            (
                PosReadActor {
                    permissions: vec!["pos.sell".into()],
                    ..actor()
                },
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
            ),
            (
                PosReadActor {
                    modules: vec!["finance".into()],
                    ..actor()
                },
                FailureKind::Forbidden,
                "POS_MODULE_FORBIDDEN",
            ),
            (
                PosReadActor {
                    expires_at: NOW.get(),
                    ..actor()
                },
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
            ),
            (
                PosReadActor {
                    department_id: Some(RecordId::new("department").unwrap()),
                    ..actor()
                },
                FailureKind::Forbidden,
                "POS_BRANCH_REQUIRED",
            ),
        ] {
            let error = pos_workspace(
                &current,
                &query(),
                &PosWorkspaceEntitlements::default(),
                &mut probe,
                NOW,
            )
            .await
            .unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.code, code);
        }
        assert_eq!(probe.calls, 0);
    });
}

#[test]
fn query_and_package_deny_before_any_owner_read() {
    sqlx::test_block_on(async {
        let mut probe = Probe::default();
        for (bounded, code) in [
            (
                PosWorkspaceQuery {
                    search: "x".repeat(101),
                    ..PosWorkspaceQuery::default()
                },
                "POS_REQUEST_INVALID",
            ),
            (
                PosWorkspaceQuery {
                    sale_id: Some("bad\u{0000}sale".into()),
                    ..PosWorkspaceQuery::default()
                },
                "POS_REQUEST_INVALID",
            ),
            (
                PosWorkspaceQuery {
                    limit: 0,
                    ..PosWorkspaceQuery::default()
                },
                "POS_LIMIT_INVALID",
            ),
            (
                PosWorkspaceQuery {
                    limit: MAX_SALES + 1,
                    ..PosWorkspaceQuery::default()
                },
                "POS_LIMIT_INVALID",
            ),
        ] {
            let error = pos_workspace(
                &actor(),
                &bounded,
                &PosWorkspaceEntitlements::default(),
                &mut probe,
                NOW,
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, code);
        }
        let denied = PosWorkspaceEntitlements {
            package_enforced: true,
            sales_history: false,
            kitchen_history: true,
            shifts_history: true,
            room_charge_enabled: false,
            purchases: None,
        };
        let error = pos_workspace(
            &actor(),
            &PosWorkspaceQuery::default(),
            &denied,
            &mut probe,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, FailureKind::Forbidden);
        assert_eq!(error.code, "PACKAGE_REQUIRED");
        assert_eq!(probe.calls, 0);

        // The retained wrapper selected the capability from the tab.
        let shifts = PosWorkspaceQuery {
            tab: Some(PosWorkspaceTab::Shifts),
            ..PosWorkspaceQuery::default()
        };
        let allowed = PosWorkspaceEntitlements {
            package_enforced: true,
            sales_history: false,
            kitchen_history: false,
            shifts_history: true,
            room_charge_enabled: false,
            purchases: Some(serde_json::json!({"pos.shifts": {"history": true}})),
        };
        probe.result = Some(Ok(PosWorkspaceSource {
            sale: None,
            lines: Vec::new(),
            payments: Vec::new(),
            ..source()
        }));
        let workspace = pos_workspace(&actor(), &shifts, &allowed, &mut probe, NOW)
            .await
            .unwrap();
        assert_eq!(probe.calls, 1);
        assert_eq!(
            workspace.entitlements.purchases(),
            serde_json::json!({"pos.shifts": {"history": true}})
        );
    });
}

#[test]
fn workspace_serializes_the_exact_retained_fields() {
    sqlx::test_block_on(async {
        let mut probe = Probe::default();
        let workspace = pos_workspace(
            &actor(),
            &query(),
            &PosWorkspaceEntitlements::default(),
            &mut probe,
            NOW,
        )
        .await
        .unwrap();
        let value = workspace.to_wire();
        assert_eq!(
            keys(&value),
            WORKSPACE_KEYS.iter().map(|key| (*key).to_owned()).collect()
        );
        assert_eq!(value["businessDate"], BUSINESS_DATE);
        assert_eq!(value["day"]["businessDate"], BUSINESS_DATE);
        assert_eq!(value["day"]["state"], "prepared");
        assert_eq!(value["day"]["close"]["prepared_by"], "cashier");
        assert_eq!(
            value["day"]["close"]["confirmed_by"],
            serde_json::Value::Null
        );
        assert_eq!(value["registers"][0]["warehouse_name"], "Warehouse");
        assert_eq!(value["registers"][0]["mode"], "retail");
        assert_eq!(value["settings"]["currency"], "THB");
        assert_eq!(value["settings"]["cashier_discount_bps"], 500);
        assert_eq!(value["shifts"][0]["cash_expected_minor"], 150_000);
        assert_eq!(value["shifts"][0]["counted_minor"], serde_json::Value::Null);
        assert_eq!(value["cashMovements"][0]["amount_minor"], 50_000);
        assert_eq!(
            value["cashMovements"][0]["approved_by"],
            serde_json::Value::Null
        );
        assert_eq!(value["cashSummary"]["pendingCount"], 1);
        assert_eq!(value["cashSummary"]["pendingMinor"], 50_000);
        assert_eq!(value["cashSummary"]["approvedCount"], 0);
        assert_eq!(value["sales"][0]["number"], "POS-0001");
        assert_eq!(value["sale"]["total_minor"], 107_000);
        assert_eq!(value["lines"][0]["fulfillment_state"], "delivered");
        assert_eq!(value["payments"][0]["provider_payment_confirmed"], false);
        assert_eq!(value["payments"][0]["tendered_minor"], 120_000);
        assert_eq!(value["userId"], "cashier");
        assert_eq!(value["companyId"], "company");
        assert_eq!(value["branchId"], "branch");
        assert_eq!(value["entitlements"]["packageEnforced"], false);
        assert_eq!(value["entitlements"]["roomChargeEnabled"], true);
        assert_eq!(value["purchases"], serde_json::Value::Null);

        let permissions = value["permissions"].as_object().unwrap();
        assert_eq!(permissions.len(), POS_PERMISSION_NAMES.len());
        assert_eq!(permissions["pos.read"], true);
        assert_eq!(permissions["finance.write"], false);
        assert_eq!(probe.calls, 1);
    });
}

#[test]
fn closed_day_and_missing_sale_state_are_reported() {
    sqlx::test_block_on(async {
        let mut closed = day_close();
        closed.state = "confirmed".into();
        closed.confirmed_by = Some("reviewer".into());
        closed.confirmed_at = Some(1_700_000_500_000);
        let mut probe = Probe {
            result: Some(Ok(PosWorkspaceSource {
                day_close: Some(closed),
                ..source()
            })),
            ..Probe::default()
        };
        let workspace = pos_workspace(
            &actor(),
            &query(),
            &PosWorkspaceEntitlements::default(),
            &mut probe,
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(workspace.day.state, "confirmed");
        assert_eq!(
            workspace
                .day
                .close
                .as_ref()
                .unwrap()
                .confirmed_by
                .as_deref(),
            Some("reviewer")
        );
    });
}

#[test]
fn selected_sale_outside_scope_is_not_found() {
    sqlx::test_block_on(async {
        let mut foreign = sale();
        foreign.company_id = "foreign".into();
        let mut probe = Probe {
            result: Some(Ok(PosWorkspaceSource {
                sale: Some(foreign),
                ..source()
            })),
            ..Probe::default()
        };
        let error = pos_workspace(
            &actor(),
            &query(),
            &PosWorkspaceEntitlements::default(),
            &mut probe,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, FailureKind::NotFound);
        assert_eq!(error.code, "POS_NOT_FOUND");

        let mut probe = Probe {
            result: Some(Ok(PosWorkspaceSource {
                sale: None,
                ..source()
            })),
            ..Probe::default()
        };
        let error = pos_workspace(
            &actor(),
            &query(),
            &PosWorkspaceEntitlements::default(),
            &mut probe,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "POS_NOT_FOUND");
    });
}

#[test]
fn company_branch_and_orphan_rows_fail_closed() {
    let assemble = |source: PosWorkspaceSource| {
        erpx_server_pos::public::read::assemble(
            &actor(),
            &query(),
            &PosWorkspaceEntitlements::default(),
            BUSINESS_DATE,
            source,
        )
    };
    let mutations: [fn(&mut PosWorkspaceSource); 10] = [
        |value| value.registers[0].company_id = "foreign".into(),
        |value| value.registers[0].branch_id = "branch-b".into(),
        |value| value.shifts[0].company_id = "foreign".into(),
        |value| value.shifts[0].register_id = "missing".into(),
        |value| value.cash_movements[0].shift_id = "missing".into(),
        |value| value.sales[0].branch_id = "branch-b".into(),
        |value| value.sales[0].register_id = "missing".into(),
        |value| value.lines[0].sale_id = "other".into(),
        |value| value.payments[0].sale_id = "other".into(),
        |value| value.registers.push(value.registers[0].clone()),
    ];
    for mutate in mutations {
        let mut value = source();
        mutate(&mut value);
        assert_eq!(
            assemble(value).unwrap_err().code,
            "POS_SOURCE_INVALID",
            "mutation must fail closed"
        );
    }

    let mut detail_without_selection = source();
    detail_without_selection.sale = None;
    assert_eq!(
        erpx_server_pos::public::read::assemble(
            &actor(),
            &PosWorkspaceQuery::default(),
            &PosWorkspaceEntitlements::default(),
            BUSINESS_DATE,
            detail_without_selection,
        )
        .unwrap_err()
        .code,
        "POS_SOURCE_INVALID"
    );
}

#[test]
fn corrupt_amounts_references_and_dates_fail_closed() {
    let assemble = |source: PosWorkspaceSource| {
        erpx_server_pos::public::read::assemble(
            &actor(),
            &query(),
            &PosWorkspaceEntitlements::default(),
            BUSINESS_DATE,
            source,
        )
    };
    let mutations: [fn(&mut PosWorkspaceSource); 12] = [
        |value| value.settings.as_mut().unwrap().currency = "thb".into(),
        |value| value.settings.as_mut().unwrap().timezone = "UTC".into(),
        |value| value.settings.as_mut().unwrap().cashier_discount_bps = 10_001,
        |value| value.shifts[0].business_date = "2027-1-5".into(),
        |value| value.shifts[0].state = "pending".into(),
        |value| value.cash_movements[0].amount_minor = 0,
        |value| value.cash_movements[0].approved_by = Some(String::new()),
        |value| value.sales[0].state = "draft".into(),
        |value| value.sales[0].total_minor = -1,
        |value| value.lines[0].returned_milli = value.lines[0].quantity_milli + 1,
        |value| value.payments[0].tendered_minor = 1,
        |value| value.payments[0].method = "cheque".into(),
    ];
    for mutate in mutations {
        let mut value = source();
        mutate(&mut value);
        assert_eq!(
            assemble(value).unwrap_err().code,
            "POS_SOURCE_INVALID",
            "corrupt fact must fail closed"
        );
    }

    let mut day = source();
    day.day_close.as_mut().unwrap().confirmed_by = Some("cashier".into());
    day.day_close.as_mut().unwrap().confirmed_at = Some(1);
    assert_eq!(
        assemble(day).unwrap_err().code,
        "POS_SOURCE_INVALID",
        "the day close must be confirmed by a different reviewer"
    );
}

#[test]
fn source_pages_are_bounded() {
    let assemble = |source: PosWorkspaceSource| {
        erpx_server_pos::public::read::assemble(
            &actor(),
            &query(),
            &PosWorkspaceEntitlements::default(),
            BUSINESS_DATE,
            source,
        )
    };
    let mut registers = source();
    registers.registers = vec![register(); MAX_REGISTERS + 1];
    assert_eq!(assemble(registers).unwrap_err().code, "POS_WORKSPACE_LIMIT");

    let mut shifts = source();
    shifts.shifts = vec![shift(); MAX_SHIFTS + 1];
    assert_eq!(assemble(shifts).unwrap_err().code, "POS_WORKSPACE_LIMIT");

    let mut cash = source();
    cash.cash_movements = vec![movement(); MAX_CASH_MOVEMENTS + 1];
    assert_eq!(assemble(cash).unwrap_err().code, "POS_WORKSPACE_LIMIT");

    let mut sales = source();
    sales.sales = (0..=MAX_SALES).map(|_| sale()).collect();
    assert_eq!(assemble(sales).unwrap_err().code, "POS_WORKSPACE_LIMIT");

    let mut small_page = source();
    small_page.sales = vec![sale(), sale()];
    let bounded = PosWorkspaceQuery {
        limit: 1,
        ..query()
    };
    assert_eq!(
        erpx_server_pos::public::read::assemble(
            &actor(),
            &bounded,
            &PosWorkspaceEntitlements::default(),
            BUSINESS_DATE,
            small_page,
        )
        .unwrap_err()
        .code,
        "POS_WORKSPACE_LIMIT"
    );
}

#[test]
fn repository_failures_propagate_without_partial_presentation() {
    sqlx::test_block_on(async {
        let mut probe = Probe {
            result: Some(Err(BusinessError::new(
                FailureKind::Unavailable,
                "POS_SOURCE_UNAVAILABLE",
                "The POS source is unavailable.",
            ))),
            ..Probe::default()
        };
        let error = pos_workspace(
            &actor(),
            &PosWorkspaceQuery::default(),
            &PosWorkspaceEntitlements::default(),
            &mut probe,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "POS_SOURCE_UNAVAILABLE");
        assert_eq!(probe.calls, 1);
    });
}

#[test]
fn tab_selection_matches_the_retained_wants_gates() {
    assert!(PosWorkspaceQuery::default().reads_sales());
    assert!(PosWorkspaceQuery::default().reads_shifts());
    assert!(PosWorkspaceQuery::default().reads_cash());
    let sale = PosWorkspaceQuery {
        tab: Some(PosWorkspaceTab::Sale),
        ..PosWorkspaceQuery::default()
    };
    assert!(sale.reads_sales());
    assert!(sale.reads_shifts());
    assert!(!sale.reads_cash());
    let shifts = PosWorkspaceQuery {
        tab: Some(PosWorkspaceTab::Shifts),
        ..PosWorkspaceQuery::default()
    };
    assert!(!shifts.reads_sales());
    assert!(shifts.reads_shifts());
    assert!(shifts.reads_cash());
    let reports = PosWorkspaceQuery {
        tab: Some(PosWorkspaceTab::Reports),
        ..PosWorkspaceQuery::default()
    };
    assert!(!reports.reads_sales());
    assert!(!reports.reads_shifts());
    assert!(!reports.reads_cash());
    assert_eq!(
        PosWorkspaceTab::parse("kitchen"),
        Some(PosWorkspaceTab::Kitchen)
    );
    assert_eq!(PosWorkspaceTab::parse("unknown"), None);
}

#[test]
fn business_date_tracks_the_bangkok_calendar() {
    assert_eq!(business_date(TimestampMillis::new(0)), "1970-01-01");
    // One millisecond before the epoch is 1969-12-31T23:59:59.999Z, which is
    // 1970-01-01T06:59:59.999 in Asia/Bangkok.
    assert_eq!(business_date(TimestampMillis::new(-1)), "1970-01-01");
    assert_eq!(business_date(NOW), BUSINESS_DATE);
    // 2027-01-15T17:00:00Z is 2027-01-16T00:00:00 in Asia/Bangkok (UTC+7).
    assert_eq!(
        business_date(TimestampMillis::new(1_800_032_400_000)),
        "2027-01-16"
    );
    assert_eq!(
        business_date(TimestampMillis::new(1_800_032_399_999)),
        BUSINESS_DATE
    );
}

#[test]
fn workspace_payload_is_bounded_to_known_keys() {
    let workspace = erpx_server_pos::public::read::assemble(
        &actor(),
        &query(),
        &PosWorkspaceEntitlements::default(),
        BUSINESS_DATE,
        source(),
    )
    .unwrap();
    assert_eq!(workspace.permissions.len(), POS_PERMISSION_NAMES.len());
    assert_eq!(workspace.day.state, "prepared");
    assert_eq!(workspace.to_wire()["sales"].as_array().unwrap().len(), 1);
}
