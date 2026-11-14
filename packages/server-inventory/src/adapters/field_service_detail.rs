//! Inventory-owned Service stock, lots and historical consumption facts.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDetailReservation {
    pub warehouse_id: String,
    pub product_id: String,
    pub reserved_milli: i64,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServiceDetailStockKey {
    pub warehouse_id: String,
    pub product_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDetailLot {
    pub lot_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub lot_code: Option<String>,
    pub serial_number: Option<String>,
    pub quantity_milli: i64,
    pub version: i64,
    pub expiry_date: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDetailInventory {
    pub reservations: Vec<ServiceDetailReservation>,
    pub lots: Vec<ServiceDetailLot>,
    pub consumptions: Vec<String>,
}
#[derive(Clone, PartialEq, Eq)]
struct Image {
    data: ServiceDetailInventory,
    digests: Vec<String>,
}
pub struct CurrentServiceDetailInventory {
    company: CompanyId,
    order: String,
    warehouse_ids: Vec<String>,
    stock_keys: Vec<ServiceDetailStockKey>,
    include_spares: bool,
    include_history: bool,
    unit: Unit,
    image: Image,
}
impl CurrentServiceDetailInventory {
    pub fn data(&self) -> &ServiceDetailInventory {
        &self.image.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || capture(
                c,
                &self.company,
                &self.order,
                &self.warehouse_ids,
                &self.stock_keys,
                self.include_spares,
                self.include_history,
            )
            .await?
                != self.image
        {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn lock_service_detail_inventory(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("INVENTORY_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE inventory_active_reservations,inventory_lots,inventory_lot_balances,operational_inventory_consumptions IN SHARE MODE").execute(c).await?;
    Ok(())
}
pub async fn read_service_detail_inventory(
    c: &mut PgConnection,
    company: &CompanyId,
    order_id: &str,
    warehouse_ids: &[String],
    stock_keys: &[ServiceDetailStockKey],
    include_spares: bool,
    include_history: bool,
) -> Result<CurrentServiceDetailInventory, Error> {
    let unit = unit(c).await?;
    let image = capture(
        c,
        company,
        order_id,
        warehouse_ids,
        stock_keys,
        include_spares,
        include_history,
    )
    .await?;
    Ok(CurrentServiceDetailInventory {
        company: company.clone(),
        order: order_id.into(),
        warehouse_ids: warehouse_ids.to_vec(),
        stock_keys: stock_keys.to_vec(),
        include_spares,
        include_history,
        unit,
        image,
    })
}
async fn capture(
    c: &mut PgConnection,
    company: &CompanyId,
    order: &str,
    ids: &[String],
    stock_keys: &[ServiceDetailStockKey],
    include_spares: bool,
    include_history: bool,
) -> Result<Image, Error> {
    if ids.len() > 10000
        || ids
            .iter()
            .try_fold(order.len(), |n, id| n.checked_add(id.len()))
            .is_none_or(|n| n > 2097152)
    {
        return Err(limited());
    }
    if stock_keys.len() > 10000
        || stock_keys
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != stock_keys.len()
        || stock_keys
            .iter()
            .try_fold(0usize, |n, k| {
                n.checked_add(k.warehouse_id.len())?
                    .checked_add(k.product_id.len())
            })
            .is_none_or(|n| n > 2097152)
    {
        return Err(limited());
    }
    let warehouses = stock_keys
        .iter()
        .map(|k| k.warehouse_id.clone())
        .collect::<Vec<_>>();
    let products = stock_keys
        .iter()
        .map(|k| k.product_id.clone())
        .collect::<Vec<_>>();
    let stock=sqlx::query("WITH requested AS MATERIALIZED (SELECT * FROM unnest($1::text[],$2::text[]) k(warehouse_id,product_id) WHERE $3), source AS MATERIALIZED (SELECT k.*,coalesce(a.total,0)::numeric reserved,coalesce(a.images,'[]'::jsonb) images FROM requested k LEFT JOIN LATERAL (SELECT sum(r.quantity_milli) total,jsonb_agg(to_jsonb(r) ORDER BY r.source_kind,r.source_id,r.order_line_id,to_jsonb(r)::text) images FROM inventory_active_reservations r WHERE r.warehouse_id=k.warehouse_id AND r.product_id=k.product_id) a ON true), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER() bytes,count(*) OVER() count,row_number() OVER() ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN warehouse_id END warehouse_id,CASE WHEN bytes<=2097152 AND count<=10000 THEN product_id END product_id,CASE WHEN bytes<=2097152 AND count<=10000 AND reserved BETWEEN 0 AND 9007199254740991 THEN reserved::bigint END reserved_milli,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY warehouse_id,product_id").bind(&warehouses).bind(&products).bind(include_spares).fetch_all(&mut *c).await?;
    let lots=sqlx::query("WITH source AS MATERIALIZED (SELECT b.lot_id,b.warehouse_id,l.product_id,l.lot_code,l.serial_number,b.quantity_milli,b.version,l.expiry_date,to_jsonb(b) balance_image,to_jsonb(l) lot_image FROM inventory_lot_balances b JOIN inventory_lots l ON l.id=b.lot_id WHERE $3 AND l.company_id=$1 AND b.warehouse_id=ANY($2::text[]) AND l.status='active' AND b.quantity_milli>0 AND (l.expiry_date IS NULL OR l.expiry_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER() bytes,count(*) OVER() count,row_number() OVER() ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN lot_id END lot_id,CASE WHEN bytes<=2097152 AND count<=10000 THEN warehouse_id END warehouse_id,CASE WHEN bytes<=2097152 AND count<=10000 THEN product_id END product_id,CASE WHEN bytes<=2097152 AND count<=10000 THEN lot_code END lot_code,CASE WHEN bytes<=2097152 AND count<=10000 THEN serial_number END serial_number,CASE WHEN bytes<=2097152 AND count<=10000 AND quantity_milli BETWEEN 0 AND 9007199254740991 THEN quantity_milli END quantity_milli,CASE WHEN bytes<=2097152 AND count<=10000 AND version BETWEEN 1 AND 9007199254740991 THEN version END version,CASE WHEN bytes<=2097152 AND count<=10000 THEN expiry_date END expiry_date,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY expiry_date NULLS LAST,lot_code,serial_number").bind(company.as_str()).bind(ids).bind(include_spares).fetch_all(&mut *c).await?;
    let consumptions=sqlx::query("WITH source AS MATERIALIZED (SELECT p.* FROM operational_inventory_consumptions p WHERE $3 AND p.company_id=$1 AND p.source_kind='field_service' AND p.source_id=$2), measured AS (SELECT *,sum(octet_length(to_jsonb(source)::text)) OVER() bytes,count(*) OVER() count,row_number() OVER() ordinal FROM source) SELECT CASE WHEN bytes<=2097152 AND count<=10000 THEN (to_jsonb(measured)-'bytes'-'count'-'ordinal')::text END image,encode(sha256(convert_to((to_jsonb(measured)-'bytes'-'count'-'ordinal')::text,'UTF8')),'hex') digest FROM measured WHERE (bytes<=2097152 AND count<=10000) OR ordinal=1 ORDER BY created_at,id").bind(company.as_str()).bind(order).bind(include_history).fetch_all(c).await?;
    let (mut s, mut l, mut v, mut digests) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for r in stock {
        s.push(ServiceDetailReservation {
            warehouse_id: r
                .try_get::<Option<String>, _>("warehouse_id")?
                .ok_or_else(limited)?,
            product_id: r
                .try_get::<Option<String>, _>("product_id")?
                .ok_or_else(limited)?,
            reserved_milli: r
                .try_get::<Option<i64>, _>("reserved_milli")?
                .ok_or_else(limited)?,
        });
        digests.push(r.try_get("digest")?)
    }
    for r in lots {
        l.push(ServiceDetailLot {
            lot_id: r
                .try_get::<Option<String>, _>("lot_id")?
                .ok_or_else(limited)?,
            warehouse_id: r
                .try_get::<Option<String>, _>("warehouse_id")?
                .ok_or_else(limited)?,
            product_id: r
                .try_get::<Option<String>, _>("product_id")?
                .ok_or_else(limited)?,
            lot_code: r.try_get("lot_code")?,
            serial_number: r.try_get("serial_number")?,
            quantity_milli: r
                .try_get::<Option<i64>, _>("quantity_milli")?
                .ok_or_else(limited)?,
            version: r
                .try_get::<Option<i64>, _>("version")?
                .ok_or_else(limited)?,
            expiry_date: r.try_get("expiry_date")?,
        });
        digests.push(r.try_get("digest")?)
    }
    for r in consumptions {
        v.push(
            r.try_get::<Option<String>, _>("image")?
                .ok_or_else(limited)?,
        );
        digests.push(r.try_get("digest")?)
    }
    Ok(Image {
        data: ServiceDetailInventory {
            reservations: s,
            lots: l,
            consumptions: v,
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
        return Err(PersistenceError::new("INVENTORY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "FIELD_SERVICE_INVENTORY_CHANGED",
        "The Service Inventory source changed.",
    )
    .into()
}
fn limited() -> Error {
    BusinessError::new(
        FailureKind::Unavailable,
        "FIELD_SERVICE_INVENTORY_LIMIT",
        "The Service Inventory detail exceeds its bounded read.",
    )
    .into()
}
