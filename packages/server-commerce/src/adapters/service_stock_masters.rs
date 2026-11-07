//! Commerce row ownership only; external authority and Money are proved by the
//! assembled same-unit source, never inferred from these rows.
use crate::domain::service_stock_masters::{ServiceStockMaster, ServiceStockMasterIntent};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("SERVICE_STOCK_MASTER_SOURCE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The original Service stock master source changed.",
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
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
/// Utility locks must be taken before any snapshot and together with all other owner locks.
pub async fn lock_service_stock_masters(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE commerce_products,commerce_warehouses IN SHARE ROW EXCLUSIVE MODE")
        .execute(c)
        .await?;
    Ok(())
}
async fn row(
    c: &mut PgConnection,
    intent: &ServiceStockMasterIntent,
) -> Result<Option<String>, Error> {
    let sql = match &intent.master {
        ServiceStockMaster::Warehouse { .. } => {
            "SELECT CASE WHEN octet_length(to_jsonb(x)::text)<=2097152 THEN to_jsonb(x)::text ELSE NULL END FROM commerce_warehouses x WHERE x.id=$1"
        }
        ServiceStockMaster::Product { .. } => {
            "SELECT CASE WHEN octet_length(to_jsonb(x)::text)<=2097152 THEN to_jsonb(x)::text ELSE NULL END FROM commerce_products x WHERE x.id=$1"
        }
    };
    let value: Option<Option<String>> = sqlx::query_scalar(sql)
        .bind(intent.id())
        .fetch_optional(c)
        .await?;
    match value {
        None => Ok(None),
        Some(Some(v)) => Ok(Some(v)),
        Some(None) => Err(invalid()),
    }
}
/// Retain the actual current row during replay, including legitimate later edits.
pub struct CurrentServiceStockMaster {
    intent: ServiceStockMasterIntent,
    unit: Unit,
    image: String,
}
impl CurrentServiceStockMaster {
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || row(c, &self.intent).await?.as_deref() != Some(&self.image)
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_stock_master(
    c: &mut PgConnection,
    intent: &ServiceStockMasterIntent,
    scope_branch: Option<&str>,
) -> Result<Option<CurrentServiceStockMaster>, Error> {
    if !intent.valid() {
        return Err(invalid());
    }
    let unit = unit(c).await?;
    let Some(image) = row(c, intent).await? else {
        return Ok(None);
    };
    let branch = if matches!(intent.master, ServiceStockMaster::Warehouse { .. }) {
        scope_branch
    } else {
        None
    };
    let admitted: bool = sqlx::query_scalar(
        "SELECT $1::jsonb->>'company_id'=$2 AND ($3::text IS NULL OR $1::jsonb->>'branch_id'=$3)",
    )
    .bind(&image)
    .bind(intent.company.as_str())
    .bind(branch)
    .fetch_one(&mut *c)
    .await?;
    if !admitted {
        return Ok(None);
    }
    Ok(Some(CurrentServiceStockMaster {
        intent: intent.clone(),
        unit,
        image,
    }))
}
/// Concrete composition must check genuine IAM/purchase/Workflow, mapping,
/// Finance Money and, for warehouses, Organization branch witnesses on this unit.
pub trait PgServiceStockMasterSource: Sync {
    fn intent(&self) -> &ServiceStockMasterIntent;
    fn require_before(
        &self,
        c: &mut PgConnection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
    fn require_after(
        &self,
        c: &mut PgConnection,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}
pub struct PreparedServiceStockMaster {
    intent: ServiceStockMasterIntent,
    unit: Unit,
    expected: String,
}
impl PreparedServiceStockMaster {
    pub async fn prepare<S: PgServiceStockMasterSource>(
        c: &mut PgConnection,
        source: &S,
    ) -> Result<Self, Error> {
        let intent = source.intent().clone();
        if !intent.valid() {
            return Err(invalid());
        }
        let unit = unit(c).await?;
        source.require_before(c).await?;
        if row(c, &intent).await?.is_some() {
            return Err(changed());
        }
        let expected:String=match &intent.master {
   ServiceStockMaster::Warehouse{branch_id,code,name,status}=>sqlx::query_scalar("SELECT jsonb_build_object('id',$1::text,'company_id',$2::text,'branch_id',$3::text,'code',$4::text,'name',$5::text,'status',$6::text,'version',1)::text").bind(intent.id()).bind(intent.company.as_str()).bind(branch_id).bind(code).bind(name).bind(status).fetch_one(&mut *c).await?,
   ServiceStockMaster::Product{sku,name,unit,tracking_mode,price_minor,tax_bps,status}=>sqlx::query_scalar("SELECT jsonb_build_object('id',$1::text,'company_id',$2::text,'sku',$3::text,'name',$4::text,'unit',$5::text,'tracking_mode',$6::text,'item_type','stock','price_minor',$7::bigint,'tax_bps',$8::bigint,'status',$9::text,'version',1)::text").bind(intent.id()).bind(intent.company.as_str()).bind(sku).bind(name).bind(unit).bind(tracking_mode).bind(price_minor).bind(tax_bps).bind(status).fetch_one(&mut *c).await?,
  };
        Ok(Self {
            intent,
            unit,
            expected,
        })
    }
    pub async fn apply<S: PgServiceStockMasterSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
    ) -> Result<(), Error> {
        if source.intent() != &self.intent
            || unit(c).await? != self.unit
            || row(c, &self.intent).await?.is_some()
        {
            return Err(changed());
        }
        source.require_before(c).await?;
        match &self.intent.master {
            ServiceStockMaster::Warehouse {
                branch_id,
                code,
                name,
                status,
            } => {
                sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES($1,$2,$3,$4,$5,$6)").bind(self.intent.id()).bind(self.intent.company.as_str()).bind(branch_id).bind(code).bind(name).bind(status).execute(&mut *c).await?;
            }
            ServiceStockMaster::Product {
                sku,
                name,
                unit,
                tracking_mode,
                price_minor,
                tax_bps,
                status,
            } => {
                sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,unit,tracking_mode,item_type,price_minor,tax_bps,status) VALUES($1,$2,$3,$4,$5,$6,'stock',$7,$8,$9)").bind(self.intent.id()).bind(self.intent.company.as_str()).bind(sku).bind(name).bind(unit).bind(tracking_mode).bind(price_minor).bind(tax_bps).bind(status).execute(&mut *c).await?;
            }
        }
        if row(c, &self.intent).await?.as_deref() != Some(&self.expected) {
            return Err(changed());
        }
        Ok(())
    }
    pub async fn require_current<S: PgServiceStockMasterSource>(
        &self,
        c: &mut PgConnection,
        source: &S,
    ) -> Result<(), Error> {
        if source.intent() != &self.intent
            || unit(c).await? != self.unit
            || row(c, &self.intent).await?.as_deref() != Some(&self.expected)
        {
            return Err(changed());
        }
        source.require_after(c).await
    }
}

/// Only the retained mapping and source-relation schema are needed by the
/// Stock workspace; unrelated Commerce order/history rows are not read.
pub struct CurrentServiceStockMapping {
    company: erpx_server_shared::public::CompanyId,
    unit: Unit,
    mapping: Option<String>,
    schema: bool,
}
impl CurrentServiceStockMapping {
    pub fn mapping_json(&self) -> Option<&str> {
        self.mapping.as_deref()
    }
    pub fn schema_available(&self) -> bool {
        self.schema
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let (mapping, schema) = stock_mapping(c, &self.company).await?;
        if unit(c).await? != self.unit || mapping != self.mapping || schema != self.schema {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_stock_mapping(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE commerce_settings IN SHARE MODE")
        .execute(&mut *c)
        .await?;
    sqlx::query("SAVEPOINT service_stock_mapping_schema_lock")
        .execute(&mut *c)
        .await?;
    let locked = sqlx::query("LOCK TABLE commerce_service_parts_posting_sources IN SHARE MODE")
        .execute(&mut *c)
        .await;
    if let Err(error) = locked {
        sqlx::query("ROLLBACK TO SAVEPOINT service_stock_mapping_schema_lock")
            .execute(&mut *c)
            .await?;
        sqlx::query("RELEASE SAVEPOINT service_stock_mapping_schema_lock")
            .execute(&mut *c)
            .await?;
        if error.as_database_error().and_then(|e| e.code()).as_deref() != Some("42P01") {
            return Err(error.into());
        }
        return Ok(());
    }
    sqlx::query("RELEASE SAVEPOINT service_stock_mapping_schema_lock")
        .execute(c)
        .await?;
    Ok(())
}
async fn stock_mapping(
    c: &mut PgConnection,
    company: &erpx_server_shared::public::CompanyId,
) -> Result<(Option<String>, bool), Error> {
    let schema: bool = sqlx::query_scalar(
        "SELECT to_regclass('commerce_service_parts_posting_sources') IS NOT NULL",
    )
    .fetch_one(&mut *c)
    .await?;
    let found:Option<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(to_jsonb(x)::text)<=2097152 THEN to_jsonb(x)::text END FROM commerce_settings x WHERE company_id=$1").bind(company.as_str()).fetch_optional(c).await?;
    let mapping = match found {
        None => None,
        Some(Some(value)) => Some(value),
        Some(None) => return Err(invalid()),
    };
    Ok((mapping, schema))
}
pub async fn read_service_stock_mapping(
    c: &mut PgConnection,
    company: &erpx_server_shared::public::CompanyId,
) -> Result<CurrentServiceStockMapping, Error> {
    let unit = unit(c).await?;
    let (mapping, schema) = stock_mapping(c, company).await?;
    Ok(CurrentServiceStockMapping {
        company: company.clone(),
        unit,
        mapping,
        schema,
    })
}
