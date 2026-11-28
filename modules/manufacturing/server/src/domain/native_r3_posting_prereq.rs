//! Pure domain for the native R3 Manufacturing posting-setup prerequisites read.
//!
//! Only the owner-owned reference projections are shaped here. Finance
//! authority, purchased-package admission, the `contextKey` image and the
//! `available`/`can*` decisions remain composition facts and are never invented
//! by this slice. The caller supplies the actor and the base posting rows read
//! by the existing native posting-setup owner.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;

pub fn data_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_POSTING_PREREQUISITES_UNAVAILABLE",
        "The Manufacturing posting prerequisites are unavailable.",
    )
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, BusinessError> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(data_invalid)
}

/// The retained period projection: `startDate`/`endDate` are the first ten
/// characters of the SQL date and `version` is `close_version`.
fn period(row: &Value) -> Result<Value, BusinessError> {
    let object = row.as_object().ok_or_else(data_invalid)?;
    let start = object.get("startDate").and_then(Value::as_str).ok_or_else(data_invalid)?;
    let end = object.get("endDate").and_then(Value::as_str).ok_or_else(data_invalid)?;
    if start.len() < 10 || end.len() < 10 {
        return Err(data_invalid());
    }
    Ok(serde_json::json!({
        "id": object.get("id").ok_or_else(data_invalid)?,
        "name": object.get("name").ok_or_else(data_invalid)?,
        "startDate": &start[..10],
        "endDate": &end[..10],
        "state": object.get("state").ok_or_else(data_invalid)?,
        "version": object.get("version").ok_or_else(data_invalid)?,
    }))
}

/// The retained FX-rate projection. A decimal with a fractional part keeps the
/// TS trailing-zero/`'.'` trim; an integer text value is preserved verbatim.
fn currency_per_usd(raw: &str) -> String {
    if raw.contains('.') {
        let trimmed = raw.trim_end_matches('0');
        trimmed.strip_suffix('.').unwrap_or(trimmed).to_owned()
    } else {
        raw.to_owned()
    }
}

fn rate(row: &Value) -> Result<Value, BusinessError> {
    let object = row.as_object().ok_or_else(data_invalid)?;
    let effective = object.get("effectiveDate").and_then(Value::as_str).ok_or_else(data_invalid)?;
    if effective.len() < 10 {
        return Err(data_invalid());
    }
    let raw = object.get("currencyPerUsd").and_then(Value::as_str).ok_or_else(data_invalid)?;
    Ok(serde_json::json!({
        "id": object.get("id").ok_or_else(data_invalid)?,
        "currency": object.get("currency").ok_or_else(data_invalid)?,
        "effectiveDate": &effective[..10],
        "currencyPerUsd": currency_per_usd(raw),
        "confirmedAt": object.get("confirmedAt").ok_or_else(data_invalid)?,
    }))
}

/// Project the owner-owned references into the exact `charts`/`accounts`/
/// `periods`/`rates` rows the retained GET returned. `base` is the output of
/// the existing native posting-setup read (`configuration`, `charts`,
/// `accounts`); the `configuration` key is required but not emitted here.
pub fn project_prerequisites(base: &Value, periods: &Value, rates: &Value) -> Result<Value, BusinessError> {
    if !base.as_object().is_some_and(|object| object.contains_key("configuration")) {
        return Err(data_invalid());
    }
    let periods = array(periods, "periods")?
        .iter()
        .map(period)
        .collect::<Result<Vec<_>, _>>()?;
    let rates = array(rates, "rates")?
        .iter()
        .map(rate)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(serde_json::json!({
        "charts": array(base, "charts")?,
        "accounts": array(base, "accounts")?,
        "periods": periods,
        "rates": rates,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({"configuration": null, "charts": [{"id":"c","name":"THB","baseCurrency":"THB"}], "accounts": []})
    }

    #[test]
    fn period_dates_are_truncated_and_version_kept() {
        let rows = json!({"periods":[{"id":"p","name":"2026","startDate":"2026-01-01T00:00:00Z","endDate":"2026-12-31T00:00:00Z","state":"open","version":3}]});
        let out = project_prerequisites(&base(), &rows, &json!({"rates":[]})).unwrap();
        assert_eq!(out["periods"][0]["startDate"], "2026-01-01");
        assert_eq!(out["periods"][0]["endDate"], "2026-12-31");
        assert_eq!(out["periods"][0]["version"], 3);
    }

    #[test]
    fn rate_decimal_trims_trailing_zeroes_only_when_fractional() {
        assert_eq!(currency_per_usd("32.000000000000000000"), "32");
        assert_eq!(currency_per_usd("36.5000"), "36.5");
        assert_eq!(currency_per_usd("37"), "37");
        let rows = json!({"rates":[{"id":"r","currency":"USD","effectiveDate":"2026-01-02","currencyPerUsd":"36.5000","confirmedAt":100}]});
        let out = project_prerequisites(&base(), &json!({"periods":[]}), &rows).unwrap();
        assert_eq!(out["rates"][0]["currencyPerUsd"], "36.5");
    }

    #[test]
    fn malformed_rows_are_rejected_without_inventing_fields() {
        assert_eq!(project_prerequisites(&json!({"charts":[]}), &json!({"periods":[]}), &json!({"rates":[]})).unwrap_err().code, "MANUFACTURING_POSTING_PREREQUISITES_UNAVAILABLE");
        assert!(project_prerequisites(&base(), &json!({"periods":[{}]}), &json!({"rates":[]})).is_err());
        assert!(project_prerequisites(&base(), &json!({"periods":[]}), &json!({"rates":[{"id":"r","currency":"USD","effectiveDate":"x","currencyPerUsd":"1","confirmedAt":1}]})).is_err());
    }
}
