//! Inventory's historical lot allocation partition. These rows contain no
//! Company or branch; Commerce must retain the scoped actual fulfillment,
//! order and return sources before any caller uses the resulting facts.
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{Connection, PgConnection, Row};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone)]
pub enum QualityFulfillmentAllocationRequest {
    QuarantineHold {
        hold_id: String,
    },
    OriginalAllocation {
        allocation_id: String,
        fulfillment_line_id: String,
        lot_id: String,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityFulfillmentAllocation {
    pub id: String,
    pub fulfillment_line_id: String,
    pub lot_id: String,
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub disposition: String,
    pub quality_hold_id: Option<String>,
}
pub struct CurrentQualityFulfillmentAllocations {
    request: QualityFulfillmentAllocationRequest,
    rows: Vec<QualityFulfillmentAllocation>,
    unit: Unit,
}
impl CurrentQualityFulfillmentAllocations {
    pub fn rows(&self) -> &[QualityFulfillmentAllocation] {
        &self.rows
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || read(c, &self.request).await? != self.rows {
            return Err(changed());
        }
        Ok(())
    }
}
/// Acquire before any source snapshot, with all other owner locks.
pub async fn lock_quality_fulfillment_allocations(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(invalid());
    }
    sqlx::query("LOCK TABLE inventory_fulfillment_lot_allocations IN SHARE MODE")
        .execute(c)
        .await?;
    Ok(())
}
pub async fn inspect_quality_fulfillment_allocations(
    c: &mut PgConnection,
    request: QualityFulfillmentAllocationRequest,
) -> Result<CurrentQualityFulfillmentAllocations, Error> {
    let unit = unit(c).await?;
    let rows = read(c, &request).await?;
    Ok(CurrentQualityFulfillmentAllocations {
        request,
        rows,
        unit,
    })
}
async fn read(
    c: &mut PgConnection,
    request: &QualityFulfillmentAllocationRequest,
) -> Result<Vec<QualityFulfillmentAllocation>, Error> {
    let (hold, id, line, lot) = match request {
        QualityFulfillmentAllocationRequest::QuarantineHold { hold_id } => {
            (Some(hold_id.as_str()), None, None, None)
        }
        QualityFulfillmentAllocationRequest::OriginalAllocation {
            allocation_id,
            fulfillment_line_id,
            lot_id,
        } => (
            None,
            Some(allocation_id.as_str()),
            Some(fulfillment_line_id.as_str()),
            Some(lot_id.as_str()),
        ),
    };
    let rows = sqlx::query("SELECT id,fulfillment_line_id,lot_id,quantity_milli,value_minor,disposition,quality_hold_id FROM inventory_fulfillment_lot_allocations WHERE ($1::text IS NOT NULL AND quality_hold_id=$1 AND disposition='quarantine') OR ($2::text IS NOT NULL AND id=$2 AND fulfillment_line_id=$3 AND lot_id=$4) ORDER BY id FOR SHARE")
        .bind(hold).bind(id).bind(line).bind(lot).fetch_all(c).await?;
    rows.into_iter()
        .map(|row| {
            Ok(QualityFulfillmentAllocation {
                id: row.try_get("id")?,
                fulfillment_line_id: row.try_get("fulfillment_line_id")?,
                lot_id: row.try_get("lot_id")?,
                quantity_milli: row.try_get("quantity_milli")?,
                value_minor: row.try_get("value_minor")?,
                disposition: row.try_get("disposition")?,
                quality_hold_id: row.try_get("quality_hold_id")?,
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
        return Err(
            PersistenceError::new("INVENTORY_QUALITY_ALLOCATION_TRANSACTION_REQUIRED").into(),
        );
    }
    let held:bool=sqlx::query_scalar("SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') AND c.relpersistence='p' AND to_regclass(c.relname::text)=c.oid AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l WHERE l.pid=pg_backend_pid() AND l.granted AND l.relation=c.oid AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock')) FROM pg_class c WHERE c.oid='public.inventory_fulfillment_lot_allocations'::regclass").fetch_optional(&mut *c).await?.unwrap_or(false);
    if !held {
        return Err(PersistenceError::new("INVENTORY_QUALITY_ALLOCATION_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("INVENTORY_QUALITY_ALLOCATION_TRANSACTION_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INVENTORY_QUALITY_ALLOCATION_CHANGED",
        "The original fulfillment allocation changed.",
    )
    .into()
}
