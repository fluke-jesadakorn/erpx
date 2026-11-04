//! Native Quality PostgreSQL read and cancellation adapters.
//!
//! Only Quality-owned relations are read or written. Cross-owner labels,
//! purchase admission and Workflow receipt/outbox retention remain
//! composition/public-port concerns.
use crate::application::quality::{
    QualityAlert, QualityCancellation, QualityCancellationCommand, QualityCancellationReceipt,
    QualityCancellationRepository, QualityDetailData, QualityDisposition, QualityFuture,
    QualityHold, QualityInspection, QualityMeasurement, QualityReadRepository,
    QualityWorkspaceData,
};
use crate::domain::quality::{
    CancellationAction, HoldStatus, InspectionResult, QualityHoldFacts, QualityMetric,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use sqlx::{Connection, PgConnection, Row};

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
async fn same_person(
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

async fn serializable(c: &mut PgConnection) -> Result<(), Error> {
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
struct Unit {
    database: String,
    backend: i32,
    xid: String,
    postmaster: String,
}

async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
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

fn parse_status(value: &str) -> Result<HoldStatus, Error> {
    HoldStatus::parse(value).ok_or_else(|| invalid("A Quality hold status is invalid.").into())
}

fn parse_result(value: Option<String>) -> Result<Option<InspectionResult>, Error> {
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

fn safe(value: i64) -> Result<i64, Error> {
    if (0..=crate::domain::quality::MAX_SAFE_INTEGER).contains(&value) {
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

async fn read_holds(
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

async fn read_specification_metrics(
    c: &mut PgConnection,
    company: &str,
    spec_id: Option<&str>,
) -> Result<Vec<QualityMetric>, Error> {
    let sql = "SELECT m.id,m.code,m.label,m.unit,m.minimum_value::text minimum,m.maximum_value::text maximum,m.ordinal FROM inventory_quality_spec_metrics m JOIN inventory_quality_specs s ON s.id=m.spec_id WHERE s.company_id=$1 AND ($2::text IS NULL OR m.spec_id=$2) ORDER BY m.spec_id,m.ordinal LIMIT 1001";
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
        serializable(c).await?;
        if unit(c).await? != self.unit {
            return Err(changed().into());
        }
        if read_holds(c, &self.company, self.branch.as_deref(), None).await? != self.data.holds {
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
    serializable(c).await?;
    let data = QualityWorkspaceData {
        holds: read_holds(c, company.as_str(), branch.map(RecordId::as_str), None).await?,
        specification_metrics: read_specification_metrics(c, company.as_str(), None).await?,
        alerts: read_alerts(c, company.as_str(), None).await?,
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
        serializable(c).await?;
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

async fn read_detail(
    c: &mut PgConnection,
    company: &str,
    branch: Option<&str>,
    id: &str,
) -> Result<Option<QualityDetailData>, Error> {
    let mut hold_rows = read_holds(c, company, branch, Some(id)).await?;
    let Some(hold) = hold_rows.pop() else {
        return Ok(None);
    };
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
    let cancellation_rows = sqlx::query(
        "SELECT id,hold_id,version,action,reason,actor_id,created_at FROM inventory_quality_cancellations WHERE hold_id=$1 ORDER BY version DESC LIMIT 201",
    )
    .bind(id)
    .fetch_all(&mut *c)
    .await?;
    if cancellation_rows.len() > MAX_CANCELLATIONS {
        return Err(bounded("cancellations").into());
    }
    let cancellations = cancellation_rows
        .iter()
        .map(|row| {
            Ok(QualityCancellation {
                id: row.try_get("id")?,
                hold_id: row.try_get("hold_id")?,
                version: safe(row.try_get("version")?)?,
                action: parse_action(&row.try_get::<String, _>("action")?)?,
                reason: row.try_get("reason")?,
                actor_id: row.try_get("actor_id")?,
                created_at: safe(row.try_get("created_at")?)?,
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
    Ok(Some(QualityDetailData {
        spec_metrics: read_specification_metrics(c, company, hold.spec_id.as_deref()).await?,
        alerts: read_alerts(c, company, Some(id)).await?,
        hold,
        inspections,
        measurements,
        cancellations,
        dispositions,
    }))
}

pub async fn read_quality_detail(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    id: &str,
) -> Result<Option<CurrentQualityDetail>, Error> {
    serializable(c).await?;
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
    let cancellation = format!("quality-cancel-{}", command.command_key.as_str());
    if let Some(row) = sqlx::query(
        "SELECT hold_id,action,reason,actor_id,version FROM inventory_quality_cancellations WHERE id=$1",
    )
    .bind(&cancellation)
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
