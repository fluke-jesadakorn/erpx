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
    ActivityActor, ActivityChangeInput, ActivityChangePlan, ActivityCreateInput, ActivityCreatePlan, ActivityReassignIntent, prepare_activity_reassign, validate_change_schema_and_permissions,
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
        actor: &ActivityActor,
        lead_id: &str,
    ) -> Result<Option<LeadFacts>, Error> {
        serializable(&mut *self.connection).await?;
        // The legacy opportunity commands update the opportunity before the
        // lead, and increment the lead version in the same transaction.
        // Lock in that order so a concurrent close cannot pass this gate on a
        // stale stage. Qualification inserts an opportunity and updates its
        // lead in the same transaction; the lead lock also fences that case.
        let stage = sqlx::query_scalar::<_, String>(
            "SELECT v.stage FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id WHERE l.id=$1 AND l.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4) FOR UPDATE OF v",
        )
        .bind(lead_id)
        .bind(&actor.company_id)
        .bind(actor.branch_id.as_deref())
        .bind(actor.department_id.as_deref())
        .fetch_optional(&mut *self.connection)
        .await?;
        let row = sqlx::query(
            "SELECT status,version FROM crm_leads WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) AND ($4::text IS NULL OR department_id=$4) FOR UPDATE",
        )
        .bind(lead_id)
        .bind(&actor.company_id)
        .bind(actor.branch_id.as_deref())
        .bind(actor.department_id.as_deref())
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
        actor: &ActivityActor,
        activity_id: &str,
    ) -> Result<Option<ActivityFacts>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT x.owner_id,x.due_date,x.status,x.version FROM public.crm_lead_activities x JOIN public.crm_leads l ON l.id=x.lead_id WHERE x.id=$1 AND x.company_id=$2 AND l.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4) FOR UPDATE OF l,x",
        )
        .bind(activity_id)
        .bind(&actor.company_id)
        .bind(actor.branch_id.as_deref())
        .bind(actor.department_id.as_deref())
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
            return Err(BusinessError::new(erpx_server_shared::public::FailureKind::Conflict, "CONFLICT", "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.").into());
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

/// Owner-local staged reassign. The caller must authorize and check Workflow
/// replay before this read, and retain Sales, purchase, notification and ledger
/// effects in this same transaction. This type is not command authorization.
pub struct PreparedCrmActivityReassign {
    actor: ActivityActor,
    activity_id: String,
    command_key: String,
    intent: ActivityReassignIntent,
    unit: (String,i32,String,String),
}
impl PreparedCrmActivityReassign {
    /// Validated normalized recipient for the caller's Sales eligibility and
    /// notification preparation. This is not an eligibility proof.
    pub fn owner_id(&self) -> &str { &self.intent.owner_id }
    pub fn due_date(&self) -> &str { &self.intent.due_date }
    pub fn expected_version(&self) -> i64 { self.intent.expected_version }
}

/// Owner-local after-image and event witness; it does not attest recipient,
/// notification, Workflow receipt, purchase or selected actor authority.
pub struct RetainedCrmActivityReassign {
    actor: ActivityActor,
    activity_id: String,
    command_key: String,
    intent: ActivityReassignIntent,
    now: i64,
    unit: (String,i32,String,String),
}

async fn activity_unit(c:&mut PgConnection)->Result<(String,i32,String,String),Error> {
    serializable(c).await?;
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn activity_conflict()->BusinessError {
    BusinessError::new(erpx_server_shared::public::FailureKind::Conflict,"CONFLICT","The record changed, a reference is unavailable, or this value already exists. Refresh and review again.")
}
impl PgCrmActivityCommands<'_> {
    pub async fn prepare_reassign(&mut self, actor:&ActivityActor, activity_id:&str, command_key:&str, input:&ActivityChangeInput)->Result<PreparedCrmActivityReassign,Error> {
        validate_change_schema_and_permissions(actor,input)?;
        if command_key.is_empty() {return Err(BusinessError::invalid("CRM_ACTIVITY_INPUT_INVALID","A command key is required.").into());}
        let record=self.activity(actor,activity_id).await?.ok_or_else(|| BusinessError::new(erpx_server_shared::public::FailureKind::NotFound,"CRM_ACTIVITY_NOT_FOUND","CRM activity not found in this scope."))?;
        let intent=prepare_activity_reassign(actor,input,&record.status)?;
        if record.version != intent.expected_version {return Err(activity_conflict().into());}
        let unit=activity_unit(&mut *self.connection).await?;
        Ok(PreparedCrmActivityReassign{actor:actor.clone(),activity_id:activity_id.to_owned(),command_key:command_key.to_owned(),intent,unit})
    }
    pub async fn apply_reassign(&mut self, prepared:PreparedCrmActivityReassign, now:i64)->Result<RetainedCrmActivityReassign,Error> {
        if activity_unit(&mut *self.connection).await? != prepared.unit {return Err(activity_conflict().into());}
        let next=prepared.intent.expected_version.checked_add(1).ok_or_else(activity_conflict)?;
        let changed=sqlx::query("UPDATE public.crm_lead_activities x SET owner_id=$1,due_date=$2,status='open',outcome=NULL,version=x.version+1,updated_at=$3 FROM public.crm_leads l WHERE x.id=$4 AND x.company_id=$5 AND x.version=$6 AND x.status='open' AND l.id=x.lead_id AND l.company_id=$5 AND ($7::text IS NULL OR l.branch_id=$7) AND ($8::text IS NULL OR l.department_id=$8)")
            .bind(&prepared.intent.owner_id).bind(&prepared.intent.due_date).bind(now).bind(&prepared.activity_id).bind(&prepared.actor.company_id).bind(prepared.intent.expected_version).bind(prepared.actor.branch_id.as_deref()).bind(prepared.actor.department_id.as_deref()).execute(&mut *self.connection).await?.rows_affected();
        if changed != 1 {return Err(activity_conflict().into());}
        let snapshot=serde_json::json!({"action":"reassign","expectedVersion":prepared.intent.expected_version,"ownerId":prepared.intent.owner_id,"dueDate":prepared.intent.due_date,"reason":prepared.intent.reason});
        sqlx::query("INSERT INTO public.crm_lead_activity_events(id,activity_id,version,action,actor_id,owner_id,due_date,note,snapshot_json,created_at) VALUES($1,$2,$3,'reassign',$4,$5,$6,$7,$8,$9)")
            .bind(format!("crm-activity-event-{}",prepared.command_key)).bind(&prepared.activity_id).bind(next).bind(&prepared.actor.user_id).bind(&prepared.intent.owner_id).bind(&prepared.intent.due_date).bind(&prepared.intent.reason).bind(&snapshot).bind(now).execute(&mut *self.connection).await?;
        let retained=RetainedCrmActivityReassign{actor:prepared.actor,activity_id:prepared.activity_id,command_key:prepared.command_key,intent:prepared.intent,now,unit:prepared.unit};
        retained.require_current(&mut *self.connection).await?;
        Ok(retained)
    }
}
/// Notification input derived only from the staged CRM after-image. This is
/// not notification authorization or proof that Collaboration wrote an effect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrmReassignNotificationFacts {
    pub command_key: String,
    pub company_id: String,
    pub recipient: String,
    pub activity_id: String,
    pub version: i64,
    pub status: &'static str,
    pub updated_at: i64,
}
impl RetainedCrmActivityReassign {
    /// Composition must retain this witness and call `require_current` before
    /// and after the Collaboration effect in the same transaction.
    pub fn notification_facts(&self) -> CrmReassignNotificationFacts {
        CrmReassignNotificationFacts {
            command_key:self.command_key.clone(), company_id:self.actor.company_id.clone(),
            recipient:self.intent.owner_id.clone(), activity_id:self.activity_id.clone(),
            version:self.intent.expected_version+1, status:"open", updated_at:self.now,
        }
    }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if activity_unit(&mut *c).await? != self.unit {return Err(activity_conflict().into());}
        let snapshot=serde_json::json!({"action":"reassign","expectedVersion":self.intent.expected_version,"ownerId":self.intent.owner_id,"dueDate":self.intent.due_date,"reason":self.intent.reason});
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM public.crm_lead_activities x JOIN public.crm_leads l ON l.id=x.lead_id AND l.company_id=x.company_id JOIN public.crm_lead_activity_events e ON e.activity_id=x.id WHERE x.id=$1 AND x.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$4) AND x.owner_id=$5 AND x.due_date=$6 AND x.status='open' AND x.outcome IS NULL AND x.version=$7 AND x.updated_at=$8 AND e.id=$9 AND e.version=$7 AND e.action='reassign' AND e.actor_id=$10 AND e.owner_id=$5 AND e.due_date=$6 AND e.note=$11 AND e.snapshot_json=$12 AND e.created_at=$8)")
            .bind(&self.activity_id).bind(&self.actor.company_id).bind(self.actor.branch_id.as_deref()).bind(self.actor.department_id.as_deref()).bind(&self.intent.owner_id).bind(&self.intent.due_date).bind(self.intent.expected_version+1).bind(self.now).bind(format!("crm-activity-event-{}",self.command_key)).bind(&self.actor.user_id).bind(&self.intent.reason).bind(snapshot).fetch_one(c).await?;
        if !valid {return Err(activity_conflict().into());} Ok(())
    }
}

impl ActivityRepository for PgCrmActivityCommands<'_> {
    type Error = Error;

    fn lead<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        lead_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<LeadFacts>, Self::Error> {
        Box::pin(async move { PgCrmActivityCommands::lead(self, actor, lead_id).await })
    }

    fn activity<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        activity_id: &'a str,
    ) -> ActivitiesFuture<'a, Option<ActivityFacts>, Self::Error> {
        Box::pin(
            async move { PgCrmActivityCommands::activity(self, actor, activity_id).await },
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
