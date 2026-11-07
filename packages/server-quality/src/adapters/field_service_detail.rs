//! Quality-owned active hold facts for scoped Service stock/lot presentation.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDetailHold {
    pub warehouse_id: String,
    pub product_id: String,
    pub lot_id: Option<String>,
    pub quantity_scope: String,
    pub held_quantity_milli: Option<i64>,
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    data: Vec<ServiceDetailHold>,
    digests: Vec<String>,
}
pub struct CurrentServiceDetailQuality {
    company: CompanyId,
    warehouse_ids: Vec<String>,
    include_spares: bool,
    unit: Unit,
    image: Image,
}
impl CurrentServiceDetailQuality {
    pub fn data(&self) -> &Vec<ServiceDetailHold> {
        &self.image.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(c, &self.company, &self.warehouse_ids, self.include_spares).await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_detail_quality(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("QUALITY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE inventory_quality_holds IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_service_detail_quality(
    c: &mut PgConnection,
    company: &CompanyId,
    warehouse_ids: &[String],
    include_spares: bool,
) -> Result<CurrentServiceDetailQuality, Error> {
    let unit = unit(c).await?;
    let image = capture(c, company, warehouse_ids, include_spares).await?;
    Ok(CurrentServiceDetailQuality {
        company: company.clone(),
        warehouse_ids: warehouse_ids.to_vec(),
        include_spares,
        unit,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    ids: &[String],
    include_spares: bool,
) -> Result<Image, Error> {
    if ids.len() > 10000
        || ids
            .iter()
            .try_fold(0usize, |n, id| n.checked_add(id.len()))
            .is_none_or(|n| n > 2097152)
    {
        return Err(limited());
    }
    let rows=sqlx::query("WITH source AS MATERIALIZED (SELECT h.* FROM inventory_quality_holds h WHERE $2 AND h.company_id=$3 AND h.warehouse_id=ANY($1::text[]) AND h.status NOT IN ('released','cancelled','disposed')), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER() bytes,count(*) OVER() count,row_number() OVER() ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN warehouse_id END warehouse_id,CASE WHEN bytes<=2097152 AND count<=10000 THEN product_id END product_id,CASE WHEN bytes<=2097152 AND count<=10000 THEN lot_id END lot_id,CASE WHEN bytes<=2097152 AND count<=10000 THEN quantity_scope END quantity_scope,CASE WHEN bytes<=2097152 AND count<=10000 THEN held_quantity_milli END held_quantity_milli,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY warehouse_id,product_id,id").bind(ids).bind(include_spares).bind(company.as_str()).fetch_all(c).await?;
    let mut data = Vec::new();
    let mut digests = Vec::new();
    for r in rows {
        data.push(ServiceDetailHold {
            warehouse_id: r
                .try_get::<Option<String>, _>("warehouse_id")?
                .ok_or_else(limited)?,
            product_id: r
                .try_get::<Option<String>, _>("product_id")?
                .ok_or_else(limited)?,
            lot_id: r.try_get("lot_id")?,
            quantity_scope: r
                .try_get::<Option<String>, _>("quantity_scope")?
                .ok_or_else(limited)?,
            held_quantity_milli: r.try_get("held_quantity_milli")?,
        });
        digests.push(r.try_get("digest")?)
    }
    Ok(Image { data, digests })
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("QUALITY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_QUALITY_CHANGED",
        "The Service Quality source changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_QUALITY_LIMIT",
        "The Service Quality detail exceeds its bounded read.",
    )
    .into()
}
