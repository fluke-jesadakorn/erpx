//! Pure domain rules for the native Finance chart-list read.
//!
//! This module owns the byte-identical TypeScript statement head, the
//! Company/Holding validation and the wire shape. It executes no SQL and
//! performs no I/O; the sqlx execution lives in `postgres.rs`.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

/// The statement rendered by the retired TypeScript authority:
/// `financeReferenceListSql('finance_charts','company_id=?','name')`.
pub const TS_CHART_LIST_SQL: &str =
    "SELECT * FROM finance_charts WHERE company_id=? ORDER BY name";

/// The same statement executed by the owner, with one positional bind and a
/// `to_jsonb` projection so every column of every row crosses the wire.
pub const PG_CHART_LIST_SQL: &str =
    "SELECT to_jsonb(c)::text FROM finance_charts c WHERE c.company_id=$1 ORDER BY c.name";

/// Maximum rows carried by the bounded read. The statement itself is unchanged;
/// an oversized reply fails closed instead of being silently truncated.
pub const MAX_CHART_ROWS: usize = 20_001;

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_NATIVE_LANE_6_UNAVAILABLE",
        "The native Finance chart-list read is unavailable.",
    )
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_NATIVE_LANE_6_REQUEST_INVALID",
        "Use a bounded Company and a registered Holding target.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_NATIVE_LANE_6_CONFIG_INVALID",
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

/// Every TS `listCharts` row is an object of `finance_charts` columns. Reject
/// anything else before it reaches the transport.
pub fn project_row(raw: &str) -> Result<Value, BusinessError> {
    let value: Value = serde_json::from_str(raw).map_err(|_| unavailable())?;
    if !matches!(value, Value::Object(_)) {
        return Err(unavailable());
    }
    Ok(value)
}

pub fn chart_list_wire(rows: Vec<Value>) -> Result<Value, BusinessError> {
    if rows.len() > MAX_CHART_ROWS
        || rows.iter().any(|row| !matches!(row, Value::Object(_)))
    {
        return Err(unavailable());
    }
    Ok(serde_json::json!({"data": rows}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lane_6_statement_head_is_byte_identical() {
        assert_eq!(
            TS_CHART_LIST_SQL,
            "SELECT * FROM finance_charts WHERE company_id=? ORDER BY name"
        );
        assert_eq!(TS_CHART_LIST_SQL.matches('?').count(), 1);
        assert_eq!(PG_CHART_LIST_SQL.matches('$').count(), 1);
        assert!(PG_CHART_LIST_SQL.contains("to_jsonb(c)::text"));
    }

    #[test]
    fn native_lane_6_projection_requires_full_row_objects() {
        let row = project_row(r#"{"id":"c1","name":"Operations ledger","status":"active"}"#).unwrap();
        assert_eq!(row["name"], "Operations ledger");
        assert!(project_row("[1]").is_err());
        assert!(chart_list_wire(vec![row]).is_ok());
        assert!(chart_list_wire(vec![Value::from(1)]).is_err());
        assert!(chart_list_wire(vec![Value::from(1); MAX_CHART_ROWS + 1]).is_err());
    }

    #[test]
    fn native_lane_6_validates_company_holding_and_host() {
        assert!(valid_company_id("acme-co"));
        assert!(!valid_company_id(""));
        assert!(!valid_company_id("bad/company"));
        assert!(valid_slug("northstar"));
        assert!(!valid_slug("Acme"));
        assert!(valid_host("acme.jesadakorn.com"));
        assert!(!valid_host(""));
    }
}
