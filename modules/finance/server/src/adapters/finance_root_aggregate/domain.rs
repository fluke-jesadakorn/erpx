//! Pure domain rules for the native Finance root aggregate read.
//!
//! This module owns the frozen TypeScript statement heads and the wire
//! projection. It performs no I/O; every SELECT is executed by `postgres.rs`.

use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::{Map, Value};

/// Frozen TypeScript authority for `listPayments` (`finance_rd_invoice2_payments`).
pub const TS_PAYMENTS_SQL: &str = "SELECT p.id,p.payment_number,p.kind,p.payment_date,p.currency,p.amount_minor,p.outstanding_minor,p.status,f.name AS party_name\n    FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id\n    WHERE p.company_id=? ORDER BY p.payment_date DESC,p.payment_number DESC LIMIT ?";

/// Owner execution of the same statement with a projection of exactly the
/// retired column list. No caller value becomes SQL; both values are binds.
pub const PG_PAYMENTS_SQL: &str = "SELECT jsonb_build_object('id',p.id,'payment_number',p.payment_number,'kind',p.kind,'payment_date',p.payment_date,'currency',p.currency,'amount_minor',p.amount_minor,'outstanding_minor',p.outstanding_minor,'status',p.status,'party_name',f.name)::text FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id WHERE p.company_id=$1 ORDER BY p.payment_date DESC,p.payment_number DESC LIMIT $2";

/// `listPayments` and `financeInvoiceListNative` both bind a bounded 100 rows.
pub const DEFAULT_LIMIT: i64 = 100;
pub const MAX_LIMIT: i64 = 100;

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FINANCE_ROOT_AGGREGATE_UNAVAILABLE",
        "The native Finance root aggregate read is unavailable.",
    )
}

pub fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_ROOT_AGGREGATE_REQUEST_INVALID",
        "Use a bounded Company and a registered Holding target.",
    )
}

pub fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "FINANCE_ROOT_AGGREGATE_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

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

pub fn valid_chart_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '?' | '#' | '\\'))
}

/// The Next route selects `chartId` from the query, then the chart named
/// `Operations ledger`, then the first chart by name. Carry that exact
/// selection into the owner so journals and reports use the same ledger.
pub fn select_chart_id(charts: &[Value], requested: Option<&str>) -> Result<String, BusinessError> {
    if let Some(id) = requested {
        if !valid_chart_id(id) {
            return Err(invalid_request());
        }
        return Ok(id.to_owned());
    }
    let named = charts.iter().find(|row| {
        row.get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name == "Operations ledger")
    });
    let id = named
        .or_else(|| charts.first())
        .and_then(|row| row.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !valid_chart_id(id) {
        return Err(invalid_request());
    }
    Ok(id.to_owned())
}

pub fn project_row(raw: &str) -> Result<Value, BusinessError> {
    let value: Value = serde_json::from_str(raw).map_err(|_| unavailable())?;
    if !matches!(value, Value::Object(_)) {
        return Err(unavailable());
    }
    Ok(value)
}

/// Finance destination prefixes carried by `canonicalDestinationLinks`.
/// The five collections in this aggregate do not emit destination fields, so
/// the projection is an identity transform for the exact retired payload.
const ROUTE_PREFIXES: [(&str, &str); 5] = [
    ("finance/sales", "finance/documents/sales"),
    ("finance/invoices", "finance/documents/invoices"),
    ("finance/payments", "finance/cash/payments"),
    ("finance/accounting", "finance/accounting/journals"),
    ("finance/reports", "finance/reports/company"),
];

fn word_chars(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_destination_key(key: &str) -> bool {
    key == "href"
        || key
            .strip_suffix("Href")
            .is_some_and(word_chars)
        || key
            .strip_suffix("_href")
            .is_some_and(word_chars)
}

/// Mirrors the `canonicalDestinationLink` branch used by this payload.
pub fn canonical_destination_link(href: &str) -> String {
    if href.starts_with("/api/")
        || href.starts_with("/holdings/")
        || href.starts_with("/platform")
        || href.starts_with("http://")
        || href.starts_with("https://")
    {
        return href.to_owned();
    }
    let rest = href.strip_prefix('/').unwrap_or(href);
    let source = rest.split(['?', '#']).next().unwrap_or("");
    let matched = ROUTE_PREFIXES
        .iter()
        .filter(|(prefix, _)| source == *prefix || source.starts_with(&format!("{prefix}/")))
        .max_by_key(|(prefix, _)| prefix.len());
    let Some((prefix, path)) = matched else {
        return href.to_owned();
    };
    format!("/holdings/__route_scope/{path}{}", &rest[prefix.len()..])
        .strip_prefix("/holdings/__route_scope")
        .unwrap_or_default()
        .to_owned()
}

/// Recursive `{data: ...}` projection matching the retired Next response.
pub fn canonical_envelope(data: Value) -> Value {
    fn walk(value: &mut Value) {
        match value {
            Value::Array(items) => items.iter_mut().for_each(walk),
            Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    if is_destination_key(key)
                        && let Value::String(href) = item
                    {
                        *href = canonical_destination_link(href);
                    } else {
                        walk(item);
                    }
                }
            }
            _ => {}
        }
    }
    let mut value = Value::Object(Map::from_iter([("data".to_owned(), data)]));
    walk(&mut value);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_aggregate_owner_statement_keeps_the_retired_head() {
        assert!(TS_PAYMENTS_SQL.matches('?').count() == 2);
        assert!(!TS_PAYMENTS_SQL.contains("$1"));
        assert!(PG_PAYMENTS_SQL.matches('$').count() == 2);
        assert!(PG_PAYMENTS_SQL.contains("LEFT JOIN finance_parties f ON f.id=p.party_id"));
        assert!(PG_PAYMENTS_SQL.contains("'party_name',f.name"));
        assert!(PG_PAYMENTS_SQL.ends_with("LIMIT $2"));
    }

    #[test]
    fn root_aggregate_validates_scope() {
        assert!(valid_company_id("acme-co"));
        assert!(!valid_company_id("bad/company"));
        assert!(valid_slug("northstar"));
        assert!(!valid_slug("Acme"));
        assert!(valid_host("acme.jesadakorn.com"));
        assert!(!valid_host(""));
        assert!(project_row(r#"{"id":"p1"}"#).is_ok());
        assert!(project_row("[1]").is_err());
    }

    #[test]
    fn root_aggregate_chart_selection_matches_the_retired_route() {
        let charts = vec![
            serde_json::json!({"id": "a", "name": "Other ledger"}),
            serde_json::json!({"id": "b", "name": "Operations ledger"}),
        ];
        assert_eq!(select_chart_id(&charts, None).unwrap(), "b");
        assert_eq!(select_chart_id(&charts, Some("a")).unwrap(), "a");
        assert!(select_chart_id(&[], None).is_err());
    }

    #[test]
    fn root_aggregate_canonical_projection_is_identity_for_ledger_rows() {
        assert_eq!(canonical_destination_link("/api/v1/holdings/acme/finance"), "/api/v1/holdings/acme/finance");
        assert_eq!(canonical_destination_link("/finance/payments"), "/finance/cash/payments");
        let data = serde_json::json!({"charts": [{"id": "c1"}], "report": {"balanceSheet": {"assets": "1"}}});
        let wire = canonical_envelope(data.clone());
        assert_eq!(wire["data"], data);
        assert!(wire.get("href").is_none());
    }
}
