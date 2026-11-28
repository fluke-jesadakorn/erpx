//! Pure domain for the native Maintenance lane-36 attachment-target read.
//!
//! Both live TS consumers target the same owner-local relation
//! (`maintenance_work_orders`). This module owns the closed target type, the
//! bounded identifiers and the exact descriptor response shape. No SQL or I/O
//! lives here.

use erpx_platform_shared_server::public::BusinessError;
use serde::Serialize;
use serde_json::{Value, json};

/// The only owner-local descriptor type admitted by both fact readers.
pub const TARGET_MAINTENANCE_WORK: &str = "maintenance_work";
pub const TARGET_TYPES: [&str; 1] = [TARGET_MAINTENANCE_WORK];

pub fn valid_target_type(value: &str) -> bool {
    TARGET_TYPES.contains(&value)
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MAINTENANCE_LANE36_QUERY_INVALID",
        "Use one bounded Maintenance attachment target read.",
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescriptorScope {
    pub target_id: String,
    pub company_id: String,
}

impl DescriptorScope {
    pub fn new(target_type: &str, target_id: &str, company_id: &str) -> Result<Self, BusinessError> {
        if !valid_target_type(target_type) || !valid_id(target_id) || !valid_id(company_id) {
            return Err(invalid());
        }
        Ok(Self {
            target_id: target_id.to_owned(),
            company_id: company_id.to_owned(),
        })
    }

    pub fn oracle_key(&self) -> Value {
        json!([self.target_id.as_str(), self.company_id.as_str()])
    }
}

/// Exact descriptor row shared by the module-scoped and policy fact readers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DescriptorRow {
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub version: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_target_type_and_bounded_ids() {
        assert!(valid_target_type(TARGET_MAINTENANCE_WORK));
        assert!(!valid_target_type("finance_journal"));
        assert!(valid_id("work-order-1"));
        assert!(!valid_id(""));
        assert!(!valid_id("x/y"));
    }

    #[test]
    fn scope_is_bounded_and_pins_oracle_key() {
        let scope = DescriptorScope::new(TARGET_MAINTENANCE_WORK, "work-1", "company-1").unwrap();
        assert_eq!(scope.oracle_key(), json!(["work-1", "company-1"]));
        assert!(DescriptorScope::new("other", "work-1", "company-1").is_err());
        assert!(DescriptorScope::new(TARGET_MAINTENANCE_WORK, "work-1", "").is_err());
    }

    #[test]
    fn descriptor_shape_is_closed() {
        let row = DescriptorRow {
            company_id: "company-1".into(),
            branch_id: Some("branch-1".into()),
            department_id: None,
            version: 3,
        };
        let value = serde_json::to_value(&row).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 4);
        assert_eq!(value["company_id"], "company-1");
        assert!(value["department_id"].is_null());
    }
}
