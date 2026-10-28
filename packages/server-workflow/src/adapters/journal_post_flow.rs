//! Retained Workflow route and event for a manual/opening journal post. Every
//! operation uses the caller's transaction; this adapter never commits.
use crate::domain::journal_post_flow::{JournalFlowCandidate, select_journal_primary};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FLOW_PRIMARY_REQUIRED",
        "The current journal reviewer route changed.",
    )
    .into()
}
fn transaction(c: &PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        Err(PersistenceError::new("WORKFLOW_OWNER_TRANSACTION_REQUIRED").into())
    } else {
        Ok(())
    }
}
pub async fn lock_journal_post_flow_sources(c: &mut PgConnection) -> Result<(), Error> {
    transaction(c)?;
    sqlx::query("LOCK TABLE flow_routes,flow_events IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *c)
        .await?;
    sqlx::query("LOCK TABLE flow_defaults IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Route {
    version: i64,
    defaults_revision: i64,
    steps: String,
    created_at: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalFlowBasis {
    company: CompanyId,
    journal: String,
    transaction_id: i64,
    database: String,
    backend_pid: i32,
    postmaster_started: String,
    route: Option<Route>,
    defaults_revision: i64,
    default_primary: Option<String>,
}
impl JournalFlowBasis {
    pub fn preferred_primary(&self) -> Result<Option<String>, Error> {
        if let Some(route) = &self.route {
            let invalid = || Error::from(PersistenceError::new("FLOW_ROUTE_INVALID"));
            let value: Value = serde_json::from_str(&route.steps).map_err(|_| invalid())?;
            if value.is_null()
                || value
                    .as_object()
                    .is_some_and(|v| v.values().any(Value::is_null))
                || value
                    .as_array()
                    .is_some_and(|v| v.iter().any(Value::is_null))
            {
                return Err(invalid());
            }
            // JavaScript property access on non-null primitives has no
            // primaryUserId. A present non-string primary remains selected and
            // cannot match any canonical candidate.
            if let Some(primary) = value
                .get("draft")
                .and_then(Value::as_object)
                .and_then(|step| step.get("primaryUserId"))
                && !primary.is_null()
            {
                return primary
                    .as_str()
                    .map(|id| Some(id.to_owned()))
                    .ok_or_else(changed);
            }
        }
        Ok(self.default_primary.clone())
    }
}
pub async fn read_journal_flow_basis(
    c: &mut PgConnection,
    company: &CompanyId,
    journal: &str,
) -> Result<JournalFlowBasis, Error> {
    transaction(c)?;
    let (transaction_id, database, backend_pid, postmaster_started): (i64, String, i32, String) =
        sqlx::query_as("SELECT txid_current()::bigint,current_database()::text,pg_backend_pid(),pg_postmaster_start_time()::text")
            .fetch_one(&mut *c).await?;
    let route=sqlx::query("SELECT version,defaults_revision,steps_json,created_at FROM flow_routes WHERE company_id=$1 AND kind='journal' AND record_id=$2")
        .bind(company.as_str()).bind(journal).fetch_optional(&mut *c).await?
        .map(|r|Ok::<_,sqlx::Error>(Route{version:r.try_get("version")?,defaults_revision:r.try_get("defaults_revision")?,steps:r.try_get("steps_json")?,created_at:r.try_get("created_at")?})).transpose()?;
    let defaults_revision: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(revision),0) FROM flow_defaults WHERE company_id=$1",
    )
    .bind(company.as_str())
    .fetch_one(&mut *c)
    .await?;
    let revision = route
        .as_ref()
        .map_or(defaults_revision, |r| r.defaults_revision);
    let default_primary=sqlx::query_scalar("SELECT primary_user_id FROM flow_defaults WHERE company_id=$1 AND kind='journal' AND step_key='draft' AND branch_id IS NULL AND department_id IS NULL AND revision<=$2 ORDER BY revision DESC LIMIT 1")
        .bind(company.as_str()).bind(revision).fetch_optional(c).await?;
    Ok(JournalFlowBasis {
        company: company.clone(),
        journal: journal.into(),
        transaction_id,
        database,
        backend_pid,
        postmaster_started,
        route,
        defaults_revision,
        default_primary,
    })
}
pub struct JournalPostFlowWitness {
    basis: JournalFlowBasis,
    route: Route,
    actor: String,
    actor_name: String,
    command: String,
    event: String,
    version: i64,
    now: i64,
}
pub struct JournalPostFlowRequest<'a> {
    pub candidates: &'a [JournalFlowCandidate],
    pub mapped_preferred: Option<&'a str>,
    pub actor: &'a str,
    pub actor_name: &'a str,
    pub actor_can_participate: bool,
    pub command: &'a str,
    pub event: &'a str,
    pub version: i64,
    pub now: i64,
}
impl JournalPostFlowWitness {
    pub fn prepare(
        basis: JournalFlowBasis,
        input: JournalPostFlowRequest<'_>,
    ) -> Result<Self, Error> {
        let JournalPostFlowRequest {
            candidates,
            mapped_preferred,
            actor,
            actor_name,
            actor_can_participate,
            command,
            event,
            version,
            now,
        } = input;
        let selected = select_journal_primary(
            candidates,
            mapped_preferred,
            None,
            actor,
            actor_can_participate,
        )?;
        let route=basis.route.clone().unwrap_or_else(||Route {version:1,defaults_revision:basis.defaults_revision,steps:json!({"draft":{"primaryUserId":selected.user_id,"teamName":selected.team_name,"defaultRevision":basis.defaults_revision}}).to_string(),created_at:now});
        Ok(Self {
            basis,
            route,
            actor: actor.into(),
            actor_name: actor_name.into(),
            command: command.into(),
            event: event.into(),
            version,
            now,
        })
    }
    pub async fn apply_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        if read_journal_flow_basis(c, &self.basis.company, &self.basis.journal).await? != self.basis
        {
            return Err(changed());
        }
        if self.basis.route.is_none() {
            sqlx::query("INSERT INTO flow_routes(company_id,kind,record_id,defaults_revision,steps_json,created_at) VALUES($1,'journal',$2,$3,$4,$5)")
                .bind(self.basis.company.as_str()).bind(&self.basis.journal).bind(self.route.defaults_revision).bind(&self.route.steps).bind(self.route.created_at).execute(&mut *c).await?;
        }
        sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES($1,$2,$3,'journal',$4,'draft','post',$5,$6,'',$7,$8)")
            .bind(&self.event).bind(&self.command).bind(self.basis.company.as_str()).bind(&self.basis.journal).bind(&self.actor).bind(&self.actor_name).bind(self.version).bind(self.now).execute(c).await?;
        Ok(())
    }
    pub async fn apply_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        let count=sqlx::query("UPDATE flow_events SET next_step_key='posted',next_user_id=NULL WHERE id=$1 AND command_id=$2 AND company_id=$3 AND kind='journal' AND record_id=$4")
            .bind(&self.event).bind(&self.command).bind(self.basis.company.as_str()).bind(&self.basis.journal).execute(c).await?.rows_affected();
        if count != 1 {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn retain(&self, c: &mut PgConnection) -> Result<(), Error> {
        transaction(c)?;
        let current = read_journal_flow_basis(c, &self.basis.company, &self.basis.journal).await?;
        if current.transaction_id != self.basis.transaction_id
            || current.database != self.basis.database
            || current.backend_pid != self.basis.backend_pid
            || current.postmaster_started != self.basis.postmaster_started
            || current.route.as_ref() != Some(&self.route)
            || current.defaults_revision != self.basis.defaults_revision
            || current.default_primary != self.basis.default_primary
        {
            return Err(changed());
        }
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM flow_events WHERE id=$1 AND command_id=$2 AND company_id=$3 AND kind='journal' AND record_id=$4 AND step_key='draft' AND action='post' AND actor_user_id=$5 AND actor_name=$6 AND reason='' AND record_version=$7 AND created_at=$8 AND next_step_key='posted' AND next_user_id IS NULL)")
            .bind(&self.event).bind(&self.command).bind(self.basis.company.as_str()).bind(&self.basis.journal).bind(&self.actor).bind(&self.actor_name).bind(self.version).bind(self.now).fetch_one(c).await?;
        if !exists {
            return Err(changed());
        }
        Ok(())
    }
}
