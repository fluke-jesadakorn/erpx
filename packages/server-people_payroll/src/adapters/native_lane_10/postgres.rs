//! sqlx execution of the migrated `activeLeavePolicySettings` read. The
//! statement keeps the owner projection, active-status filter and Company scope;
//! the D1 `?` bind is rewritten for Postgres and the caller's Company travels as
//! a bound scalar rather than interpolated SQL.

use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::BusinessError;
use sha2::Digest;
use sqlx::Row;

use super::application::{LeavePolicyFuture, LeavePolicyRead};
use super::domain::{LeavePolicySetting, unavailable};

/// `activeLeavePolicySettings` in `adapters/flow-settings-facts.ts`, built by
/// `ppRd43ActiveLeavePolicyProjection('name,version,status')`.
pub const SQL_ACTIVE_LEAVE_POLICIES: &str = "SELECT name,version,status FROM leave_policy_versions WHERE company_id=$1 AND status='active' ORDER BY name,version DESC";

pub struct PgLeavePolicyRead {
    pool: DatabasePool,
}

impl PgLeavePolicyRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl LeavePolicyRead for PgLeavePolicyRead {
    fn active<'a>(
        &'a self,
        company: &'a str,
    ) -> LeavePolicyFuture<'a, Vec<LeavePolicySetting>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(SQL_ACTIVE_LEAVE_POLICIES)
                .bind(company.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|row| {
                    Ok(LeavePolicySetting {
                        name: row.try_get::<String, _>("name").map_err(|_| unavailable())?,
                        version: i64::from(row.try_get::<i32, _>("version").map_err(|_| unavailable())?),
                        status: row.try_get::<String, _>("status").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_keeps_owner_scope_and_active_filter() {
        assert!(SQL_ACTIVE_LEAVE_POLICIES.starts_with("SELECT name,version,status FROM leave_policy_versions"));
        assert!(SQL_ACTIVE_LEAVE_POLICIES.contains("company_id=$1"));
        assert!(SQL_ACTIVE_LEAVE_POLICIES.contains("status='active'"));
        assert!(SQL_ACTIVE_LEAVE_POLICIES.contains("ORDER BY name,version DESC"));
        assert!(!SQL_ACTIVE_LEAVE_POLICIES.contains('?'));
    }

    #[test]
    fn pins_rendered_sql_oracle() {
        let hex = sha2::Sha256::digest(SQL_ACTIVE_LEAVE_POLICIES.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "b42d6e6c68076be9bd3bc8f9dbc0d5da378ad9e3de5928f6d395d8ad76b3eaca",
            "update the oracle in native-lane-10 active-leave-policies client"
        );
    }
}
