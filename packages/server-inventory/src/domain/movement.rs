//! Portable inventory movement-history projection. Every row is an owner fact
//! from `inventory_ledger`; source-specific issue writers remain with their
//! owning modules.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovementRow {
    pub id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub sku: String,
    pub warehouse_name: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub movement_date: String,
    pub source_type: String,
    pub source_id: String,
    pub order_number: String,
    pub fulfillment_date: String,
    pub created_at: i64,
}

pub const MAX_ROWS: usize = 50;
pub const MAX_SAFE: i64 = 9_007_199_254_740_991;

pub fn validate_row(row: MovementRow) -> Result<MovementRow, BusinessError> {
    let bounded = |value: i64| (-MAX_SAFE..=MAX_SAFE).contains(&value);
    if row.id.is_empty()
        || row.warehouse_id.is_empty()
        || row.product_id.is_empty()
        || row.source_type.is_empty()
        || !bounded(row.quantity_milli)
        || !bounded(row.value_minor)
        || !(0..=MAX_SAFE).contains(&row.created_at)
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "INVENTORY_MOVEMENT_INVALID",
            "The Inventory movement source is unavailable.",
        ));
    }
    Ok(row)
}

pub fn encode_cursor(created_at: i64, id: &str) -> String {
    format!("{created_at}:{id}")
}

pub fn parse_cursor(value: &str) -> Result<(i64, String), BusinessError> {
    let (created, id) = value.split_once(':').ok_or_else(|| {
        BusinessError::invalid(
            "INVENTORY_MOVEMENT_CURSOR_INVALID",
            "Use the cursor from the current movement page.",
        )
    })?;
    let created_at = created.parse::<i64>().map_err(|_| {
        BusinessError::invalid(
            "INVENTORY_MOVEMENT_CURSOR_INVALID",
            "Use the cursor from the current movement page.",
        )
    })?;
    if !(0..=MAX_SAFE).contains(&created_at) || id.is_empty() || id.len() > 200 {
        return Err(BusinessError::invalid(
            "INVENTORY_MOVEMENT_CURSOR_INVALID",
            "Use the cursor from the current movement page.",
        ));
    }
    Ok((created_at, id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_bounded_cursor() {
        let cursor = encode_cursor(1_793_757_600_000, "movement-1");
        assert_eq!(
            parse_cursor(&cursor).unwrap(),
            (1_793_757_600_000, "movement-1".into())
        );
        assert_eq!(
            parse_cursor("not-a-cursor").unwrap_err().code,
            "INVENTORY_MOVEMENT_CURSOR_INVALID"
        );
    }

    #[test]
    fn rejects_empty_movement_facts() {
        let row = MovementRow {
            id: "m".into(),
            warehouse_id: "w".into(),
            product_id: "p".into(),
            sku: "SKU".into(),
            warehouse_name: "Warehouse".into(),
            quantity_milli: 1,
            value_minor: 0,
            movement_date: "2026-11-04".into(),
            source_type: "commerce".into(),
            source_id: "o".into(),
            order_number: "SO-1".into(),
            fulfillment_date: "2026-11-04".into(),
            created_at: 1,
        };
        let mut empty = row;
        empty.id.clear();
        assert_eq!(
            validate_row(empty).unwrap_err().code,
            "INVENTORY_MOVEMENT_INVALID"
        );
    }
}
