//! sqlx execution of the migrated `GET .../people` list read. This is the only
//! place the retired TypeScript statement now lives: it is executed here on a
//! Company-scoped `PgUnitOfWork`, with no TypeScript prepare/batch fallback.
//!
//! The owner projection sources and the latest employee-engagement key are
//! inlined from `pp-read-tail-r18-sources.ts`, `pp-read-tail-r20-sources.ts`
//! and `organization-read-tail-sources-r55.ts`. Only placeholder style changes
//! (`?` -> `$1..$10`); columns, projections, predicates, binds and ordering
//! match the retired statement exactly.

use super::application::{ListFuture, ListPeopleQuery, PeopleListRead};
use super::domain::{PersonListRow, unavailable};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::BusinessError;
use sqlx::Row;

/// Executed statement; ten binds keep the retired bind order exactly:
/// `[company, branch, branch, scope_department, scope_department,
///   level, level, level, filter_department, filter_department]`.
pub const SQL_PEOPLE_LIST: &str = r#"SELECT p.id,p.user_id,p.legal_name,p.preferred_name,p.email,p.country_code,p.status,p.version,e.id AS engagement_id,e.job_title,e.worker_type,e.employment_type,e.status AS engagement_status,e.start_date,e.currency,e.level_id,e.department_id,e.position_id,e.manager_person_id,(SELECT name FROM staff_levels WHERE id=e.level_id) level_name,n.display_name AS branch_name FROM (SELECT "id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone" FROM hr_people) p LEFT JOIN (SELECT "id","person_id","company_id","branch_id","department_id","worker_type","employment_type","status","start_date","end_date","manager_person_id","job_title","currency","version","created_by","created_at","updated_at","pay_group_id","level_id","position_id","self_service_top_level" FROM hr_engagements) e ON e.id=(SELECT x.id FROM (SELECT "id","person_id","company_id","branch_id","department_id","worker_type","employment_type","status","start_date","end_date","manager_person_id","job_title","currency","version","created_by","created_at","updated_at","pay_group_id","level_id","position_id","self_service_top_level" FROM hr_engagements) x WHERE x.person_id=p.id AND x.company_id=p.company_id ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1) LEFT JOIN organization_nodes n ON n.id=e.branch_id WHERE p.company_id=$1 AND p.merged_into_id IS NULL AND e.worker_type='employee' AND ($2::text IS NULL OR e.branch_id=$3) AND ($4::text IS NULL OR e.department_id=$5) AND ($6::text IS NULL OR ($7::text='unset' AND e.level_id IS NULL) OR e.level_id=$8) AND ($9::text IS NULL OR e.department_id=$10) ORDER BY p.legal_name,p.id"#;

/// SHA-256 of the executed statement, pinned in the TS parity test.
pub const SQL_ORACLE_SHA256: &str =
    "d2fa0333c95d055fcb9d7a77a222906b18b6e40ed7f3d94a6b1e38b02235b460";

fn text(row: &sqlx::postgres::PgRow, column: &str) -> Result<Option<String>, BusinessError> {
    row.try_get::<Option<String>, _>(column)
        .map_err(|_| unavailable())
}

fn integer(row: &sqlx::postgres::PgRow, column: &str) -> Result<i64, BusinessError> {
    row.try_get::<Option<i64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .map(i64::from)
        })
        .ok_or_else(unavailable)
}

/// Row-to-domain mapping in declaration order. Every nullable column is carried
/// as `Option` so a NULL in Postgres stays JSON `null`, exactly as the retired
/// TypeScript `all()` result did.
fn pack(row: &sqlx::postgres::PgRow) -> Result<PersonListRow, BusinessError> {
    Ok(PersonListRow {
        id: text(row, "id")?,
        user_id: text(row, "user_id")?,
        legal_name: text(row, "legal_name")?,
        preferred_name: text(row, "preferred_name")?,
        email: text(row, "email")?,
        country_code: text(row, "country_code")?,
        status: text(row, "status")?,
        version: integer(row, "version")?,
        engagement_id: text(row, "engagement_id")?,
        job_title: text(row, "job_title")?,
        worker_type: text(row, "worker_type")?,
        employment_type: text(row, "employment_type")?,
        engagement_status: text(row, "engagement_status")?,
        start_date: text(row, "start_date")?,
        currency: text(row, "currency")?,
        level_id: text(row, "level_id")?,
        department_id: text(row, "department_id")?,
        position_id: text(row, "position_id")?,
        manager_person_id: text(row, "manager_person_id")?,
        level_name: text(row, "level_name")?,
        branch_name: text(row, "branch_name")?,
    })
}

pub struct PgPeopleListRead {
    pool: DatabasePool,
}

impl PgPeopleListRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PeopleListRead for PgPeopleListRead {
    fn list<'a>(&'a self, query: &'a ListPeopleQuery) -> ListFuture<'a, Vec<PersonListRow>> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let query = query.clone();
        Box::pin(async move {
            let rows = unit
                .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                    let q = query.clone();
                    Box::pin(async move {
                        let rows = sqlx::query(SQL_PEOPLE_LIST)
                            .bind(q.company.as_str())
                            .bind(q.branch.as_deref())
                            .bind(q.branch.as_deref())
                            .bind(q.department.as_deref())
                            .bind(q.department.as_deref())
                            .bind(q.level.as_deref())
                            .bind(q.level.as_deref())
                            .bind(q.level.as_deref())
                            .bind(q.filter_department.as_deref())
                            .bind(q.filter_department.as_deref())
                            .fetch_all(&mut *connection)
                            .await
                            .map_err(|_| unavailable())?;
                        rows.iter()
                            .map(pack)
                            .collect::<Result<Vec<_>, BusinessError>>()
                            .map_err(Into::into)
                    })
                })
                .await
                .map_err(|error| match error {
                    PgTransactionError::Business(error) => error,
                    PgTransactionError::Persistence(_) => unavailable(),
                })?;
            Ok(rows)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    #[test]
    fn statement_keeps_owner_scope_projection_and_bind_order() {
        for fragment in [
            "FROM hr_people",
            "FROM hr_engagements",
            "organization_nodes n ON n.id=e.branch_id",
            "staff_levels WHERE id=e.level_id",
            "p.company_id=$1",
            "p.merged_into_id IS NULL",
            "e.worker_type='employee'",
            "e.branch_id=$3",
            "e.department_id=$5",
            "$7::text='unset'",
            "e.level_id=$8",
            "e.department_id=$10",
            "ORDER BY p.legal_name,p.id",
        ] {
            assert!(SQL_PEOPLE_LIST.contains(fragment), "missing {fragment}");
        }
        assert_eq!(SQL_PEOPLE_LIST.matches('$').count(), 10);
        assert!(!SQL_PEOPLE_LIST.contains('?'));
    }

    #[test]
    fn sql_oracle_matches_the_transport_constant() {
        let hex = sha2::Sha256::digest(SQL_PEOPLE_LIST.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hex, SQL_ORACLE_SHA256);
    }
}
