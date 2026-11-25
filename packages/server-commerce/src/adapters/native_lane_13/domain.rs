//! Pure domain for the native Commerce entrypoint identity read family.
//!
//! These facts gate module routing and native-eligibility decisions. The owned
//! response is deliberately small: an order/fulfillment/return kind, and the
//! mutable order state only for the `order_facts` capability. No Company,
//! permission or cross-owner projection is inferred here.

use erpx_server_shared::public::{BusinessError, RecordId};
use serde_json::Value;

/// The five owner-local reads retired from `adapters/entrypoint-facts.ts`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EntrypointKind {
    OrderKind,
    OrderFacts,
    FulfillmentKind,
    InvoiceKind,
    ReturnKind,
}

impl EntrypointKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OrderKind => "orderKind",
            Self::OrderFacts => "orderFacts",
            Self::FulfillmentKind => "fulfillmentKind",
            Self::InvoiceKind => "invoiceKind",
            Self::ReturnKind => "returnKind",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "orderKind" => Some(Self::OrderKind),
            "orderFacts" => Some(Self::OrderFacts),
            "fulfillmentKind" => Some(Self::FulfillmentKind),
            "invoiceKind" => Some(Self::InvoiceKind),
            "returnKind" => Some(Self::ReturnKind),
            _ => None,
        }
    }

    pub const fn carries_state(self) -> bool {
        matches!(self, Self::OrderFacts)
    }
}

/// Bounded request carried over the wire. No SQL or caller-supplied predicate
/// is accepted; only the registered discriminator and one record identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntrypointQuery {
    pub kind: EntrypointKind,
    pub id: String,
}

/// Owner response. `state` is present only for `order_facts`, preserving the
/// exact retired call-site shapes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntrypointFacts {
    pub kind: String,
    pub state: Option<String>,
}

impl EntrypointFacts {
    pub fn into_value(self) -> Value {
        let mut value = serde_json::json!({"kind": self.kind});
        if let Some(state) = self.state {
            value
                .as_object_mut()
                .expect("object literal")
                .insert("state".to_owned(), Value::String(state));
        }
        value
    }
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_ENTRYPOINT_READ_REQUEST_INVALID",
        "Use one bounded Commerce entrypoint read.",
    )
}

/// Record identities are bounded before they reach a bind parameter.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
}

pub fn validate(value: &Value, company: &str) -> Result<(EntrypointQuery, String), BusinessError> {
    let kind = value.get("kind").and_then(Value::as_str).ok_or_else(invalid)?;
    let id = value.get("id").and_then(Value::as_str).ok_or_else(invalid)?;
    let kind = EntrypointKind::parse(kind).ok_or_else(invalid)?;
    if !valid_id(id) || !valid_id(company) {
        return Err(invalid());
    }
    Ok((EntrypointQuery { kind, id: id.to_owned() }, company.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_registry_is_closed() {
        for kind in [
            "orderKind",
            "orderFacts",
            "fulfillmentKind",
            "invoiceKind",
            "returnKind",
        ] {
            let parsed = EntrypointKind::parse(kind).expect("registered kind");
            assert_eq!(parsed.as_str(), kind);
        }
        assert!(EntrypointKind::parse("SELECT 1").is_none());
        assert!(EntrypointKind::parse("").is_none());
        assert!(EntrypointKind::OrderFacts.carries_state());
        assert!(!EntrypointKind::OrderKind.carries_state());
    }

    #[test]
    fn validation_rejects_unbounded_identities() {
        assert!(!valid_id(""));
        assert!(!valid_id("bad id!"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_id("order-1"));
        let request = serde_json::json!({"kind": "orderFacts", "id": "order-1"});
        let (query, company) = validate(&request, "co-1").expect("valid request");
        assert_eq!(query.kind, EntrypointKind::OrderFacts);
        assert_eq!(query.id, "order-1");
        assert_eq!(company, "co-1");
        assert!(validate(&serde_json::json!({"kind": "drop", "id": "x"}), "co-1").is_err());
        assert!(validate(&request, "bad company!").is_err());
    }

    #[test]
    fn response_shape_omits_state_for_kind_reads() {
        let kind = EntrypointFacts {
            kind: "sales".to_owned(),
            state: None,
        }
        .into_value();
        assert_eq!(kind, serde_json::json!({"kind": "sales"}));
        assert!(kind.get("state").is_none());
        let facts = EntrypointFacts {
            kind: "sales".to_owned(),
            state: Some("draft".to_owned()),
        }
        .into_value();
        assert_eq!(facts, serde_json::json!({"kind": "sales", "state": "draft"}));
    }
}
