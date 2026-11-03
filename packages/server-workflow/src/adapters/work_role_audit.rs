//! The original role-selection audit, lent by actual current Identity sources.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::future::Future;
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone)]
pub struct WorkRoleSelectionAudit {
    pub id: String,
    pub command_id: String,
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: String,
    pub revision: i64,
    pub restoring: bool,
    pub created_at: i64,
    pub authority: Value,
}
pub trait PgWorkRoleSelectionAuditSource: Sync {
    fn facts(&self) -> &WorkRoleSelectionAudit;
    fn require_current(
        &self,
        c: &mut PgConnection,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "The original selected work role or its audit changed.",
    )
    .into()
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("WORK_ROLE_AUDIT_UNIT_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
pub async fn lock_work_role_audit_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(changed());
    }
    sqlx::query("LOCK TABLE audit_events IN ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn image(c: &mut PgConnection, id: &str) -> Result<Option<Value>, Error> {
    Ok(
        sqlx::query_scalar("SELECT to_jsonb(a) FROM audit_events a WHERE id=$1 FOR SHARE")
            .bind(id)
            .fetch_optional(c)
            .await?,
    )
}
pub struct RetainedWorkRoleSelectionAudit {
    unit: Unit,
    id: String,
    expected: Value,
}
impl RetainedWorkRoleSelectionAudit {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || image(c, &self.id).await?.as_ref() != Some(&self.expected)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn write_work_role_selection_audit<S: PgWorkRoleSelectionAuditSource>(
    c: &mut PgConnection,
    source: &S,
) -> Result<RetainedWorkRoleSelectionAudit, Error> {
    source.require_current(c).await?;
    let unit = unit(c).await?;
    let f = source.facts();
    if !(1..=9_007_199_254_740_991).contains(&f.revision)
        || !(0..=9_007_199_254_740_991).contains(&f.created_at)
        || f.authority["assignmentId"] != f.assignment_id
        || f.authority["companyId"] != f.company_id
        || f.authority["revision"] != f.revision
    {
        return Err(changed());
    }
    let purpose = if f.restoring {
        "Restored saved work assignment for a new session"
    } else {
        "User selected their active work assignment"
    };
    let authority: String = sqlx::query_scalar("SELECT $1::jsonb::text")
        .bind(f.authority.to_string())
        .fetch_one(&mut *c)
        .await?;
    let expected = json!({"id":f.id,"command_id":f.command_id,"actor_id":f.user_id,"assignment_id":f.assignment_id,"action":"work.role.select","record_type":"work_context","record_id":f.company_id,"record_version":f.revision,"purpose":purpose,"created_at":f.created_at,"authority_json":authority});
    let expected: Value =
        sqlx::query_scalar("SELECT to_jsonb(jsonb_populate_record(NULL::audit_events,$1::jsonb))")
            .bind(expected.to_string())
            .fetch_one(&mut *c)
            .await?;
    sqlx::query("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json)VALUES($1,$2,$3,$4,'work.role.select','work_context',$5,$6,$7,$8,$9)").bind(&f.id).bind(&f.command_id).bind(&f.user_id).bind(&f.assignment_id).bind(&f.company_id).bind(f.revision).bind(purpose).bind(f.created_at).bind(authority).execute(&mut *c).await?;
    let retained = RetainedWorkRoleSelectionAudit {
        unit,
        id: f.id.clone(),
        expected,
    };
    retained.require_current(c).await?;
    source.require_current(c).await?;
    Ok(retained)
}
