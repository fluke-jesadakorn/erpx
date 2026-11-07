use crate::domain::maintenance::MaintenanceAssetFacts;
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, HoldingId, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Transaction = (String, i32, String, String);
mod effects;
pub use effects::{
    MaintenancePhysicalChange, MaintenancePhysicalReceipt, PreparedMaintenancePhysicalChange,
};
/// Same-unit physical-source descriptor; never a Person/Work/purchase grant.
pub struct CurrentMaintenanceAsset {
    facts: MaintenanceAssetFacts,
    transaction: Transaction,
}
impl CurrentMaintenanceAsset {
    pub fn facts(&self) -> &MaintenanceAssetFacts {
        &self.facts
    }
    /// Full owned source retention for an enclosing Maintenance purchase/write
    /// boundary. The ordinary minimal relationship reader keeps its semantics.
    pub async fn retain_unchanged_image(
        &self,
        c: &mut PgConnection,
    ) -> Result<RetainedMaintenanceAssetImage, Error> {
        self.require_current(c).await?;
        let digest = image_digest(c, &self.facts).await?.ok_or_else(changed)?;
        Ok(RetainedMaintenanceAssetImage {
            facts: self.facts.clone(),
            transaction: self.transaction.clone(),
            digest,
        })
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.transaction {
            return Err(changed().into());
        }
        let current = read_current_maintenance_asset(
            c,
            &self.facts.holding_id,
            &self.facts.company_id,
            &self.facts.asset_id,
        )
        .await?
        .ok_or_else(changed)?;
        if current.facts != self.facts {
            return Err(changed().into());
        }
        Ok(())
    }
}
/// Preparation occurs before the owning coordinator's first data snapshot.
pub async fn lock_maintenance_asset_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ASSET_MAINTENANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE operational_assets IN ROW SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

/// The coordinator must call this before its first data snapshot. This lock
/// protects only Asset sources; it does not authorize a Maintenance command.
pub async fn lock_maintenance_creation_asset_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ASSET_MAINTENANCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE operational_assets IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}

/// A scoped source observation, not an authorization or a guarantee that no
/// open Maintenance work exists. The coordinator must prove those separately.
pub struct CurrentMaintenanceCreationAsset {
    source: CurrentMaintenanceAsset,
    lifecycle: String,
}

impl CurrentMaintenanceCreationAsset {
    pub fn facts(&self) -> &MaintenanceAssetFacts { self.source.facts() }
    pub fn lifecycle(&self) -> &str { &self.lifecycle }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.source.transaction || !creation_lock_held(c).await? {
            return Err(changed().into());
        }
        self.source.require_current(c).await?;
        let current: Option<String> = sqlx::query_scalar("SELECT lifecycle_status FROM operational_assets WHERE id=$1 AND company_id=$2")
            .bind(self.source.facts.asset_id.as_str()).bind(self.source.facts.company_id.as_str())
            .fetch_optional(c).await?;
        if current.as_deref() != Some(self.lifecycle.as_str()) { return Err(changed().into()); }
        Ok(())
    }
}

/// Retains the legacy create admission fields without materializing unrelated
/// Asset metadata. Lock timing before the snapshot is a coordinator obligation.
pub async fn inspect_maintenance_creation_asset(
    c: &mut PgConnection, holding: &HoldingId, company: &CompanyId,
    branch: Option<&RecordId>, department: Option<&RecordId>, asset: &RecordId,
    expected_version: i64,
) -> Result<Option<CurrentMaintenanceCreationAsset>, Error> {
    transaction(c).await?;
    if !creation_lock_held(c).await? { return Err(changed().into()); }
    let Some(source) = read_current_maintenance_asset(c, holding, company, asset).await? else { return Ok(None) };
    let facts = source.facts();
    if facts.version != expected_version || branch.is_some_and(|b| b != &facts.branch_id)
        || department.is_some_and(|d| Some(d) != facts.department_id.as_ref()) {
        return Ok(None);
    }
    let lifecycle: String = sqlx::query_scalar("SELECT lifecycle_status FROM operational_assets WHERE id=$1 AND company_id=$2")
        .bind(asset.as_str()).bind(company.as_str()).fetch_one(c).await?;
    if !matches!(lifecycle.as_str(), "registered" | "in_service" | "maintenance") { return Ok(None); }
    Ok(Some(CurrentMaintenanceCreationAsset { source, lifecycle }))
}

async fn creation_lock_held(c: &mut PgConnection) -> Result<bool, Error> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND relation='operational_assets'::regclass AND mode='ShareRowExclusiveLock' AND granted)")
        .fetch_one(c).await?)
}
pub async fn read_current_maintenance_asset(
    c: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    asset: &RecordId,
) -> Result<Option<CurrentMaintenanceAsset>, Error> {
    let transaction = transaction(c).await?;
    // Names, costs, currency, custodians and private profile fields are not
    // needed to prove this relationship and are never transferred here.
    let row=sqlx::query("SELECT CASE WHEN octet_length(id)<=200 THEN id END id,CASE WHEN octet_length(company_id)<=200 THEN company_id END company_id,CASE WHEN octet_length(branch_id)<=200 THEN branch_id END branch_id,octet_length(department_id)::bigint department_bytes,CASE WHEN octet_length(department_id)<=200 THEN department_id END department_id,version::bigint version FROM operational_assets WHERE id=$1 AND company_id=$2 FOR UPDATE")
        .bind(asset.as_str()).bind(company.as_str()).fetch_optional(c).await?;
    let Some(row) = row else { return Ok(None) };
    if row
        .try_get::<Option<i64>, _>("department_bytes")?
        .is_some_and(|n| n > 200)
    {
        return Err(invalid().into());
    }
    let facts = MaintenanceAssetFacts {
        holding_id: holding.clone(),
        company_id: CompanyId::new(
            row.try_get::<Option<String>, _>("company_id")?
                .ok_or_else(invalid)?,
        )?,
        asset_id: RecordId::new(
            row.try_get::<Option<String>, _>("id")?
                .ok_or_else(invalid)?,
        )?,
        branch_id: RecordId::new(
            row.try_get::<Option<String>, _>("branch_id")?
                .ok_or_else(invalid)?,
        )?,
        department_id: row
            .try_get::<Option<String>, _>("department_id")?
            .map(RecordId::new)
            .transpose()?,
        version: row.try_get("version")?,
    };
    if &facts.company_id != company || &facts.asset_id != asset {
        return Err(changed().into());
    }
    Ok(Some(CurrentMaintenanceAsset { facts, transaction }))
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("ASSET_MAINTENANCE_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("ASSET_MAINTENANCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ASSET_MAINTENANCE_SOURCE_CHANGED",
        "The actual physical Asset source changed.",
    )
}
fn invalid() -> PersistenceError {
    PersistenceError::new("ASSET_MAINTENANCE_SOURCE_INVALID")
}

/// Full owned Asset image, not a commercial grant or an intentional mutation
/// protocol. Hashing stays inside PostgreSQL; names/costs never leave this port.
pub struct RetainedMaintenanceAssetImage {
    facts: MaintenanceAssetFacts,
    transaction: Transaction,
    digest: String,
}
impl RetainedMaintenanceAssetImage {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if transaction(c).await? != self.transaction {
            return Err(changed().into());
        }
        let current = read_current_maintenance_asset(
            c,
            &self.facts.holding_id,
            &self.facts.company_id,
            &self.facts.asset_id,
        )
        .await?
        .ok_or_else(changed)?;
        if current.facts != self.facts
            || image_digest(c, &self.facts).await?.as_deref() != Some(&self.digest)
        {
            return Err(changed().into());
        }
        Ok(())
    }
}
async fn image_digest(
    c: &mut PgConnection,
    f: &MaintenanceAssetFacts,
) -> Result<Option<String>, Error> {
    Ok(sqlx::query_scalar("SELECT encode(sha256(convert_to(to_jsonb(a)::text,'UTF8')),'hex') FROM operational_assets a WHERE id=$1 AND company_id=$2")
        .bind(f.asset_id.as_str()).bind(f.company_id.as_str()).fetch_optional(c).await?)
}
