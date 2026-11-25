//! Pure response types, request validation and invariants for native lane 17.
//!
//! This module owns no SQL and no I/O. It admits one of six closed owner-local
//! read kinds (four landed-cost reads and two replenishment reads), bounds every
//! caller value, and fixes the response shape that the owner-local axum route
//! returns. Nothing here grants authority; the caller's Holding/company context
//! is carried by headers and checked against the row scope on the application side.

use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::BusinessError;

/// Owner-local native route mounted by composition.
pub const READ_PATH: &str = "/api/v1/holdings/{holding_slug}/inventory/native-lane-17/read";

/// The closed owner-local read kinds: landed-cost reads and the replenishment
/// reads (reorder policies and service spares).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaneReadKind {
    /// `scopedCost`: one cost in the caller's company/branch.
    LandedCostScoped,
    /// `landedCostsForOrder`: costs joined to a purchase order in scope.
    LandedCostsForOrder,
    /// `landedCostOptions` moved-stock blocker probe.
    LandedCostMovedStock,
    /// `landedCostWork` open-cost rows in scope.
    LandedCostWorkRows,
    /// `listInventoryReorderPolicies`: company/branch scoped policy rows.
    ReorderPolicies,
    /// `availableServiceSpares`: company/branch scoped lot spares.
    ServiceSpares,
}

impl LaneReadKind {
    pub const ALL: [Self; 6] = [
        Self::LandedCostScoped,
        Self::LandedCostsForOrder,
        Self::LandedCostMovedStock,
        Self::LandedCostWorkRows,
        Self::ReorderPolicies,
        Self::ServiceSpares,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::LandedCostScoped => "landed_cost_scoped",
            Self::LandedCostsForOrder => "landed_costs_for_order",
            Self::LandedCostMovedStock => "landed_cost_moved_stock",
            Self::LandedCostWorkRows => "landed_cost_work_rows",
            Self::ReorderPolicies => "reorder_policies",
            Self::ServiceSpares => "service_spares",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// Number of values the retained TS read binds into the owner statement.
    pub fn value_count(self) -> usize {
        match self {
            Self::LandedCostScoped => 4,
            Self::LandedCostsForOrder => 4,
            Self::LandedCostMovedStock => 3,
            Self::LandedCostWorkRows => 2,
            Self::ReorderPolicies => 3,
            Self::ServiceSpares => 2,
        }
    }

    /// Position of the company id among the caller values, when the retained
    /// statement already carries a company predicate. The moved-stock probe is
    /// order-scoped in the TS source, so the native route adds its own company
    /// witness instead of trusting an absent caller company value.
    pub fn company_value_index(self) -> Option<usize> {
        match self {
            Self::LandedCostScoped | Self::LandedCostsForOrder => Some(1),
            Self::LandedCostWorkRows | Self::ReorderPolicies | Self::ServiceSpares => Some(0),
            Self::LandedCostMovedStock => None,
        }
    }

    /// Whether the owner must append its own company witness to the statement.
    pub fn needs_company_witness(self) -> bool {
        self.company_value_index().is_none()
    }
}

/// A bounded, already-authorized read request. The native app owns the SQL and
/// the projection; this struct is the entire wire contract.
#[derive(Clone, Debug)]
pub struct LaneReadRequest {
    pub company: String,
    pub kind: LaneReadKind,
    pub values: Vec<Value>,
}

/// Fixed owner response: only an array of row objects is ever returned.
#[derive(Clone, Debug, serde::Serialize)]
pub struct LaneReadResponse {
    pub rows: Vec<Value>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

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

/// A caller value is a bounded scalar or an explicit null (branch scope).
fn valid_value(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(_) => true,
        Value::Number(number) => number.as_i64().is_some(),
        // Values are always bound parameters, never SQL text. A bounded scalar
        // is enough here; identity-shaped values are checked against the header.
        Value::String(text) => !text.is_empty() && text.len() <= 200,
        Value::Array(_) | Value::Object(_) => false,
    }
}

impl LaneReadRequest {
    /// Fail-closed validation before any SQL is chosen or bound.
    pub fn validate(&self) -> Result<(), BusinessError> {
        if !valid_id(&self.company) {
            return Err(invalid(
                "INVENTORY_LANE17_COMPANY_INVALID",
                "Use one bounded Company context.",
            ));
        }
        if self.values.len() != self.kind.value_count()
            || !self.values.iter().all(valid_value)
        {
            return Err(invalid(
                "INVENTORY_LANE17_VALUES_INVALID",
                "Use the bounded native Inventory read values.",
            ));
        }
        if let Some(index) = self.kind.company_value_index() {
            if self.values[index].as_str() != Some(self.company.as_str()) {
                return Err(invalid(
                    "INVENTORY_LANE17_SCOPE_INVALID",
                    "The read Company differs from the request Company.",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::serde_json::json;

    #[test]
    fn read_kinds_are_a_closed_allow_list() {
        for kind in LaneReadKind::ALL {
            assert_eq!(LaneReadKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(LaneReadKind::parse("landed_cost_write"), None);
        assert_eq!(LaneReadKind::parse(""), None);
    }

    #[test]
    fn company_context_must_match_the_bound_company_value() {
        let request = LaneReadRequest {
            company: "company-1".into(),
            kind: LaneReadKind::LandedCostScoped,
            values: vec![json!("cost-1"), json!("company-1"), json!(null), json!(null)],
        };
        assert!(request.validate().is_ok());
        let changed = LaneReadRequest { company: "company-2".into(), ..request };
        assert_eq!(
            changed.validate().unwrap_err().code,
            "INVENTORY_LANE17_SCOPE_INVALID"
        );
    }

    #[test]
    fn caller_values_are_bounded_scalars() {
        let base = LaneReadRequest {
            company: "company-1".into(),
            kind: LaneReadKind::LandedCostsForOrder,
            values: vec![json!("order-1"), json!("company-1"), json!(null), json!(null)],
        };
        assert!(base.validate().is_ok());
        for bad in [
            vec![json!("order-1"), json!("company-1"), json!(null)],
            vec![json!(["x"]), json!("company-1"), json!(null), json!(null)],
            vec![json!("order-1"), json!("company-1"), json!({"x": 1}), json!(null)],
            vec![json!("x".repeat(201)), json!("company-1"), json!(null), json!(null)],
        ] {
            assert_eq!(
                LaneReadRequest { values: bad, ..base.clone() }
                    .validate()
                    .unwrap_err()
                    .code,
                "INVENTORY_LANE17_VALUES_INVALID"
            );
        }
    }

    #[test]
    fn replenishment_reads_are_company_scoped_without_a_witness() {
        for kind in [LaneReadKind::ReorderPolicies, LaneReadKind::ServiceSpares] {
            assert!(!kind.needs_company_witness());
            assert_eq!(kind.company_value_index(), Some(0));
        }
        assert_eq!(LaneReadKind::ReorderPolicies.value_count(), 3);
        assert_eq!(LaneReadKind::ServiceSpares.value_count(), 2);
        let request = LaneReadRequest {
            company: "company-1".into(),
            kind: LaneReadKind::ReorderPolicies,
            values: vec![json!("company-1"), json!("branch-1"), json!("branch-1")],
        };
        assert!(request.validate().is_ok());
        assert_eq!(
            LaneReadRequest { company: "company-2".into(), ..request }
                .validate()
                .unwrap_err()
                .code,
            "INVENTORY_LANE17_SCOPE_INVALID"
        );
    }

    #[test]
    fn moved_stock_probe_always_needs_an_owner_company_witness() {
        assert!(LaneReadKind::LandedCostMovedStock.needs_company_witness());
        for kind in LaneReadKind::ALL {
            if kind != LaneReadKind::LandedCostMovedStock {
                assert!(!kind.needs_company_witness());
            }
        }
    }
}
