//! Native Ecommerce checkout detail-link domain: pure row types, validation and
//! projection invariants for the owner-executed fulfillment/return link reads.
//!
//! Fulfillment links are Ecommerce-owned; return links join the closed Commerce
//! return projection. Both statements are SELECT-only and carry no write, lock
//! or receipt authority. Rows stay opaque JSON so every retired column,
//! including the Commerce alias columns, survives the cutover.

use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

pub const MAX_ID_LEN: usize = 200;
pub const MAX_ROWS: usize = 5000;
pub const MAX_KEY_LEN: usize = 64;
pub const MAX_JSON_LEN: usize = 64 * 1024;

/// One opaque owner row. The retired readers returned every selected column,
/// so projection validates shape without dropping a field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkRow(pub Value);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailLinks {
    pub fulfillment: Vec<LinkRow>,
    pub returns: Vec<LinkRow>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn validate_checkout(checkout_id: &str) -> Result<(), BusinessError> {
    if !valid_id(checkout_id) {
        return Err(invalid(
            "ECOMMERCE_DETAIL_LINKS_REQUEST_INVALID",
            "Use one bounded Ecommerce checkout identity.",
        ));
    }
    Ok(())
}

fn project_row(value: Value) -> Result<LinkRow, BusinessError> {
    let object = value.as_object().ok_or_else(|| {
        invalid(
            "ECOMMERCE_DETAIL_LINKS_RESPONSE_INVALID",
            "The native Ecommerce detail links returned an invalid row.",
        )
    })?;
    if object.len() > MAX_KEY_LEN
        || !matches!(object.get("id"), Some(Value::String(_)))
        || !matches!(object.get("checkout_id"), Some(Value::String(_)))
        || object.keys().any(|key| key.len() > MAX_KEY_LEN)
        || object.values().any(|item| {
            !matches!(item, Value::String(_) | Value::Number(_) | Value::Null)
        })
        || serde_json::to_string(object).is_ok_and(|text| text.len() > MAX_JSON_LEN)
    {
        return Err(invalid(
            "ECOMMERCE_DETAIL_LINKS_RESPONSE_INVALID",
            "The native Ecommerce detail links returned an invalid row.",
        ));
    }
    Ok(LinkRow(Value::Object(object.clone())))
}

/// Refuse drifted or oversized rows instead of emitting a partial payload.
pub fn project(links: DetailLinks) -> Result<DetailLinks, BusinessError> {
    if links.fulfillment.len() > MAX_ROWS || links.returns.len() > MAX_ROWS {
        return Err(invalid(
            "ECOMMERCE_DETAIL_LINKS_RESPONSE_INVALID",
            "The native Ecommerce detail links returned too many rows.",
        ));
    }
    Ok(DetailLinks {
        fulfillment: links
            .fulfillment
            .into_iter()
            .map(|row| project_row(row.0))
            .collect::<Result<Vec<_>, _>>()?,
        returns: links
            .returns
            .into_iter()
            .map(|row| project_row(row.0))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_DETAIL_LINKS_UNAVAILABLE",
        "The native Ecommerce detail link service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> LinkRow {
        LinkRow(serde_json::json!({
            "id": "link-1",
            "checkout_id": "checkout-1",
            "fulfillment_id": "fulfillment-1",
            "source_order_version": 2,
            "status": "confirmed",
            "created_at": 42,
        }))
    }

    #[test]
    fn checkout_validation_is_bounded() {
        assert!(validate_checkout("checkout-1").is_ok());
        assert_eq!(
            validate_checkout("").unwrap_err().code,
            "ECOMMERCE_DETAIL_LINKS_REQUEST_INVALID"
        );
        assert_eq!(
            validate_checkout("bad id").unwrap_err().code,
            "ECOMMERCE_DETAIL_LINKS_REQUEST_INVALID"
        );
        assert!(validate_checkout(&"x".repeat(MAX_ID_LEN + 1)).is_err());
    }

    #[test]
    fn projection_keeps_every_column_and_refuses_drift() {
        let links = DetailLinks {
            fulfillment: vec![row()],
            returns: vec![LinkRow(serde_json::json!({
                "id": "return-link-1",
                "checkout_id": "checkout-1",
                "current_return_status": "approved",
                "current_return_version": 3,
            }))],
        };
        let projected = project(links).unwrap();
        let object = projected.fulfillment[0].0.as_object().unwrap();
        assert_eq!(object.len(), 6);
        assert_eq!(object["fulfillment_id"], serde_json::json!("fulfillment-1"));
        assert_eq!(projected.returns[0].0["current_return_version"], serde_json::json!(3));

        let mut drifted = DetailLinks { fulfillment: vec![row()], returns: vec![] };
        drifted.fulfillment[0].0["unexpected"] = serde_json::json!(true);
        assert_eq!(
            project(drifted).unwrap_err().code,
            "ECOMMERCE_DETAIL_LINKS_RESPONSE_INVALID"
        );

        let missing = DetailLinks {
            fulfillment: vec![LinkRow(serde_json::json!({"id": "link-1"}))],
            returns: vec![],
        };
        assert_eq!(
            project(missing).unwrap_err().code,
            "ECOMMERCE_DETAIL_LINKS_RESPONSE_INVALID"
        );
    }

    #[test]
    fn oversized_row_lists_fail_closed() {
        let links = DetailLinks {
            fulfillment: vec![row(); MAX_ROWS + 1],
            returns: vec![],
        };
        assert_eq!(
            project(links).unwrap_err().code,
            "ECOMMERCE_DETAIL_LINKS_RESPONSE_INVALID"
        );
    }
}
