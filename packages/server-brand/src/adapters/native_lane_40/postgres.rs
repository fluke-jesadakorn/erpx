//! Native lane 40 Brand postgres adapter: sqlx execution of the management
//! authority guards on the registered private Holding pool.
//!
//! Each statement is byte-identical to the retired owner literal apart from
//! the `?` binds becoming positional `$1`/`$n`. The relation, projection and
//! `kind`/`status` predicates stay owner-controlled; only bounded bind values
//! come from the request. No TypeScript unit, lock, receipt or plan is involved.

use crate::adapters::native_lane_40::application::{
    BranchGuardQuery, CompanyGuardQuery, GuardFuture, ManagementGuardRead,
};
use crate::adapters::native_lane_40::domain::{project_branch, validate_target, ActiveBranch};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

/// Canonical active organization node projection (Organization owner source),
/// inlined verbatim so the executed statement matches the retired literal.
#[cfg(test)]
const ORG_NODES: &str =
    "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes";

/// Exact owner active-Branch guard statement. Binds: `$1` id, `$2` company.
pub const BRANCH_GUARD_SQL: &str = concat!(
    "SELECT id,display_name FROM (",
    "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes",
    ") organization_nodes WHERE id=$1 AND company_id=$2 AND kind='branch' AND status='active'"
);

/// Exact owner active-Company guard statement. Bind: `$1` id.
pub const COMPANY_GUARD_SQL: &str = concat!(
    "SELECT 1 FROM (",
    "SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes",
    ") organization_nodes WHERE id=$1 AND kind='company' AND status='active'"
);

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BRAND_MANAGEMENT_READ_UNAVAILABLE",
        "The native Brand management read is unavailable.",
    )
}

/// Pool-backed owner reader. The pool is the registered private Holding pool;
/// this is the only layer that executes SQL for this capability.
pub struct PgManagementGuardRead {
    pool: DatabasePool,
}

impl PgManagementGuardRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl ManagementGuardRead for PgManagementGuardRead {
    fn active_branch<'a>(
        &'a self,
        query: &'a BranchGuardQuery,
    ) -> GuardFuture<'a, Option<ActiveBranch>> {
        let pool = self.pool.clone();
        let id = query.id.clone();
        let company_id = query.company_id.clone();
        Box::pin(async move {
            validate_target(&id, Some(&company_id))?;
            let row = sqlx::query(BRANCH_GUARD_SQL)
                .bind(&id)
                .bind(&company_id)
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            match row {
                None => Ok(None),
                Some(row) => {
                    let id: String = row.try_get("id").map_err(|_| unavailable())?;
                    let display_name: String = row.try_get("display_name").map_err(|_| unavailable())?;
                    Ok(Some(project_branch(&id, &display_name)?))
                }
            }
        })
    }

    fn company_exists<'a>(&'a self, query: &'a CompanyGuardQuery) -> GuardFuture<'a, bool> {
        let pool = self.pool.clone();
        let id = query.id.clone();
        Box::pin(async move {
            validate_target(&id, None)?;
            let row = sqlx::query(COMPANY_GUARD_SQL)
                .bind(&id)
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            Ok(row.is_some())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_sql_keeps_the_owner_projection_and_predicates() {
        assert!(BRANCH_GUARD_SQL.starts_with("SELECT id,display_name FROM ("));
        assert!(BRANCH_GUARD_SQL.contains(ORG_NODES));
        assert!(BRANCH_GUARD_SQL.contains("id=$1 AND company_id=$2"));
        assert!(BRANCH_GUARD_SQL.contains("kind='branch' AND status='active'"));
        assert!(COMPANY_GUARD_SQL.starts_with("SELECT 1 FROM ("));
        assert!(COMPANY_GUARD_SQL.contains(ORG_NODES));
        assert!(COMPANY_GUARD_SQL.contains("id=$1"));
        assert!(COMPANY_GUARD_SQL.contains("kind='company' AND status='active'"));
    }

    #[test]
    fn sql_never_contains_a_caller_value() {
        assert!(!BRANCH_GUARD_SQL.contains('?'));
        assert!(!COMPANY_GUARD_SQL.contains('?'));
        assert!(!BRANCH_GUARD_SQL.contains("br-1"));
        assert!(!BRANCH_GUARD_SQL.contains("co-1"));
    }
}
