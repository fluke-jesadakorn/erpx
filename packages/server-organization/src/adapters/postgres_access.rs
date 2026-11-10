//! PostgreSQL adapter for the bounded Organization access projection.
//!
//! The caller owns the Holding transaction and current session proof. This
//! adapter reads only the Company rows that the projection window exposes; it
//! never widens the roster beyond the retained employee engagement query.
use crate::{
    application::access::{AccessFuture, OrganizationAccessRepository},
    domain::access::{
        AccessLevel, AccessNode, AccessPerson, AccessPosition, OrganizationAccessActor,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{postgres::PgRow, Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const ROW_LIMIT: usize = 4_000;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ORGANIZATION_ACCESS_UNAVAILABLE",
        "Organization and access facts are unavailable.",
    )
}

fn transaction_required() -> Error {
    PersistenceError::new("ORGANIZATION_ACCESS_TRANSACTION_REQUIRED").into()
}

fn person(row: &PgRow) -> Result<AccessPerson, Error> {
    Ok(AccessPerson {
        person_id: row.try_get("person_id")?,
        user_id: row.try_get("user_id")?,
        legal_name: row.try_get("legal_name")?,
        email: row.try_get("email")?,
        engagement_id: row.try_get("engagement_id")?,
        engagement_version: row.try_get("engagement_version")?,
        person_version: row.try_get("person_version")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        position_id: row.try_get("position_id")?,
        manager_person_id: row.try_get("manager_person_id")?,
        level_id: row.try_get("level_id")?,
        level_number: row.try_get("level_number")?,
        level_name: row.try_get("level_name")?,
        status: row.try_get("status")?,
        person_status: row.try_get("person_status")?,
        start_date: row.try_get("start_date")?,
        end_date: row.try_get("end_date")?,
        job_title: row.try_get("job_title")?,
        issues: Vec::new(),
    })
}

fn position(row: &PgRow) -> Result<AccessPosition, Error> {
    Ok(AccessPosition {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        title: row.try_get("title")?,
        level_id: row.try_get("level_id")?,
        level_number: row.try_get("level_number")?,
        reports_to_position_id: row.try_get("reports_to_position_id")?,
        requirements: row.try_get("requirements")?,
        is_ceo: row.try_get("is_ceo")?,
        roster_required: row.try_get("roster_required")?,
        active: row.try_get("active")?,
        version: row.try_get("version")?,
    })
}

fn node(row: &PgRow) -> Result<AccessNode, Error> {
    Ok(AccessNode {
        id: row.try_get("id")?,
        kind: row.try_get("kind")?,
        slug: row.try_get("slug")?,
        display_name: row.try_get("display_name")?,
        branch_id: row.try_get("branch_id")?,
        status: row.try_get("status")?,
        hierarchy_revision: row.try_get("hierarchy_revision")?,
    })
}

fn level(row: &PgRow) -> Result<AccessLevel, Error> {
    Ok(AccessLevel {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        level_number: row.try_get("level_number")?,
    })
}

pub struct PgOrganizationAccess<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgOrganizationAccess<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn transaction(&self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(transaction_required());
        }
        Ok(())
    }

    pub async fn read(
        &mut self,
        actor: &OrganizationAccessActor,
        clock: &(dyn erpx_server_shared::public::Clock + Sync),
    ) -> Result<crate::domain::access::OrganizationAccessData, Error> {
        crate::application::access::read_organization_access(self, actor, clock).await
    }
}

impl OrganizationAccessRepository for PgOrganizationAccess<'_> {
    type Error = Error;

    fn access_roster<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> AccessFuture<'a, (Vec<AccessPerson>, Vec<AccessPosition>), Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let rows = sqlx::query(
                "SELECT p.id person_id,p.user_id,p.legal_name,p.email,p.version person_version, \
                 p.status person_status,e.id engagement_id,e.version engagement_version, \
                 e.company_id,e.branch_id,e.department_id,e.position_id,e.manager_person_id, \
                 e.level_id,l.level_number::bigint level_number,l.name level_name,e.status, \
                 e.start_date,e.end_date,e.job_title \
                 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1 \
                 WHERE p.company_id=$1 AND p.merged_into_id IS NULL AND p.status<>'archived' \
                 AND e.worker_type='employee' AND e.status<>'ended' \
                 ORDER BY p.legal_name,e.id LIMIT $2",
            )
            .bind(company.as_str())
            .bind((ROW_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > ROW_LIMIT {
                return Err(unavailable().into());
            }
            let people = rows.iter().map(person).collect::<Result<Vec<_>, _>>()?;
            let rows = sqlx::query(
                "SELECT id,company_id,branch_id,department_id,title,level_id, \
                 level_number::bigint level_number,reports_to_position_id,requirements, \
                 is_ceo::bigint is_ceo,roster_required::bigint roster_required, \
                 active::bigint active,version::bigint version \
                 FROM (SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id, \
                       l.level_number,p.reports_to_position_id,p.requirements,p.is_ceo, \
                       p.roster_required,p.active,p.version \
                       FROM organization_positions p \
                       JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id \
                       WHERE p.company_id=$1) positions \
                 ORDER BY level_number,title,id LIMIT $2",
            )
            .bind(company.as_str())
            .bind((ROW_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > ROW_LIMIT {
                return Err(unavailable().into());
            }
            let positions = rows.iter().map(position).collect::<Result<Vec<_>, _>>()?;
            Ok((people, positions))
        })
    }

    fn access_nodes<'a>(
        &'a mut self,
        actor: &'a OrganizationAccessActor,
    ) -> AccessFuture<'a, Vec<AccessNode>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let rows = sqlx::query(
                "SELECT id,kind,slug,display_name,branch_id,status,hierarchy_revision::bigint hierarchy_revision \
                 FROM organization_nodes WHERE company_id=$1 AND kind IN ('company','branch','department') \
                 AND ($2::text IS NULL OR kind='company' OR id=$2 OR branch_id=$2) \
                 AND ($3::text IS NULL OR kind<>'department' OR id=$3) \
                 ORDER BY display_name,id LIMIT $4",
            )
            .bind(&actor.company_id)
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .bind((ROW_LIMIT + 1) as i64)
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > ROW_LIMIT {
                return Err(unavailable().into());
            }
            rows.iter().map(node).collect()
        })
    }

    fn access_levels<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> AccessFuture<'a, Vec<AccessLevel>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let rows = sqlx::query(
                "SELECT id,name,level_number::bigint level_number FROM staff_levels \
                 WHERE company_id=$1 AND active=1 AND level_number IS NOT NULL \
                 ORDER BY level_number LIMIT 101",
            )
            .bind(company.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            if rows.len() > 100 {
                return Err(unavailable().into());
            }
            rows.iter().map(level).collect()
        })
    }
}
