//! sqlx execution for the native Company acceptance read.
//!
//! Every statement below is the byte-identical predicate/projection/bind order
//! of the retired TypeScript read (owner SQL from `organization_owner_write.rs`,
//! the staff-level acceptance count from Operations lane 28 and the placement
//! count from the people_payroll acceptance facts), with only the TypeScript
//! `?` placeholders changed to sqlx/Postgres `$n`. The owner owns the scoped
//! binds and the row-to-domain projection; no SQL text crosses the transport
//! and no TypeScript prepares or executes these statements.

use crate::adapters::native_acceptance_read::application::{AcceptanceRead, ReadFuture};
use crate::adapters::native_acceptance_read::domain::{
    AcceptanceFacts, AcceptanceQuery, AcceptanceRow, CompanyRow, EventRow,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;
use std::time::{SystemTime, UNIX_EPOCH};

pub const COMPANY_SQL: &str =
    "SELECT id,display_name,status FROM organization_nodes WHERE id=$1 AND company_id=$2 AND kind='company'";
pub const BRANCHES_SQL: &str =
    "SELECT count(*) count FROM organization_nodes WHERE company_id=$1 AND kind='branch' AND status='active'";
pub const LEVELS_SQL: &str = concat!(
    "SELECT count(*)::bigint active_count FROM (SELECT ",
    "\"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\"",
    " FROM staff_levels) staff_levels WHERE company_id=$1 AND active=1 AND level_number BETWEEN 1 AND 6",
);
pub const CEO_SQL: &str =
    "SELECT count(*) count FROM organization_positions WHERE company_id=$1 AND active=1 AND is_ceo=1";
pub const CURRENT_SQL: &str = "SELECT * FROM company_business_acceptance WHERE company_id=$1";
pub const EVENTS_SQL: &str = "SELECT id,action,actor_id,version,reason,created_at FROM company_business_acceptance_events WHERE company_id=$1 ORDER BY version,created_at,id";
pub const AUTHORITY_SQL: &str = concat!(
    "SELECT 1 FROM assignments WHERE id=$1 AND company_id=$2 AND iam_role_allows(role,company_id,$3)",
    " AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)",
    " AND scope_kind='holding' AND revoked_at IS NULL",
    " AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)",
    " AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)",
    " AND valid_from<=$4 AND valid_until>$5",
);
/// Cross-owner, read-only placement projection. `staff_levels` is
/// Organization-owned; `hr_engagements`/`hr_people` are read here only to keep
/// the acceptance payload complete until a people_payroll native port is
/// mounted. The lead owns that seam handoff; no write, lock or decision is
/// inferred from this SELECT.
pub const PLACEMENT_SQL: &str = concat!(
    "SELECT count(*) count FROM hr_engagements e",
    " JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id",
    " LEFT JOIN (SELECT \"id\",\"company_id\",\"code\",\"name\",\"rank\",\"active\",\"version\",\"created_at\",\"updated_at\",\"level_number\" FROM staff_levels) l ON l.id=e.level_id AND l.company_id=e.company_id",
    " WHERE e.company_id=$1 AND e.status='active' AND p.status='active'",
    " AND (l.id IS NULL OR l.active!=1 OR e.position_id IS NULL)",
);

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_ACCEPTANCE_READ_UNAVAILABLE",
        "The native Company acceptance read is unavailable.",
    )
}

fn now_millis() -> i64 {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis())
        .unwrap_or(i64::MAX)
}

fn company_row(row: sqlx::postgres::PgRow) -> Result<CompanyRow, BusinessError> {
    Ok(CompanyRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        display_name: row.try_get("display_name").map_err(|_| unavailable())?,
        status: row.try_get("status").map_err(|_| unavailable())?,
    })
}

fn acceptance_row(row: sqlx::postgres::PgRow) -> Result<AcceptanceRow, BusinessError> {
    Ok(AcceptanceRow {
        company_id: row.try_get("company_id").map_err(|_| unavailable())?,
        status: row.try_get("status").map_err(|_| unavailable())?,
        version: row.try_get("version").map_err(|_| unavailable())?,
        requested_by: row.try_get("requested_by").map_err(|_| unavailable())?,
        reviewed_by: row.try_get("reviewed_by").map_err(|_| unavailable())?,
        requested_at: row.try_get("requested_at").map_err(|_| unavailable())?,
        reviewed_at: row.try_get("reviewed_at").map_err(|_| unavailable())?,
        reason: row.try_get("reason").map_err(|_| unavailable())?,
        review_reason: row.try_get("review_reason").map_err(|_| unavailable())?,
        updated_at: row.try_get("updated_at").map_err(|_| unavailable())?,
    })
}

fn event_row(row: sqlx::postgres::PgRow) -> Result<EventRow, BusinessError> {
    Ok(EventRow {
        id: row.try_get("id").map_err(|_| unavailable())?,
        action: row.try_get("action").map_err(|_| unavailable())?,
        actor_id: row.try_get("actor_id").map_err(|_| unavailable())?,
        version: row.try_get("version").map_err(|_| unavailable())?,
        reason: row.try_get("reason").map_err(|_| unavailable())?,
        created_at: row.try_get("created_at").map_err(|_| unavailable())?,
    })
}

/// Pg-backed owner reader. All statements run on one owner unit so the checks,
/// current record, decision facts and event list form a consistent snapshot.
/// No lock is taken and no command is executed by this read port.
pub struct PgAcceptanceRead {
    pool: DatabasePool,
}

impl PgAcceptanceRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl AcceptanceRead for PgAcceptanceRead {
    fn read<'a>(&'a self, query: &'a AcceptanceQuery) -> ReadFuture<'a, AcceptanceFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let company = query.company.clone();
        let assignment = query.assignment.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let company = company.clone();
                let assignment = assignment.clone();
                Box::pin(async move {
                    let company_row = sqlx::query(COMPANY_SQL)
                        .bind(company.as_str())
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .map(company_row)
                        .transpose()?;
                    let branches = sqlx::query_scalar::<_, i64>(BRANCHES_SQL)
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let levels = sqlx::query_scalar::<_, i64>(LEVELS_SQL)
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let ceo = sqlx::query_scalar::<_, i64>(CEO_SQL)
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let incomplete = sqlx::query_scalar::<_, i64>(PLACEMENT_SQL)
                        .bind(company.as_str())
                        .fetch_one(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let current = sqlx::query(CURRENT_SQL)
                        .bind(company.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .map(acceptance_row)
                        .transpose()?;
                    let event_rows = sqlx::query(EVENTS_SQL)
                        .bind(company.as_str())
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    let events = event_rows
                        .into_iter()
                        .map(event_row)
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let now = now_millis();
                    let authority = sqlx::query_scalar::<_, i32>(AUTHORITY_SQL)
                        .bind(assignment.as_str())
                        .bind(company.as_str())
                        .bind("organization.manage")
                        .bind(now)
                        .bind(now)
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .is_some();
                    Ok(AcceptanceFacts {
                        company: company_row,
                        branches,
                        levels,
                        ceo,
                        incomplete,
                        current,
                        events,
                        authority,
                    })
                })
            })
            .await
            .map_err(|error| match error {
                PgTransactionError::Business(error) => error,
                PgTransactionError::Persistence(_) => unavailable(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_owner_statement_is_select_only_and_company_scoped() {
        let statements = [
            COMPANY_SQL,
            BRANCHES_SQL,
            LEVELS_SQL,
            CEO_SQL,
            CURRENT_SQL,
            EVENTS_SQL,
            PLACEMENT_SQL,
        ];
        for statement in statements {
            let head = statement.trim_start().to_ascii_uppercase();
            assert!(head.starts_with("SELECT"), "owner read must be SELECT-only");
            assert!(!statement.contains('?'), "binds must be sqlx placeholders");
            assert!(statement.contains("company_id=$"), "read must be Company-keyed");
        }
        let authority = AUTHORITY_SQL.trim_start().to_ascii_uppercase();
        assert!(authority.starts_with("SELECT 1 FROM ASSIGNMENTS"));
        assert!(AUTHORITY_SQL.contains("$4") && AUTHORITY_SQL.contains("$5"));
    }

    #[test]
    fn stated_placeholder_counts_match_the_bind_order() {
        assert_eq!(COMPANY_SQL.matches('$').count(), 2);
        assert_eq!(AUTHORITY_SQL.matches("$").count(), 5);
    }
}
