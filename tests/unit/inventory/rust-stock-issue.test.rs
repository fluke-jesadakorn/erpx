//! Portable sales-delivery plan guards. These cases do not prove IAM, purchase,
//! Commerce fulfillment persistence, reservation writes or HTTP behavior.
use erpx_server_inventory::public::stock_issue::*;

fn order(state: &str, kind: &str, version: i64) -> StockIssueOrder {
    StockIssueOrder {
        id: "order-1".into(),
        number: "SO-1".into(),
        kind: kind.into(),
        state: state.into(),
        version,
        warehouse_id: "warehouse".into(),
        branch_id: "branch".into(),
        department_id: None,
        order_date: "2026-11-01".into(),
        currency: "THB".into(),
        party_name: "Customer".into(),
    }
}

fn stock(quantity: i64, value: i64, reserved: i64, quarantined: i64) -> StockIssueStock {
    StockIssueStock {
        quantity_milli: quantity,
        value_minor: value,
        version: 3,
        reserved_milli: reserved,
        quarantined_milli: quarantined,
    }
}

fn lot(id: &str, tracking: &str, quantity: i64, value: i64) -> StockIssueLot {
    StockIssueLot {
        id: id.into(),
        lot_code: (tracking == "lot").then(|| format!("L-{id}")),
        serial_number: (tracking == "serial").then(|| format!("S-{id}")),
        status: "active".into(),
        quantity_milli: quantity,
        value_minor: value,
        version: 2,
        quality_hold_milli: 0,
        quality_status: None,
    }
}

fn reservation(id: &str, remaining: i64, active: bool) -> StockIssueReservation {
    StockIssueReservation {
        id: id.into(),
        order_line_id: "line-1".into(),
        quantity_milli: remaining + 500,
        consumed_milli: 100,
        released_milli: 400,
        expires_at: 9_007_199_254_740_991,
        active,
    }
}

fn line(
    tracking: &str,
    stock: Option<StockIssueStock>,
    lots: Vec<StockIssueLot>,
    reservations: Vec<StockIssueReservation>,
) -> StockIssueLine {
    StockIssueLine {
        id: "line-1".into(),
        product_id: "product".into(),
        sku: "SKU".into(),
        description: "Product".into(),
        unit: "pcs".into(),
        tracking_mode: tracking.into(),
        quantity_milli: 10_000,
        fulfilled_milli: 0,
        remaining_milli: 10_000,
        price_minor: 2_500,
        tax_bps: 0,
        ordinal: 1,
        stock,
        lots,
        lots_truncated: false,
        reservations,
        reservations_truncated: false,
    }
}

fn source(_tracking: &str, lines: Vec<StockIssueLine>) -> StockIssueSource {
    StockIssueSource {
        company_id: "company".into(),
        order: order("confirmed", "sales", 4),
        lines,
    }
}

fn input(quantity: i64, allocations: Vec<(&str, i64)>) -> StockIssueInput {
    StockIssueInput {
        expected_order_version: 4,
        date: "2026-11-04".into(),
        lines: vec![StockIssueLineInput {
            line_id: "line-1".into(),
            quantity_milli: quantity,
            allocations: allocations
                .into_iter()
                .map(|(lot_id, quantity_milli)| StockIssueAllocationInput {
                    lot_id: lot_id.into(),
                    quantity_milli,
                })
                .collect(),
        }],
    }
}

fn assert_code<T: std::fmt::Debug>(
    result: Result<T, erpx_server_shared::public::BusinessError>,
    code: &str,
) {
    match result {
        Err(error) => assert_eq!(error.code, code),
        Ok(value) => panic!("expected {code}, got {value:?}"),
    }
}

#[test]
fn issues_untracked_stock_at_current_proportional_value() {
    let plan = prepare_stock_issue(
        &source(
            "none",
            vec![line(
                "none",
                Some(stock(10_000, 5_000, 0, 0)),
                vec![],
                vec![],
            )],
        ),
        &input(4_000, vec![]),
    )
    .unwrap();
    assert_eq!(plan.total_value_minor, 2_000);
    assert_eq!(plan.lines[0].stock_value_minor, 2_000);
    assert_eq!(plan.lines[0].expected_stock_version, 3);
    assert!(plan.lines[0].allocations.is_empty());
    assert_eq!(plan.warehouse_id, "warehouse");
    assert_eq!(plan.branch_id, "branch");

    assert_code(
        prepare_stock_issue(
            &source(
                "none",
                vec![line(
                    "none",
                    Some(stock(10_000, 5_000, 0, 0)),
                    vec![],
                    vec![],
                )],
            ),
            &input(10_001, vec![]),
        ),
        "OVER_FULFILLMENT",
    );
    assert_code(
        prepare_stock_issue(
            &source(
                "none",
                vec![line(
                    "none",
                    Some(stock(10_000, 5_000, 0, 0)),
                    vec![],
                    vec![],
                )],
            ),
            &StockIssueInput {
                expected_order_version: 3,
                ..input(1_000, vec![])
            },
        ),
        "STOCK_ISSUE_SOURCE_CHANGED",
    );
    let mut purchase = source(
        "none",
        vec![line(
            "none",
            Some(stock(10_000, 5_000, 0, 0)),
            vec![],
            vec![],
        )],
    );
    purchase.order.kind = "purchase".into();
    assert_code(
        prepare_stock_issue(&purchase, &input(1_000, vec![])),
        "SALES_ORDER_REQUIRED",
    );
    let mut draft = source(
        "none",
        vec![line(
            "none",
            Some(stock(10_000, 5_000, 0, 0)),
            vec![],
            vec![],
        )],
    );
    draft.order.state = "draft".into();
    assert_code(
        prepare_stock_issue(&draft, &input(1_000, vec![])),
        "ORDER_NOT_ISSUABLE",
    );
}

#[test]
fn allocates_lot_values_and_requires_the_full_quantity() {
    let plan = prepare_stock_issue(
        &source(
            "lot",
            vec![line(
                "lot",
                Some(stock(2_000, 1_000, 0, 0)),
                vec![lot("a", "lot", 1_000, 333), lot("b", "lot", 1_000, 667)],
                vec![],
            )],
        ),
        &input(2_000, vec![("a", 1_000), ("b", 1_000)]),
    )
    .unwrap();
    assert_eq!(plan.total_value_minor, 1_000);
    assert_eq!(
        plan.lines[0]
            .allocations
            .iter()
            .map(|allocation| allocation.value_minor)
            .collect::<Vec<_>>(),
        vec![333, 667]
    );
    assert_eq!(plan.lines[0].allocations[0].expected_balance_version, 2);

    assert_code(
        prepare_stock_issue(
            &source(
                "lot",
                vec![line(
                    "lot",
                    Some(stock(2_000, 1_000, 0, 0)),
                    vec![lot("a", "lot", 1_000, 333), lot("b", "lot", 1_000, 667)],
                    vec![],
                )],
            ),
            &input(2_000, vec![("a", 1_000)]),
        ),
        "LOT_ALLOCATION_REQUIRED",
    );
    assert_code(
        prepare_stock_issue(
            &source(
                "lot",
                vec![line(
                    "lot",
                    Some(stock(2_000, 1_000, 0, 0)),
                    vec![lot("a", "lot", 1_000, 333), lot("b", "lot", 1_000, 667)],
                    vec![],
                )],
            ),
            &input(2_000, vec![("a", 1_000), ("a", 1_000)]),
        ),
        "LOT_ALLOCATION_DUPLICATE",
    );
    assert_code(
        prepare_stock_issue(
            &source(
                "serial",
                vec![line(
                    "serial",
                    Some(stock(1_000, 1_000, 0, 0)),
                    vec![lot("a", "serial", 1_000, 1_000)],
                    vec![],
                )],
            ),
            &input(1_000, vec![("a", 2_000)]),
        ),
        "SERIAL_QUANTITY_INVALID",
    );
}

#[test]
fn excludes_the_orders_own_reservation_but_not_other_holds() {
    let available = source(
        "none",
        vec![line(
            "none",
            Some(stock(10_000, 10_000, 4_000, 1_000)),
            vec![],
            vec![reservation("reservation-1", 2_000, true)],
        )],
    );
    // 10,000 - 8,000 = 2,000 remains and exactly covers the other 2,000
    // reserved quantity plus the 1,000 quarantined quantity.
    assert_eq!(
        prepare_stock_issue(&available, &input(8_000, vec![]))
            .unwrap()
            .total_value_minor,
        8_000
    );
    assert_code(
        prepare_stock_issue(&available, &input(8_001, vec![])),
        "INSUFFICIENT_STOCK",
    );

    let expired = source(
        "none",
        vec![line(
            "none",
            Some(stock(10_000, 10_000, 4_000, 1_000)),
            vec![],
            vec![reservation("reservation-1", 2_000, false)],
        )],
    );
    assert_code(
        prepare_stock_issue(&expired, &input(6_001, vec![])),
        "INSUFFICIENT_STOCK",
    );
    assert!(prepare_stock_issue(&expired, &input(6_000, vec![])).is_ok());

    let mut truncated = source(
        "none",
        vec![line(
            "none",
            Some(stock(10_000, 10_000, 0, 0)),
            vec![],
            vec![reservation("reservation-1", 2_000, true)],
        )],
    );
    truncated.lines[0].reservations_truncated = true;
    assert_code(
        prepare_stock_issue(&truncated, &input(1_000, vec![])),
        "STOCK_ISSUE_SOURCE_INVALID",
    );
}

#[test]
fn rejects_quarantined_or_depleted_lots() {
    let mut held = lot("a", "lot", 1_000, 1_000);
    held.quality_hold_milli = 1_000;
    held.quality_status = Some("held".into());
    assert_code(
        prepare_stock_issue(
            &source(
                "lot",
                vec![line(
                    "lot",
                    Some(stock(1_000, 1_000, 0, 0)),
                    vec![held],
                    vec![],
                )],
            ),
            &input(1_000, vec![("a", 1_000)]),
        ),
        "LOT_UNAVAILABLE",
    );
    assert_code(
        prepare_stock_issue(
            &source(
                "lot",
                vec![line(
                    "lot",
                    Some(stock(1_000, 1_000, 0, 1_000)),
                    vec![lot("a", "lot", 1_000, 1_000)],
                    vec![],
                )],
            ),
            &input(1_000, vec![("a", 1_000)]),
        ),
        "INSUFFICIENT_STOCK",
    );
    let mut depleted = lot("a", "lot", 1_000, 1_000);
    depleted.status = "depleted".into();
    assert_code(
        prepare_stock_issue(
            &source(
                "lot",
                vec![line(
                    "lot",
                    Some(stock(1_000, 1_000, 0, 0)),
                    vec![depleted],
                    vec![],
                )],
            ),
            &input(1_000, vec![("a", 1_000)]),
        ),
        "LOT_UNAVAILABLE",
    );
}

#[test]
fn rejects_a_tampered_plan_before_the_write_boundary() {
    let plan = prepare_stock_issue(
        &source(
            "lot",
            vec![line(
                "lot",
                Some(stock(2_000, 1_000, 0, 0)),
                vec![lot("a", "lot", 1_000, 333), lot("b", "lot", 1_000, 667)],
                vec![reservation("reservation-1", 2_000, true)],
            )],
        ),
        &input(2_000, vec![("a", 1_000), ("b", 1_000)]),
    )
    .unwrap();
    let mut tampered = plan.clone();
    tampered.total_value_minor += 1;
    assert_code(
        validate_stock_issue_plan(&tampered),
        "STOCK_ISSUE_VALUE_INVALID",
    );
    let mut tampered = plan.clone();
    tampered.lines[0].allocations[0].allocation_index = 1;
    assert_code(
        validate_stock_issue_plan(&tampered),
        "STOCK_ISSUE_PLAN_INVALID",
    );
    let mut tampered = plan.clone();
    tampered.lines[0].allocations[0].quantity_milli += 1;
    assert_code(
        validate_stock_issue_plan(&tampered),
        "LOT_ALLOCATION_REQUIRED",
    );
    let mut tampered = plan.clone();
    tampered.lines[0].allocations[0].value_minor += 1;
    assert_code(
        validate_stock_issue_plan(&tampered),
        "STOCK_ISSUE_VALUE_INVALID",
    );
    let mut tampered = plan.clone();
    tampered.lines[0].expected_stock_version = 0;
    assert_code(
        validate_stock_issue_plan(&tampered),
        "STOCK_ISSUE_PLAN_INVALID",
    );
    let mut tampered = plan.clone();
    tampered.lines[0].reservations[0].remaining_milli += 1;
    assert_code(
        validate_stock_issue_plan(&tampered),
        "STOCK_ISSUE_PLAN_INVALID",
    );
}
