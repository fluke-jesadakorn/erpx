//! Quality-owned decision facts only. No reads of Inventory stock/lot balances
//! or other owners, and no permission, release or mutation capability.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection, Row};
use std::collections::BTreeSet;
type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumptionHoldRequest {
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub warehouse_id: RecordId,
    pub product_id: RecordId,
    pub lot_ids: Vec<RecordId>,
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
    whole: bool,
    unscoped_whole: bool,
    partial: i64,
    blocked_lots: Vec<RecordId>,
}
/// Opaque same-transaction semantic facts. Private construction, no Clone or
/// Deserialize. An unchanged effective decision does not require fetching or
/// retaining irrelevant Quality reason/inspection bodies or every hold ID.
pub struct CurrentConsumptionHolds {
    request: ConsumptionHoldRequest,
    image: Image,
    transaction: Transaction,
}
impl CurrentConsumptionHolds {
    pub fn request(&self) -> &ConsumptionHoldRequest {
        &self.request
    }
    /// 0252: any active whole-quantity hold quarantines all aggregate stock.
    pub fn whole_quantity_hold(&self) -> bool {
        self.image.whole
    }
    /// 0283: the dedicated QUALITY_HOLD error applies only to whole + no lot.
    pub fn unscoped_whole_quantity_hold(&self) -> bool {
        self.image.unscoped_whole
    }
    /// Exact while representable, otherwise capped at i64::MAX. Every admitted
    /// Inventory stock quantity is <=JS-safe i64, so min(stock,sum) is unchanged.
    pub fn partial_held_milli(&self) -> i64 {
        self.image.partial
    }
    pub fn blocked_lots(&self) -> &[RecordId] {
        &self.image.blocked_lots
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction {
            return Err(changed());
        }
        let current = read_consumption_holds(c, &self.request).await?;
        if current.image != self.image {
            return Err(changed());
        }
        Ok(())
    }
}
/// Utility-only absence fence, before the first SERIALIZABLE snapshot and all
/// owner policy reads. Protects newly inserted denials as well as current rows.
pub async fn lock_consumption_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("QUALITY_CONSUMPTION_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE inventory_quality_holds IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn read_consumption_holds(
    c: &mut PgConnection,
    request: &ConsumptionHoldRequest,
) -> Result<CurrentConsumptionHolds, Error> {
    require_unit(c).await?;
    let lots: Vec<_> = request.lot_ids.iter().map(RecordId::as_str).collect();
    if lots.len() > 500 || lots.iter().copied().collect::<BTreeSet<_>>().len() != lots.len() {
        return Err(BusinessError::invalid(
            "QUALITY_CONSUMPTION_LOTS_INVALID",
            "Provide at most 500 distinct current lot identifiers.",
        )
        .into());
    }
    // One aggregate row and at most the supplied 500 bounded lot IDs. No
    // unbounded client collection or historical/free-form hold bodies. SUM is
    // NUMERIC in PostgreSQL and capped before conversion, never bigint overflow.
    let row=sqlx::query("SELECT COALESCE(bool_or(h.company_id<>$1 OR h.branch_id<>$2 OR h.product_id<>$4 OR h.quantity_scope NOT IN ('whole','partial')),false) invalid_scope,COALESCE(bool_or(h.product_id=$4 AND h.quantity_scope='whole'),false) whole,COALESCE(bool_or(h.product_id=$4 AND h.quantity_scope='whole' AND h.lot_id IS NULL),false) unscoped_whole,LEAST(COALESCE(SUM(h.held_quantity_milli)FILTER(WHERE h.product_id=$4 AND h.quantity_scope='partial'),0),9223372036854775807::numeric)::bigint partial,ARRAY(SELECT lot FROM unnest($5::text[]) lot WHERE EXISTS(SELECT 1 FROM inventory_quality_holds selected WHERE selected.warehouse_id=$3 AND selected.lot_id=lot AND selected.status NOT IN ('released','cancelled','disposed'))) blocked FROM inventory_quality_holds h WHERE h.warehouse_id=$3 AND h.status NOT IN ('released','cancelled','disposed') AND (h.product_id=$4 OR h.lot_id=ANY($5::text[]))")
        .bind(request.company_id.as_str()).bind(request.branch_id.as_str()).bind(request.warehouse_id.as_str()).bind(request.product_id.as_str()).bind(&lots).fetch_one(&mut *c).await?;
    if row.try_get::<bool, _>("invalid_scope")? {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "QUALITY_CONSUMPTION_SCOPE_INVALID",
            "The relevant Quality hold source is outside this Company or branch.",
        )
        .into());
    }
    let blocked: Vec<String> = row.try_get("blocked")?;
    let blocked_lots = blocked
        .into_iter()
        .map(|id| RecordId::new(id).map_err(|_| changed()))
        .collect::<Result<_, _>>()?;
    Ok(CurrentConsumptionHolds {
        request: request.clone(),
        image: Image {
            whole: row.try_get("whole")?,
            unscoped_whole: row.try_get("unscoped_whole")?,
            partial: row.try_get("partial")?,
            blocked_lots,
        },
        transaction: transaction(c).await?,
    })
}
async fn require_unit(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("QUALITY_CONSUMPTION_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("QUALITY_CONSUMPTION_SERIALIZABLE_REQUIRED").into());
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
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_CONSUMPTION_SOURCE_CHANGED",
        "The retained Quality consumption decision changed.",
    )
    .into()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawConsumptionHoldRequest {
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub warehouse_id: String,
    pub product_id: String,
    pub lot_ids: Vec<String>,
}
#[derive(Eq, PartialEq)]
struct RawImage {
    whole: bool,
    unscoped_whole: bool,
    partial: i64,
    blocked_lots: Vec<String>,
}
pub struct CurrentRawConsumptionHolds {
    request: RawConsumptionHoldRequest,
    image: RawImage,
    transaction: Transaction,
}
impl CurrentRawConsumptionHolds {
    pub fn whole_quantity_hold(&self) -> bool {
        self.image.whole
    }
    pub fn unscoped_whole_quantity_hold(&self) -> bool {
        self.image.unscoped_whole
    }
    pub fn partial_held_milli(&self) -> i64 {
        self.image.partial
    }
    pub fn blocked_lots(&self) -> &[String] {
        &self.image.blocked_lots
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_unit(c).await?;
        if transaction(c).await? != self.transaction {
            return Err(changed());
        }
        let current = read_consumption_holds_raw(c, &self.request).await?;
        if current.image != self.image {
            return Err(changed());
        }
        Ok(())
    }
}
pub async fn read_consumption_holds_raw(
    c: &mut PgConnection,
    request: &RawConsumptionHoldRequest,
) -> Result<CurrentRawConsumptionHolds, Error> {
    require_unit(c).await?;
    let lots: Vec<_> = request.lot_ids.iter().map(String::as_str).collect();
    if lots.len() > 500 || lots.iter().copied().collect::<BTreeSet<_>>().len() != lots.len() {
        return Err(BusinessError::invalid(
            "QUALITY_CONSUMPTION_LOTS_INVALID",
            "Provide at most 500 distinct current lot identifiers.",
        )
        .into());
    }
    // One aggregate row and at most the supplied 500 bounded lot IDs. No
    // unbounded client collection or historical/free-form hold bodies. SUM is
    // NUMERIC in PostgreSQL and capped before conversion, never bigint overflow.
    let row=sqlx::query("SELECT COALESCE(bool_or(h.company_id<>$1 OR h.branch_id<>$2 OR h.product_id<>$4 OR h.quantity_scope NOT IN ('whole','partial')),false) invalid_scope,COALESCE(bool_or(h.product_id=$4 AND h.quantity_scope='whole'),false) whole,COALESCE(bool_or(h.product_id=$4 AND h.quantity_scope='whole' AND h.lot_id IS NULL),false) unscoped_whole,LEAST(COALESCE(SUM(h.held_quantity_milli)FILTER(WHERE h.product_id=$4 AND h.quantity_scope='partial'),0),9223372036854775807::numeric)::bigint partial,ARRAY(SELECT lot FROM unnest($5::text[]) lot WHERE EXISTS(SELECT 1 FROM inventory_quality_holds selected WHERE selected.warehouse_id=$3 AND selected.lot_id=lot AND selected.status NOT IN ('released','cancelled','disposed'))) blocked FROM inventory_quality_holds h WHERE h.warehouse_id=$3 AND h.status NOT IN ('released','cancelled','disposed') AND (h.product_id=$4 OR h.lot_id=ANY($5::text[]))")
        .bind(request.company_id.as_str()).bind(request.branch_id.as_str()).bind(request.warehouse_id.as_str()).bind(request.product_id.as_str()).bind(&lots).fetch_one(&mut *c).await?;
    if row.try_get::<bool, _>("invalid_scope")? {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "QUALITY_CONSUMPTION_SCOPE_INVALID",
            "The relevant Quality hold source is outside this Company or branch.",
        )
        .into());
    }
    let blocked: Vec<String> = row.try_get("blocked")?;
    let blocked_lots = blocked;
    Ok(CurrentRawConsumptionHolds {
        request: request.clone(),
        image: RawImage {
            whole: row.try_get("whole")?,
            unscoped_whole: row.try_get("unscoped_whole")?,
            partial: row.try_get("partial")?,
            blocked_lots,
        },
        transaction: transaction(c).await?,
    })
}
