//! Pure domain kind registry for the remaining native Manufacturing writes.
//!
//! Closed, owner-owned kinds for every TypeScript write literal retired in wave
//! 3. No IO, no SQL and no fallback: the application executor and owner Postgres
//! adapter own the actual write. The caller supplies only positional data
//! values; the statement text is selected here and in the adapter, never from
//! the request.
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use erpx_platform_runtime_server::public::http_support::serde_json::Value;

pub const OWNER_WRITE_OPERATION: &str = "manufacturing.owner-write.v1";

/// (SQL discriminator, exact arity, ledger record type).
pub fn kind_spec(kind: &str) -> Option<(&'static str, usize, &'static str)> {
    Some(match kind {
        "production.posting" => ("posting", 12, "manufacturing_order_postings"),
        "production.cost-movement" => ("cost_movement", 11, "manufacturing_cost_movements"),
        "order.create" => ("order_create", 11, "manufacturing_orders"),
        "order.release-event" => ("order_release_event", 5, "manufacturing_order_events"),
        "order.reservation" => ("order_reservation", 8, "manufacturing_material_reservations"),
        "order.output-report" => ("order_output_report", 5, "manufacturing_orders"),
        "order.operation-event" => ("order_operation_event", 9, "manufacturing_operation_events"),
        "order.reservation-release" => ("reservation_release", 1, "manufacturing_material_reservations"),
        "order.transition" => ("order_transition", 3, "manufacturing_orders"),
        "order.event" => ("order_event", 9, "manufacturing_order_events"),
        _ => return None,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerWrite {
    pub kind: &'static str,
    pub values: Vec<Value>,
    pub record_type: &'static str,
    pub record_id: String,
}

pub fn parse_owner_write(raw: &Value) -> Result<OwnerWrite, BusinessError> {
    let kind = raw.get("kind").and_then(Value::as_str).ok_or_else(invalid)?;
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
    Ok(OwnerWrite {
        kind: key,
        values,
        record_type,
        record_id,
    })
}

pub fn company(raw: &Value) -> Result<String, BusinessError> {
    match raw.get("companyId") {
        Some(Value::String(value)) if !value.is_empty() && value.len() <= 200 => Ok(value.clone()),
        _ => Err(invalid()),
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_OWNER_WRITE_INVALID",
        "Review the Manufacturing owner write.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_OWNER_WRITE_UNAVAILABLE",
        "The native Manufacturing service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    #[test]
    fn closed_registry_and_exact_arities() {
        let cases = [
            ("production.posting", 12),
            ("production.cost-movement", 11),
            ("order.create", 11),
            ("order.release-event", 5),
            ("order.reservation", 8),
            ("order.output-report", 5),
            ("order.operation-event", 9),
            ("order.reservation-release", 1),
            ("order.transition", 3),
            ("order.event", 9),
        ];
        for (kind, arity) in cases {
            let (_, spec_arity, _) = kind_spec(kind).unwrap_or_else(|| panic!("kind {kind} missing"));
            assert_eq!(spec_arity, arity, "arity mismatch for {kind}");
        }
        assert!(kind_spec("not_an_owner_write").is_none());
    }

    #[test]
    fn parse_requires_known_kind_and_exact_values() {
        let raw = json!({"kind":"order.transition","companyId":"c","recordId":"o","values":["planned",1,"o"]});
        let write = parse_owner_write(&raw).unwrap();
        assert_eq!(write.kind, "order_transition");
        assert_eq!(write.values.len(), 3);
        let short = json!({"kind":"order.transition","companyId":"c","recordId":"o","values":["planned"]});
        assert!(parse_owner_write(&short).is_err());
        assert!(company(&raw).is_ok());
    }
}
