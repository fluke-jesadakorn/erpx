//! Scoped hold facts for an attachment coordinator. Source fields are stored
//! labels only; they do not establish ancestry, purchase or actor authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachmentHoldFacts {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub lot_id: Option<String>,
    pub version: i64,
    pub status: String,
    pub source_type: Option<String>,
    pub source_id: Option<String>,
    pub spec_id: Option<String>,
    pub spec_version: Option<i64>,
    pub created_by: String,
    pub held_quantity_milli: Option<i64>,
}
/// Acquire before the first data snapshot in the coordinator's reviewed lock order.
pub async fn lock_attachment_hold_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() { return Err(PersistenceError::new("QUALITY_ATTACHMENT_TRANSACTION_REQUIRED").into()); }
    sqlx::query("LOCK TABLE inventory_quality_holds IN SHARE MODE").execute(c).await?;
    Ok(())
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction() || sqlx::query_scalar::<_, String>("SHOW transaction_isolation").fetch_one(&mut *c).await? != "serializable" {
        return Err(PersistenceError::new("QUALITY_ATTACHMENT_SERIALIZABLE_REQUIRED").into());
    }
    let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND relation='inventory_quality_holds'::regclass AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock'))").fetch_one(&mut *c).await?;
    if !held { return Err(PersistenceError::new("QUALITY_ATTACHMENT_HOLD_LOCK_REQUIRED").into()); }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
async fn capture(c: &mut PgConnection, company: &str, branch: Option<&str>, id: &str) -> Result<Option<AttachmentHoldFacts>, Error> {
    let row = sqlx::query("SELECT id,company_id,branch_id,warehouse_id,product_id,lot_id,version,status,source_type,source_id,spec_id,spec_version,created_by,held_quantity_milli FROM inventory_quality_holds WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3)")
        .bind(id).bind(company).bind(branch).fetch_optional(c).await?;
    row.map(|r| -> Result<AttachmentHoldFacts, sqlx::Error> { Ok(AttachmentHoldFacts {
        id:r.try_get("id")?,company_id:r.try_get("company_id")?,branch_id:r.try_get("branch_id")?,
        warehouse_id:r.try_get("warehouse_id")?,product_id:r.try_get("product_id")?,lot_id:r.try_get("lot_id")?,
        version:r.try_get("version")?,status:r.try_get("status")?,source_type:r.try_get("source_type")?,
        source_id:r.try_get("source_id")?,spec_id:r.try_get("spec_id")?,spec_version:r.try_get("spec_version")?,
        created_by:r.try_get("created_by")?,held_quantity_milli:r.try_get("held_quantity_milli")?,
    }) }).transpose().map_err(Into::into)
}
pub struct CurrentAttachmentHold { company: String, branch: Option<String>, id: String, unit: Unit, facts: Option<AttachmentHoldFacts> }
/// A missing, wrong-Company or wrong-Branch target produces retained absence.
pub async fn inspect_attachment_hold(c: &mut PgConnection, company: &str, branch: Option<&str>, id: &str) -> Result<CurrentAttachmentHold, Error> {
    let unit=unit(c).await?;
    let facts=capture(c,company,branch,id).await?;
    Ok(CurrentAttachmentHold { company:company.to_owned(),branch:branch.map(str::to_owned),id:id.to_owned(),unit,facts })
}
impl CurrentAttachmentHold {
    pub fn facts(&self) -> Option<&AttachmentHoldFacts> { self.facts.as_ref() }
    pub async fn require_current(&self,c:&mut PgConnection)->Result<(),Error> {
        if unit(c).await? != self.unit || capture(c,&self.company,self.branch.as_deref(),&self.id).await? != self.facts {
            return Err(BusinessError::new(FailureKind::Conflict,"QUALITY_ATTACHMENT_HOLD_CHANGED","The Quality attachment target changed.").into());
        }
        Ok(())
    }
}
