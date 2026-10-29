//! Current, scoped Commerce master facts for a Service first physical Count.
//! This is a factual same-unit image, never an actor or purchase grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
fn invalid() -> Error {
    PersistenceError::new("SERVICE_COUNT_MASTER_SOURCE_INVALID").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED",
        "The selected Stock master source changed.",
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCountMasters {
    pub warehouse: Option<String>,
    pub product: Option<String>,
    pub warehouses: String,
    pub products: String,
}
pub struct CurrentServiceCountMasters {
    company: String,
    branch: Option<String>,
    warehouse: Option<String>,
    product: Option<String>,
    unit: Unit,
    data: ServiceCountMasters,
}
impl CurrentServiceCountMasters {
    pub fn data(&self) -> &ServiceCountMasters {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(
                c,
                &self.company,
                self.branch.as_deref(),
                self.warehouse.as_deref(),
                self.product.as_deref(),
            )
            .await?
                != self.data
        {
            return Err(changed());
        }
        Ok(())
    }
}
/// Call after `lock_service_stock_masters` and all other owner locks.
pub async fn read_service_count_masters(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    warehouse: Option<&str>,
    product: Option<&str>,
) -> Result<CurrentServiceCountMasters, Error> {
    if company.is_empty()
        || warehouse.is_some_and(str::is_empty)
        || product.is_some_and(str::is_empty)
    {
        return Err(invalid());
    }
    let transaction = unit(c).await?;
    let data = capture(c, company, branch, warehouse, product).await?;
    Ok(CurrentServiceCountMasters {
        company: company.into(),
        branch: branch.map(str::to_owned),
        warehouse: warehouse.map(str::to_owned),
        product: product.map(str::to_owned),
        unit: transaction,
        data,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    warehouse: Option<&str>,
    product: Option<&str>,
) -> Result<ServiceCountMasters, Error> {
    let row=sqlx::query(r#"WITH w AS MATERIALIZED (SELECT x FROM commerce_warehouses x WHERE x.company_id=$1 AND x.status='active' AND ($2::text IS NULL OR x.branch_id=$2) ORDER BY x.name,x.id LIMIT 501), p AS MATERIALIZED (SELECT x FROM commerce_products x WHERE x.company_id=$1 AND x.status='active' AND x.item_type='stock' ORDER BY x.sku,x.id LIMIT 501), stats AS (SELECT (SELECT count(*) FROM w) wn,(SELECT count(*) FROM p) pn,(SELECT COALESCE(sum(octet_length(to_jsonb(w.x)::text)),0) FROM w)+(SELECT COALESCE(sum(octet_length(to_jsonb(p.x)::text)),0) FROM p) bytes), image AS (SELECT CASE WHEN wn<=500 AND pn<=500 AND bytes<=2097152 THEN jsonb_build_object('warehouses',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',(w.x).id,'name',(w.x).name,'branchId',(w.x).branch_id) ORDER BY (w.x).name,(w.x).id) FROM w),'[]'::jsonb),'products',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',(p.x).id,'sku',(p.x).sku,'name',(p.x).name,'unit',(p.x).unit,'trackingMode',(p.x).tracking_mode) ORDER BY (p.x).sku,(p.x).id) FROM p),'[]'::jsonb),'warehouse',(SELECT row_to_json(w.x)::text FROM w WHERE (w.x).id=$3),'product',(SELECT row_to_json(p.x)::text FROM p WHERE (p.x).id=$4))::text END payload FROM stats) SELECT CASE WHEN octet_length(payload)<=2097152 THEN payload END payload FROM image"#).bind(company).bind(branch).bind(warehouse).bind(product).fetch_one(c).await?;
    let payload: String = row
        .try_get::<Option<String>, _>("payload")?
        .ok_or_else(invalid)?;
    // Retain JSON in PostgreSQL's rendering; extraction does not alter the image.
    let v: serde_json::Value = serde_json::from_str(&payload).map_err(|_| invalid())?;
    Ok(ServiceCountMasters {
        warehouse: v["warehouse"].as_str().map(str::to_owned),
        product: v["product"].as_str().map(str::to_owned),
        warehouses: v["warehouses"].to_string(),
        products: v["products"].to_string(),
    })
}

/// Original `SELECT s.*` column order for the retained ECMAScript context key.
/// Workflow separately validates the setup receipt and Finance the chart.
pub struct CurrentServiceCountMapping {
    company: String,
    unit: Unit,
    image: Option<String>,
}
impl CurrentServiceCountMapping {
    pub fn image(&self) -> Option<&str> {
        self.image.as_deref()
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || mapping(c, &self.company).await? != self.image {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_count_mapping(
    c: &mut PgConnection,
    company: &str,
) -> Result<CurrentServiceCountMapping, Error> {
    if company.is_empty() {
        return Err(invalid());
    }
    let transaction = unit(c).await?;
    let image = mapping(c, company).await?;
    Ok(CurrentServiceCountMapping {
        company: company.into(),
        unit: transaction,
        image,
    })
}
async fn mapping(c: &mut PgConnection, company: &str) -> Result<Option<String>, Error> {
    let v:Option<Option<String>>=sqlx::query_scalar("SELECT CASE WHEN octet_length(row_to_json(s)::text)<=2097152 THEN row_to_json(s)::text END FROM commerce_settings s WHERE company_id=$1").bind(company).fetch_optional(c).await?;
    match v {
        None => Ok(None),
        Some(Some(v)) => Ok(Some(v)),
        Some(None) => Err(invalid()),
    }
}

/// Historical Count presentation includes archived masters. This factual read
/// does not authorize Count access and is distinct from active preparation.
pub struct CurrentServiceCountRecordMasters {
    company: String,
    branch: Option<String>,
    warehouse: String,
    product: String,
    unit: Unit,
    data: Option<(String, String)>,
}
impl CurrentServiceCountRecordMasters {
    pub fn data(&self) -> Option<(&str, &str)> {
        self.data.as_ref().map(|(w, p)| (w.as_str(), p.as_str()))
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || record_masters(
                c,
                &self.company,
                self.branch.as_deref(),
                &self.warehouse,
                &self.product,
            )
            .await?
                != self.data
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_service_count_record_masters(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    warehouse: &str,
    product: &str,
) -> Result<CurrentServiceCountRecordMasters, Error> {
    if company.is_empty() || warehouse.is_empty() || product.is_empty() {
        return Err(invalid());
    }
    let transaction = unit(c).await?;
    let data = record_masters(c, company, branch, warehouse, product).await?;
    Ok(CurrentServiceCountRecordMasters {
        company: company.into(),
        branch: branch.map(str::to_owned),
        warehouse: warehouse.into(),
        product: product.into(),
        unit: transaction,
        data,
    })
}
async fn record_masters(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    warehouse: &str,
    product: &str,
) -> Result<Option<(String, String)>, Error> {
    let row:Option<(Option<String>,Option<String>)>=sqlx::query_as("SELECT CASE WHEN octet_length(row_to_json(w)::text)<=1048576 THEN row_to_json(w)::text END,CASE WHEN octet_length(row_to_json(p)::text)<=1048576 THEN row_to_json(p)::text END FROM commerce_warehouses w JOIN commerce_products p ON p.id=$4 AND p.company_id=$1 WHERE w.id=$3 AND w.company_id=$1 AND ($2::text IS NULL OR w.branch_id=$2)").bind(company).bind(branch).bind(warehouse).bind(product).fetch_optional(c).await?;
    match row {
        None => Ok(None),
        Some((Some(w), Some(p))) => Ok(Some((w, p))),
        Some(_) => Err(invalid()),
    }
}
