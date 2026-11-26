//! Pure domain rules for the native Finance invoice-list read.
//!
//! This module owns the byte-identical TypeScript statement head, the
//! Company/Holding validation, the limit bound and the wire shape. It executes
//! no SQL and performs no I/O; sqlx execution lives in `postgres.rs`.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

/// The statement rendered by the retired TypeScript authority:
/// `FINANCE_RD_INVOICE2_INVOICES` with `[companyId, limit]` binds.
pub const TS_INVOICE_LIST_SQL: &str =
    "SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name\n    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id\n    WHERE i.company_id=? ORDER BY i.invoice_date DESC,i.invoice_number DESC LIMIT ?";

/// The same statement executed by the owner with two positional binds and a
/// `to_jsonb` projection of exactly the retired column list.
pub const PG_INVOICE_LIST_SQL: &str =
    "SELECT to_jsonb(listed)::text FROM (SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=$1 ORDER BY i.invoice_date DESC,i.invoice_number DESC LIMIT $2) listed";

pub const DEFAULT_LIMIT: i64 = 100;
pub const MAX_LIMIT: i64 = 100;
/// The statement bound to 100 rows can never return more than the bound plus
/// one sentinel row; an oversized reply fails closed rather than truncating.
pub const MAX_INVOICE_ROWS: usize = 101;

pub const REQUIRED_FIELDS: [&str; 10] = [
    "id",
    "invoice_number",
    "kind",
    "invoice_date",
    "due_date",
    "currency",
    "total_minor",
    "outstanding_minor",
    "status",
    "party_name",
];

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_NATIVE_LANE_7_UNAVAILABLE",
        "The native Finance invoice-list read is unavailable.",
    )
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_NATIVE_LANE_7_REQUEST_INVALID",
        "Use a bounded Company, limit and registered Holding target.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_NATIVE_LANE_7_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

/// Company ids are opaque bounded identifiers used only as a SQL bind.
pub fn valid_company_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub fn valid_host(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}

/// Mirror the retired `Math.max(1, Math.min(limit, 100))` bind exactly.
pub fn bound_limit(limit: i64) -> i64 {
    limit.clamp(1, MAX_LIMIT)
}

fn scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

/// Every retired invoice-list row is an object of the ten projected columns.
/// Reject anything else before it reaches the transport.
pub fn project_row(raw: &str) -> Result<Value, BusinessError> {
    let value: Value = serde_json::from_str(raw).map_err(|_| unavailable())?;
    if !matches!(value, Value::Object(_))
        || REQUIRED_FIELDS
            .iter()
            .any(|field| value.get(*field).map(scalar) != Some(true))
    {
        return Err(unavailable());
    }
    Ok(value)
}

pub fn invoice_list_wire(rows: Vec<Value>) -> Result<Value, BusinessError> {
    if rows.len() > MAX_INVOICE_ROWS
        || rows.iter().any(|row| {
            !matches!(row, Value::Object(_))
                || REQUIRED_FIELDS
                    .iter()
                    .any(|field| row.get(*field).map(scalar) != Some(true))
        })
    {
        return Err(unavailable());
    }
    Ok(serde_json::json!({"data": rows}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lane_7_statement_head_is_byte_identical() {
        assert_eq!(TS_INVOICE_LIST_SQL.matches('?').count(), 2);
        assert!(!TS_INVOICE_LIST_SQL.contains("$1"));
        assert!(TS_INVOICE_LIST_SQL.contains(
            "ORDER BY i.invoice_date DESC,i.invoice_number DESC LIMIT ?"
        ));
        assert_eq!(PG_INVOICE_LIST_SQL.matches('$').count(), 2);
        assert!(PG_INVOICE_LIST_SQL.contains("to_jsonb(listed)::text"));
        assert!(PG_INVOICE_LIST_SQL.contains("p.name AS party_name"));
        assert!(PG_INVOICE_LIST_SQL.contains("LIMIT $2) listed"));
    }

    #[test]
    fn native_lane_7_projection_requires_the_full_retired_row() {
        let raw = r#"{"id":"i1","invoice_number":"INV-1","kind":"purchase","invoice_date":"2026-01-01","due_date":"2026-01-31","currency":"THB","total_minor":1000,"outstanding_minor":0,"status":"draft","party_name":"Acme"}"#;
        let row = project_row(raw).unwrap();
        assert_eq!(row["party_name"], "Acme");
        assert!(project_row(r#"{"id":"i1","name":"partial"}"#).is_err());
        assert!(project_row("[1]").is_err());
        assert!(invoice_list_wire(vec![row]).is_ok());
        assert!(invoice_list_wire(vec![Value::from(1)]).is_err());
        assert!(invoice_list_wire(vec![Value::Null; MAX_INVOICE_ROWS + 1]).is_err());
    }

    #[test]
    fn native_lane_7_validates_company_holding_host_and_limit() {
        assert!(valid_company_id("acme:co-1"));
        assert!(!valid_company_id("bad/company"));
        assert!(valid_slug("northstar"));
        assert!(!valid_slug("Acme"));
        assert!(valid_host("acme.jesadakorn.com"));
        assert!(!valid_host(""));
        assert_eq!(bound_limit(0), 1);
        assert_eq!(bound_limit(1000), MAX_LIMIT);
        assert_eq!(bound_limit(7), 7);
    }
}
