//! PostgreSQL adapter for the native CRM activity create / complete /
//! cancel slice. Only `crm_lead_activities` and `crm_lead_activity_events`
//! are written here. The caller owns the retained serializable Holding
/// transaction. Purchase enforcement, the Workflow ledger and staff
/// notifications stay in the TS runtime (`purchases.ts`, `executeCommand`,
/// `prepareCrmActivityNotification`) until the coordinator wires the Rust
/// ledger path.
use crate::application::activities_native::{
    ActivitiesFuture, ActivityFacts, ActivityRepository, LeadFacts,
};
use crate::domain::activities_native::{
    ActivityChangeInput, ActivityChangePlan, ActivityCreateInput, ActivityCreatePlan,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_ACTIVITY_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_ACTIVITY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgCrmActivityCommands<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCrmActivityCommands<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn lead(
        &mut self,
        company_id: &str,
        lead_id: &str,
    ) -> Result<Option<LeadFacts>, Error> {
        serializable(&mut *self.connection).await?;
        // The legacy opportunity commands update the opportunity before the
        // lead, and increment the lead version in the same transaction.
        // Lock in that order so a concurrent close cannot pass this gate on a
        // stale stage. Qualification inserts an opportunity and updates its
        // lead in the same transaction; the lead lock also fences that case.
        let stage = sqlx::query_scalar::<_, String>(
            "SELECT v.stage FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id WHERE l.id=$1 AND l.company_id=$2 FOR UPDATE OF v",
        )
        .bind(lead_id)
        .bind(company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        let row = sqlx::query(
            "SELECT status,version FROM crm_leads WHERE id=$1 AND company_id=$2 FOR UPDATE",
        )
        .bind(lead_id)
        .bind(company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(LeadFacts {
                status: row.try_get("status")?,
                stage,
                version: row.try_get("version")?,
            })
        })
        .transpose()
    }

    pub async fn activity(
        &mut self,
        company_id: &str,
        activity_id: &str,
    ) -> Result<Option<ActivityFacts>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT owner_id,due_date,status,version FROM crm_lead_activities WHERE id=$1 AND company_id=$2",
        )
        .bind(activity_id)
        .bind(company_id)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(ActivityFacts {
                owner_id: row.try_get("owner_id")?,
                due_date: row.try_get("due_date")?,
                status: row.try_get("status")?,
                version: row.try_get("version")?,
            })
        })
        .transpose()
    }

    pub async fn insert_activity(
        &mut self,
        company_id: &str,
        record_id: &str,
        command_key: &str,
        created_by: &str,
        plan: &ActivityCreatePlan,
        input: &ActivityCreateInput,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        sqlx::query(
            "INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(record_id)
        .bind(company_id)
        .bind(&plan.lead_id)
        .bind(&plan.activity_type)
        .bind(&plan.subject)
        .bind(&plan.details)
        .bind(&plan.owner_id)
        .bind(&plan.due_date)
        .bind(created_by)
        .bind(now)
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO crm_lead_activity_events(id,activity_id,version,action,actor_id,owner_id,due_date,note,snapshot_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(format!("crm-activity-event-{command_key}"))
        .bind(record_id)
        .bind(1i64)
        .bind("create")
        .bind(created_by)
        .bind(&plan.owner_id)
        .bind(&plan.due_date)
        .bind(&plan.details)
        .bind(serde_json::json!({"leadId": plan.lead_id, "expectedVersion": input.expected_version,
            "activityType": plan.activity_type, "subject": plan.subject, "details": plan.details,
            "ownerId": plan.owner_id, "dueDate": plan.due_date}))
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }

    pub async fn update_activity(
        &mut self,
        company_id: &str,
        record_id: &str,
        command_key: &str,
        actor_id: &str,
        record: &ActivityFacts,
        input: &ActivityChangeInput,
        plan: &ActivityChangePlan,
        now: i64,
    ) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        let affected = sqlx::query(
            "UPDATE crm_lead_activities SET status=$1,outcome=$2,version=version+1,updated_at=$3 WHERE id=$4 AND company_id=$5 AND version=$6 AND status='open'",
        )
        .bind(&plan.status)
        .bind(plan.outcome.as_deref())
        .bind(now)
        .bind(record_id)
        .bind(company_id)
        .bind(plan.expected_version)
        .execute(&mut *self.connection)
        .await?
        .rows_affected();
        if affected != 1 {
            return Err(PersistenceError::new("CRM_ACTIVITY_UPDATE_CONFLICT").into());
        }
        let version =
            sqlx::query_scalar::<_, i64>("SELECT version FROM crm_lead_activities WHERE id=$1")
                .bind(record_id)
                .fetch_one(&mut *self.connection)
                .await?;
        sqlx::query(
            "INSERT INTO crm_lead_activity_events(id,activity_id,version,action,actor_id,owner_id,due_date,note,snapshot_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(format!("crm-activity-event-{command_key}"))
        .bind(record_id)
        .bind(version)
        .bind(&plan.action)
        .bind(actor_id)
        .bind(&record.owner_id)
        .bind(&record.due_date)
        .bind(plan.outcome.as_deref().unwrap_or(""))
        .bind(serde_json::json!({"action": input.action, "expectedVersion": input.expected_version,
            "reason": plan.outcome}))
        .bind(now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

impl ActivityRepository for PgCrmActivityCommands<'_> {
    type Error = Error;

    fn lead<'a>(
        &'a mut self,
        company_id: &'a str,
        lead_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<LeadFacts>, Self::Error> {
        Box::pin(async move { PgCrmActivityCommands::lead(self, company_id, lead_id).await })
    }

    fn activity<'a>(
        &'a mut self,
        company_id: &'a str,
        activity_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<ActivityFacts>, Self::Error> {
        Box::pin(
            async move { PgCrmActivityCommands::activity(self, company_id, activity_id).await },
        )
    }

    fn insert_activity<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        command_key: &'a str,
        created_by: &'a str,
        plan: &'a ActivityCreatePlan,
        input: &'a ActivityCreateInput,
        now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error> {
        Box::pin(async move {
            PgCrmActivityCommands::insert_activity(
                self,
                company_id,
                record_id,
                command_key,
                created_by,
                plan,
                input,
                now,
            )
            .await
        })
    }

    fn update_activity<'a>(
        &'a mut self,
        company_id: &'a str,
        record_id: &'a str,
        command_key: &'a str,
        actor_id: &'a str,
        record: &'a ActivityFacts,
        input: &'a ActivityChangeInput,
        plan: &'a ActivityChangePlan,
        now: i64,
    ) -> ActivitiesFuture<'a, (), Self::Error> {
        Box::pin(async move {
            PgCrmActivityCommands::update_activity(
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
}
