//! Pure domain for the native employee-request purchase-order scope read.
//!
//! No SQL, transport or I/O. The shape is the exact six-column set the retired
//! TypeScript `.first()` reader returned.

use erpx_platform_shared_server::public::{BusinessError, RecordId};
use serde_json::Value;

/// The purchase-order facts the Records link-order action checks before it
/// stages a guard. `branch_id`/`department_id` stay nullable exactly as the
/// retired row did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmployeeOrder {
    pub kind: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub state: String,
    pub version: i64,
}

impl EmployeeOrder {
    /// Exact retired payload: six columns, no extra or renamed field.
    pub fn into_value(self) -> Value {
        serde_json::json!({
            "kind": self.kind,
            "company_id": self.company_id,
            "branch_id": self.branch_id,
            "department_id": self.department_id,
            "state": self.state,
            "version": self.version,
        })
    }
}

/// Native envelope. `null` is the exact equivalent of a retired `.first()`
/// miss; the Records caller's `if (!order)` check keeps its behavior.
pub fn project(order: Option<EmployeeOrder>) -> Value {
    serde_json::json!({
        "data": order.map(EmployeeOrder::into_value),
    })
}

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_EMPLOYEE_ORDER_READ_REQUEST_INVALID",
        "Use one bounded Commerce employee-request order read.",
    )
}

/// Identities are bounded before they reach a bind parameter.
pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && RecordId::new(value).is_ok()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:".contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> EmployeeOrder {
        EmployeeOrder {
            kind: "purchase".to_owned(),
            company_id: "co-1".to_owned(),
            branch_id: Some("br-1".to_owned()),
            department_id: None,
            state: "approved".to_owned(),
            version: 3,
        }
    }

    #[test]
    fn payload_has_every_retired_column() {
        let value = order().into_value();
        let object = value.as_object().expect("object");
        assert_eq!(object.len(), 6);
        for key in [
            "kind",
            "company_id",
            "branch_id",
            "department_id",
            "state",
            "version",
        ] {
            assert!(object.contains_key(key), "missing {key}");
        }
        assert_eq!(value["kind"], "purchase");
        assert_eq!(value["version"], 3);
        assert!(value["department_id"].is_null());
    }

    #[test]
    fn envelope_keeps_a_miss_as_null() {
        let value = project(Some(order()));
        assert_eq!(value.as_object().expect("object").len(), 1);
        assert_eq!(value["data"]["company_id"], "co-1");
        assert_eq!(project(None), serde_json::json!({"data": null}));
    }

    #[test]
    fn identities_are_bounded() {
        assert!(!valid_id(""));
        assert!(!valid_id("bad id!"));
        assert!(!valid_id(&"x".repeat(201)));
        assert!(valid_id("order-1"));
        assert!(valid_id("inv:issued.1"));
    }
}
