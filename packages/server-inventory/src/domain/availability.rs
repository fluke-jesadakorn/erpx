//! Portable stock-availability projection. The available quantity is derived
//! from the owner facts, never stored or trusted from a caller label.
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailabilityRow {
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

impl AvailabilityRow {
    /// Available stock excludes the larger of reservations or quarantine,
    /// because an existing reservation can itself be quarantined. In-transit
    /// stock is reported separately until receipt and never added here.
    pub fn available_milli(&self) -> i64 {
        self.quantity_milli
            .saturating_sub(self.reserved_milli.max(self.quarantined_milli))
            .max(0)
    }
}

pub const MAX_ROWS: usize = 1_000;
pub const MAX_SAFE: i64 = 9_007_199_254_740_991;

pub fn validate_row(mut row: AvailabilityRow) -> Result<AvailabilityRow, BusinessError> {
    let bounded = |value: i64| (0..=MAX_SAFE).contains(&value);
    if row.warehouse_id.is_empty()
        || row.product_id.is_empty()
        || !bounded(row.quantity_milli)
        || !bounded(row.reserved_milli)
        || !bounded(row.quarantined_milli)
        || !bounded(row.in_transit_milli)
        || !bounded(row.value_minor)
        || !(1..=MAX_SAFE).contains(&row.version)
    {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "INVENTORY_AVAILABILITY_INVALID",
            "The Inventory availability source is unavailable.",
        ));
    }
    row.available_milli = row.available_milli();
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(quantity: i64, reserved: i64, quarantined: i64, transit: i64) -> AvailabilityRow {
        AvailabilityRow {
            warehouse_id: "w".into(),
            product_id: "p".into(),
            sku: "SKU".into(),
            name: "Product".into(),
            unit: "pcs".into(),
            tracking_mode: "none".into(),
            warehouse_name: "Warehouse".into(),
            quantity_milli: quantity,
            reserved_milli: reserved,
            quarantined_milli: quarantined,
            in_transit_milli: transit,
            value_minor: 0,
            version: 1,
            available_milli: 0,
        }
    }

    #[test]
    fn excludes_the_larger_of_reservation_or_quarantine() {
        assert_eq!(row(10_000, 3_000, 0, 5_000).available_milli(), 7_000);
        assert_eq!(row(10_000, 3_000, 4_000, 0).available_milli(), 6_000);
        assert_eq!(row(10_000, 4_000, 3_000, 0).available_milli(), 6_000);
        assert_eq!(row(2_000, 3_000, 0, 0).available_milli(), 0);
    }

    #[test]
    fn rejects_negative_or_unbounded_facts() {
        assert_eq!(
            validate_row(row(-1, 0, 0, 0)).unwrap_err().code,
            "INVENTORY_AVAILABILITY_INVALID"
        );
        assert_eq!(
            validate_row(row(1, 0, 0, 0)).unwrap().quantity_milli,
            1
        );
        let mut zero_version = row(1, 0, 0, 0);
        zero_version.version = 0;
        assert_eq!(
            validate_row(zero_version).unwrap_err().code,
            "INVENTORY_AVAILABILITY_INVALID"
        );
    }
}
