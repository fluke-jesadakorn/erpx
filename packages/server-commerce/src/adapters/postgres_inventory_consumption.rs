//! Commerce-owned current product/warehouse/settings facts. No Inventory stock,
//! Quality holds, Finance configuration, current authority or effects live here.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumptionCommerceRequest {
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub warehouse_id: RecordId,
    pub product_id: RecordId,
}
/// Data, never authority. Consumers retain the opaque current Commerce proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommercePostingSettings {
    pub company_id: CompanyId,
    pub chart_id: RecordId,
    pub inventory_account_id: RecordId,
    pub cogs_account_id: RecordId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommerceTrackingMode {
    None,
    Lot,
    Serial,
}
#[derive(Eq, PartialEq)]
struct Transaction {
    database: String,
    backend: i32,
    xid: String,
    postmaster: String,
}
#[derive(Eq, PartialEq)]
struct Image {
    warehouse_version: i64,
    product_version: i64,
    tracking: CommerceTrackingMode,
    settings: Option<CommercePostingSettings>,
}
/// Positive same-transaction facts; no Clone/Deserialize or public constructor.
pub struct CurrentConsumptionCommerce {
    request: ConsumptionCommerceRequest,
    image: Image,
    transaction: Transaction,
}
impl CurrentConsumptionCommerce {
    pub fn request(&self) -> &ConsumptionCommerceRequest {
        &self.request
    }
    pub fn tracking_mode(&self) -> CommerceTrackingMode {
        self.image.tracking
    }
    pub fn posting_settings(&self) -> Option<&CommercePostingSettings> {
        self.image.settings.as_ref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction {
            return Err(changed());
        }
        let current = read_consumption_source(c, &self.request)
            .await?
            .ok_or_else(changed)?;
        if current.image != self.image {
            return Err(changed());
        }
        Ok(())
    }
}
/// Utility only; call before the first Holding snapshot with every other owner
/// source fence. The table fences also protect a missing settings row.
pub async fn lock_consumption_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("COMMERCE_CONSUMPTION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE commerce_warehouses,commerce_products,commerce_settings IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_consumption_source(
    c: &mut PgConnection,
    request: &ConsumptionCommerceRequest,
) -> Result<Option<CurrentConsumptionCommerce>, Error> {
    require_unit(c).await?;
    // Matching supplied bounded IDs and schema-constrained status/mode values
    // avoid transferring names, descriptions, or arbitrary product payloads.
    let row=sqlx::query("SELECT w.version warehouse_version,p.version product_version,p.tracking_mode FROM commerce_warehouses w JOIN commerce_products p ON p.id=$4 AND p.company_id=w.company_id WHERE w.company_id=$1 AND w.branch_id=$2 AND w.id=$3 AND w.status='active' AND p.status='active' FOR SHARE OF w,p")
        .bind(request.company_id.as_str()).bind(request.branch_id.as_str()).bind(request.warehouse_id.as_str()).bind(request.product_id.as_str()).fetch_optional(&mut *c).await?;
    let Some(row) = row else { return Ok(None) };
    let warehouse_version: i64 = row.try_get("warehouse_version")?;
    let product_version: i64 = row.try_get("product_version")?;
    if !(1..=9_007_199_254_740_991).contains(&warehouse_version)
        || !(1..=9_007_199_254_740_991).contains(&product_version)
    {
        return Err(invalid());
    }
    let tracking = match row.try_get::<&str, _>("tracking_mode")? {
        "none" => CommerceTrackingMode::None,
        "lot" => CommerceTrackingMode::Lot,
        "serial" => CommerceTrackingMode::Serial,
        _ => return Err(invalid()),
    };
    // Guard variable persisted TEXT in PostgreSQL before transfer. The actual
    // Commerce currency is deliberately not a posting-currency requirement:
    // B0 obtains both posting/base currency from the current Finance chart.
    let settings=sqlx::query("SELECT CASE WHEN octet_length(chart_id)<=200 THEN chart_id END chart_id,CASE WHEN octet_length(inventory_account_id)<=200 THEN inventory_account_id END inventory_account_id,CASE WHEN octet_length(cogs_account_id)<=200 THEN cogs_account_id END cogs_account_id FROM commerce_settings WHERE company_id=$1 FOR SHARE")
        .bind(request.company_id.as_str()).fetch_optional(&mut *c).await?;
    let settings = settings
        .map(|r| -> Result<CommercePostingSettings, Error> {
            let id = |name: &str| -> Result<RecordId, Error> {
                RecordId::new(r.try_get::<Option<String>, _>(name)?.ok_or_else(invalid)?)
                    .map_err(|_| invalid())
            };
            Ok(CommercePostingSettings {
                company_id: request.company_id.clone(),
                chart_id: id("chart_id")?,
                inventory_account_id: id("inventory_account_id")?,
                cogs_account_id: id("cogs_account_id")?,
            })
        })
        .transpose()?;
    Ok(Some(CurrentConsumptionCommerce {
        request: request.clone(),
        image: Image {
            warehouse_version,
            product_version,
            tracking,
            settings,
        },
        transaction: transaction(c).await?,
    }))
}
async fn require_unit(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("COMMERCE_CONSUMPTION_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("COMMERCE_CONSUMPTION_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}
async fn transaction(c: &mut PgConnection) -> Result<Transaction, Error> {
    let r=sqlx::query("SELECT current_database() database,pg_backend_pid() backend,pg_current_xact_id()::text xid,pg_postmaster_start_time()::text postmaster").fetch_one(c).await?;
    Ok(Transaction {
        database: r.try_get("database")?,
        backend: r.try_get("backend")?,
        xid: r.try_get("xid")?,
        postmaster: r.try_get("postmaster")?,
    })
}
fn invalid() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMERCE_CONSUMPTION_SOURCE_INVALID",
        "The current Commerce consumption source is outside its bounded contract.",
    )
    .into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_CONSUMPTION_SOURCE_CHANGED",
        "The retained Commerce consumption source changed.",
    )
    .into()
}
