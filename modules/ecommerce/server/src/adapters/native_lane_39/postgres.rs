//! Lane 39 Ecommerce postgres adapter: sqlx execution of the two owner-local
//! workspace list reads.
//!
//! Both statements are literal owner SQL with positional binds; the branch and
//! department predicates are the same NULL-tolerant scope the retained TS read
//! used, so bind order and row selection are preserved exactly.

use crate::adapters::native_lane_39::application::{WorkspaceListFuture, WorkspaceListRead};
use crate::adapters::native_lane_39::domain::{
    CartRow, ChannelRow, WorkspaceLists, WorkspaceScope, validate_scope,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::Row;

/// Retired owner literal: `SELECT * FROM ecommerce_channels WHERE company_id=? ORDER BY updated_at DESC`.
pub const CHANNELS_SQL: &str =
    "SELECT * FROM ecommerce_channels WHERE company_id=$1 ORDER BY updated_at DESC";

/// Retired owner literal: `SELECT c.* FROM ecommerce_carts c WHERE <cartScope> ORDER BY c.updated_at DESC`.
pub const CARTS_SQL: &str =
    "SELECT c.* FROM ecommerce_carts c WHERE c.company_id=$1 AND ($2::text IS NULL OR c.branch_id=$2) \
     AND ($3::text IS NULL OR c.department_id=$3) ORDER BY c.updated_at DESC";

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ECOMMERCE_WORKSPACE_LIST_UNAVAILABLE",
        "The native Ecommerce workspace service is unavailable.",
    )
}

pub struct PgWorkspaceListRead {
    pool: DatabasePool,
}

impl PgWorkspaceListRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl WorkspaceListRead for PgWorkspaceListRead {
    fn lists<'a>(
        &'a self,
        scope: WorkspaceScope<'a>,
    ) -> WorkspaceListFuture<'a, WorkspaceLists> {
        let pool = self.pool.clone();
        let company = scope.company_id.to_owned();
        let branch = scope.branch_id.map(str::to_owned);
        let department = scope.department_id.map(str::to_owned);
        Box::pin(async move {
            let checked = WorkspaceScope {
                company_id: &company,
                branch_id: branch.as_deref(),
                department_id: department.as_deref(),
            };
            validate_scope(&checked)?;

            let channel_rows = sqlx::query(CHANNELS_SQL)
                .bind(company.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            let channels = channel_rows
                .into_iter()
                .map(|row| {
                    Ok(ChannelRow {
                        id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                        company_id: row.try_get::<String, _>("company_id").map_err(|_| unavailable())?,
                        name: row.try_get::<String, _>("name").map_err(|_| unavailable())?,
                        channel_type: row.try_get::<String, _>("channel_type").map_err(|_| unavailable())?,
                        configuration_json: row
                            .try_get::<String, _>("configuration_json")
                            .map_err(|_| unavailable())?,
                        status: row.try_get::<String, _>("status").map_err(|_| unavailable())?,
                        created_by: row.try_get::<String, _>("created_by").map_err(|_| unavailable())?,
                        reviewed_by: row
                            .try_get::<Option<String>, _>("reviewed_by")
                            .map_err(|_| unavailable())?,
                        version: row.try_get::<i64, _>("version").map_err(|_| unavailable())?,
                        created_at: row.try_get::<i64, _>("created_at").map_err(|_| unavailable())?,
                        updated_at: row.try_get::<i64, _>("updated_at").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>()?;

            let cart_rows = sqlx::query(CARTS_SQL)
                .bind(company.as_str())
                .bind(branch.as_deref())
                .bind(department.as_deref())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            let carts = cart_rows
                .into_iter()
                .map(|row| {
                    Ok(CartRow {
                        id: row.try_get::<String, _>("id").map_err(|_| unavailable())?,
                        company_id: row.try_get::<String, _>("company_id").map_err(|_| unavailable())?,
                        branch_id: row.try_get::<String, _>("branch_id").map_err(|_| unavailable())?,
                        department_id: row
                            .try_get::<Option<String>, _>("department_id")
                            .map_err(|_| unavailable())?,
                        channel_id: row.try_get::<String, _>("channel_id").map_err(|_| unavailable())?,
                        party_id: row.try_get::<String, _>("party_id").map_err(|_| unavailable())?,
                        warehouse_id: row.try_get::<String, _>("warehouse_id").map_err(|_| unavailable())?,
                        currency: row.try_get::<String, _>("currency").map_err(|_| unavailable())?,
                        status: row.try_get::<String, _>("status").map_err(|_| unavailable())?,
                        created_by: row.try_get::<String, _>("created_by").map_err(|_| unavailable())?,
                        version: row.try_get::<i64, _>("version").map_err(|_| unavailable())?,
                        created_at: row.try_get::<i64, _>("created_at").map_err(|_| unavailable())?,
                        updated_at: row.try_get::<i64, _>("updated_at").map_err(|_| unavailable())?,
                    })
                })
                .collect::<Result<Vec<_>, BusinessError>>()?;

            Ok(WorkspaceLists { channels, carts })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_oracles_pin_the_retired_owner_literals() {
        assert_eq!(
            CHANNELS_SQL,
            "SELECT * FROM ecommerce_channels WHERE company_id=$1 ORDER BY updated_at DESC"
        );
        let normalized = CARTS_SQL.replace("\\\n     ", " ");
        assert_eq!(
            normalized,
            "SELECT c.* FROM ecommerce_carts c WHERE c.company_id=$1 AND ($2::text IS NULL OR c.branch_id=$2) \
             AND ($3::text IS NULL OR c.department_id=$3) ORDER BY c.updated_at DESC"
        );
        assert_eq!(CHANNELS_SQL.matches('$').count(), 1);
        // $2 and $3 appear twice each in the NULL-tolerant Branch/Department
        // predicates; there are exactly three distinct positional binds.
        let binds = normalized
            .match_indices('$')
            .map(|(index, _)| {
                normalized[index + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
            })
            .collect::<std::collections::BTreeSet<String>>();
        assert_eq!(binds.len(), 3);
        assert!(binds.contains("1") && binds.contains("2") && binds.contains("3"));
    }
}
