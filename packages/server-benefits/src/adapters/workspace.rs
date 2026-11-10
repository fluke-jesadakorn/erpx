//! PostgreSQL read adapter for the native Benefits enrollment workspace.
//!
//! Only the Benefits owner's `benefit_plans`, `benefit_enrollment_windows`,
//! `benefit_dependents` and `benefit_elections` tables are read here. Package
//! facts and People worker/legal-name facts arrive through public ports
//! supplied by composition. The caller owns the retained serializable Holding
//! transaction; this adapter never creates, publishes, retires, verifies,
//! submits, acknowledges, releases or deletes any Benefits or cross-owner row.
use crate::application::workspace::{
    BenefitsDetailRepository, BenefitsFuture, BenefitsWorkspaceRepository,
};
use crate::domain::workspace::{
    BenefitDependentRow, BenefitElectionDetailSource, BenefitElectionEventRow, BenefitElectionRow,
    BenefitPayrollInputRow, BenefitPlanRow, BenefitWindowRow, source_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

pub struct PgBenefitsWorkspaceRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgBenefitsWorkspaceRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    async fn ready(&mut self) -> Result<(), Error> {
        if !self.connection.is_in_transaction() {
            return Err(
                PersistenceError::new("BENEFITS_WORKSPACE_READ_TRANSACTION_REQUIRED").into(),
            );
        }
        if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?
            != "serializable"
        {
            return Err(
                PersistenceError::new("BENEFITS_WORKSPACE_READ_SERIALIZABLE_REQUIRED").into(),
            );
        }
        Ok(())
    }

    pub async fn plans(
        &mut self,
        company: &CompanyId,
        limit: usize,
    ) -> Result<Vec<BenefitPlanRow>, Error> {
        self.ready().await?;
        let rows = sqlx::query(
            "SELECT id,company_id,name,provider,coverage_type,currency,\
             employee_deduction_minor,employer_cost_minor,effective_from,effective_to,\
             status,version,created_by,reviewed_by,created_at,updated_at,\
             required_eligibility_code FROM benefit_plans \
             WHERE company_id=$1 ORDER BY updated_at DESC,id COLLATE \"C\" LIMIT $2",
        )
        .bind(company.as_str())
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(BenefitPlanRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    name: row.try_get("name")?,
                    provider: row.try_get("provider")?,
                    coverage_type: row.try_get("coverage_type")?,
                    currency: row.try_get("currency")?,
                    employee_deduction_minor: row.try_get("employee_deduction_minor")?,
                    employer_cost_minor: row.try_get("employer_cost_minor")?,
                    effective_from: row.try_get("effective_from")?,
                    effective_to: row.try_get("effective_to")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                    required_eligibility_code: row.try_get("required_eligibility_code")?,
                })
            })
            .collect()
    }

    pub async fn windows(
        &mut self,
        company: &CompanyId,
        limit: usize,
    ) -> Result<Vec<BenefitWindowRow>, Error> {
        self.ready().await?;
        let rows = sqlx::query(
            "SELECT id,company_id,name,opens_on,closes_on,effective_on,status,version,\
             created_by,reviewed_by,created_at,updated_at FROM benefit_enrollment_windows \
             WHERE company_id=$1 ORDER BY updated_at DESC,id COLLATE \"C\" LIMIT $2",
        )
        .bind(company.as_str())
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(BenefitWindowRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    name: row.try_get("name")?,
                    opens_on: row.try_get("opens_on")?,
                    closes_on: row.try_get("closes_on")?,
                    effective_on: row.try_get("effective_on")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }

    pub async fn dependents(
        &mut self,
        company: &CompanyId,
        person_id: &str,
        limit: usize,
    ) -> Result<Vec<BenefitDependentRow>, Error> {
        self.ready().await?;
        let rows = sqlx::query(
            "SELECT id,company_id,person_id,name,relationship,birth_date,status,version,\
             created_by,reviewed_by,created_at,updated_at FROM benefit_dependents \
             WHERE company_id=$1 AND person_id=$2 \
             ORDER BY updated_at DESC,id COLLATE \"C\" LIMIT $3",
        )
        .bind(company.as_str())
        .bind(person_id)
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(BenefitDependentRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    person_id: row.try_get("person_id")?,
                    name: row.try_get("name")?,
                    relationship: row.try_get("relationship")?,
                    birth_date: row.try_get("birth_date")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    created_by: row.try_get("created_by")?,
                    reviewed_by: row.try_get("reviewed_by")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }

    /// Distinct employee person IDs for the reviewer workspace name projection.
    pub async fn election_person_ids(
        &mut self,
        company: &CompanyId,
        limit: usize,
    ) -> Result<Vec<String>, Error> {
        self.ready().await?;
        Ok(sqlx::query_scalar(
            "SELECT DISTINCT person_id FROM benefit_elections WHERE company_id=$1 ORDER BY person_id LIMIT $2",
        )
        .bind(company.as_str())
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?)
    }

    pub async fn elections(
        &mut self,
        company: &CompanyId,
        person_id: Option<&str>,
        reviewer: bool,
        limit: usize,
    ) -> Result<Vec<BenefitElectionRow>, Error> {
        self.ready().await?;
        if !reviewer && person_id.is_none() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            "SELECT e.id,e.company_id,e.person_id,e.engagement_id,e.plan_id,p.name AS plan_name,\
             ''::text AS employee_name,e.plan_version,e.window_id,e.window_version,\
             e.coverage_type,e.currency,e.employee_deduction_minor,e.employer_cost_minor,\
             e.effective_from,e.effective_to,e.status,e.version,e.created_at,e.updated_at \
             FROM benefit_elections e \
             JOIN benefit_plans p ON p.id=e.plan_id AND p.company_id=e.company_id \
             WHERE e.company_id=$1 AND ($2::text IS NULL OR e.person_id=$2) \
             ORDER BY e.updated_at DESC,e.id COLLATE \"C\" LIMIT $3",
        )
        .bind(company.as_str())
        .bind(person_id)
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                let employee_name: String = row.try_get("employee_name")?;
                if !employee_name.is_empty() {
                    return Err(source_invalid().into());
                }
                Ok(BenefitElectionRow {
                    id: row.try_get("id")?,
                    company_id: row.try_get("company_id")?,
                    person_id: row.try_get("person_id")?,
                    engagement_id: row.try_get("engagement_id")?,
                    plan_id: row.try_get("plan_id")?,
                    plan_name: row.try_get("plan_name")?,
                    employee_name,
                    plan_version: row.try_get("plan_version")?,
                    window_id: row.try_get("window_id")?,
                    window_version: row.try_get("window_version")?,
                    coverage_type: row.try_get("coverage_type")?,
                    currency: row.try_get("currency")?,
                    employee_deduction_minor: row.try_get("employee_deduction_minor")?,
                    employer_cost_minor: row.try_get("employer_cost_minor")?,
                    effective_from: row.try_get("effective_from")?,
                    effective_to: row.try_get("effective_to")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }
    /// One Company-scoped election detail with the joined current plan version
    /// and status. Reviewers read any Company election; an employee is limited
    /// to their own Person.
    pub async fn election_detail(
        &mut self,
        company: &CompanyId,
        id: &str,
        person_id: Option<&str>,
        reviewer: bool,
    ) -> Result<Option<BenefitElectionDetailSource>, Error> {
        self.ready().await?;
        if !reviewer && person_id.is_none() {
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT e.id,e.company_id,e.person_id,e.engagement_id,e.plan_id,p.name AS plan_name,\
             ''::text AS employee_name,e.plan_version,p.version AS plan_current_version,\
             p.status AS plan_status,e.window_id,e.window_version,e.coverage_type,\
             e.dependent_ids_json,e.currency,e.employee_deduction_minor,e.employer_cost_minor,\
             e.effective_from,e.effective_to,e.status,e.created_by,e.submitted_by,\
             e.provider_reference,e.acknowledged_by,e.released_by,e.version,e.created_at,\
             e.updated_at \
             FROM benefit_elections e \
             JOIN benefit_plans p ON p.id=e.plan_id AND p.company_id=e.company_id \
             WHERE e.id=$1 AND e.company_id=$2 AND ($3::text IS NULL OR e.person_id=$3)",
        )
        .bind(id)
        .bind(company.as_str())
        .bind(person_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let employee_name: String = row.try_get("employee_name")?;
        if !employee_name.is_empty() {
            return Err(source_invalid().into());
        }
        let dependent_ids_json: String = row.try_get("dependent_ids_json")?;
        let dependent_ids: Vec<String> =
            serde_json::from_str(&dependent_ids_json).map_err(|_| source_invalid())?;
        Ok(Some(BenefitElectionDetailSource {
            election: BenefitElectionRow {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                person_id: row.try_get("person_id")?,
                engagement_id: row.try_get("engagement_id")?,
                plan_id: row.try_get("plan_id")?,
                plan_name: row.try_get("plan_name")?,
                employee_name,
                plan_version: row.try_get("plan_version")?,
                window_id: row.try_get("window_id")?,
                window_version: row.try_get("window_version")?,
                coverage_type: row.try_get("coverage_type")?,
                currency: row.try_get("currency")?,
                employee_deduction_minor: row.try_get("employee_deduction_minor")?,
                employer_cost_minor: row.try_get("employer_cost_minor")?,
                effective_from: row.try_get("effective_from")?,
                effective_to: row.try_get("effective_to")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            },
            plan_current_version: row.try_get("plan_current_version")?,
            plan_status: row.try_get("plan_status")?,
            created_by: row.try_get("created_by")?,
            submitted_by: row.try_get("submitted_by")?,
            provider_reference: row.try_get("provider_reference")?,
            acknowledged_by: row.try_get("acknowledged_by")?,
            released_by: row.try_get("released_by")?,
            dependent_ids,
        }))
    }

    pub async fn election_history(
        &mut self,
        company: &CompanyId,
        election_id: &str,
        limit: usize,
    ) -> Result<Vec<BenefitElectionEventRow>, Error> {
        self.ready().await?;
        let rows = sqlx::query(
            "SELECT v.id,v.election_id,v.version,v.action,v.actor_id,v.reason,v.status,v.created_at \
             FROM benefit_election_events v \
             JOIN benefit_elections e ON e.id=v.election_id AND e.company_id=$2 \
             WHERE v.election_id=$1 ORDER BY v.version LIMIT $3",
        )
        .bind(election_id)
        .bind(company.as_str())
        .bind(limit as i64)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(BenefitElectionEventRow {
                    id: row.try_get("id")?,
                    election_id: row.try_get("election_id")?,
                    version: row.try_get("version")?,
                    action: row.try_get("action")?,
                    actor_id: row.try_get("actor_id")?,
                    reason: row.try_get("reason")?,
                    status: row.try_get("status")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }

    pub async fn payroll_input(
        &mut self,
        company: &CompanyId,
        election_id: &str,
    ) -> Result<Option<BenefitPayrollInputRow>, Error> {
        self.ready().await?;
        let row = sqlx::query(
            "SELECT id,election_id,company_id,person_id,engagement_id,currency,\
             employee_deduction_minor,employer_cost_minor,effective_from,effective_to,\
             source_election_version,status,released_by,created_at \
             FROM benefit_payroll_inputs WHERE election_id=$1 AND company_id=$2",
        )
        .bind(election_id)
        .bind(company.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(BenefitPayrollInputRow {
                id: row.try_get("id")?,
                election_id: row.try_get("election_id")?,
                company_id: row.try_get("company_id")?,
                person_id: row.try_get("person_id")?,
                engagement_id: row.try_get("engagement_id")?,
                currency: row.try_get("currency")?,
                employee_deduction_minor: row.try_get("employee_deduction_minor")?,
                employer_cost_minor: row.try_get("employer_cost_minor")?,
                effective_from: row.try_get("effective_from")?,
                effective_to: row.try_get("effective_to")?,
                source_election_version: row.try_get("source_election_version")?,
                status: row.try_get("status")?,
                released_by: row.try_get("released_by")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .transpose()
    }
}

impl BenefitsDetailRepository for PgBenefitsWorkspaceRead<'_> {
    type Error = Error;

    fn election_detail<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
        person_id: Option<&'a str>,
        reviewer: bool,
    ) -> BenefitsFuture<'a, Option<BenefitElectionDetailSource>, Self::Error> {
        Box::pin(async move {
            PgBenefitsWorkspaceRead::election_detail(self, company, id, person_id, reviewer).await
        })
    }

    fn election_history<'a>(
        &'a mut self,
        company: &'a CompanyId,
        election_id: &'a str,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitElectionEventRow>, Self::Error> {
        Box::pin(async move {
            PgBenefitsWorkspaceRead::election_history(self, company, election_id, limit).await
        })
    }

    fn payroll_input<'a>(
        &'a mut self,
        company: &'a CompanyId,
        election_id: &'a str,
    ) -> BenefitsFuture<'a, Option<BenefitPayrollInputRow>, Self::Error> {
        Box::pin(
            async move { PgBenefitsWorkspaceRead::payroll_input(self, company, election_id).await },
        )
    }
}

impl BenefitsWorkspaceRepository for PgBenefitsWorkspaceRead<'_> {
    type Error = Error;

    fn plans<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitPlanRow>, Self::Error> {
        Box::pin(async move { PgBenefitsWorkspaceRead::plans(self, company, limit).await })
    }

    fn windows<'a>(
        &'a mut self,
        company: &'a CompanyId,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitWindowRow>, Self::Error> {
        Box::pin(async move { PgBenefitsWorkspaceRead::windows(self, company, limit).await })
    }

    fn dependents<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: &'a str,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitDependentRow>, Self::Error> {
        Box::pin(async move {
            PgBenefitsWorkspaceRead::dependents(self, company, person_id, limit).await
        })
    }

    fn elections<'a>(
        &'a mut self,
        company: &'a CompanyId,
        person_id: Option<&'a str>,
        reviewer: bool,
        limit: usize,
    ) -> BenefitsFuture<'a, Vec<BenefitElectionRow>, Self::Error> {
        Box::pin(async move {
            PgBenefitsWorkspaceRead::elections(self, company, person_id, reviewer, limit).await
        })
    }
}
