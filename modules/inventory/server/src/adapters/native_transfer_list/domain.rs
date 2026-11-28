//! Pure request/response contract for the native stock-transfer list read.
//!
//! This slice owns exactly one operation: the holding-scoped `listTransfers`
//! read that the TypeScript compatibility adapter used to execute itself. The
//! domain owns no SQL and no I/O. The caller's Company/branch context arrives
//! through headers, is bounded here, and is bound as a value by the Postgres
//! adapter; no predicate from the request is ever interpolated into SQL.

use erpx_platform_runtime_server::public::http_support::serde_json::Value;
use erpx_platform_shared_server::public::BusinessError;

/// Owner-local native route mounted by composition.
pub const READ_PATH: &str = "/api/v1/holdings/{holding_slug}/inventory/native-transfer-list/read";

/// Number of values the retained transfer-list statement binds in order:
/// Company, branch, branch, branch.
pub const VALUE_COUNT: usize = 4;

/// A bounded, already-authorized transfer-list read request. The wire body is
/// empty; identity and branch scope come only from the request headers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferListRequest {
    pub company: String,
    /// `None` is the holding-wide scope the TS adapter received as a null
    /// `actor.branchId`.
    pub branch: Option<String>,
}

/// Fixed owner response: only an array of complete TransferRow objects.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TransferListResponse {
    pub rows: Vec<Value>,
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

impl TransferListRequest {
    /// Fail-closed validation before the statement is chosen or bound.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company) {
            return Err(invalid(
                "INVENTORY_TRANSFER_LIST_COMPANY_INVALID",
                "Use one bounded Company context.",
            ));
        }
        if self.branch.as_deref().is_some_and(|branch| !valid_id(branch)) {
            return Err(invalid(
                "INVENTORY_TRANSFER_LIST_BRANCH_INVALID",
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
        for id in ["company-1", "h001", "warehouse.a:1", "x_y"] {
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
        let holding = TransferListRequest {
            company: "company-1".into(),
            branch: None,
        };
        assert!(holding.validate().is_ok());
        let branch = TransferListRequest {
            company: "company-1".into(),
            branch: Some("branch-1".into()),
        };
        assert!(branch.validate().is_ok());
        for bad in [
            TransferListRequest {
                company: String::new(),
                branch: None,
            },
            TransferListRequest {
                company: "company-1".into(),
                branch: Some("bad branch".into()),
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }

    #[test]
    fn values_keep_the_retained_four_value_bind_order() {
        let holding = TransferListRequest {
            company: "company-1".into(),
            branch: None,
        };
        let values = holding.values();
        assert_eq!(values.len(), VALUE_COUNT);
        assert_eq!(values[0], Value::String("company-1".into()));
        assert!(values[1..].iter().all(|value| *value == Value::Null));

        let branch = TransferListRequest {
            company: "company-1".into(),
            branch: Some("branch-1".into()),
        };
        let values = branch.values();
        assert_eq!(values.len(), VALUE_COUNT);
        assert!(values[1..]
            .iter()
            .all(|value| *value == Value::String("branch-1".into())));
    }
}
