//! Portable goods-receipt guards for the Commerce-owned purchase command.
//! These cases do not prove IAM, purchase, Quality or a PostgreSQL effect.
use erpx_server_inventory::public::stock_receipt::*;

fn source(tracking: &str, lots: Vec<StockReceiptLot>) -> StockReceiptSource {
    StockReceiptSource {
        company_id: "company".into(),
        order: StockReceiptOrder {
            id: "order-1".into(),
            number: "PO-1".into(),
            kind: "purchase".into(),
            state: "confirmed".into(),
            version: 3,
            warehouse_id: "warehouse".into(),
            branch_id: "branch".into(),
            department_id: None,
            order_date: "2026-11-01".into(),
            currency: "THB".into(),
            party_name: "Supplier".into(),
        },
        lines: vec![StockReceiptLine {
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
            stock: Some(StockReceiptStock {
                quantity_milli: 1_000,
                value_minor: 1_000,
                version: 2,
                reserved_milli: 0,
                quarantined_milli: 0,
            }),
            lots,
            lots_truncated: false,
        }],
    }
}

fn allocation(
    lot: Option<&str>,
    serial: Option<&str>,
    quantity: i64,
    disposition: ReceiptDisposition,
) -> StockReceiptAllocationInput {
    StockReceiptAllocationInput {
        lot_code: lot.map(Into::into),
        serial_number: serial.map(Into::into),
        quantity_milli: quantity,
        disposition,
        quarantine_reason: (disposition == ReceiptDisposition::Quarantine)
            .then(|| "Packaging inspection".into()),
        quarantine_due_date: (disposition == ReceiptDisposition::Quarantine)
            .then(|| "2026-11-10".into()),
        manufactured_date: None,
        expiry_date: None,
    }
}

fn input(
    quantity: i64,
    value: i64,
    allocations: Vec<StockReceiptAllocationInput>,
) -> StockReceiptInput {
    StockReceiptInput {
        expected_order_version: 3,
        date: "2026-11-04".into(),
        lines: vec![StockReceiptLineInput {
            line_id: "line-1".into(),
            quantity_milli: quantity,
            stock_value_minor: value,
            allocations,
        }],
    }
}

#[test]
fn prepares_a_mixed_available_and_quarantine_receipt() {
    let plan = prepare_stock_receipt(
        &source("lot", vec![]),
        &input(
            3_000,
            1_000,
            vec![
                allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                allocation(Some("L-B"), None, 1_000, ReceiptDisposition::Quarantine),
                allocation(Some("L-C"), None, 1_000, ReceiptDisposition::Available),
            ],
        ),
    )
    .unwrap();
    assert_eq!(plan.company_id, "company");
    assert_eq!(plan.warehouse_id, "warehouse");
    assert_eq!(plan.branch_id, "branch");
    assert_eq!(plan.total_value_minor, 1_000);
    assert_eq!(plan.lines[0].expected_stock_version, Some(2));
    assert_eq!(
        plan.lines[0]
            .allocations
            .iter()
            .map(|allocation| allocation.value_minor)
            .collect::<Vec<_>>(),
        vec![333, 333, 334]
    );
    let request = quality_request(
        &plan,
        &plan.lines[0],
        &plan.lines[0].allocations[1],
        "lot-allocation-1".into(),
        "lot-1".into(),
    )
    .unwrap();
    assert_eq!(request.lot_identity, "L-B");
    assert_eq!(request.reason, "Packaging inspection");
    assert_eq!(request.due_date, "2026-11-10");
}

#[test]
fn rejects_untracked_allocations_serial_quantities_and_duplicate_identities() {
    assert_eq!(
        prepare_stock_receipt(
            &source("none", vec![]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("L"),
                    None,
                    1_000,
                    ReceiptDisposition::Available
                )]
            )
        )
        .unwrap_err()
        .code,
        "LOT_ALLOCATION_UNEXPECTED"
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("serial", vec![]),
            &input(
                2_000,
                2_000,
                vec![allocation(
                    None,
                    Some("SN-1"),
                    2_000,
                    ReceiptDisposition::Available
                )]
            )
        )
        .unwrap_err()
        .code,
        "SERIAL_QUANTITY_INVALID"
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![]),
            &input(
                2_000,
                2_000,
                vec![
                    allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                    allocation(Some("L-A"), None, 1_000, ReceiptDisposition::Available),
                ]
            )
        )
        .unwrap_err()
        .code,
        "LOT_ALLOCATION_DUPLICATE"
    );
}

#[test]
fn rejects_stale_order_version_and_over_receipt() {
    let mut stale = input(
        1_000,
        1_000,
        vec![allocation(
            Some("L-A"),
            None,
            1_000,
            ReceiptDisposition::Available,
        )],
    );
    stale.expected_order_version = 2;
    assert_eq!(
        prepare_stock_receipt(&source("lot", vec![]), &stale)
            .unwrap_err()
            .code,
        "STOCK_RECEIPT_SOURCE_CHANGED"
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![]),
            &input(
                11_000,
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    11_000,
                    ReceiptDisposition::Available
                )]
            )
        )
        .unwrap_err()
        .code,
        "OVER_FULFILLMENT"
    );
}

#[test]
fn refuses_available_receipt_into_a_quarantined_lot_but_allows_quarantine() {
    let held = StockReceiptLot {
        id: "lot-1".into(),
        lot_code: Some("L-A".into()),
        serial_number: None,
        status: "active".into(),
        quantity_milli: 1_000,
        value_minor: 1_000,
        version: 2,
        quality_hold_milli: 1_000,
        quality_status: Some("held".into()),
    };
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![held.clone()]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    1_000,
                    ReceiptDisposition::Quarantine
                )]
            )
        )
        .unwrap()
        .lines[0]
            .allocations[0]
            .existing_lot_id
            .as_deref(),
        Some("lot-1")
    );
    assert_eq!(
        prepare_stock_receipt(
            &source("lot", vec![held]),
            &input(
                1_000,
                1_000,
                vec![allocation(
                    Some("L-A"),
                    None,
                    1_000,
                    ReceiptDisposition::Available
                )]
            )
        )
        .unwrap_err()
        .code,
        "LOT_UNAVAILABLE"
    );
}
