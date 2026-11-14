//! Native Quality PostgreSQL read and cancellation adapters.
//!
//! Only Quality-owned relations are read or written. Cross-owner labels,
//! purchase admission and Workflow receipt/outbox retention remain
//! composition/public-port concerns.
use crate::application::quality::{
    QualityAlert, QualityCancellation, QualityCancellationCommand, QualityCancellationReceipt,
    QualityCancellationRepository, QualityDetailCoreData, QualityDetailData, QualityDisposition,
    QualityFuture, QualityHold, QualityHoldProduct, QualityInspection, QualityLot,
    QualityMeasurement, QualityProduct, QualityReadRepository, QualitySpecification, QualityStock,
    QualityWorkspaceData,
};
use crate::domain::quality::{
    CancellationAction, HoldStatus, InspectionResult, QualityHoldFacts, QualityMetric, SpecStatus,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use futures_util::TryStreamExt;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use std::collections::BTreeMap;

type Error = PgTransactionError<BusinessError>;

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => BusinessError::new(
            FailureKind::Unavailable,
            "QUALITY_UNAVAILABLE",
            "The Quality source is unavailable.",
        ),
    }
}

const MAX_HOLDS: usize = 1000;
const MAX_INSPECTIONS: usize = 1000;
const MAX_MEASUREMENTS: usize = 5000;
const MAX_METRICS: usize = 1000;
const MAX_CANCELLATIONS: usize = 200;
const MAX_CANCELLATION_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_DISPOSITIONS: usize = 200;
const MAX_ALERTS: usize = 1000;

fn invalid(message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Unavailable, "QUALITY_READ_INVALID", message)
}

fn bounded(kind: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_READ_LIMIT",
        match kind {
            "holds" => "The Quality hold list exceeds its bounded read.",
            "inspections" => "The Quality inspection history exceeds its bounded read.",
            "measurements" => "The Quality measurement history exceeds its bounded read.",
            "metrics" => "The Quality specification history exceeds its bounded read.",
            "cancellations" => "The Quality cancellation history exceeds its bounded read.",
            "dispositions" => "The Quality disposition history exceeds its bounded read.",
            _ => "The Quality escalation history exceeds its bounded read.",
        },
    )
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_READ_CHANGED",
        "The Quality read source changed.",
    )
}

fn cancellation_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_CANCELLATION_CONFLICT",
        "The saved Quality cancellation differs from this request.",
    )
}

fn hold_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_HOLD_NOT_FOUND",
        "Quality hold not found.",
    )
}

fn independent_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INDEPENDENT_CANCELLATION_REQUIRED",
        "A different Quality manager must review cancellation.",
    )
}

/// Quality retains the IAM-owned merged-person fact at the write boundary.
/// The same stable function already guards the cancelled status in the
/// database; checking it before the update preserves the owner error code and
/// also covers the reject transition, which does not change to cancelled.
pub(crate) async fn same_person(
    c: &mut PgConnection,
    left: &str,
    right: &str,
    company: &str,
) -> Result<bool, Error> {
    if left == right {
        return Ok(true);
    }
    if right.is_empty() {
        return Ok(false);
    }
    Ok(sqlx::query_scalar("SELECT iam_same_person($1,$2,$3)")
        .bind(left)
        .bind(right)
        .bind(company)
        .fetch_one(c)
        .await?)
}

pub(crate) async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("QUALITY_TRANSACTION_REQUIRED").into());
    }
    let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
        .fetch_one(&mut *c)
        .await?;
    if isolation != "serializable" {
        return Err(PersistenceError::new("QUALITY_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Unit {
    database: String,
    backend: i32,
    xid: String,
    postmaster: String,
}

pub(crate) async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    let row = sqlx::query(
        "SELECT current_database() database,pg_backend_pid() backend,pg_current_xact_id()::text xid,pg_postmaster_start_time()::text postmaster",
    )
    .fetch_one(&mut *c)
    .await?;
    Ok(Unit {
        database: row.try_get("database")?,
        backend: row.try_get("backend")?,
        xid: row.try_get("xid")?,
        postmaster: row.try_get("postmaster")?,
    })
}

pub async fn lock_quality_read_sources(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    sqlx::query(
        "LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_measurements,inventory_quality_specs,inventory_quality_spec_metrics,inventory_quality_cancellations,inventory_quality_disposition_events,inventory_quality_alerts IN SHARE MODE",
    )
    .execute(c)
    .await?;
    Ok(())
}

/// Acquire cancellation writer intent before taking shared Quality source
/// locks and before any snapshot. Composition must first acquire the earlier
/// Commercial/Commerce owner locks used by the surrounding command.
pub async fn lock_quality_cancellation_writes(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("QUALITY_SERIALIZABLE_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE inventory_quality_holds,inventory_quality_cancellations IN SHARE ROW EXCLUSIVE MODE").execute(c).await?;
    Ok(())
}

// This proves the writer-intent locks are held by this backend now. It does
// not prove when they were acquired: composition must acquire them before
// the transaction's first snapshot and in the surrounding owner lock order.
async fn require_quality_cancellation_write_locks(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('inventory_quality_holds'::regclass,'inventory_quality_cancellations'::regclass)")
        .fetch_one(c).await?;
    if !held {
        return Err(PersistenceError::new("QUALITY_CANCELLATION_WRITE_LOCKS_REQUIRED").into());
    }
    Ok(())
}

pub(crate) fn parse_status(value: &str) -> Result<HoldStatus, Error> {
    HoldStatus::parse(value).ok_or_else(|| invalid("A Quality hold status is invalid.").into())
}

pub(crate) fn parse_result(value: Option<String>) -> Result<Option<InspectionResult>, Error> {
    value
        .map(|value| {
            InspectionResult::parse(&value)
                .ok_or_else(|| invalid("A Quality inspection result is invalid.").into())
        })
        .transpose()
}

fn parse_action(value: &str) -> Result<CancellationAction, Error> {
    CancellationAction::parse(value)
        .ok_or_else(|| invalid("A Quality cancellation action is invalid.").into())
}

pub(crate) fn safe(value: i64) -> Result<i64, Error> {
    if (0..=crate::domain::quality::MAX_SAFE_INTEGER).contains(&value) {
        Ok(value)
    } else {
        Err(invalid("A Quality integer is outside the safe range.").into())
    }
}

// Legacy PostgreSQL INT8 decoding permits signed JavaScript-safe values.
// Cancellation evidence has no positive-version or timestamp constraint.
fn safe_signed(value: i64) -> Result<i64, Error> {
    if (-crate::domain::quality::MAX_SAFE_INTEGER..=crate::domain::quality::MAX_SAFE_INTEGER)
        .contains(&value)
    {
        Ok(value)
    } else {
        Err(invalid("A Quality integer is outside the safe range.").into())
    }
}

fn hold_from_row(row: &sqlx::postgres::PgRow) -> Result<QualityHold, Error> {
    Ok(QualityHold {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        warehouse_id: row.try_get("warehouse_id")?,
        product_id: row.try_get("product_id")?,
        lot_id: row.try_get("lot_id")?,
        reason: row.try_get("reason")?,
        due_date: row.try_get("due_date")?,
        status: parse_status(&row.try_get::<String, _>("status")?)?,
        created_by: row.try_get("created_by")?,
        last_inspector: row.try_get("last_inspector")?,
        last_result: parse_result(row.try_get("last_result")?)?,
        released_by: row.try_get("released_by")?,
        release_reason: row.try_get("release_reason")?,
        released_at: row
            .try_get::<Option<i64>, _>("released_at")?
            .map(safe)
            .transpose()?,
        version: safe(row.try_get("version")?)?,
        created_at: safe(row.try_get("created_at")?)?,
        updated_at: safe(row.try_get("updated_at")?)?,
        spec_id: row.try_get("spec_id")?,
        spec_version: row
            .try_get::<Option<i64>, _>("spec_version")?
            .map(safe)
            .transpose()?,
        quantity_scope: row.try_get("quantity_scope")?,
        held_quantity_milli: row
            .try_get::<Option<i64>, _>("held_quantity_milli")?
            .map(safe)
            .transpose()?,
        source_type: row.try_get("source_type")?,
        source_id: row.try_get("source_id")?,
        cancel_requested_by: row.try_get("cancel_requested_by")?,
        cancel_reason: row.try_get("cancel_reason")?,
        cancelled_by: row.try_get("cancelled_by")?,
        cancelled_at: row
            .try_get::<Option<i64>, _>("cancelled_at")?
            .map(safe)
            .transpose()?,
        disposition_kind: row.try_get("disposition_kind")?,
        disposition_reason: row.try_get("disposition_reason")?,
        disposition_requested_by: row.try_get("disposition_requested_by")?,
        disposition_count_id: row.try_get("disposition_count_id")?,
        disposition_quantity_milli: row
            .try_get::<Option<i64>, _>("disposition_quantity_milli")?
            .map(safe)
            .transpose()?,
        disposed_by: row.try_get("disposed_by")?,
        disposed_at: row
            .try_get::<Option<i64>, _>("disposed_at")?
            .map(safe)
            .transpose()?,
        inspection_count: safe(row.try_get("inspection_count")?)?,
    })
}

pub(crate) async fn read_holds(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: Option<&str>,
) -> Result<Vec<QualityHold>, Error> {
    let sql = "SELECT id,company_id,branch_id,warehouse_id,product_id,lot_id,reason,due_date,status,created_by,last_inspector,last_result,released_by,release_reason,released_at,version,created_at,updated_at,spec_id,spec_version,quantity_scope,held_quantity_milli,source_type,source_id,cancel_requested_by,cancel_reason,cancelled_by,cancelled_at,disposition_kind,disposition_reason,disposition_requested_by,disposition_count_id,disposition_quantity_milli,disposed_by,disposed_at,(SELECT count(*) FROM inventory_quality_inspections i WHERE i.hold_id=h.id) inspection_count FROM inventory_quality_holds h WHERE h.company_id=$1 AND ($2::text IS NULL OR h.branch_id=$2) AND ($3::text IS NULL OR h.id=$3) ORDER BY h.due_date,h.id LIMIT 1001";
    let rows = sqlx::query(sql)
        .bind(company)
        .bind(branch)
        .bind(id)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > if id.is_some() { 1 } else { MAX_HOLDS } {
        return Err(bounded("holds").into());
    }
    rows.iter().map(hold_from_row).collect()
}

fn hold_product_from_row(row: &sqlx::postgres::PgRow) -> Result<QualityHoldProduct, Error> {
    Ok(QualityHoldProduct {
        product_name: row.try_get("product_name")?,
        product_unit: row.try_get("product_unit")?,
        tracking_mode: row.try_get("tracking_mode")?,
        warehouse_name: row.try_get("warehouse_name")?,
        lot_code: row.try_get("lot_code")?,
        serial_number: row.try_get("serial_number")?,
        expiry_date: row.try_get("expiry_date")?,
        eligible_count: safe(row.try_get("eligible_count")?)?,
    })
}

/// Product-response hold rows. This is a read-only projection over the same
/// Commerce/Inventory source facts the retained page used; current Quality
/// authority and version comparison remain with the application use case.
pub(crate) async fn read_hold_views(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: Option<&str>,
) -> Result<(Vec<QualityHold>, BTreeMap<String, QualityHoldProduct>), Error> {
    let sql = "SELECT h.id,h.company_id,h.branch_id,h.warehouse_id,h.product_id,h.lot_id,h.reason,h.due_date,h.status,h.created_by,h.last_inspector,h.last_result,h.released_by,h.release_reason,h.released_at,h.version,h.created_at,h.updated_at,h.spec_id,h.spec_version,h.quantity_scope,h.held_quantity_milli,h.source_type,h.source_id,h.cancel_requested_by,h.cancel_reason,h.cancelled_by,h.cancelled_at,h.disposition_kind,h.disposition_reason,h.disposition_requested_by,h.disposition_count_id,h.disposition_quantity_milli,h.disposed_by,h.disposed_at, \
        (SELECT count(*) FROM inventory_quality_inspections i WHERE i.hold_id=h.id) inspection_count, \
        p.name product_name,p.unit product_unit,p.tracking_mode,w.name warehouse_name,l.lot_code,l.serial_number,l.expiry_date, \
        (SELECT count(DISTINCT a.user_id) FROM assignments a \
          WHERE a.company_id=h.company_id \
            AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=h.branch_id)) \
            AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
            AND iam_role_allows(a.role,a.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END) \
            AND NOT iam_same_person(a.user_id,h.created_by,h.company_id) \
            AND (h.status<>'cancel_pending' OR NOT iam_same_person(a.user_id,h.cancel_requested_by,h.company_id)) \
            AND (h.status<>'awaiting_release' OR NOT iam_same_person(a.user_id,h.last_inspector,h.company_id))) eligible_count \
        FROM inventory_quality_holds h \
        JOIN commerce_products p ON p.id=h.product_id \
        JOIN commerce_warehouses w ON w.id=h.warehouse_id \
        LEFT JOIN inventory_lots l ON l.id=h.lot_id \
        WHERE h.company_id=$1 AND ($2::text IS NULL OR h.branch_id=$2) AND ($3::text IS NULL OR h.id=$3) \
        ORDER BY h.due_date,h.id LIMIT 1001";
    let rows = sqlx::query(sql)
        .bind(company)
        .bind(branch)
        .bind(id)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > if id.is_some() { 1 } else { MAX_HOLDS } {
        return Err(bounded("holds").into());
    }
    let mut holds = Vec::with_capacity(rows.len());
    let mut products = BTreeMap::new();
    for row in &rows {
        let hold = hold_from_row(row)?;
        products.insert(hold.id.clone(), hold_product_from_row(row)?);
        holds.push(hold);
    }
    Ok((holds, products))
}

async fn read_workspace_stock(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
) -> Result<Vec<QualityStock>, Error> {
    let rows = sqlx::query(
        "SELECT s.warehouse_id,s.product_id,s.quantity_milli,s.version,p.name product_name,p.tracking_mode,w.name warehouse_name \
         FROM commerce_stock s \
         JOIN commerce_products p ON p.id=s.product_id \
         JOIN commerce_warehouses w ON w.id=s.warehouse_id \
         WHERE w.company_id=$1 AND p.company_id=w.company_id AND w.status='active' AND p.status='active' \
           AND ($2::text IS NULL OR w.branch_id=$2) \
         ORDER BY w.name,p.name LIMIT 1001",
    )
    .bind(company)
    .bind(branch)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_HOLDS {
        return Err(bounded("holds").into());
    }
    rows.iter()
        .map(|row| {
            Ok(QualityStock {
                warehouse_id: row.try_get("warehouse_id")?,
                product_id: row.try_get("product_id")?,
                quantity_milli: safe(row.try_get("quantity_milli")?)?,
                version: safe(row.try_get("version")?)?,
                product_name: row.try_get("product_name")?,
                tracking_mode: row.try_get("tracking_mode")?,
                warehouse_name: row.try_get("warehouse_name")?,
            })
        })
        .collect()
}

async fn read_workspace_lots(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
) -> Result<Vec<QualityLot>, Error> {
    let rows = sqlx::query(
        "SELECT l.id,l.product_id,l.tracking_mode,l.lot_code,l.serial_number,l.expiry_date,b.warehouse_id,b.quantity_milli,b.version balance_version,p.name product_name,w.name warehouse_name, \
            CASE WHEN EXISTS(SELECT 1 FROM inventory_quality_holds h WHERE h.lot_id=l.id AND h.warehouse_id=b.warehouse_id AND h.status NOT IN ('released','cancelled','disposed')) THEN 1 ELSE 0 END::bigint quality_held \
         FROM inventory_lots l \
         JOIN inventory_lot_balances b ON b.lot_id=l.id \
         JOIN commerce_products p ON p.id=l.product_id \
         JOIN commerce_warehouses w ON w.id=b.warehouse_id \
         WHERE l.company_id=$1 AND b.quantity_milli>0 AND p.company_id=w.company_id \
           AND ($2::text IS NULL OR w.branch_id=$2) \
         ORDER BY w.name,p.name,l.expiry_date NULLS LAST,l.id LIMIT 1001",
    )
    .bind(company)
    .bind(branch)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_HOLDS {
        return Err(bounded("holds").into());
    }
    rows.iter()
        .map(|row| {
            Ok(QualityLot {
                id: row.try_get("id")?,
                warehouse_id: row.try_get("warehouse_id")?,
                product_id: row.try_get("product_id")?,
                quantity_milli: safe(row.try_get("quantity_milli")?)?,
                balance_version: safe(row.try_get("balance_version")?)?,
                product_name: row.try_get("product_name")?,
                warehouse_name: row.try_get("warehouse_name")?,
                tracking_mode: row.try_get("tracking_mode")?,
                lot_code: row.try_get("lot_code")?,
                serial_number: row.try_get("serial_number")?,
                expiry_date: row.try_get("expiry_date")?,
                quality_held: row.try_get::<i64, _>("quality_held")? == 1,
            })
        })
        .collect()
}

async fn read_workspace_products(
    c: &mut PgConnection,
    company: &str,
) -> Result<Vec<QualityProduct>, Error> {
    let rows = sqlx::query(
        "SELECT id,sku,name FROM commerce_products WHERE company_id=$1 AND status='active' ORDER BY sku,name LIMIT 1001",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_HOLDS {
        return Err(bounded("holds").into());
    }
    rows.iter()
        .map(|row| {
            Ok(QualityProduct {
                id: row.try_get("id")?,
                sku: row.try_get("sku")?,
                name: row.try_get("name")?,
            })
        })
        .collect()
}

async fn read_workspace_specifications(
    c: &mut PgConnection,
    company: &str,
) -> Result<Vec<QualitySpecification>, Error> {
    let rows = sqlx::query(
        "SELECT s.id,s.product_id,s.name,s.status,s.version,p.sku,p.name product_name \
         FROM inventory_quality_specs s JOIN commerce_products p ON p.id=s.product_id \
         WHERE s.company_id=$1 ORDER BY p.sku,s.created_at DESC LIMIT 1001",
    )
    .bind(company)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_HOLDS {
        return Err(bounded("metrics").into());
    }
    rows.iter()
        .map(|row| {
            Ok(QualitySpecification {
                id: row.try_get("id")?,
                product_id: row.try_get("product_id")?,
                name: row.try_get("name")?,
                status: SpecStatus::parse(&row.try_get::<String, _>("status")?).ok_or_else(|| {
                    invalid("A Quality specification status is invalid.")
                })?,
                version: safe(row.try_get("version")?)?,
                sku: row.try_get("sku")?,
                product_name: row.try_get("product_name")?,
            })
        })
        .collect()
}

pub(crate) async fn read_specification_metrics(    c: &mut PgConnection,
    company: &str,
    spec_id: Option<&str>,
) -> Result<Vec<QualityMetric>, Error> {
    let sql = "SELECT m.id,m.spec_id,m.code,m.label,m.unit,m.minimum_value::text minimum,m.maximum_value::text maximum,m.ordinal FROM inventory_quality_spec_metrics m JOIN inventory_quality_specs s ON s.id=m.spec_id WHERE s.company_id=$1 AND ($2::text IS NULL OR m.spec_id=$2) ORDER BY m.spec_id,m.ordinal LIMIT 1001";
    let rows = sqlx::query(sql)
        .bind(company)
        .bind(spec_id)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > MAX_METRICS {
        return Err(bounded("metrics").into());
    }
    rows.iter()
        .map(|row| {
            Ok(QualityMetric {
                id: row.try_get("id")?,
                spec_id: row.try_get("spec_id")?,
                code: row.try_get("code")?,
                label: row.try_get("label")?,
                unit: row.try_get("unit")?,
                minimum: row.try_get("minimum")?,
                maximum: row.try_get("maximum")?,
                ordinal: safe(row.try_get("ordinal")?)?,
            })
        })
        .collect()
}

async fn read_alerts(
    c: &mut PgConnection,
    company: &str,
    hold_id: Option<&str>,
) -> Result<Vec<QualityAlert>, Error> {
    let sql = "SELECT a.id,a.hold_id,a.phase,a.source_version,a.due_date::text due_date,a.detected_at,a.version,a.acknowledged_by,a.acknowledged_at,a.reason FROM inventory_quality_alerts a JOIN inventory_quality_holds h ON h.id=a.hold_id WHERE h.company_id=$1 AND ($2::text IS NULL OR a.hold_id=$2) ORDER BY a.due_date,a.id LIMIT 1001";
    let rows = sqlx::query(sql)
        .bind(company)
        .bind(hold_id)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > MAX_ALERTS {
        return Err(bounded("alerts").into());
    }
    rows.iter()
        .map(|row| {
            Ok(QualityAlert {
                id: row.try_get("id")?,
                hold_id: row.try_get("hold_id")?,
                phase: row.try_get("phase")?,
                source_version: safe(row.try_get("source_version")?)?,
                due_date: row.try_get("due_date")?,
                detected_at: safe(row.try_get("detected_at")?)?,
                version: safe(row.try_get("version")?)?,
                acknowledged_by: row.try_get("acknowledged_by")?,
                acknowledged_at: row
                    .try_get::<Option<i64>, _>("acknowledged_at")?
                    .map(safe)
                    .transpose()?,
                reason: row.try_get("reason")?,
            })
        })
        .collect()
}

pub struct CurrentQualityWorkspace {
    unit: Unit,
    company: String,
    branch: Option<String>,
    data: QualityWorkspaceData,
}

impl CurrentQualityWorkspace {
    pub fn data(&self) -> &QualityWorkspaceData {
        &self.data
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_detail_core_locks(c).await?;
        if unit(c).await? != self.unit {
            return Err(changed().into());
        }
        let (holds, products) =
            read_hold_views(c, &self.company, self.branch.as_deref(), None).await?;
        if holds != self.data.holds || products != self.data.hold_products {
            return Err(changed().into());
        }
        if read_workspace_stock(c, &self.company, self.branch.as_deref()).await? != self.data.stock {
            return Err(changed().into());
        }
        if read_workspace_lots(c, &self.company, self.branch.as_deref()).await? != self.data.lots {
            return Err(changed().into());
        }
        if read_workspace_products(c, &self.company).await? != self.data.products {
            return Err(changed().into());
        }
        if read_workspace_specifications(c, &self.company).await? != self.data.specifications {
            return Err(changed().into());
        }
        if read_specification_metrics(c, &self.company, None).await?
            != self.data.specification_metrics
        {
            return Err(changed().into());
        }
        if read_alerts(c, &self.company, None).await? != self.data.alerts {
            return Err(changed().into());
        }
        Ok(())
    }
}

pub async fn read_quality_workspace(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
) -> Result<CurrentQualityWorkspace, Error> {
    require_quality_detail_core_locks(c).await?;
    let (holds, hold_products) =
        read_hold_views(c, company.as_str(), branch.map(RecordId::as_str), None).await?;
    let stock = read_workspace_stock(c, company.as_str(), branch.map(RecordId::as_str)).await?;
    let lots = read_workspace_lots(c, company.as_str(), branch.map(RecordId::as_str)).await?;
    let products = read_workspace_products(c, company.as_str()).await?;
    let specifications = read_workspace_specifications(c, company.as_str()).await?;
    let specification_metrics = read_specification_metrics(c, company.as_str(), None).await?;
    let alerts = read_alerts(c, company.as_str(), None).await?;
    let data = QualityWorkspaceData {
        holds,
        hold_products,
        stock,
        lots,
        products,
        specifications,
        specification_metrics,
        alerts,
    };
    Ok(CurrentQualityWorkspace {
        unit: unit(c).await?,
        company: company.as_str().to_owned(),
        branch: branch.map(|branch| branch.as_str().to_owned()),
        data,
    })
}

pub struct CurrentQualityDetail {
    unit: Unit,
    company: String,
    branch: Option<String>,
    id: String,
    data: QualityDetailData,
}

impl CurrentQualityDetail {
    pub fn data(&self) -> &QualityDetailData {
        &self.data
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_detail_core_locks(c).await?;
        if unit(c).await? != self.unit {
            return Err(changed().into());
        }
        if read_detail(c, &self.company, self.branch.as_deref(), &self.id)
            .await?
            .as_ref()
            != Some(&self.data)
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

async fn read_cancellation_events(
    c: &mut PgConnection,
    id: &str,
    excluded: Option<&str>,
) -> Result<Vec<QualityCancellation>, Error> {
    let rows = sqlx::query(
        "SELECT id,hold_id,version,action,reason,actor_id,created_at FROM inventory_quality_cancellations WHERE hold_id=$1 AND ($2::text IS NULL OR id<>$2) ORDER BY version DESC LIMIT 201",
    )
    .bind(id)
    .bind(excluded)
    .fetch_all(&mut *c)
    .await?;
    if rows.len() > MAX_CANCELLATIONS {
        return Err(bounded("cancellations").into());
    }
    let cancellations = rows
        .iter()
        .map(|row| {
            Ok(QualityCancellation {
                id: row.try_get("id")?,
                hold_id: row.try_get("hold_id")?,
                version: safe_signed(row.try_get("version")?)?,
                action: parse_action(&row.try_get::<String, _>("action")?)?,
                reason: row.try_get("reason")?,
                actor_id: row.try_get("actor_id")?,
                created_at: safe_signed(row.try_get("created_at")?)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(cancellations)
}

/// A bounded page retained in one serializable Quality read unit. This is an
/// owner fact, not an authorization decision. The caller must acquire Quality
/// source locks before the first snapshot and keep every page in that unit.
pub struct CurrentQualityCancellationHistoryPage {
    unit: Unit,
    company: CompanyId,
    branch: Option<RecordId>,
    hold: QualityHold,
    before: Option<i64>,
    limit: usize,
    rows: Vec<QualityCancellation>,
    next_before: Option<i64>,
    revision: String,
}

impl CurrentQualityCancellationHistoryPage {
    pub fn rows(&self) -> &[QualityCancellation] {
        &self.rows
    }
    pub fn next_before(&self) -> Option<i64> {
        self.next_before
    }
    pub fn head_version(&self) -> i64 {
        self.hold.version
    }
    pub fn history_revision(&self) -> &str {
        &self.revision
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let current = read_quality_cancellation_history_page(
            c,
            &self.company,
            self.branch.as_ref(),
            self.hold.id.as_str(),
            self.before,
            Some(self.hold.version),
            Some(&self.revision),
            self.limit,
        )
        .await?;
        if current.unit != self.unit
            || current.hold != self.hold
            || current.rows != self.rows
            || current.next_before != self.next_before
            || current.revision != self.revision
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

fn history_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_HISTORY_CHANGED",
        "Quality cancellation history changed; reload it.",
    )
}

fn hash_frame(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}

// Stream fixed-size per-field digests in a fixed unique order. Each original
// byte length and its 32-byte SHA-256 are framed independently; integers use
// fixed-width big-endian encoding. Historical text is never concatenated or
// transferred to the caller merely to calculate the revision.
async fn cancellation_history_revision(
    c: &mut PgConnection,
    company: &CompanyId,
    hold: &QualityHold,
    excluded: Option<&str>,
) -> Result<String, Error> {
    let mut hash = Sha256::new();
    hash_frame(&mut hash, b"erpx-quality-cancellation-history-v1");
    hash_frame(&mut hash, company.as_str().as_bytes());
    hash_frame(&mut hash, hold.id.as_bytes());
    hash.update(hold.version.to_be_bytes());
    let mut rows = sqlx::query("SELECT octet_length(id) id_len,sha256(convert_to(id,'UTF8')) id_hash, octet_length(hold_id) hold_len,sha256(convert_to(hold_id,'UTF8')) hold_hash, version, action, octet_length(reason) reason_len,sha256(convert_to(reason,'UTF8')) reason_hash, octet_length(actor_id) actor_len,sha256(convert_to(actor_id,'UTF8')) actor_hash, created_at FROM inventory_quality_cancellations WHERE hold_id=$1 AND ($2::text IS NULL OR id<>$2) ORDER BY version DESC")
        .bind(&hold.id).bind(excluded).fetch(&mut *c);
    let mut count = 0_u64;
    while let Some(row) = rows.try_next().await? {
        let version = safe_signed(row.try_get("version")?)?;
        let action: String = row.try_get("action")?;
        parse_action(&action)?;
        let created_at = safe_signed(row.try_get("created_at")?)?;
        for (length, digest) in [("id_len", "id_hash"), ("hold_len", "hold_hash")] {
            let length: i32 = row.try_get(length)?;
            let digest: Vec<u8> = row.try_get(digest)?;
            hash.update((length as u64).to_be_bytes());
            hash_frame(&mut hash, &digest);
        }
        hash.update(version.to_be_bytes());
        hash_frame(&mut hash, action.as_bytes());
        for (length, digest) in [("reason_len", "reason_hash"), ("actor_len", "actor_hash")] {
            let length: i32 = row.try_get(length)?;
            let digest: Vec<u8> = row.try_get(digest)?;
            hash.update((length as u64).to_be_bytes());
            hash_frame(&mut hash, &digest);
        }
        hash.update(created_at.to_be_bytes());
        count = count
            .checked_add(1)
            .ok_or_else(|| invalid("Quality cancellation history is too large."))?;
    }
    drop(rows);
    hash.update(count.to_be_bytes());
    Ok(format!("qch1:{:x}", hash.finalize()))
}

#[allow(clippy::too_many_arguments)]
pub async fn read_quality_cancellation_history_page(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    hold_id: &str,
    before: Option<i64>,
    expected_head: Option<i64>,
    expected_revision: Option<&str>,
    limit: usize,
) -> Result<CurrentQualityCancellationHistoryPage, Error> {
    serializable(c).await?;
    if !(1..=MAX_CANCELLATIONS).contains(&limit)
        || before.is_some_and(|v| safe_signed(v).is_err())
        || expected_head.is_some_and(|v| v < 1 || safe(v).is_err())
        || expected_revision.is_some_and(|v| {
            v.len() != 69
                || !v.starts_with("qch1:")
                || !v[5..]
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        || (expected_head.is_some() != expected_revision.is_some())
        || (before.is_some() && expected_head.is_none())
    {
        return Err(invalid("The Quality cancellation history page is invalid.").into());
    }
    // Held-lock proof does not attest when locks were taken. Composition owns
    // the earlier-than-snapshot guarantee and the cross-owner ordering.
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=2 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('inventory_quality_holds'::regclass,'inventory_quality_cancellations'::regclass)")
        .fetch_one(&mut *c).await?;
    if !held {
        return Err(PersistenceError::new("QUALITY_HISTORY_READ_LOCKS_REQUIRED").into());
    }
    let hold = read_holds(
        c,
        company.as_str(),
        branch.map(RecordId::as_str),
        Some(hold_id),
    )
    .await?
    .pop()
    .ok_or_else(hold_not_found)?;
    let revision = cancellation_history_revision(c, company, &hold, None).await?;
    if expected_head.is_some_and(|head| head != hold.version)
        || expected_revision.is_some_and(|expected| expected != revision)
    {
        return Err(history_changed().into());
    }
    let mut raw = sqlx::query("SELECT CASE WHEN bytes*6+512 <= $4 THEN id END id, CASE WHEN bytes*6+512 <= $4 THEN hold_id END hold_id, version, action, CASE WHEN bytes*6+512 <= $4 THEN reason END reason, CASE WHEN bytes*6+512 <= $4 THEN actor_id END actor_id, created_at, bytes FROM (SELECT *, octet_length(id)::bigint+octet_length(hold_id)+octet_length(action)+octet_length(reason)+octet_length(actor_id) bytes FROM inventory_quality_cancellations WHERE hold_id=$1 AND ($2::bigint IS NULL OR version<$2) ORDER BY version DESC LIMIT $3) page ORDER BY version DESC")
        .bind(hold_id).bind(before).bind((limit + 1) as i64).bind(MAX_CANCELLATION_PAGE_BYTES as i64).fetch(&mut *c);
    let mut rows = Vec::new();
    let mut bytes = 0_usize;
    let mut has_more = false;
    while let Some(row) = raw.try_next().await? {
        if rows.len() == limit {
            has_more = true;
            break;
        }
        let raw_bytes: i64 = row.try_get("bytes")?;
        let weight = usize::try_from(raw_bytes)
            .ok()
            .and_then(|n| n.checked_mul(6))
            .and_then(|n| n.checked_add(512))
            .ok_or_else(|| bounded("cancellation history row"))?;
        if weight > MAX_CANCELLATION_PAGE_BYTES {
            return Err(bounded("cancellation history row").into());
        }
        if bytes
            .checked_add(weight)
            .is_none_or(|n| n > MAX_CANCELLATION_PAGE_BYTES)
        {
            has_more = true;
            break;
        }
        let required = |field: &str| -> Result<String, Error> {
            row.try_get::<Option<String>, _>(field)?
                .ok_or_else(|| bounded("cancellation history row").into())
        };
        rows.push(QualityCancellation {
            id: required("id")?,
            hold_id: required("hold_id")?,
            version: safe_signed(row.try_get("version")?)?,
            action: parse_action(&row.try_get::<String, _>("action")?)?,
            reason: required("reason")?,
            actor_id: required("actor_id")?,
            created_at: safe_signed(row.try_get("created_at")?)?,
        });
        bytes += weight;
    }
    drop(raw);
    let next_before = if has_more {
        rows.last().map(|row| row.version)
    } else {
        None
    };
    Ok(CurrentQualityCancellationHistoryPage {
        unit: unit(c).await?,
        company: company.clone(),
        branch: branch.cloned(),
        hold,
        before,
        limit,
        rows,
        next_before,
        revision,
    })
}

async fn read_detail_core_data(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
) -> Result<Option<QualityDetailCoreData>, Error> {
    let (mut hold_rows, mut hold_products) = read_hold_views(c, company, branch, Some(id)).await?;
    let Some(hold) = hold_rows.pop() else {
        return Ok(None);
    };
    let product = hold_products
        .remove(&hold.id)
        .unwrap_or_else(QualityHoldProduct::empty);
    let inspection_rows = sqlx::query(
        "SELECT i.id,i.hold_id,i.reviewed_version,i.result,i.evidence,i.inspected_by,i.created_at,i.spec_id,i.spec_version FROM inventory_quality_inspections i WHERE i.hold_id=$1 ORDER BY i.created_at,i.id LIMIT 1001",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    if inspection_rows.len() > MAX_INSPECTIONS {
        return Err(bounded("inspections").into());
    }
    let inspections = inspection_rows
        .iter()
        .map(|row| {
            Ok(QualityInspection {
                id: row.try_get("id")?,
                hold_id: row.try_get("hold_id")?,
                reviewed_version: safe(row.try_get("reviewed_version")?)?,
                result: InspectionResult::parse(&row.try_get::<String, _>("result")?)
                    .ok_or_else(|| invalid("A Quality inspection result is invalid."))?,
                evidence: row.try_get("evidence")?,
                inspected_by: row.try_get("inspected_by")?,
                created_at: safe(row.try_get("created_at")?)?,
                spec_id: row.try_get("spec_id")?,
                spec_version: row
                    .try_get::<Option<i64>, _>("spec_version")?
                    .map(safe)
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let measurement_rows = sqlx::query(
        "SELECT m.id,m.inspection_id,m.metric_id,m.code,m.label,m.unit,m.minimum_value::text minimum,m.maximum_value::text maximum,m.measured_value::text measured_value,m.passed,m.ordinal FROM inventory_quality_measurements m JOIN inventory_quality_inspections i ON i.id=m.inspection_id WHERE i.hold_id=$1 ORDER BY i.created_at,m.ordinal LIMIT 5001",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    if measurement_rows.len() > MAX_MEASUREMENTS {
        return Err(bounded("measurements").into());
    }
    let measurements = measurement_rows
        .iter()
        .map(|row| {
            Ok(QualityMeasurement {
                id: row.try_get("id")?,
                inspection_id: row.try_get("inspection_id")?,
                metric_id: row.try_get("metric_id")?,
                code: row.try_get("code")?,
                label: row.try_get("label")?,
                unit: row.try_get("unit")?,
                minimum: row.try_get("minimum")?,
                maximum: row.try_get("maximum")?,
                measured_value: row.try_get("measured_value")?,
                passed: row.try_get::<i64, _>("passed")? == 1,
                ordinal: safe(row.try_get("ordinal")?)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let disposition_rows = sqlx::query(
        "SELECT id,hold_id,count_id,action,actor_id,reviewed_version,reason,created_at FROM inventory_quality_disposition_events WHERE hold_id=$1 ORDER BY created_at,id LIMIT 201",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    if disposition_rows.len() > MAX_DISPOSITIONS {
        return Err(bounded("dispositions").into());
    }
    let dispositions = disposition_rows
        .iter()
        .map(|row| {
            Ok(QualityDisposition {
                id: row.try_get("id")?,
                hold_id: row.try_get("hold_id")?,
                count_id: row.try_get("count_id")?,
                action: row.try_get("action")?,
                actor_id: row.try_get("actor_id")?,
                reviewed_version: safe(row.try_get("reviewed_version")?)?,
                reason: row.try_get("reason")?,
                created_at: safe(row.try_get("created_at")?)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Some(QualityDetailCoreData {
        // Legacy detail returns no metrics when the hold has no spec. The
        // workspace separately uses None to request all Company metrics.
        spec_metrics: match hold.spec_id.as_deref() {
            Some(spec_id) => read_specification_metrics(c, company, Some(spec_id)).await?,
            None => Vec::new(),
        },
        alerts: read_alerts(c, company, Some(id)).await?,
        hold,
        product,
        inspections,
        measurements,
        dispositions,
    }))
}

async fn read_detail(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
) -> Result<Option<QualityDetailData>, Error> {
    let Some(core) = read_detail_core_data(c, company, branch, id).await? else {
        return Ok(None);
    };
    let cancellations = read_cancellation_events(c, id, None).await?;
    Ok(Some(QualityDetailData {
        hold: core.hold,
        product: core.product,
        inspections: core.inspections,
        measurements: core.measurements,
        cancellations,
        dispositions: core.dispositions,
        spec_metrics: core.spec_metrics,
        alerts: core.alerts,
    }))
}

/// Scoped detail facts without cancellation history. This owner witness is
/// retained in one unit; it does not grant authorization or purchase access.
pub struct CurrentQualityDetailCore {
    unit: Unit,
    company: String,
    branch: Option<String>,
    id: String,
    data: QualityDetailCoreData,
}

async fn require_quality_detail_core_locks(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=8 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('inventory_quality_holds'::regclass,'inventory_quality_inspections'::regclass,'inventory_quality_measurements'::regclass,'inventory_quality_specs'::regclass,'inventory_quality_spec_metrics'::regclass,'inventory_quality_cancellations'::regclass,'inventory_quality_disposition_events'::regclass,'inventory_quality_alerts'::regclass)")
        .fetch_one(c).await?;
    if !held {
        return Err(PersistenceError::new("QUALITY_DETAIL_READ_LOCKS_REQUIRED").into());
    }
    Ok(())
}

impl CurrentQualityDetailCore {
    pub fn data(&self) -> &QualityDetailCoreData {
        &self.data
    }
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_detail_core_locks(c).await?;
        if unit(c).await? != self.unit
            || read_detail_core_data(c, &self.company, self.branch.as_deref(), &self.id)
                .await?
                .as_ref()
                != Some(&self.data)
        {
            return Err(changed().into());
        }
        Ok(())
    }
}

pub async fn read_quality_detail_core(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    id: &str,
) -> Result<Option<CurrentQualityDetailCore>, Error> {
    require_quality_detail_core_locks(c).await?;
    let Some(data) =
        read_detail_core_data(c, company.as_str(), branch.map(RecordId::as_str), id).await?
    else {
        return Ok(None);
    };
    Ok(Some(CurrentQualityDetailCore {
        unit: unit(c).await?,
        company: company.as_str().to_owned(),
        branch: branch.map(|b| b.as_str().to_owned()),
        id: id.to_owned(),
        data,
    }))
}

pub async fn read_quality_detail(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    id: &str,
) -> Result<Option<CurrentQualityDetail>, Error> {
    require_quality_detail_core_locks(c).await?;
    let Some(data) = read_detail(c, company.as_str(), branch.map(RecordId::as_str), id).await?
    else {
        return Ok(None);
    };
    Ok(Some(CurrentQualityDetail {
        unit: unit(c).await?,
        company: company.as_str().to_owned(),
        branch: branch.map(|branch| branch.as_str().to_owned()),
        id: id.to_owned(),
        data,
    }))
}

/// Composition-owned adapter: the caller opens the serializable unit, reads the
/// current workspace/details, and invokes the pure application use cases.
pub struct PgQualityRead<'a> {
    pub connection: &'a mut PgConnection,
}

impl QualityReadRepository for PgQualityRead<'_> {
    fn workspace<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a RecordId>,
    ) -> QualityFuture<'a, QualityWorkspaceData> {
        Box::pin(async move {
            read_quality_workspace(&mut *self.connection, company, branch)
                .await
                .map(|current| current.data)
                .map_err(business)
        })
    }

    fn detail<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch: Option<&'a RecordId>,
        id: &'a str,
    ) -> QualityFuture<'a, Option<QualityDetailData>> {
        Box::pin(async move {
            read_quality_detail(&mut *self.connection, company, branch, id)
                .await
                .map(|current| current.map(|current| current.data))
                .map_err(business)
        })
    }
}

pub struct PgQualityCancellation<'a> {
    pub connection: &'a mut PgConnection,
}

/// A transaction-bound Quality cancellation effect. This is an owner effect,
/// not an authorization or purchase proof; the caller must keep its current
/// authority and purchase witnesses in the same serializable transaction.
pub struct PreparedQualityCancellation {
    source: CancellationSource,
    command: QualityCancellationCommand,
    company: CompanyId,
    branch: Option<RecordId>,
    actor: UserId,
    now: i64,
    receipt: QualityCancellationReceipt,
}

// Cancellation depends on the exact scoped hold (including inspection count)
// and its full prior cancellation evidence, not unrelated Company metrics or
// alerts. The public detail read keeps its independent bounds.
struct CancellationSource {
    unit: Unit,
    hold: QualityHold,
    history_revision: String,
}

async fn cancellation_source(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    id: &RecordId,
) -> Result<Option<CancellationSource>, Error> {
    serializable(c).await?;
    let Some(hold) = read_holds(
        c,
        company.as_str(),
        branch.map(RecordId::as_str),
        Some(id.as_str()),
    )
    .await?
    .pop() else {
        return Ok(None);
    };
    let history_revision = cancellation_history_revision(c, company, &hold, None).await?;
    Ok(Some(CancellationSource {
        unit: unit(c).await?,
        hold,
        history_revision,
    }))
}

impl PreparedQualityCancellation {
    pub fn receipt(&self) -> &QualityCancellationReceipt {
        &self.receipt
    }

    pub fn hold_facts(&self) -> QualityHoldFacts {
        self.source.hold.facts()
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_cancellation_write_locks(c).await?;
        let current = cancellation_source(
            c,
            &self.company,
            self.branch.as_ref(),
            &self.command.hold_id,
        )
        .await?
        .ok_or_else(changed)?;
        if current.unit != self.source.unit
            || current.hold != self.source.hold
            || current.history_revision != self.source.history_revision
        {
            return Err(changed().into());
        }
        Ok(())
    }

    /// Apply exactly once. Workflow must resolve an existing receipt before
    /// preparing this effect; an existing owner event is never a replay proof.
    pub async fn apply(&self, c: &mut PgConnection) -> Result<RetainedQualityCancellation, Error> {
        self.require_current(c).await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inventory_quality_cancellations WHERE id=$1)",
        )
        .bind(&self.receipt.cancellation_id)
        .fetch_one(&mut *c)
        .await?;
        if exists {
            return Err(cancellation_conflict().into());
        }
        let receipt = apply_quality_cancellation(
            c,
            &self.command,
            &self.company,
            self.branch.as_ref(),
            &self.actor,
            self.now,
        )
        .await?;
        if receipt != self.receipt {
            return Err(changed().into());
        }
        let mut expected_hold = self.source.hold.clone();
        expected_hold.status = receipt.status;
        expected_hold.version = receipt.version;
        expected_hold.updated_at = self.now;
        match self.command.action {
            CancellationAction::Request => {
                expected_hold.cancel_requested_by = Some(self.actor.as_str().to_owned());
                expected_hold.cancel_reason = Some(self.command.reason.clone());
            }
            CancellationAction::Approve => {
                expected_hold.cancelled_by = Some(self.actor.as_str().to_owned());
                expected_hold.cancelled_at = Some(self.now);
            }
            CancellationAction::Reject => {}
        }
        let retained = RetainedQualityCancellation {
            unit: unit(c).await?,
            command: self.command.clone(),
            company: self.company.clone(),
            branch: self.branch.clone(),
            actor: self.actor.clone(),
            receipt,
            expected_hold,
            preimage_hold: self.source.hold.clone(),
            previous_revision: self.source.history_revision.clone(),
            now: self.now,
        };
        retained.require_current(c).await?;
        Ok(retained)
    }
}

/// Retained owner-effect witness; it grants no permission to execute.
pub struct RetainedQualityCancellation {
    unit: Unit,
    command: QualityCancellationCommand,
    company: CompanyId,
    branch: Option<RecordId>,
    actor: UserId,
    receipt: QualityCancellationReceipt,
    expected_hold: QualityHold,
    preimage_hold: QualityHold,
    previous_revision: String,
    now: i64,
}

impl RetainedQualityCancellation {
    pub fn receipt(&self) -> &QualityCancellationReceipt {
        &self.receipt
    }

    /// The exact saved hold after-image, including the new status, version,
    /// cancellation requester/approver and cancellation reason fields.
    pub fn hold(&self) -> &QualityHold {
        &self.expected_hold
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_cancellation_write_locks(c).await?;
        if unit(c).await? != self.unit {
            return Err(changed().into());
        }
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inventory_quality_cancellations e JOIN inventory_quality_holds h ON h.id=e.hold_id WHERE e.id=$1 AND h.id=$2 AND h.company_id=$3 AND ($4::text IS NULL OR h.branch_id=$4) AND e.actor_id=$5 AND e.action=$6 AND e.reason=$7 AND e.version=$8 AND h.version=$8 AND h.status=$9 AND e.created_at=$10)")
            .bind(&self.receipt.cancellation_id).bind(self.command.hold_id.as_str()).bind(self.company.as_str())
            .bind(self.branch.as_ref().map(RecordId::as_str)).bind(self.actor.as_str())
            .bind(self.command.action.as_str()).bind(&self.command.reason).bind(self.receipt.version)
            .bind(self.receipt.status.as_str()).bind(self.now).fetch_one(&mut *c).await?;
        if !valid {
            return Err(changed().into());
        }
        // Re-hash every prior event, excluding only the separately verified new
        // event. The preimage hold seeds the same revision; the current hold
        // itself is separately compared with its exact expected after-image.
        let hold = read_holds(
            c,
            self.company.as_str(),
            self.branch.as_ref().map(RecordId::as_str),
            Some(self.command.hold_id.as_str()),
        )
        .await?
        .pop()
        .ok_or_else(changed)?;
        if hold != self.expected_hold {
            return Err(changed().into());
        }
        let revision = cancellation_history_revision(
            c,
            &self.company,
            &self.preimage_hold,
            Some(&self.receipt.cancellation_id),
        )
        .await?;
        if revision != self.previous_revision {
            return Err(changed().into());
        }
        Ok(())
    }
}

/// Prepare against a scoped, current owner row in the caller's transaction.
/// The caller must validate current actor permission, purchase and Workflow
/// replay independently before invoking this owner port.
pub async fn prepare_quality_cancellation(
    c: &mut PgConnection,
    command: &QualityCancellationCommand,
    company: &CompanyId,
    branch: Option<&RecordId>,
    actor: &UserId,
    now: i64,
) -> Result<PreparedQualityCancellation, Error> {
    require_quality_cancellation_write_locks(c).await?;
    let normalized = crate::application::quality::normalize_quality_cancellation(command)?;
    if normalized != *command {
        return Err(BusinessError::invalid(
            "QUALITY_CANCELLATION_REASON",
            "Normalize the cancellation reason before fingerprinting.",
        )
        .into());
    }
    let locked: Option<String> = sqlx::query_scalar(
        "SELECT id FROM inventory_quality_holds WHERE id=$1 AND company_id=$2 AND ($3::text IS NULL OR branch_id=$3) FOR UPDATE",
    )
    .bind(command.hold_id.as_str())
    .bind(company.as_str())
    .bind(branch.map(RecordId::as_str))
    .fetch_optional(&mut *c)
    .await?;
    if locked.is_none() {
        return Err(hold_not_found().into());
    }
    let source = cancellation_source(c, company, branch, &command.hold_id)
        .await?
        .ok_or_else(hold_not_found)?;
    let facts = source.hold.facts();
    if matches!(
        command.action,
        CancellationAction::Approve | CancellationAction::Reject
    ) && (same_person(c, actor.as_str(), &facts.created_by, company.as_str()).await?
        || same_person(
            c,
            actor.as_str(),
            facts.cancel_requested_by.as_deref().unwrap_or(""),
            company.as_str(),
        )
        .await?)
    {
        return Err(independent_required().into());
    }
    let plan = crate::domain::quality::plan_cancellation(
        &facts,
        actor.as_str(),
        command.action,
        command.expected_version,
    )?;
    let id = command.hold_id.as_str().to_owned();
    let receipt = QualityCancellationReceipt {
        id: id.clone(),
        hold_id: id,
        cancellation_id: format!("quality-cancel-{}", command.command_key.as_str()),
        status: plan.target,
        version: plan.version,
    };
    Ok(PreparedQualityCancellation {
        source,
        command: command.clone(),
        company: company.clone(),
        branch: branch.cloned(),
        actor: actor.clone(),
        now,
        receipt,
    })
}

impl QualityCancellationRepository for PgQualityCancellation<'_> {
    fn cancel<'a>(
        &'a mut self,
        command: &'a QualityCancellationCommand,
        company: &'a CompanyId,
        branch: Option<&'a RecordId>,
        actor_id: &'a UserId,
        now: i64,
    ) -> QualityFuture<'a, QualityCancellationReceipt> {
        Box::pin(async move {
            apply_quality_cancellation(
                &mut *self.connection,
                command,
                company,
                branch,
                actor_id,
                now,
            )
            .await
            .map_err(business)
        })
    }
}

pub async fn apply_quality_cancellation(
    c: &mut PgConnection,
    command: &QualityCancellationCommand,
    company: &CompanyId,
    branch: Option<&RecordId>,
    actor_id: &UserId,
    now: i64,
) -> Result<QualityCancellationReceipt, Error> {
    serializable(c).await?;
    require_quality_cancellation_write_locks(c).await?;
    let cancellation = format!("quality-cancel-{}", command.command_key.as_str());
    if let Some(row) = sqlx::query(
        "SELECT c.hold_id,c.action,c.reason,c.actor_id,c.version FROM inventory_quality_cancellations c JOIN inventory_quality_holds h ON h.id=c.hold_id WHERE c.id=$1 AND h.company_id=$2 AND ($3::text IS NULL OR h.branch_id=$3)",
    )
    .bind(&cancellation)
    .bind(company.as_str())
    .bind(branch.map(RecordId::as_str))
    .fetch_optional(&mut *c)
    .await?
    {
        let hold_id: String = row.try_get("hold_id")?;
        let action: String = row.try_get("action")?;
        let reason: String = row.try_get("reason")?;
        let saved_actor: String = row.try_get("actor_id")?;
        let version: i64 = row.try_get("version")?;
        if hold_id != command.hold_id.as_str()
            || action != command.action.as_str()
            || reason != command.reason
            || saved_actor != actor_id.as_str()
        {
            return Err(cancellation_conflict().into());
        }
        return Ok(QualityCancellationReceipt {
            id: hold_id.clone(),
            hold_id,
            cancellation_id: cancellation,
            status: match command.action {
                CancellationAction::Request => HoldStatus::CancelPending,
                CancellationAction::Approve => HoldStatus::Cancelled,
                CancellationAction::Reject => HoldStatus::Held,
            },
            version: safe(version)?,
        });
    }
    let row = sqlx::query(
        "SELECT status,version,created_by,cancel_requested_by,last_result,EXISTS(SELECT 1 FROM inventory_quality_inspections i WHERE i.hold_id=h.id) has_inspections FROM inventory_quality_holds h WHERE h.id=$1 AND h.company_id=$2 AND ($3::text IS NULL OR h.branch_id=$3) FOR UPDATE",
    )
    .bind(command.hold_id.as_str())
    .bind(company.as_str())
    .bind(branch.map(RecordId::as_str))
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(hold_not_found)?;
    let facts = QualityHoldFacts {
        id: command.hold_id.as_str().to_owned(),
        status: parse_status(&row.try_get::<String, _>("status")?)?,
        version: safe(row.try_get("version")?)?,
        created_by: row.try_get("created_by")?,
        cancel_requested_by: row.try_get("cancel_requested_by")?,
        last_inspector: None,
        last_result: parse_result(row.try_get("last_result")?)?,
        has_inspections: row.try_get("has_inspections")?,
    };
    if matches!(
        command.action,
        CancellationAction::Approve | CancellationAction::Reject
    ) && (same_person(c, actor_id.as_str(), &facts.created_by, company.as_str()).await?
        || same_person(
            c,
            actor_id.as_str(),
            facts.cancel_requested_by.as_deref().unwrap_or(""),
            company.as_str(),
        )
        .await?)
    {
        return Err(independent_required().into());
    }
    let plan = crate::domain::quality::plan_cancellation(
        &facts,
        actor_id.as_str(),
        command.action,
        command.expected_version,
    )?;
    let updated = match command.action {
        CancellationAction::Request => sqlx::query(
            "UPDATE inventory_quality_holds SET status='cancel_pending',cancel_requested_by=$4,cancel_reason=$5,version=version+1,updated_at=$6 WHERE id=$1 AND company_id=$2 AND version=$3 AND status='held' AND last_result IS NULL",
        )
        .bind(command.hold_id.as_str())
        .bind(company.as_str())
        .bind(command.expected_version)
        .bind(actor_id.as_str())
        .bind(&command.reason)
        .bind(now)
        .execute(&mut *c)
        .await?,
        CancellationAction::Approve => sqlx::query(
            "UPDATE inventory_quality_holds SET status='cancelled',cancelled_by=$4,cancelled_at=$6,version=version+1,updated_at=$6 WHERE id=$1 AND company_id=$2 AND version=$3 AND status='cancel_pending' AND last_result IS NULL",
        )
        .bind(command.hold_id.as_str())
        .bind(company.as_str())
        .bind(command.expected_version)
        .bind(actor_id.as_str())
        .bind(&command.reason)
        .bind(now)
        .execute(&mut *c)
        .await?,
        CancellationAction::Reject => sqlx::query(
            "UPDATE inventory_quality_holds SET status='held',version=version+1,updated_at=$6 WHERE id=$1 AND company_id=$2 AND version=$3 AND status='cancel_pending' AND last_result IS NULL",
        )
        .bind(command.hold_id.as_str())
        .bind(company.as_str())
        .bind(command.expected_version)
        .bind(actor_id.as_str())
        .bind(&command.reason)
        .bind(now)
        .execute(&mut *c)
        .await?,
    };
    if updated.rows_affected() != 1 {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "The Quality hold changed. Reload and review before submitting.",
        )
        .into());
    }
    sqlx::query(
        "INSERT INTO inventory_quality_cancellations(id,hold_id,version,action,reason,actor_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(&cancellation)
    .bind(command.hold_id.as_str())
    .bind(plan.version)
    .bind(command.action.as_str())
    .bind(&command.reason)
    .bind(actor_id.as_str())
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(QualityCancellationReceipt {
        id: command.hold_id.as_str().to_owned(),
        hold_id: command.hold_id.as_str().to_owned(),
        cancellation_id: cancellation,
        status: plan.target,
        version: plan.version,
    })
}
