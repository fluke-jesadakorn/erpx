//! Pure domain kind registry for the native Ecommerce wave2 owner-write slice.
//!
//! Every Ecommerce TS write literal is retired in favour of one of these closed
//! owner-owned kinds. The kind allow-list and arity rules live here (pure); the
//! application executor and the owner Postgres adapter own the actual write.
//! No TypeScript SQL copy, no fallback and no JSON SQL mirror remain.
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;

pub const WAVE2_OPERATION: &str = "ecommerce.wave2.owner-write";
pub const CHECKOUT_RECORD_TYPE: &str = "ecommerce_checkouts";
pub const CART_RECORD_TYPE: &str = "ecommerce_carts";
pub const CHANNEL_RECORD_TYPE: &str = "ecommerce_channels";
pub const LISTING_RECORD_TYPE: &str = "ecommerce_listings";

/// A validated native owner write: one closed `kind`, its positional values and
/// the aggregate record id. Values are data only; never SQL text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerWrite {
    pub kind: &'static str,
    pub values: Vec<Value>,
    pub record_type: &'static str,
    pub record_id: String,
}

/// Closed owner kind -> (SQL discriminator, expected value count, record type).
/// The match here is the single source of truth for accepted writes.
pub fn kind_spec(kind: &str) -> Option<(&'static str, usize, &'static str)> {
    Some(match kind {
        "channel.create" => ("channel_create", 8, CHANNEL_RECORD_TYPE),
        "listing.create" => ("listing_create", 9, LISTING_RECORD_TYPE),
        "cart.create" => ("cart_create", 11, CART_RECORD_TYPE),
        "cart.add-line" => ("cart_add_line", 8, CART_RECORD_TYPE),
        "cart.version-bump" => ("cart_version_bump", 2, CART_RECORD_TYPE),
        "cart.abandon" => ("cart_abandon", 2, CART_RECORD_TYPE),
        "checkout.create" => ("checkout_create", 10, CHECKOUT_RECORD_TYPE),
        "checkout.confirm" => ("checkout_confirm", 4, CHECKOUT_RECORD_TYPE),
        "checkout.fulfillment" => ("checkout_fulfillment", 5, CHECKOUT_RECORD_TYPE),
        "checkout.return" => ("checkout_return", 9, CHECKOUT_RECORD_TYPE),
        "channel.review" => ("channel_review", 4, CHANNEL_RECORD_TYPE),
        "listing.review" => ("listing_review", 4, LISTING_RECORD_TYPE),
        _ => return None,
    })
}

/// Parse a retained owner-write request. `recordId` is supplied by the caller
/// from its already-authorized context; values are bounded in count and size.
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
        || values.iter().any(|value| match value {
            Value::String(text) => text.len() > 8192,
            Value::Number(_) | Value::Bool(_) | Value::Null => false,
            _ => true,
        })
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
        "ECOMMERCE_WAVE2_INVALID",
        "Review the E-commerce check-out write.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        erpx_platform_shared_server::public::FailureKind::Unavailable,
        "ECOMMERCE_WAVE2_UNAVAILABLE",
        "The native E-commerce service is unavailable.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn closed_kind_registry() {
        assert!(kind_spec("channel.create").is_some());
        assert!(kind_spec("checkout.confirm").is_some());
        assert!(kind_spec("bogus.kind").is_none());
    }

    #[test]
    fn parse_validates_arity_and_values() {
        let good = json!({"kind":"cart.abandon","recordId":"c","values":[1,"c"]});
        assert!(parse_owner_write(&good).is_ok());
        let bad = json!({"kind":"cart.abandon","recordId":"c","values":[1]});
        assert!(parse_owner_write(&bad).is_err());
        let nested = json!({"kind":"cart.abandon","recordId":"c","values":[1,{"x":1}]});
        assert!(parse_owner_write(&nested).is_err());
    }
}
