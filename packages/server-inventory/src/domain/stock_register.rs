//! Portable stock-register projection. Every row is an Inventory-owned fact
//! read in the caller's already-authorized Company/branch transaction. The
//! register keeps aggregate stock, per-lot traceability, the Count lifecycle
//! and the lot reconciliation totals together so a page cannot silently mix
//! facts from different revisions.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

pub const MAX_STOCK_ROWS: usize = 1_000;
pub const MAX_LOT_ROWS: usize = 2_000;
pub const MAX_COUNT_ROWS: usize = 200;
pub const MAX_SAFE: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockRegisterRow {
    pub warehouse_id: String,
    pub product_id: String,
    pub sku: String,
    pub name: String,
    pub unit: String,
    pub tracking_mode: String,
    pub warehouse_name: String,
    pub quantity_milli: i64,
    pub reserved_milli: i64,
    pub quarantined_milli: i64,
    pub in_transit_milli: i64,
    pub value_minor: i64,
    pub version: i64,
    /// Derived by `validate_row`; never accepted as a caller fact.
    pub available_milli: i64,
}

impl StockRegisterRow {
    /// Available stock excludes the larger of reservations or quarantine,
    /// because an existing reservation can itself be quarantined. In-transit
    /// stock is reported separately until receipt and never added here.
    pub fn available_milli(&self) -> i64 {
        self.quantity_milli
            .saturating_sub(self.reserved_milli.max(self.quarantined_milli))
            .max(0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockRegisterLot {
    pub id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub sku: String,
    pub tracking_mode: String,
    pub warehouse_name: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub status: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
    pub quality_hold_milli: i64,
    pub quality_status: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockRegisterCount {
    pub id: String,
    pub number: String,
    pub status: String,
    pub version: i64,
    pub count_date: String,
    pub reason: String,
    pub warehouse_id: String,
    pub warehouse_name: String,
    pub net_value_minor: i64,
    pub posted_at: Option<i64>,
    pub journal_id: Option<String>,
    pub initial_physical_source: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Aggregate tracked-stock control totals. `consistent` is true only when the
/// tracked aggregate quantity and value equal the per-lot balances, which is
/// the journal-independent reconciliation the stock register promises.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockRegisterTotals {
    pub tracked_quantity_milli: i64,
    pub tracked_value_minor: i64,
    pub lot_quantity_milli: i64,
    pub lot_value_minor: i64,
    pub consistent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockRegister {
    pub stock: Vec<StockRegisterRow>,
    pub lots: Vec<StockRegisterLot>,
    pub counts: Vec<StockRegisterCount>,
    pub totals: StockRegisterTotals,
    pub currency: String,
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVENTORY_STOCK_REGISTER_INVALID",
        "The Inventory stock register source is unavailable.",
    )
}

fn bounded(value: i64) -> bool {
    (0..=MAX_SAFE).contains(&value)
}

/// Count value adjustments are signed: a scrap or shortage posts a negative
/// net value and must remain visible in the register.
fn signed(value: i64) -> bool {
    (-MAX_SAFE..=MAX_SAFE).contains(&value)
}

fn tracking(value: &str) -> bool {
    matches!(value, "none" | "lot" | "serial")
}

fn lot_status(value: &str) -> bool {
    matches!(value, "active" | "depleted" | "scrapped" | "returned")
}

fn count_status(value: &str) -> bool {
    matches!(
        value,
        "draft" | "submitted" | "approved" | "rejected" | "posted" | "cancelled"
    )
}

fn optional_text(value: &Option<String>, max: usize) -> bool {
    value
        .as_ref()
        .is_none_or(|text| !text.is_empty() && text.len() <= max)
}

pub fn validate_row(mut row: StockRegisterRow) -> Result<StockRegisterRow, BusinessError> {
    if row.warehouse_id.is_empty()
        || row.product_id.is_empty()
        || row.sku.is_empty()
        || row.name.is_empty()
        || !tracking(&row.tracking_mode)
        || !bounded(row.quantity_milli)
        || !bounded(row.reserved_milli)
        || !bounded(row.quarantined_milli)
        || !bounded(row.in_transit_milli)
        || !bounded(row.value_minor)
        || !(1..=MAX_SAFE).contains(&row.version)
    {
        return Err(unavailable());
    }
    row.available_milli = row.available_milli();
    Ok(row)
}

pub fn validate_lot(lot: StockRegisterLot) -> Result<StockRegisterLot, BusinessError> {
    let identity_valid = match lot.tracking_mode.as_str() {
        "lot" => lot.serial_number.is_none() && optional_text(&lot.lot_code, 200),
        "serial" => lot.lot_code.is_none() && optional_text(&lot.serial_number, 200),
        _ => false,
    };
    if lot.id.is_empty()
        || lot.warehouse_id.is_empty()
        || lot.product_id.is_empty()
        || lot.sku.is_empty()
        || !identity_valid
        || !lot_status(&lot.status)
        || !bounded(lot.quantity_milli)
        || !bounded(lot.value_minor)
        || !(1..=MAX_SAFE).contains(&lot.version)
        || !bounded(lot.quality_hold_milli)
        || lot.quality_hold_milli > lot.quantity_milli
        || !optional_text(&lot.manufactured_date, 40)
        || !optional_text(&lot.expiry_date, 40)
        || !optional_text(&lot.quality_status, 40)
    {
        return Err(unavailable());
    }
    Ok(lot)
}

pub fn validate_count(count: StockRegisterCount) -> Result<StockRegisterCount, BusinessError> {
    if count.id.is_empty()
        || count.number.is_empty()
        || !count_status(&count.status)
        || !(1..=MAX_SAFE).contains(&count.version)
        || count.count_date.is_empty()
        || count.reason.is_empty()
        || count.warehouse_id.is_empty()
        || count.warehouse_name.is_empty()
        || !signed(count.net_value_minor)
        || count.posted_at.is_some_and(|value| !bounded(value))
        || !optional_text(&count.journal_id, 200)
        || !bounded(count.created_at)
        || !bounded(count.updated_at)
    {
        return Err(unavailable());
    }
    Ok(count)
}

pub fn totals(stock: &[StockRegisterRow], lots: &[StockRegisterLot]) -> StockRegisterTotals {
    let mut totals = StockRegisterTotals::default();
    for row in stock.iter().filter(|row| row.tracking_mode != "none") {
        totals.tracked_quantity_milli = totals
            .tracked_quantity_milli
            .saturating_add(row.quantity_milli);
        totals.tracked_value_minor = totals.tracked_value_minor.saturating_add(row.value_minor);
    }
    for lot in lots {
        totals.lot_quantity_milli = totals.lot_quantity_milli.saturating_add(lot.quantity_milli);
        totals.lot_value_minor = totals.lot_value_minor.saturating_add(lot.value_minor);
    }
    totals.consistent = totals.tracked_quantity_milli == totals.lot_quantity_milli
        && totals.tracked_value_minor == totals.lot_value_minor;
    totals
}

pub fn validate_register(
    stock: Vec<StockRegisterRow>,
    lots: Vec<StockRegisterLot>,
    counts: Vec<StockRegisterCount>,
    currency: String,
) -> Result<StockRegister, BusinessError> {
    if stock.len() > MAX_STOCK_ROWS
        || lots.len() > MAX_LOT_ROWS
        || counts.len() > MAX_COUNT_ROWS
        || currency.len() != 3
        || !currency.bytes().all(|value| value.is_ascii_uppercase())
    {
        return Err(unavailable());
    }
    let stock = stock
        .into_iter()
        .map(validate_row)
        .collect::<Result<Vec<_>, _>>()?;
    let lots = lots
        .into_iter()
        .map(validate_lot)
        .collect::<Result<Vec<_>, _>>()?;
    let counts = counts
        .into_iter()
        .map(validate_count)
        .collect::<Result<Vec<_>, _>>()?;
    let totals = totals(&stock, &lots);
    Ok(StockRegister {
        stock,
        lots,
        counts,
        totals,
        currency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
            sku: "SKU".into(),
            name: "Product".into(),
            unit: "pcs".into(),
            tracking_mode: tracking.into(),
            warehouse_name: "Warehouse".into(),
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
            sku: "SKU".into(),
            tracking_mode: tracking.into(),
            warehouse_name: "Warehouse".into(),
            lot_code: (tracking == "lot").then(|| "L1".into()),
            serial_number: (tracking == "serial").then(|| "S1".into()),
            manufactured_date: None,
            expiry_date: None,
            status: "active".into(),
            quantity_milli: quantity,
            value_minor: value,
            version: 1,
            quality_hold_milli: 0,
            quality_status: None,
        }
    }

    fn count(id: &str, status: &str, version: i64) -> StockRegisterCount {
        StockRegisterCount {
            id: id.into(),
            number: "CNT-1".into(),
            status: status.into(),
            version,
            count_date: "2026-11-04".into(),
            reason: "Physical count".into(),
            warehouse_id: "w".into(),
            warehouse_name: "Warehouse".into(),
            net_value_minor: 0,
            posted_at: None,
            journal_id: None,
            initial_physical_source: false,
            created_at: 1,
            updated_at: 2,
        }
    }

    #[test]
    fn derives_available_quantity_from_the_larger_hold() {
        let mut row = stock("w", "p", "none", 10_000, 1_000);
        row.reserved_milli = 3_000;
        assert_eq!(validate_row(row.clone()).unwrap().available_milli, 7_000);
        row.quarantined_milli = 4_000;
        assert_eq!(validate_row(row).unwrap().available_milli, 6_000);
    }

    #[test]
    fn reconciles_tracked_aggregate_stock_with_lot_balances() {
        let register = validate_register(
            vec![
                stock("w", "p", "lot", 3_000, 3_000),
                stock("w", "u", "none", 7_000, 7_000),
            ],
            vec![
                lot("l1", "lot", 2_000, 2_000),
                lot("l2", "lot", 1_000, 1_000),
            ],
            vec![count("c1", "posted", 4)],
            "THB".into(),
        )
        .unwrap();
        assert_eq!(register.totals.tracked_quantity_milli, 3_000);
        assert_eq!(register.totals.tracked_value_minor, 3_000);
        assert_eq!(register.totals.lot_quantity_milli, 3_000);
        assert!(register.totals.consistent);

        let mismatch = validate_register(
            vec![stock("w", "p", "lot", 3_000, 3_000)],
            vec![lot("l1", "lot", 2_000, 2_000)],
            vec![],
            "THB".into(),
        )
        .unwrap();
        assert!(!mismatch.totals.consistent);
    }

    #[test]
    fn rejects_invalid_identity_lifecycle_and_bounds() {
        let mut invalid_lot = lot("l1", "lot", 1_000, 1_000);
        invalid_lot.serial_number = Some("S1".into());
        assert_eq!(
            validate_lot(invalid_lot).unwrap_err().code,
            "INVENTORY_STOCK_REGISTER_INVALID"
        );
        assert_eq!(
            validate_count(count("c1", "unknown", 1)).unwrap_err().code,
            "INVENTORY_STOCK_REGISTER_INVALID"
        );
        assert_eq!(
            validate_count(count("c1", "draft", 0)).unwrap_err().code,
            "INVENTORY_STOCK_REGISTER_INVALID"
        );
        assert_eq!(
            validate_register(vec![], vec![], vec![], "thb".into())
                .unwrap_err()
                .code,
            "INVENTORY_STOCK_REGISTER_INVALID"
        );
    }
}
