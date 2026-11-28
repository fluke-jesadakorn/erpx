//! Pure domain for the native Manufacturing production accounting-profiles
//! owner read. This module owns the exact projection, its bounds and the
//! response shape. It emits no SQL, opens no connection and performs no IO.
//!
//! The projection is the byte-for-byte successor of the frozen
//! `RD_ACCOUNTING_PROFILES` read plan (`public/native_command.rs`). The only
//! difference is the Postgres bind marker (`$1` instead of `?`); the column
//! list, table, predicate and ordering are unchanged.

use erpx_platform_shared_server::public::BusinessError;
use serde::Serialize;

/// Exact owner SQL for the production workspace/detail accounting-profiles
/// projection. Manufacturing-owned relation only: no cross-owner join.
pub const ACCOUNTING_PROFILES_SQL: &str = "SELECT id,status,version,currency,chart_id,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id FROM manufacturing_accounting_profiles WHERE company_id=$1 ORDER BY updated_at DESC";

pub const MAX_IDENTIFIER: usize = 200;

pub fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_PRODUCTION_PROFILES_INVALID",
        "Use a bounded Manufacturing company scope.",
    )
}

/// The live caller already resolved this company from its WorkContext; the
/// owner validates the wire fact again so a malformed scope never reaches SQL.
pub fn validate_company(value: &str) -> Result<(), BusinessError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER
        || value.contains('\u{0000}')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid());
    }
    Ok(())
}

/// Complete row returned by the retired TypeScript operation. Field names are
/// intentionally snake_case: the live caller returns raw SQL rows and maps
/// `results` directly into the production workspace payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccountingProfileRow {
    pub id: String,
    pub status: String,
    pub version: i64,
    pub currency: String,
    pub chart_id: String,
    pub inventory_account_id: String,
    pub wip_account_id: String,
    pub labor_account_id: String,
    pub overhead_account_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn company_scope_is_bounded() {
        assert!(validate_company("company-1").is_ok());
        assert!(validate_company("company_1").is_ok());
        assert!(validate_company("").is_err());
        assert!(validate_company("bad company!").is_err());
        assert!(validate_company("bad\0id").is_err());
        assert!(validate_company(&"x".repeat(MAX_IDENTIFIER + 1)).is_err());
    }

    #[test]
    fn row_serializes_to_the_exact_live_caller_field_names() {
        let row = AccountingProfileRow {
            id: "profile-1".into(),
            status: "approved".into(),
            version: 3,
            currency: "THB".into(),
            chart_id: "chart-1".into(),
            inventory_account_id: "inv".into(),
            wip_account_id: "wip".into(),
            labor_account_id: "labor".into(),
            overhead_account_id: "overhead".into(),
        };
        assert_eq!(
            serde_json::to_value(&row).unwrap(),
            serde_json::json!({
                "id": "profile-1",
                "status": "approved",
                "version": 3,
                "currency": "THB",
                "chart_id": "chart-1",
                "inventory_account_id": "inv",
                "wip_account_id": "wip",
                "labor_account_id": "labor",
                "overhead_account_id": "overhead",
            })
        );
    }

    #[test]
    fn owner_sql_matches_the_frozen_read_plan_with_only_the_bind_marker_changed() {
        let registered = crate::public::native_command::PLAN_ACCOUNTING_PROFILES.sql;
        assert_eq!(ACCOUNTING_PROFILES_SQL, registered.replace('?', "$1"));
        assert_eq!(ACCOUNTING_PROFILES_SQL.matches("$1").count(), 1);
        assert!(ACCOUNTING_PROFILES_SQL.starts_with("SELECT "));
        assert!(!ACCOUNTING_PROFILES_SQL.contains(';'));
    }
}
