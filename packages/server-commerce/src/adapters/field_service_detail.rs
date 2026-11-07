//! Service detail display and selectable Commerce references; no grant.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDetailWarehouse {
    pub id: String,
    pub name: String,
    pub name_rank: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDetailProduct {
    pub id: String,
    pub sku: String,
    pub name: String,
    pub unit: String,
    pub tracking_mode: String,
    pub active: bool,
    pub sku_rank: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDetailCommerceStock {
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDetailCommerce {
    pub warehouses: Vec<ServiceDetailWarehouse>,
    pub products: Vec<ServiceDetailProduct>,
    pub stock: Vec<ServiceDetailCommerceStock>,
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    data: ServiceDetailCommerce,
    digests: Vec<String>,
}
pub struct CurrentServiceDetailCommerce {
    company: CompanyId,
    branch: String,
    include_spares: bool,
    ids: Vec<String>,
    unit: Unit,
    image: Image,
}
impl CurrentServiceDetailCommerce {
    pub fn data(&self) -> &ServiceDetailCommerce {
        &self.image.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(
                c,
                &self.company,
                &self.branch,
                self.include_spares,
                &self.ids,
            )
            .await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_detail_commerce(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("COMMERCE_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE commerce_warehouses,commerce_products,commerce_stock IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_detail_commerce(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &str,
    include_spares: bool,
    product_ids: &[String],
) -> Result<CurrentServiceDetailCommerce, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company, branch, include_spares, product_ids).await?;
    Ok(CurrentServiceDetailCommerce {
        company: company.clone(),
        branch: branch.into(),
        include_spares,
        ids: product_ids.to_vec(),
        unit,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: &str,
    include_spares: bool,
    ids: &[String],
) -> Result<Image, Error> {
    if ids.len() > 10000
        || ids
            .iter()
            .try_fold(branch.len(), |n, id| n.checked_add(id.len()))
            .is_none_or(|n| n > 2097152)
    {
        return Err(limited());
    }
    let warehouses=sqlx::query("WITH source AS MATERIALIZED (SELECT w.*,dense_rank() OVER(ORDER BY w.name) name_rank FROM commerce_warehouses w WHERE $3 AND w.company_id=$1 AND w.branch_id=$2 AND w.status='active'), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER() bytes,count(*) OVER() count,row_number() OVER() ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN id END id,CASE WHEN bytes<=2097152 AND count<=10000 THEN name END name,CASE WHEN bytes<=2097152 AND count<=10000 THEN name_rank END name_rank,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY name,id").bind(company.as_str()).bind(branch).bind(include_spares).fetch_all(&mut *c).await?;
    let products=sqlx::query("WITH source AS MATERIALIZED (SELECT p.*,dense_rank() OVER(ORDER BY p.sku) sku_rank FROM commerce_products p WHERE p.company_id=$1 AND (p.id=ANY($2::text[]) OR ($3 AND p.status='active'))), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER() bytes,count(*) OVER() count,row_number() OVER() ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN id END id,CASE WHEN bytes<=2097152 AND count<=10000 THEN sku END sku,CASE WHEN bytes<=2097152 AND count<=10000 THEN name END name,CASE WHEN bytes<=2097152 AND count<=10000 THEN unit END unit,CASE WHEN bytes<=2097152 AND count<=10000 THEN tracking_mode END tracking_mode,CASE WHEN bytes<=2097152 AND count<=10000 THEN sku_rank END sku_rank,(status='active') active,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY sku,id").bind(company.as_str()).bind(ids).bind(include_spares).fetch_all(&mut *c).await?;
    let stocks=sqlx::query("WITH source AS MATERIALIZED (SELECT s.* FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id JOIN commerce_products p ON p.id=s.product_id WHERE $3 AND w.company_id=$1 AND w.branch_id=$2 AND w.status='active' AND p.company_id=w.company_id AND p.status='active' AND s.quantity_milli>0), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER() bytes,count(*) OVER() count,row_number() OVER() ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN warehouse_id END warehouse_id,CASE WHEN bytes<=2097152 AND count<=10000 THEN product_id END product_id,CASE WHEN bytes<=2097152 AND count<=10000 AND quantity_milli BETWEEN 0 AND 9007199254740991 THEN quantity_milli END quantity_milli,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY warehouse_id,product_id").bind(company.as_str()).bind(branch).bind(include_spares).fetch_all(c).await?;
    let mut stock = Vec::new();
    let mut ws = Vec::new();
    let mut ps = Vec::new();
    let mut digests = Vec::new();
    for r in warehouses {
        ws.push(ServiceDetailWarehouse {
            id: r.try_get::<Option<String>, _>("id")?.ok_or_else(limited)?,
            name: r
                .try_get::<Option<String>, _>("name")?
                .ok_or_else(limited)?,
            name_rank: r
                .try_get::<Option<i64>, _>("name_rank")?
                .ok_or_else(limited)?,
        });
        digests.push(r.try_get("digest")?)
    }
    for r in products {
        ps.push(ServiceDetailProduct {
            id: r.try_get::<Option<String>, _>("id")?.ok_or_else(limited)?,
            sku: r.try_get::<Option<String>, _>("sku")?.ok_or_else(limited)?,
            name: r
                .try_get::<Option<String>, _>("name")?
                .ok_or_else(limited)?,
            unit: r
                .try_get::<Option<String>, _>("unit")?
                .ok_or_else(limited)?,
            tracking_mode: r
                .try_get::<Option<String>, _>("tracking_mode")?
                .ok_or_else(limited)?,
            active: r.try_get("active")?,
            sku_rank: r
                .try_get::<Option<i64>, _>("sku_rank")?
                .ok_or_else(limited)?,
        });
        digests.push(r.try_get("digest")?)
    }
    for r in stocks {
        stock.push(ServiceDetailCommerceStock {
            warehouse_id: r
                .try_get::<Option<String>, _>("warehouse_id")?
                .ok_or_else(limited)?,
            product_id: r
                .try_get::<Option<String>, _>("product_id")?
                .ok_or_else(limited)?,
            quantity_milli: r
                .try_get::<Option<i64>, _>("quantity_milli")?
                .ok_or_else(limited)?,
        });
        digests.push(r.try_get("digest")?);
    }
    Ok(Image {
        data: ServiceDetailCommerce {
            warehouses: ws,
            products: ps,
            stock,
        },
        digests,
    })
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("COMMERCE_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_COMMERCE_CHANGED",
        "The Service Commerce source changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_COMMERCE_LIMIT",
        "The Service Commerce detail exceeds its bounded read.",
    )
    .into()
}
