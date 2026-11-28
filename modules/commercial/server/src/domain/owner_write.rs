//! Pure domain kind registry for native Commercial owner writes.
//!
//! The remaining Commercial TS write literals map to closed owner-owned kinds.
//! The allow-list and arity rules live here (pure); the application executor and
//! the owner Postgres adapter own the actual write. No TS SQL or fallback.
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;

pub const OWNER_WRITE_OPERATION: &str = "commercial.owner-write.v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerWrite {
    pub kind: &'static str,
    pub values: Vec<Value>,
    pub record_type: &'static str,
    pub record_id: String,
}

/// Closed owner kind -> (SQL discriminator, expected arity, record type).
pub fn kind_spec(kind: &str) -> Option<(&'static str, usize, &'static str)> {
    Some(match kind {
        "review.evidence" => ("review_evidence", 12, "company_package_review_evidence"),
        "grant.revision-anchor" => ("grant_revision_anchor", 1, "company_package_revisions"),
        "change-set.receipt" => ("change_set_receipt", 6, "company_package_change_sets"),
        "usage.snapshot-ensure" => ("usage_snapshot_ensure", 10, "billing_usage_snapshots"),
        "usage.snapshot-finish" => ("usage_snapshot_finish", 14, "billing_usage_snapshots"),
        "usage.meter" => ("usage_meter", 28, "billing_usage_meters"),
        "usage.meter-prune" => ("usage_meter_prune", 3, "billing_usage_meters"),
        "usage.seat" => ("usage_seat", 5, "billing_seat_usage"),
        "usage.seat-prune" => ("usage_seat_prune", 3, "billing_seat_usage"),
        "purchase.assertion" => ("purchase_assertion", 3, "command_assertions"),
        _ => return None,
    })
}

pub fn parse_owner_write(raw: &Value) -> Result<OwnerWrite, BusinessError> {
    let kind = raw
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let (key, arity, record_type) = kind_spec(kind).ok_or_else(invalid)?;
    let values = match raw.get("values") {
        Some(Value::Array(items)) => items.clone(),
        _ => return Err(invalid()),
    };
    if values.len() != arity
        || values
            .iter()
            .any(|value| matches!(value, Value::Object(_) | Value::Array(_)))
    {
        return Err(invalid());
    }
    let record_id = match raw.get("recordId") {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= 200 => value.clone(),
        _ => return Err(invalid()),
    };
    let _ = key;
    Ok(OwnerWrite {
        kind: key,
        values,
        record_type,
        record_id,
    })
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCIAL_OWNER_WRITE_INVALID",
        "Review the Commercial owner write.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Unavailable,
        "COMMERCIAL_OWNER_WRITE_UNAVAILABLE",
        "The native Commercial service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn closed_registry_and_arity() {
        assert!(kind_spec("review.evidence").is_some());
        assert!(kind_spec("bogus").is_none());
        let good = json!({"kind":"grant.revision-anchor","recordId":"c","values":["c"]});
        assert!(parse_owner_write(&good).is_ok());
        let bad = json!({"kind":"grant.revision-anchor","recordId":"c","values":[]});
        assert!(parse_owner_write(&bad).is_err());
    }
}
