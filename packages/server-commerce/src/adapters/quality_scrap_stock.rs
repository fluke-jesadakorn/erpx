//! Commerce-owned stock/product/branch facts for a Quality scrap disposition.
//! Inventory owns stock quantities and values; this port joins the current
//! Commerce product and warehouse policy to the same stock row.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityScrapStockFacts {
    pub warehouse_id: String,
    pub product_id: String,
    pub branch_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
    pub product_version: i64,
    pub name: String,
    pub unit: String,
    pub tracking_mode: String,
}

pub struct CurrentQualityScrapStock {
    company: CompanyId,
    warehouse_id: String,
    product_id: String,
    facts: QualityScrapStockFacts,
    unit: Unit,
}

impl CurrentQualityScrapStock {
    pub fn facts(&self) -> &QualityScrapStockFacts {
        &self.facts
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(
                c,
                &self.company,
                &self.warehouse_id,
                &self.product_id,
            )
            .await?
            .as_ref()
                != Some(&self.facts)
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Utility-only locks; acquire before the first serializable snapshot with the
/// other owner sources.
pub async fn lock_quality_scrap_stock_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE commerce_stock,commerce_products,commerce_warehouses IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}

/// None means the stock row, current product or warehouse is missing, inactive
/// or outside the Company; it is never permission to create one.
pub async fn inspect_quality_scrap_stock(
    c: &mut PgConnection,
    company: &CompanyId,
    warehouse_id: &str,
    product_id: &str,
) -> Result<Option<CurrentQualityScrapStock>, Error> {
    let unit = unit(c).await?;
    Ok(
        read(c, company, warehouse_id, product_id)
            .await?
            .map(|facts| CurrentQualityScrapStock {
                company: company.clone(),
                warehouse_id: warehouse_id.to_owned(),
                product_id: product_id.to_owned(),
                facts,
                unit,
            }),
    )
}

async fn read(
    c: &mut PgConnection,
    company: &CompanyId,
    warehouse_id: &str,
    product_id: &str,
) -> Result<Option<QualityScrapStockFacts>, Error> {
    let row = sqlx::query(
        "SELECT s.warehouse_id,s.product_id,w.branch_id,s.quantity_milli,s.value_minor,s.version,p.version product_version,p.name,p.unit,p.tracking_mode \
         FROM commerce_stock s JOIN commerce_products p ON p.id=s.product_id AND p.company_id=$3 \
         JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=$3 \
         WHERE s.warehouse_id=$1 AND s.product_id=$2 AND p.status='active' AND w.status='active' \
         FOR SHARE OF s,p,w",
    )
    .bind(warehouse_id)
    .bind(product_id)
    .bind(company.as_str())
    .fetch_optional(c)
    .await?;
    row.map(|r| {
        Ok(QualityScrapStockFacts {
            warehouse_id: r.try_get("warehouse_id")?,
            product_id: r.try_get("product_id")?,
            branch_id: r.try_get("branch_id")?,
            quantity_milli: r.try_get("quantity_milli")?,
            value_minor: r.try_get("value_minor")?,
            version: r.try_get("version")?,
            product_version: r.try_get("product_version")?,
            name: r.try_get("name")?,
            unit: r.try_get("unit")?,
            tracking_mode: r.try_get("tracking_mode")?,
        })
    })
    .transpose()
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
    PersistenceError::new("COMMERCE_QUALITY_SCRAP_TRANSACTION_REQUIRED").into()
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "COMMERCE_QUALITY_SCRAP_CHANGED",
        "The Quality scrap stock, product or warehouse changed.",
    )
    .into()
}
