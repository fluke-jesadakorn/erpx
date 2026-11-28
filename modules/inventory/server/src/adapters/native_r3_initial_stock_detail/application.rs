//! Application mapping for the POS Initial Physical Stock native detail read.
//! The port returns owner-read rows; this module formats the exact retained
//! `PosInitialPhysicalStockCountRecord` field set. It executes no SQL.

use super::domain::{DetailQuery, DetailRow, LotRow, SOURCE_KIND};
use erpx_platform_runtime_server::public::http_support::serde_json::{self, Value};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type DetailFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<DetailRow>, BusinessError>> + Send + 'a>>;

/// SQL-free owner port. Composition supplies a Postgres implementation; no
/// caller-supplied predicate or payload can reach it.
pub trait InitialStockDetailRead: Send + Sync {
    fn read<'a>(&'a self, query: &'a DetailQuery) -> DetailFuture<'a>;
}

/// Exact `minorToDecimal(value, scale)` for safe integers.
fn decimal(value: i64, scale: u32) -> String {
    let base = 10i64.pow(scale);
    let negative = value < 0;
    let absolute = value.unsigned_abs();
    format!(
        "{}{}.{:0width$}",
        if negative { "-" } else { "" },
        absolute / base as u64,
        absolute % base as u64,
        width = scale as usize
    )
}

fn lot(row: &LotRow) -> Value {
    serde_json::json!({
        "lotCode": row.lot_code,
        "serialNumber": row.serial_number,
        "manufacturedDate": row.manufactured_date,
        "expiryDate": row.expiry_date,
        "countedQuantity": decimal(row.counted_quantity_milli, 3),
        "unitCost": row.unit_cost_minor.map(|value| decimal(value, 2)),
    })
}

/// The retained record envelope. Action flags are derived from the observed
/// Count status and the current actor's identity; purchase capability and Flow
/// routing authority remain composition-owned (see parity gap in the report).
pub fn record(query: &DetailQuery, detail: &DetailRow) -> Value {
    let count = &detail.count;
    let edit = ["draft", "rejected"].contains(&count.status.as_str())
        && [count.created_by.as_str(), count.prepared_by.as_str()]
            .contains(&query.actor_id.as_str());
    let review = ["submitted", "approved"].contains(&count.status.as_str())
        && ![count.created_by.as_str(), count.prepared_by.as_str()]
            .contains(&query.actor_id.as_str());
    let rejectable = ["submitted", "approved"].contains(&count.status.as_str());
    let line = detail.line.as_ref();
    serde_json::json!({
        "sourceKind": SOURCE_KIND,
        "id": count.id,
        "number": count.number,
        "status": count.status,
        "version": count.version,
        "countDate": count.count_date,
        "reason": count.reason,
        "warehouseId": count.warehouse_id,
        "warehouseName": count.warehouse_name,
        "productId": line.map(|row| row.product_id.as_str()),
        "sku": line.map(|row| row.sku.as_str()),
        "productName": line.map(|row| row.description.as_str()),
        "unit": line.map(|row| row.unit.as_str()),
        "trackingMode": line.map(|row| row.tracking_mode.as_str()),
        "countedQuantity": line.map(|row| decimal(row.counted_quantity_milli, 3)),
        "countedValue": line.map(|row| decimal(row.counted_value_minor, 2)),
        "unitCost": line.and_then(|row| row.unit_cost_minor).map(|value| decimal(value, 2)),
        "lots": detail.lots.iter().map(lot).collect::<Vec<_>>(),
        "journalId": count.journal_id,
        "canSubmit": edit,
        "canCancel": edit,
        "canApprove": count.status == "submitted" && review,
        "canPost": count.status == "approved" && review,
        "canReject": rejectable && review,
        "decisions": detail.decisions.iter().map(|row| serde_json::json!({
            "action": row.action,
            "reason": row.reason,
            "actorName": row.actor_name,
            "createdAt": row.created_at,
        })).collect::<Vec<_>>(),
    })
}

/// Full HTTP body envelope used by the retained Next route.
pub fn envelope(query: &DetailQuery, detail: &DetailRow) -> Value {
    serde_json::json!({ "data": record(query, detail) })
}

pub async fn read_detail(
    reader: &dyn InitialStockDetailRead,
    query: &DetailQuery,
) -> Result<Option<Value>, BusinessError> {
    Ok(reader.read(query).await?.map(|row| envelope(query, &row)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_r3_initial_stock_detail::domain::{
        CountRow, DecisionRow, LineRow,
    };

    fn query() -> DetailQuery {
        DetailQuery {
            company_id: "company-1".into(),
            branch_id: None,
            count_id: "count-1".into(),
            actor_id: "user-1".into(),
        }
    }

    fn detail() -> DetailRow {
        DetailRow {
            count: CountRow {
                id: "count-1".into(),
                number: "CNT-1".into(),
                status: "submitted".into(),
                version: 2,
                count_date: "2026-11-28".into(),
                reason: "opening".into(),
                warehouse_id: "warehouse-1".into(),
                warehouse_name: "Main".into(),
                created_by: "user-2".into(),
                prepared_by: "user-2".into(),
                journal_id: None,
            },
            line: Some(LineRow {
                product_id: "product-1".into(),
                sku: "SKU-1".into(),
                description: "Product".into(),
                unit: "ea".into(),
                tracking_mode: "none".into(),
                counted_quantity_milli: 1500,
                counted_value_minor: -1234,
                unit_cost_minor: Some(250),
            }),
            lots: vec![LotRow {
                lot_code: Some("L1".into()),
                serial_number: None,
                manufactured_date: None,
                expiry_date: None,
                counted_quantity_milli: 1000,
                unit_cost_minor: None,
            }],
            decisions: vec![DecisionRow {
                action: "submit".into(),
                reason: None,
                actor_name: "User Two".into(),
                created_at: 100,
            }],
        }
    }

    #[test]
    fn formats_signed_decimals_and_exact_record_fields() {
        let value = record(&query(), &detail());
        assert_eq!(value["sourceKind"], SOURCE_KIND);
        assert_eq!(value["countedQuantity"], "1.500");
        assert_eq!(value["countedValue"], "-12.34");
        assert_eq!(value["unitCost"], "2.50");
        assert_eq!(value["lots"][0]["countedQuantity"], "1.000");
        assert_eq!(value["decisions"][0]["actorName"], "User Two");
        assert_eq!(value["canApprove"], true);
        assert_eq!(value["canSubmit"], false);
        assert_eq!(value.as_object().unwrap().len(), 25);
    }

    #[test]
    fn envelope_wraps_one_data_key() {
        let value = envelope(&query(), &detail());
        let object = value.as_object().unwrap();
        assert_eq!(object.len(), 1);
        assert!(object.contains_key("data"));
    }
}
