//! Pure request/response contract for the native stock-transfer detail read.
//!
//! This slice owns exactly one operation: the holding/branch-scoped `getTransfer`
//! read whose SQL the TypeScript compatibility adapter used to execute itself.
//! The domain owns no SQL and no I/O. Company, branch and transfer id arrive as
//! bounded wire values and are bound by the Postgres adapter; no predicate from
//! the request is ever interpolated into SQL.

use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::BusinessError;

/// Owner-local native route mounted by composition.
pub const READ_PATH: &str = "/api/v1/holdings/{holding_slug}/inventory/native-transfer-detail/read";

/// Number of values the retained detail statement binds in order: transfer id,
/// Company, branch, branch, branch.
pub const VALUE_COUNT: usize = 5;

/// A bounded, already-authorized transfer-detail read request. The transfer id
/// travels in the request body; identity and branch scope come only from headers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferDetailRequest {
    pub company: String,
    pub branch: Option<String>,
    pub id: String,
}

/// Fixed owner response: one complete TransferRow object or `null`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TransferDetailResponse {
    pub row: Option<Value>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

/// Identities are bounded wire scalars, never SQL text.
pub fn valid_id(value: &str) -> bool {
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

impl TransferDetailRequest {
    /// Fail-closed validation before the statement is chosen or bound.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company) {
            return Err(invalid(
                "INVENTORY_TRANSFER_DETAIL_COMPANY_INVALID",
                "Use one bounded Company context.",
            ));
        }
        if !valid_id(&self.id) {
            return Err(invalid(
                "INVENTORY_TRANSFER_DETAIL_ID_INVALID",
                "Use one bounded transfer identity.",
            ));
        }
        if self.branch.as_deref().is_some_and(|branch| !valid_id(branch)) {
            return Err(invalid(
                "INVENTORY_TRANSFER_DETAIL_BRANCH_INVALID",
                "Use one bounded branch scope.",
            ));
        }
        Ok(())
    }

    /// Values bound to the byte-identical owner statement, in bind order.
    pub fn values(&self) -> Vec<Value> {
        let branch = match &self.branch {
            Some(branch) => Value::String(branch.clone()),
            None => Value::Null,
        };
        vec![
            Value::String(self.id.clone()),
            Value::String(self.company.clone()),
            branch.clone(),
            branch.clone(),
            branch,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_and_slug_allow_lists_are_bounded() {
        for id in ["company-1", "transfer-1", "warehouse.a:1", "x_y"] {
            assert!(valid_id(id));
        }
        let long = "x".repeat(201);
        for id in ["", long.as_str(), "a b", "a'b", "a;drop"] {
            assert!(!valid_id(id));
        }
        assert!(valid_slug("acme"));
        assert!(valid_slug("erpx-internal"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("a/b"));
    }

    #[test]
    fn request_validation_admits_holding_and_branch_scope_only() {
        let base = TransferDetailRequest {
            company: "company-1".into(),
            branch: None,
            id: "transfer-1".into(),
        };
        assert!(base.validate().is_ok());
        assert!(TransferDetailRequest {
            branch: Some("branch-1".into()),
            ..base.clone()
        }
        .validate()
        .is_ok());
        let long = "x".repeat(201);
        for bad in [
            TransferDetailRequest {
                company: String::new(),
                ..base.clone()
            },
            TransferDetailRequest {
                id: String::new(),
                ..base.clone()
            },
            TransferDetailRequest {
                id: long,
                ..base.clone()
            },
            TransferDetailRequest {
                branch: Some("bad branch".into()),
                ..base
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }

    #[test]
    fn values_keep_the_retained_five_value_bind_order() {
        let holding = TransferDetailRequest {
            company: "company-1".into(),
            branch: None,
            id: "transfer-1".into(),
        };
        let values = holding.values();
        assert_eq!(values.len(), VALUE_COUNT);
        assert_eq!(values[0], Value::String("transfer-1".into()));
        assert_eq!(values[1], Value::String("company-1".into()));
        assert!(values[2..].iter().all(|value| *value == Value::Null));

        let branch = TransferDetailRequest {
            branch: Some("branch-1".into()),
            ..holding
        };
        let values = branch.values();
        assert!(values[2..]
            .iter()
            .all(|value| *value == Value::String("branch-1".into())));
    }
}
