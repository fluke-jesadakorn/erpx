//! PostgreSQL adapter for the native Organization roster read.
//!
//! The caller owns the Holding transaction and current session proof. This
//! adapter reads only the Company rows the retained `organizationRoster`
//! family returns: non-ended employee engagements with their level
//! projection, and Company positions with level numbers. It performs no
//! branch/department row filtering and never substitutes transport checks;
//! the retained role gate runs in the owner use case before any row is read,
//! and the caller keeps its projection window exactly like the TypeScript
//! family.
use crate::{
    application::roster::{OrganizationRosterRepository, OrganizationRosterRead, RosterFuture},
    domain::access::{AccessPerson, AccessPosition, OrganizationAccessActor},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row, postgres::PgRow};

type Error = PgTransactionError<BusinessError>;

fn transaction_required() -> Error {
    PersistenceError::new("ORGANIZATION_ROSTER_TRANSACTION_REQUIRED").into()
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

pub struct PgOrganizationRoster<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgOrganizationRoster<'c> {
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
    ) -> Result<OrganizationRosterRead, Error> {
        crate::application::roster::read_organization_roster(self, actor).await
    }
}

impl OrganizationRosterRepository for PgOrganizationRoster<'_> {
    type Error = Error;

    fn roster_people<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> RosterFuture<'a, Vec<AccessPerson>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let rows = sqlx::query(
                "SELECT p.id person_id,p.user_id,p.legal_name,p.email,p.version::bigint person_version, \
                 p.status person_status,e.id engagement_id,e.version::bigint engagement_version, \
                 e.company_id,e.branch_id,e.department_id,e.position_id,e.manager_person_id, \
                 e.level_id,l.level_number::bigint level_number,l.name level_name,e.status, \
                 e.start_date,e.end_date,e.job_title \
                 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id \
                 LEFT JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1 \
                 WHERE p.company_id=$1 AND p.merged_into_id IS NULL AND p.status<>'archived' \
                 AND e.worker_type='employee' AND e.status<>'ended' \
                 ORDER BY p.legal_name,e.id",
            )
            .bind(company.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter().map(person).collect()
        })
    }

    fn roster_positions<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> RosterFuture<'a, Vec<AccessPosition>, Self::Error> {
        Box::pin(async move {
            self.transaction()?;
            let rows = sqlx::query(
                "SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id, \
                 p.reports_to_position_id,p.requirements, \
                 p.is_ceo::bigint is_ceo,p.roster_required::bigint roster_required, \
                 p.active::bigint active,p.version::bigint version, \
                 l.level_number::bigint level_number \
                 FROM organization_positions p \
                 JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id \
                 WHERE p.company_id=$1 ORDER BY l.level_number,p.title,p.id",
            )
            .bind(company.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
            rows.iter().map(position).collect()
        })
    }
}
