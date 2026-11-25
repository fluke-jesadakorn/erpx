//! Pure response types, validation and invariants for the lane-18 owner read.
//!
//! The capability is the Inventory-owned stock-availability snapshot consumed
//! by the POS settings page: one row per stock record with the live reservation
//! rollup and the Quality-held flag. No SQL and no HTTP concern lives here.

use serde::Serialize;

pub const ROUTE: &str = "/api/v1/holdings/{holding_slug}/inventory/lane-18/stock-availability";
pub const MAX_ROWS: usize = 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Scope<'a> {
    pub company: &'a str,
    pub branch: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StockAvailabilityRow {
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub reserved_milli: i64,
    /// The SQL result speaks 0/1; the strict TS client pins that shape.
    pub quality_held: i16,
}

impl StockAvailabilityRow {
    pub fn held(&self) -> bool {
        self.quality_held == 1
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StockAvailabilityResponse {
    pub rows: Vec<StockAvailabilityRow>,
    pub count: usize,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'+'))
}

/// Company is mandatory; an absent or empty branch means the whole Company.
/// The branch value is an already-resolved assignment scope, never a caller
/// label, so it is validated with the same identity grammar.
pub fn validate_scope(scope: Scope<'_>) -> Result<(), ()> {
    if !valid_id(scope.company) {
        return Err(());
    }
    if scope.branch.is_some_and(|branch| !valid_id(branch)) {
        return Err(());
    }
    Ok(())
}

pub fn validate_row(row: StockAvailabilityRow) -> Result<StockAvailabilityRow, ()> {
    if !valid_id(&row.warehouse_id)
        || !valid_id(&row.product_id)
        || row.quality_held != 0 && row.quality_held != 1
        || row.quantity_milli < 0
        || row.reserved_milli < 0
    {
        return Err(());
    }
    Ok(row)
}

pub fn project(rows: Vec<StockAvailabilityRow>) -> Result<StockAvailabilityResponse, ()> {
    if rows.len() > MAX_ROWS {
        return Err(());
    }
    let rows = rows
        .into_iter()
        .map(validate_row)
        .collect::<Result<Vec<_>, _>>()?;
    let count = rows.len();
    Ok(StockAvailabilityResponse { rows, count })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> StockAvailabilityRow {
        StockAvailabilityRow {
            warehouse_id: "w1".into(),
            product_id: "p1".into(),
            quantity_milli: 1200,
            reserved_milli: 200,
            quality_held: 0,
        }
    }

    #[test]
    fn scope_accepts_company_only_and_branch() {
        assert_eq!(
            validate_scope(Scope {
                company: "company-1",
                branch: None
            }),
            Ok(())
        );
        assert_eq!(
            validate_scope(Scope {
                company: "company-1",
                branch: Some("branch_2")
            }),
            Ok(())
        );
    }

    #[test]
    fn scope_rejects_empty_or_foreign_characters() {
        assert_eq!(
            validate_scope(Scope {
                company: "",
                branch: None
            }),
            Err(())
        );
        assert_eq!(
            validate_scope(Scope {
                company: "company/../x",
                branch: None
            }),
            Err(())
        );
        assert_eq!(
            validate_scope(Scope {
                company: "company",
                branch: Some("")
            }),
            Err(())
        );
    }

    #[test]
    fn projection_pins_response_shape() {
        let response = project(vec![row()]).expect("valid row");
        assert_eq!(response.count, 1);
        let json = serde_json::to_value(&response).expect("serializable");
        assert_eq!(
            json,
            serde_json::json!({"rows":[{"warehouse_id":"w1","product_id":"p1","quantity_milli":1200,"reserved_milli":200,"quality_held":0}],"count":1})
        );
    }

    #[test]
    fn result_oracle_matches_the_typescript_contract() {
        use sha2::{Digest, Sha256};
        let second = StockAvailabilityRow {
            warehouse_id: "w1".into(),
            product_id: "p2".into(),
            quantity_milli: 500,
            reserved_milli: 0,
            quality_held: 1,
        };
        let response = project(vec![row(), second]).expect("valid rows");
        let raw = serde_json::to_string(&response).expect("serializable");
        let hash: String = Sha256::digest(raw.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            hash,
            "77aaeea853b67153f2d4040897a2d3e47a21f8c48e2a63658ac90a1b90c77437"
        );
    }

    #[test]
    fn projection_rejects_ambiguous_flag_and_bad_ids() {
        assert_eq!(
            project(vec![StockAvailabilityRow {
                quality_held: 2,
                ..row()
            }]),
            Err(())
        );
        assert_eq!(
            project(vec![StockAvailabilityRow {
                warehouse_id: String::new(),
                ..row()
            }]),
            Err(())
        );
        assert_eq!(project((0..=MAX_ROWS).map(|_| row()).collect()), Err(()));
    }
}
