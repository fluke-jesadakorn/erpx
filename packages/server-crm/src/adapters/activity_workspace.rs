//! PostgreSQL adapter for the native CRM activity workspace read port.
//!
//! Every SELECT below is the exact owner statement of the retained TypeScript
//! `adapters/activities.ts` reader (`lead`, `rows`, `history`, `workRows` and
//! `recipients`) with only two mechanical changes: `?` bind markers become
//! positional `$n` for sqlx, and the composition-owned purchase filter
//! (`crmNativeReadFilter`, `true` in disabled enforcement mode) is the literal
//! `true` fragment. Purchase enforcement, actor resolution and the Workflow
//! ledger stay in TypeScript composition and are not part of this owner port.
//!
//! The caller owns the retained serializable Holding transaction; no command,
//! purchase check or independent transaction is created here. The cross-owner
//! IAM/People fragments are the closed public projection SQL copied verbatim
//! from `commercial-owner-projection-sources` and `salesRecipientEligibility`.
use crate::application::activity_workspace::{
    ActivityWorkspaceDirectory, ActivityWorkspaceFuture, ActivityWorkspaceRepository,
};
use crate::domain::activity_workspace::{
    ActivityRecipient, CrmActivityRow, CrmLeadRow,
};
use crate::domain::activities_native::ActivityActor;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::postgres::PgRow;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

/// Composition-owned purchase filter. The owner port reads the rows; the
/// `sales.crm` history gate is applied by the caller, never by this adapter.
pub const NATIVE_READ_FILTER: &str = "true";

/// Exact `lead(id)` SELECT with `?` markers converted to `$1..$6` and the
/// disabled-mode filter inlined.
pub const LEAD_SQL: &str = "SELECT l.id,l.company_id,l.branch_id,l.department_id,l.title,l.status,l.version,v.stage FROM crm_leads l LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.id=$1 AND l.company_id=$2 AND ($3::text IS NULL OR l.branch_id=$4) AND ($5::text IS NULL OR l.department_id=$6) AND true";

/// Exact `rows(leadId)` SELECT. `owner_name` and `owner_eligible` are the same
/// People/IAM projection subqueries the TypeScript reader composes; the only
/// difference is the `$1..$7` positional markers.
pub const ACTIVITIES_SQL: &str = "SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,(SELECT legal_name FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=l.company_id AND a.user_id=l.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=l.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=l.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) THEN 1 ELSE 0 END owner_eligible FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE x.company_id=$1 AND x.lead_id=$2 AND l.company_id=$3 AND ($4::text IS NULL OR l.branch_id=$5) AND ($6::text IS NULL OR l.department_id=$7) AND true ORDER BY x.due_date,x.created_at,x.id";

/// Exact immutable-history SELECT (`$1` company, `$2` lead).
pub const HISTORY_SQL: &str = "SELECT e.* FROM crm_lead_activity_events e JOIN crm_lead_activities x ON x.id=e.activity_id WHERE x.company_id=$1 AND x.lead_id=$2 ORDER BY e.created_at DESC,e.version DESC";

/// Exact `workRows()` SELECT. Same row shape as `ACTIVITIES_SQL` but scoped to
/// the selected branch/department only and ordered by due date then recency.
pub const WORK_ROWS_SQL: &str = "SELECT x.*,l.title lead_title,l.status lead_status,v.stage lead_stage,l.branch_id,l.department_id,(SELECT legal_name FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) hr_people WHERE company_id=x.company_id AND user_id=x.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name,CASE WHEN EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=l.company_id AND a.user_id=l.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=l.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=l.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) THEN 1 ELSE 0 END owner_eligible FROM crm_lead_activities x JOIN crm_leads l ON l.id=x.lead_id LEFT JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.company_id=$1 AND ($2::text IS NULL OR l.branch_id=$3) AND ($4::text IS NULL OR l.department_id=$5) AND true ORDER BY x.due_date,x.updated_at DESC";

/// Exact `recipients(leadId)` SELECT (`$1` lead, `$2` company). The
/// `eligible` predicate uses `salesRecipientEligibility('l','a.user_id')`.
pub const RECIPIENTS_SQL: &str = "SELECT DISTINCT a.user_id id,p.legal_name name FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN crm_leads l ON l.id=$1 WHERE a.company_id=$2 AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=l.company_id AND a.user_id=a.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=l.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=l.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) ORDER BY p.legal_name";

async fn read_unit(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CRM_ACTIVITY_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CRM_ACTIVITY_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

fn activity_row(row: &PgRow) -> Result<CrmActivityRow, Error> {
    Ok(CrmActivityRow {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        lead_id: row.try_get("lead_id")?,
        activity_type: row.try_get("activity_type")?,
        subject: row.try_get("subject")?,
        details: row.try_get("details")?,
        owner_id: row.try_get("owner_id")?,
        due_date: row.try_get("due_date")?,
        status: row.try_get("status")?,
        outcome: row.try_get("outcome")?,
        version: row.try_get("version")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        lead_title: row.try_get("lead_title")?,
        lead_status: row.try_get("lead_status")?,
        lead_stage: row.try_get("lead_stage")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        owner_name: row.try_get("owner_name")?,
        // `CASE WHEN ... THEN 1 ELSE 0 END` is SQL INT4 in the retained reader.
        owner_eligible: i64::from(row.try_get::<i32, _>("owner_eligible")?),
    })
}

fn event_value(row: &PgRow) -> Result<Value, Error> {
    Ok(json!({
        "id": row.try_get::<String, _>("id")?,
        "activity_id": row.try_get::<String, _>("activity_id")?,
        "version": row.try_get::<i64, _>("version")?,
        "action": row.try_get::<String, _>("action")?,
        "actor_id": row.try_get::<String, _>("actor_id")?,
        "owner_id": row.try_get::<String, _>("owner_id")?,
        "due_date": row.try_get::<String, _>("due_date")?,
        "note": row.try_get::<String, _>("note")?,
        "snapshot_json": row.try_get::<Value, _>("snapshot_json")?,
        "created_at": row.try_get::<i64, _>("created_at")?,
    }))
}

/// CRM-owned activity workspace rows on the caller's serializable unit.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteMeasurementCard {
    lead_id: String,
    subject: String,
    due_date: String,
}

pub struct PgActivityWorkspace<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgActivityWorkspace<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn lead(
        &mut self,
        actor: &ActivityActor,
        lead_id: &str,
    ) -> Result<Option<CrmLeadRow>, Error> {
        read_unit(&mut *self.connection).await?;
        let row = sqlx::query(LEAD_SQL)
            .bind(lead_id)
            .bind(&actor.company_id)
            .bind(actor.branch_id.as_deref())
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(|row| {
            Ok(CrmLeadRow {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                branch_id: row.try_get("branch_id")?,
                department_id: row.try_get("department_id")?,
                title: row.try_get("title")?,
                status: row.try_get("status")?,
                stage: row.try_get("stage")?,
                version: row.try_get("version")?,
            })
        })
        .transpose()
    }

    pub async fn activities(
        &mut self,
        actor: &ActivityActor,
        lead_id: &str,
    ) -> Result<Vec<CrmActivityRow>, Error> {
        read_unit(&mut *self.connection).await?;
        let rows = sqlx::query(ACTIVITIES_SQL)
            .bind(&actor.company_id)
            .bind(lead_id)
            .bind(&actor.company_id)
            .bind(actor.branch_id.as_deref())
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_all(&mut *self.connection)
            .await?;
        rows.iter().map(activity_row).collect()
    }

    pub async fn history(
        &mut self,
        actor: &ActivityActor,
        lead_id: &str,
    ) -> Result<Vec<Value>, Error> {
        read_unit(&mut *self.connection).await?;
        let rows = sqlx::query(HISTORY_SQL)
            .bind(&actor.company_id)
            .bind(lead_id)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.iter().map(event_value).collect()
    }

    /// The owner row must match its committed event version. Reading the latest
    /// state by primary key makes delivery replay-safe without a counter.
    pub async fn next_site_measurement(
        &mut self,
        actor: &ActivityActor,
    ) -> Result<Option<SiteMeasurementCard>, Error> {
        read_unit(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT x.lead_id, x.subject, x.due_date
             FROM crm_lead_activities x
             JOIN crm_leads l ON l.id = x.lead_id AND l.company_id = x.company_id
             WHERE x.company_id = $1 AND ($2::text IS NULL OR l.branch_id = $2)
               AND ($3::text IS NULL OR l.department_id = $3)
               AND ($4 OR x.owner_id = $5 OR x.created_by = $5)
               AND x.status = 'open' AND x.activity_type = 'meeting'
               AND (lower(ltrim(x.subject)) LIKE 'site measurement%'
                    OR ltrim(x.subject) LIKE 'นัดวัด%')
               AND EXISTS (
                   SELECT 1 FROM outbox_events e
                   WHERE e.aggregate_type = 'crm_activity' AND e.aggregate_id = x.id
                     AND e.aggregate_version = x.version
                     AND e.event_type IN ('crm.activity.create', 'crm.activity.reassign')
               )
             ORDER BY x.due_date, x.id LIMIT 1",
        )
        .bind(&actor.company_id)
        .bind(actor.branch_id.as_deref())
        .bind(actor.department_id.as_deref())
        .bind(actor.can_manage)
        .bind(&actor.user_id)
        .fetch_optional(&mut *self.connection)
        .await?;

        row.map(|row| {
            Ok(SiteMeasurementCard {
                lead_id: row.try_get("lead_id")?,
                subject: row.try_get("subject")?,
                due_date: row.try_get("due_date")?,
            })
        })
        .transpose()
    }

    pub async fn work_rows(&mut self, actor: &ActivityActor) -> Result<Vec<CrmActivityRow>, Error> {
        read_unit(&mut *self.connection).await?;
        let rows = sqlx::query(WORK_ROWS_SQL)
            .bind(&actor.company_id)
            .bind(actor.branch_id.as_deref())
            .bind(actor.branch_id.as_deref())
            .bind(actor.department_id.as_deref())
            .bind(actor.department_id.as_deref())
            .fetch_all(&mut *self.connection)
            .await?;
        rows.iter().map(activity_row).collect()
    }
}

impl ActivityWorkspaceRepository for PgActivityWorkspace<'_> {
    type Error = Error;

    fn lead<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Option<CrmLeadRow>, Self::Error> {
        Box::pin(PgActivityWorkspace::lead(self, actor, lead_id))
    }

    fn activities<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Vec<CrmActivityRow>, Self::Error> {
        Box::pin(PgActivityWorkspace::activities(self, actor, lead_id))
    }

    fn history<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
        lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Vec<Value>, Self::Error> {
        Box::pin(PgActivityWorkspace::history(self, actor, lead_id))
    }

    fn work_rows<'a>(
        &'a mut self,
        actor: &'a ActivityActor,
    ) -> ActivityWorkspaceFuture<'a, Vec<CrmActivityRow>, Self::Error> {
        Box::pin(PgActivityWorkspace::work_rows(self, actor))
    }
}

/// Current Identity/People recipient directory on the caller's unit.
pub struct PgActivityDirectory<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgActivityDirectory<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn recipients(
        &mut self,
        company_id: &str,
        lead_id: &str,
    ) -> Result<Vec<ActivityRecipient>, Error> {
        read_unit(&mut *self.connection).await?;
        let rows = sqlx::query(RECIPIENTS_SQL)
            .bind(lead_id)
            .bind(company_id)
            .fetch_all(&mut *self.connection)
            .await?;
        rows.iter()
            .map(|row| {
                Ok(ActivityRecipient {
                    id: row.try_get("id")?,
                    name: row.try_get("name")?,
                })
            })
            .collect()
    }
}

impl ActivityWorkspaceDirectory for PgActivityDirectory<'_> {
    type Error = Error;

    fn recipients<'a>(
        &'a mut self,
        company_id: &'a str,
        lead_id: &'a str,
    ) -> ActivityWorkspaceFuture<'a, Vec<ActivityRecipient>, Self::Error> {
        Box::pin(PgActivityDirectory::recipients(self, company_id, lead_id))
    }
}
