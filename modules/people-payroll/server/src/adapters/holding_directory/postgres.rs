//! sqlx execution of the retired Holding people-directory facts. Every
//! statement stays in this owner crate; the router and composition carry no SQL.
//! The `?` binds of the retired D1 statements are rewritten as Postgres `$n`
//! binds and all values arrive as binds, never interpolated SQL.

use super::application::{HoldingDirectoryFuture, HoldingDirectoryRead};
use super::domain::{
    DirectoryCompany, DirectoryLevel, DirectoryNode, DirectoryPerson, DirectoryPosition,
    HoldingDirectoryFacts, HoldingDirectoryScope, unavailable,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::Row;

/// Retired `readDirectoryFacts1` (identity assignment witness) with the exact
/// active Company join and role/principal freshness guards.
pub const ASSIGNMENT_ACTIVE_SQL: &str = "SELECT 1 FROM assignments a \
 JOIN (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) c \
   ON c.id=a.company_id AND c.kind='company' AND c.status='active' \
 WHERE a.id=$1 AND a.user_id=$2 AND a.role=$3 AND a.company_id=$4 \
   AND a.scope_kind=$5 AND a.revoked_at IS NULL \
   AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
   AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
   AND a.valid_from<=$6 AND a.valid_until>$7 LIMIT 1";

/// Retired `readDirectoryFacts2`: active Company roots, holding-wide.
pub const COMPANIES_SQL: &str =
    "SELECT id,slug,display_name FROM organization_nodes WHERE kind='company' AND status='active' ORDER BY display_name,id";

/// Retired `readDirectoryFacts3/4/5`. Byte-identical to the native Organization
/// hierarchy owner statements that the TypeScript facts client already read.
pub const NODES_SQL: &str = "SELECT n.id,n.company_id,n.kind,n.slug,n.display_name,n.parent_id,n.branch_id
      FROM organization_nodes n JOIN organization_nodes c
        ON c.id=n.company_id AND c.kind='company' AND c.status='active'
      WHERE n.status='active' ORDER BY n.company_id,n.kind,n.display_name,n.id";
pub const LEVELS_SQL: &str = "SELECT l.id,l.company_id,l.code,l.name,l.level_number
      FROM staff_levels l JOIN organization_nodes c
        ON c.id=l.company_id AND c.kind='company' AND c.status='active'
      WHERE l.active=1 ORDER BY l.company_id,l.level_number NULLS LAST,l.name,l.id";
pub const POSITIONS_SQL: &str = "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id,
        p.reports_to_position_id,p.is_ceo
      FROM organization_positions p JOIN organization_nodes c
        ON c.id=p.company_id AND c.kind='company' AND c.status='active'
      WHERE p.active=1 ORDER BY p.company_id,p.title,p.id";

/// Retired `readDirectoryFacts6`: current employees with active Company roots,
/// the latest organization position title and a bounded primary IAM member.
pub const PEOPLE_SQL: &str = "SELECT p.id AS person_id,COALESCE(p.user_id,(SELECT m.user_id \
 FROM (SELECT \"company_id\",\"user_id\",\"principal_id\",\"person_id\",\"status\",\"login_policy\",\"connection_id\",\"identity_id\",\"version\",\"emergency_until\" FROM iam_members) m \
 WHERE m.person_id=p.id AND m.company_id=p.company_id AND m.status='active' ORDER BY m.user_id LIMIT 1)) AS user_id,e.id AS engagement_id,e.company_id,e.branch_id, \
 e.department_id,e.position_id,p.legal_name,COALESCE(op.title,e.job_title) AS job_title, \
 NULLIF(btrim(p.work_email),'') AS work_email, \
 NULLIF(btrim(p.work_phone),'') AS work_phone \
 FROM hr_engagements e JOIN hr_people p \
 ON p.id=e.person_id AND p.company_id=e.company_id \
 LEFT (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"level_id\",\"reports_to_position_id\",\"is_ceo\",\"active\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"requirements\",\"roster_required\" FROM organization_positions) op \
 ON op.id=e.position_id AND op.company_id=e.company_id AND op.active=1 \
 JOIN (SELECT \"id\",\"kind\",\"slug\",\"display_name\",\"parent_id\",\"company_id\",\"branch_id\",\"status\",\"hierarchy_revision\" FROM organization_nodes) c \
 ON c.id=e.company_id AND c.kind='company' AND c.status='active' \
 WHERE p.status='active' AND p.merged_into_id IS NULL \
 AND e.worker_type='employee' AND e.status IN ('active','suspended') \
 AND e.start_date<=CURRENT_DATE::text AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text) \
 ORDER BY p.legal_name,p.id,e.id";

pub struct PgHoldingDirectoryRead {
    pool: DatabasePool,
}

impl PgHoldingDirectoryRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(i64::MAX)
}

impl HoldingDirectoryRead for PgHoldingDirectoryRead {
    fn read<'a>(
        &'a self,
        scope: &'a HoldingDirectoryScope,
    ) -> HoldingDirectoryFuture<'a, HoldingDirectoryFacts> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let assignment_id = scope.assignment_id.clone();
        let user_id = scope.user_id.clone();
        let role = scope.role.clone();
        let company_id = scope.company_id.clone();
        let assignment_scope = scope.assignment_scope.clone();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let assignment_id = assignment_id.clone();
                let user_id = user_id.clone();
                let role = role.clone();
                let company_id = company_id.clone();
                let assignment_scope = assignment_scope.clone();
                Box::pin(async move {
                    let now = now_ms();
                    let assignment_id = assignment_id.clone();
                    let user_id = user_id.clone();
                    let role = role.clone();
                    let company_id = company_id.clone();
                    let assignment_scope = assignment_scope.clone();
                    let assignment_active = sqlx::query(ASSIGNMENT_ACTIVE_SQL)
                        .bind(assignment_id.as_str())
                        .bind(user_id.as_str())
                        .bind(role.as_str())
                        .bind(company_id.as_str())
                        .bind(assignment_scope.as_str())
                        .bind(now)
                        .bind(now)
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .is_some();
                    let companies = sqlx::query(COMPANIES_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(DirectoryCompany {
                                id: row.try_get("id").map_err(|_| unavailable())?,
                                slug: row.try_get("slug").map_err(|_| unavailable())?,
                                name: row.try_get("display_name").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let nodes = sqlx::query(NODES_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(DirectoryNode {
                                id: row.try_get("id").map_err(|_| unavailable())?,
                                company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                                kind: row.try_get("kind").map_err(|_| unavailable())?,
                                slug: row.try_get("slug").map_err(|_| unavailable())?,
                                name: row.try_get("display_name").map_err(|_| unavailable())?,
                                parent_id: row.try_get("parent_id").map_err(|_| unavailable())?,
                                branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let levels = sqlx::query(LEVELS_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(DirectoryLevel {
                                id: row.try_get("id").map_err(|_| unavailable())?,
                                company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                                code: row.try_get("code").map_err(|_| unavailable())?,
                                name: row.try_get("name").map_err(|_| unavailable())?,
                                number: row.try_get("level_number").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let positions = sqlx::query(POSITIONS_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            let is_ceo: i32 = row.try_get("is_ceo").map_err(|_| unavailable())?;
                            Ok(DirectoryPosition {
                                id: row.try_get("id").map_err(|_| unavailable())?,
                                company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                                branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                                department_id: row
                                    .try_get("department_id")
                                    .map_err(|_| unavailable())?,
                                title: row.try_get("title").map_err(|_| unavailable())?,
                                level_id: row.try_get("level_id").map_err(|_| unavailable())?,
                                reports_to_position_id: row
                                    .try_get("reports_to_position_id")
                                    .map_err(|_| unavailable())?,
                                is_ceo: is_ceo == 1,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    let people = sqlx::query(PEOPLE_SQL)
                        .fetch_all(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?
                        .into_iter()
                        .map(|row| {
                            Ok(DirectoryPerson {
                                id: row.try_get("engagement_id").map_err(|_| unavailable())?,
                                person_id: row.try_get("person_id").map_err(|_| unavailable())?,
                                user_id: row.try_get("user_id").map_err(|_| unavailable())?,
                                engagement_id: row
                                    .try_get("engagement_id")
                                    .map_err(|_| unavailable())?,
                                company_id: row.try_get("company_id").map_err(|_| unavailable())?,
                                branch_id: row.try_get("branch_id").map_err(|_| unavailable())?,
                                department_id: row
                                    .try_get("department_id")
                                    .map_err(|_| unavailable())?,
                                position_id: row.try_get("position_id").map_err(|_| unavailable())?,
                                legal_name: row.try_get("legal_name").map_err(|_| unavailable())?,
                                job_title: row.try_get("job_title").map_err(|_| unavailable())?,
                                work_email: row.try_get("work_email").map_err(|_| unavailable())?,
                                work_phone: row.try_get("work_phone").map_err(|_| unavailable())?,
                            })
                        })
                        .collect::<Result<Vec<_>, BusinessError>>()?;
                    Ok(HoldingDirectoryFacts {
                        assignment_active,
                        companies,
                        nodes,
                        levels,
                        positions,
                        people,
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
