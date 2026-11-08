//! Portable stock-register admission and reconciliation. These cases do not
//! prove a transaction, IAM or PostgreSQL source; they pin the owner's field
//! and arithmetic contract.
use erpx_server_inventory::public::stock_register::*;

fn stock(
    warehouse: &str,
    product: &str,
    tracking: &str,
    quantity: i64,
    value: i64,
) -> StockRegisterRow {
    StockRegisterRow {
        warehouse_id: warehouse.into(),
        product_id: product.into(),
        sku: format!("SKU-{product}"),
        name: "Product".into(),
        unit: "pcs".into(),
        tracking_mode: tracking.into(),
        warehouse_name: format!("Warehouse {warehouse}"),
        quantity_milli: quantity,
        reserved_milli: 0,
        quarantined_milli: 0,
        in_transit_milli: 0,
        value_minor: value,
        version: 1,
        available_milli: 0,
    }
}

fn lot(id: &str, tracking: &str, quantity: i64, value: i64) -> StockRegisterLot {
    StockRegisterLot {
        id: id.into(),
        warehouse_id: "w".into(),
        product_id: "p".into(),
        sku: "SKU-p".into(),
        tracking_mode: tracking.into(),
        warehouse_name: "Warehouse w".into(),
        lot_code: (tracking == "lot").then(|| format!("L-{id}")),
        serial_number: (tracking == "serial").then(|| format!("S-{id}")),
        manufactured_date: None,
        expiry_date: Some("2027-01-01".into()),
        status: "active".into(),
        quantity_milli: quantity,
        value_minor: value,
        version: 2,
        quality_hold_milli: 0,
        quality_status: None,
    }
}

fn count(id: &str, status: &str, version: i64, initial: bool) -> StockRegisterCount {
    StockRegisterCount {
        id: id.into(),
        number: format!("CNT-{id}"),
        status: status.into(),
        version,
        count_date: "2026-11-04".into(),
        reason: "Physical count".into(),
        warehouse_id: "w".into(),
        warehouse_name: "Warehouse w".into(),
        net_value_minor: 0,
        posted_at: (status == "posted").then_some(100),
        journal_id: (status == "posted").then(|| "journal-1".into()),
        initial_physical_source: initial,
        created_at: 1,
        updated_at: 2,
    }
}

#[test]
fn derives_available_from_reservation_and_quarantine() {
    let mut row = stock("w", "p", "none", 10_000, 1_000);
    row.reserved_milli = 2_000;
    row.quarantined_milli = 4_000;
    row.in_transit_milli = 1_500;
    let row = validate_row(row).unwrap();
    assert_eq!(row.available_milli, 6_000);
    assert_eq!(row.in_transit_milli, 1_500);
}

#[test]
fn reconciles_only_tracked_stock_to_lot_balances() {
    let register = validate_register(
        vec![
            stock("w", "p", "lot", 3_000, 3_000),
            stock("w", "u", "none", 7_000, 7_000),
        ],
        vec![lot("a", "lot", 2_000, 2_000), lot("b", "lot", 1_000, 1_000)],
        vec![count("c1", "posted", 4, true)],
        "THB".into(),
    )
    .unwrap();
    assert_eq!(register.totals.tracked_quantity_milli, 3_000);
    assert_eq!(register.totals.tracked_value_minor, 3_000);
    assert_eq!(register.totals.lot_quantity_milli, 3_000);
    assert_eq!(register.totals.lot_value_minor, 3_000);
    assert!(register.totals.consistent);
    assert!(register.counts[0].initial_physical_source);
    assert_eq!(register.counts[0].version, 4);
    assert_eq!(register.counts[0].journal_id.as_deref(), Some("journal-1"));
}

#[test]
fn a_lot_shortfall_breaks_reconciliation_without_hiding_rows() {
    let register = validate_register(
        vec![stock("w", "p", "lot", 3_000, 3_000)],
        vec![lot("a", "lot", 2_000, 2_000)],
        vec![],
        "THB".into(),
    )
    .unwrap();
    assert!(!register.totals.consistent);
    assert_eq!(register.stock.len(), 1);
    assert_eq!(register.lots.len(), 1);
}

#[test]
fn keeps_signed_scrap_count_values_in_the_register() {
    let mut scrap = count("scrap", "submitted", 1, false);
    scrap.net_value_minor = -300;
    let register = validate_register(vec![], vec![], vec![scrap], "THB".into()).unwrap();
    assert_eq!(register.counts[0].net_value_minor, -300);
}

#[test]
fn rejects_serial_lot_identity_mix_and_invalid_lifecycle() {
    let mut mixed = lot("a", "lot", 1_000, 1_000);
    mixed.serial_number = Some("S".into());
    assert_eq!(
        validate_lot(mixed).unwrap_err().code,
        "INVENTORY_STOCK_REGISTER_INVALID"
    );
    assert_eq!(
        validate_count(count("c", "approved", 1, false))
            .unwrap()
            .status,
        "approved"
    );
    assert_eq!(
        validate_count(count("c", "draft", 0, false))
            .unwrap_err()
            .code,
        "INVENTORY_STOCK_REGISTER_INVALID"
    );
    assert_eq!(
        validate_register(vec![], vec![], vec![], "thb".into())
            .unwrap_err()
            .code,
        "INVENTORY_STOCK_REGISTER_INVALID"
    );
}
