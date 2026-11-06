//! PostgreSQL adapter for native CRM lead / opportunity commands.
use crate::application::leads_native::{LeadFacts, LeadRepository, LeadsFuture, OpportunityFacts};
use crate::domain::leads_native::{LeadChangeInput, LeadCreateInput, OpportunityChangeInput};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_LEAD_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_LEAD_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgCrmLeadCommands<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCrmLeadCommands<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn lead(
        &mut self,
        company_id: &str,
        lead_id: &str,
    ) -> Result<Option<LeadFacts>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT l.owner_id,l.status,l.version,l.branch_id,l.department_id,v.stage \
             FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id \
             WHERE l.id=$1 AND l.company_id=$2 FOR UPDATE OF l",
        )
        .bind(lead_id)
        .bind(company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(LeadFacts {
                owner_id: row.try_get("owner_id")?,
                status: row.try_get("status")?,
                stage: row.try_get("stage").ok(),
                version: row.try_get("version")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id").ok(),
            })
        })
        .transpose()
    }

    pub async fn opportunity(
        &mut self,
        company_id: &str,
        opportunity_id: &str,
    ) -> Result<Option<OpportunityFacts>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT o.id,o.lead_id,o.stage,o.version FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id WHERE o.id=$1 AND l.company_id=$2 FOR UPDATE OF o",
        )
        .bind(opportunity_id)
        .bind(company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(OpportunityFacts {
                id: row.try_get("id")?,
                lead_id: row.try_get("lead_id")?,
                stage: row.try_get("stage")?,
                version: row.try_get("version")?,
            })
        })
        .transpose()
    }

    pub async fn insert_lead(
        &mut self,
        company_id: &str,
        record_id: &str,
        created_by: &str,
        plan: &crate::domain::leads_native::LeadCreatePlan,
        _input: &LeadCreateInput,
        account_id: &str,
        contact_id: &str,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        sqlx::query(
            "INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,created_by,created_at,updated_at,account_id,contact_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
        )
        .bind(record_id)
        .bind(company_id)
        .bind(&plan.branch_id)
        .bind::<Option<&str>>(None)
        .bind(&plan.title)
        .bind(&plan.contact_name)
        .bind(&plan.contact_detail)
        .bind(&plan.source)
        .bind(&plan.brief)
        .bind(created_by)
        .bind(&plan.due_date)
        .bind(created_by)
        .bind(now)
        .bind(now)
        .bind(account_id)
        .bind(contact_id)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    pub async fn update_lead(
        &mut self,
        company_id: &str,
        record_id: &str,
        command_key: &str,
        actor_id: &str,
        record: &LeadFacts,
        input: &LeadChangeInput,
        plan: &crate::domain::leads_native::LeadChangePlan,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        match plan.action.as_str() {
            "reassign" => {
                sqlx::query("UPDATE crm_leads SET owner_id=$1,due_date=$2,version=version+1,updated_at=$3 WHERE id=$4 AND company_id=$5 AND version=$6")
                    .bind(plan.owner_id.as_deref())
                    .bind(plan.due_date.as_deref())
                    .bind(now)
                    .bind(record_id)
                    .bind(company_id)
                    .bind(plan.expected_version)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "disqualify" => {
                sqlx::query("UPDATE crm_leads SET status='disqualified',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
                    .bind(now)
                    .bind(record_id)
                    .bind(company_id)
                    .bind(plan.expected_version)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "reopen" => {
                sqlx::query("UPDATE crm_leads SET status='open',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
                    .bind(now)
                    .bind(record_id)
                    .bind(company_id)
                    .bind(plan.expected_version)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "qualify" => {
                sqlx::query("UPDATE crm_leads SET status='qualified',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
                    .bind(now)
                    .bind(record_id)
                    .bind(company_id)
                    .bind(plan.expected_version)
                    .execute(&mut *self.connection)
                    .await?;
            }
            _ => {}
        }
        let _ = (command_key, actor_id, record, input);
        Ok(())
    }

    pub async fn update_opportunity(
        &mut self,
        company_id: &str,
        opportunity_id: &str,
        lead_id: &str,
        command_key: &str,
        actor_id: &str,
        record: &OpportunityFacts,
        lead: &LeadFacts,
        input: &OpportunityChangeInput,
        plan: &crate::domain::leads_native::OpportunityChangePlan,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        match plan.action.as_str() {
            "link-quote" => {
                sqlx::query("UPDATE crm_opportunities SET quote_id=$1,quote_version=$2,stage='proposal',version=version+1,updated_at=$3 WHERE id=$4")
                    .bind(plan.quote_id.as_deref())
                    .bind(plan.quote_version)
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "negotiate" => {
                sqlx::query("UPDATE crm_opportunities SET stage='negotiation',version=version+1,updated_at=$1 WHERE id=$2")
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "lose" => {
                sqlx::query("UPDATE crm_opportunities SET stage='lost',version=version+1,updated_at=$1 WHERE id=$2")
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "reopen" => {
                sqlx::query("UPDATE crm_opportunities SET stage='discovery',quote_id=NULL,quote_version=NULL,version=version+1,updated_at=$1 WHERE id=$2")
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "win" => {
                sqlx::query("UPDATE crm_opportunities SET sales_id=$1,win_source='sales',won_at=$2,stage='won',version=version+1,updated_at=$2 WHERE id=$3")
                    .bind(plan.sales_id.as_deref())
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            "win-pms" => {
                sqlx::query("UPDATE crm_opportunities SET reservation_id=$1,win_source='pms',won_at=$2,stage='won',version=version+1,updated_at=$2 WHERE id=$3")
                    .bind(plan.reservation_id.as_deref())
                    .bind(now)
                    .bind(opportunity_id)
                    .execute(&mut *self.connection)
                    .await?;
            }
            _ => {}
        }
        sqlx::query(
            "UPDATE crm_leads SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3",
        )
        .bind(now)
        .bind(lead_id)
        .bind(company_id)
        .execute(&mut *self.connection)
        .await?;
        let _ = (command_key, actor_id, record, lead, input);
        Ok(())
    }
}

impl LeadRepository for PgCrmLeadCommands<'_> {
    type Error = Error;

    fn lead<'a>(
        &'a mut self,
        company_id: &'a str,
        lead_id: &'a str,
    ) -> LeadsFuture<'a, Option<LeadFacts>, Self::Error> {
        Box::pin(async move { PgCrmLeadCommands::lead(self, company_id, lead_id).await })
    }

    fn opportunity<'a>(
        &'a mut self,
        company_id: &'a str,
        opportunity_id: &'a str,
    ) -> LeadsFuture<'a, Option<OpportunityFacts>, Self::Error> {
        Box::pin(
            async move { PgCrmLeadCommands::opportunity(self, company_id, opportunity_id).await },
        )
    }

    fn insert_lead<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        created_by: &'a str,
        plan: &'a crate::domain::leads_native::LeadCreatePlan,
        _input: &'a LeadCreateInput,
        account_id: &'a str,
        contact_id: &'a str,
        now: i64,
    ) -> LeadsFuture<'a, (), Self::Error> {
        Box::pin(async move {
            PgCrmLeadCommands::insert_lead(
                self, company_id, record_id, created_by, plan, _input, account_id, contact_id, now,
            )
            .await
        })
    }

    fn update_lead<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        command_key: &'a str,
        actor_id: &'a str,
        record: &'a LeadFacts,
        input: &'a LeadChangeInput,
        plan: &'a crate::domain::leads_native::LeadChangePlan,
        now: i64,
    ) -> LeadsFuture<'a, (), Self::Error> {
        Box::pin(async move {
            PgCrmLeadCommands::update_lead(
                self,
                company_id,
                record_id,
                command_key,
                actor_id,
                record,
                input,
                plan,
                now,
            )
            .await
        })
    }

    fn update_opportunity<'a>(
        &'a mut self,
        company_id: &'a str,
        opportunity_id: &'a str,
        lead_id: &'a str,
        command_key: &'a str,
        actor_id: &'a str,
        record: &'a OpportunityFacts,
        lead: &'a LeadFacts,
        input: &'a OpportunityChangeInput,
        plan: &'a crate::domain::leads_native::OpportunityChangePlan,
        now: i64,
    ) -> LeadsFuture<'a, (), Self::Error> {
        Box::pin(async move {
            PgCrmLeadCommands::update_opportunity(
                self,
                company_id,
                opportunity_id,
                lead_id,
                command_key,
                actor_id,
                record,
                lead,
                input,
                plan,
                now,
            )
            .await
        })
    }
}
