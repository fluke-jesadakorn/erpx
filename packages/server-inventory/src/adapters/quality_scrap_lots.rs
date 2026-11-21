//! Inventory-owned lot and lot-balance facts for a Quality scrap disposition.
//! Commerce owns product/tracking policy; this port reads only Inventory rows.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
const MAX_LOTS: usize = 1000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityScrapLotFacts {
    pub id: String,
    pub tracking_mode: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub manufactured_date: Option<String>,
    pub expiry_date: Option<String>,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub balance_version: i64,
}

pub struct CurrentQualityScrapLots {
    company: CompanyId,
    product_id: String,
    warehouse_id: String,
    facts: Vec<QualityScrapLotFacts>,
    unit: Unit,
}

impl CurrentQualityScrapLots {
    pub fn facts(&self) -> &[QualityScrapLotFacts] {
        &self.facts
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.company, &self.product_id, &self.warehouse_id).await?
                != self.facts
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Utility-only locks; acquire before the first serializable snapshot with the
/// other owner sources.
pub async fn lock_quality_scrap_lot_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE inventory_lots,inventory_lot_balances IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}

/// Current positive warehouse balances in a stable lot order. Absence is a
/// fact, never permission to create a lot or balance.
pub async fn inspect_quality_scrap_lots(
    c: &mut PgConnection,
    company: &CompanyId,
    product_id: &str,
    warehouse_id: &str,
) -> Result<CurrentQualityScrapLots, Error> {
    let unit = unit(c).await?;
    let facts = read(c, company, product_id, warehouse_id).await?;
    Ok(CurrentQualityScrapLots {
        company: company.clone(),
        product_id: product_id.to_owned(),
        warehouse_id: warehouse_id.to_owned(),
        facts,
        unit,
    })
}

async fn read(
    c: &mut PgConnection,
    company: &CompanyId,
    product_id: &str,
    warehouse_id: &str,
) -> Result<Vec<QualityScrapLotFacts>, Error> {
    let rows = sqlx::query(
        "SELECT l.id,l.tracking_mode,l.lot_code,l.serial_number,l.manufactured_date,l.expiry_date,\
                b.quantity_milli,b.value_minor,b.version balance_version \
         FROM inventory_lots l JOIN inventory_lot_balances b ON b.lot_id=l.id \
         WHERE l.company_id=$1 AND l.product_id=$2 AND b.warehouse_id=$3 AND b.quantity_milli>0 \
         ORDER BY l.id LIMIT 1001",
    )
    .bind(company.as_str())
    .bind(product_id)
    .bind(warehouse_id)
    .fetch_all(c)
    .await?;
    if rows.len() > MAX_LOTS {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "INVENTORY_QUALITY_SCRAP_LIMIT",
            "The Quality scrap lot source exceeds its bounded read.",
        )
        .into());
    }
    rows.iter()
        .map(|row| {
            Ok(QualityScrapLotFacts {
                id: row.try_get("id")?,
                tracking_mode: row.try_get("tracking_mode")?,
                lot_code: row.try_get("lot_code")?,
                serial_number: row.try_get("serial_number")?,
                manufactured_date: row.try_get("manufactured_date")?,
                expiry_date: row.try_get("expiry_date")?,
                quantity_milli: row.try_get("quantity_milli")?,
                value_minor: row.try_get("value_minor")?,
                balance_version: row.try_get("balance_version")?,
            })
        })
        .collect()
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}

fn invalid() -> Error {
    PersistenceError::new("INVENTORY_QUALITY_SCRAP_TRANSACTION_REQUIRED").into()
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_QUALITY_SCRAP_LOTS_CHANGED",
        "The Quality scrap lot balances changed.",
    )
    .into()
}
