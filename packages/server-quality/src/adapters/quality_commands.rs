//! Native Quality specification and inspection owner effects.
//!
//! Every prepared value is bound to one serializable Holding transaction and
//! the caller's retained authority/purchase/Workflow witnesses. The adapter
//! never mints permission, purchase admission or a command receipt.
use super::quality_read::{
    Unit, read_holds, read_specification_metrics, safe, same_person, serializable, unit,
};
use crate::application::quality::{QualityInspection, QualityMeasurement};
use crate::application::quality_commands::{
    QualityInspectionCommand, QualityInspectionReceipt, QualitySpecAction,
    QualitySpecActionCommand, QualitySpecCommand, QualitySpecMetricInput, QualitySpecReceipt,
};
use crate::domain::quality::QualityMetric;
use crate::domain::quality::{
    HoldStatus, InspectionResult, SpecStatus, evaluate_measurements, quality_measurement_passed,
};
use erpx_server_files::public::quality_inspection_evidence::{
    CurrentQualityInspectionEvidence, CurrentQualityInspectionEvidenceSets,
    inspect_quality_inspection_evidence, inspect_quality_inspection_evidence_sets,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const MAX_SPEC_METRICS: usize = 50;
const MAX_INSPECTION_MEASUREMENTS: usize = 100;
const MAX_INSPECTION_ATTACHMENTS: usize = 10;

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}

fn spec_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_SPEC_NOT_FOUND",
        "Inspection specification not found.",
    )
}

fn version_required() -> BusinessError {
    invalid(
        "VERSION_REQUIRED",
        "Refresh the draft specification before editing.",
    )
}

fn spec_changed() -> BusinessError {
    conflict(
        "QUALITY_SPEC_CHANGED",
        "The specification changed. Reload and review before submitting.",
    )
}

fn hold_changed() -> BusinessError {
    conflict(
        "CONFLICT",
        "The Quality hold changed. Reload and review before submitting.",
    )
}

fn independent_spec_approval() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INDEPENDENT_SPEC_APPROVAL_REQUIRED",
        "Another Company Quality manager must activate this specification.",
    )
}

fn independent_inspector() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INDEPENDENT_INSPECTOR_REQUIRED",
        "Another person must inspect the held stock.",
    )
}

fn metrics_required() -> BusinessError {
    conflict(
        "QUALITY_METRICS_REQUIRED",
        "Add at least one measurement before activation.",
    )
}

fn cancellation_pending() -> BusinessError {
    conflict(
        "QUALITY_CANCELLATION_PENDING",
        "Resolve the independent cancellation review before inspecting this hold.",
    )
}

fn hold_released() -> BusinessError {
    conflict(
        "QUALITY_HOLD_RELEASED",
        "Create a new hold for a new inspection.",
    )
}

fn evidence_invalid() -> BusinessError {
    invalid(
        "QUALITY_ATTACHMENT_INVALID",
        "Choose ready, unbound inspection evidence uploaded by the current inspector.",
    )
}

fn evidence_changed() -> BusinessError {
    conflict(
        "QUALITY_ATTACHMENT_CHANGED",
        "The inspection evidence changed. Reload and review before retrying.",
    )
}

/// Writer intent for the Quality-owned command relations. Composition must
/// acquire this before the first snapshot together with the Commercial locks.
pub async fn lock_quality_command_sources(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("QUALITY_COMMAND_TRANSACTION_REQUIRED").into());
    }
    // The hold relation is the common writer entry point for cancellation,
    // inspection and read sources. Always acquire it first so their combined
    // command units cannot invert the table-lock order.
    sqlx::query(
        "LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_measurements,inventory_quality_inspection_attachments IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "LOCK TABLE inventory_quality_specs,inventory_quality_spec_metrics IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn require_quality_command_locks(c: &mut PgConnection) -> Result<(), Error> {
    serializable(c).await?;
    let held: bool = sqlx::query_scalar("SELECT count(DISTINCT relation)=6 FROM pg_locks WHERE pid=pg_backend_pid() AND locktype='relation' AND granted AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND relation IN ('inventory_quality_specs'::regclass,'inventory_quality_spec_metrics'::regclass,'inventory_quality_holds'::regclass,'inventory_quality_inspections'::regclass,'inventory_quality_measurements'::regclass,'inventory_quality_inspection_attachments'::regclass)")
        .fetch_one(&mut *c)
        .await?;
    if !held {
        return Err(PersistenceError::new("QUALITY_COMMAND_LOCKS_REQUIRED").into());
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualitySpecRecord {
    pub id: String,
    pub company_id: String,
    pub product_id: String,
    pub name: String,
    pub status: SpecStatus,
    pub version: i64,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub approval_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub activated_at: Option<i64>,
}

async fn read_spec_record(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<QualitySpecRecord>, Error> {
    let row = sqlx::query("SELECT id,company_id,product_id,name,status,version,created_by,approved_by,approval_reason,created_at,updated_at,activated_at FROM inventory_quality_specs WHERE id=$1 AND company_id=$2 FOR UPDATE")
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *c)
        .await?;
    row.map(|row| {
        Ok(QualitySpecRecord {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            product_id: row.try_get("product_id")?,
            name: row.try_get("name")?,
            status: SpecStatus::parse(&row.try_get::<String, _>("status")?).ok_or_else(|| {
                invalid(
                    "QUALITY_SPEC_INVALID",
                    "The specification status is invalid.",
                )
            })?,
            version: safe(row.try_get("version")?)?,
            created_by: row.try_get("created_by")?,
            approved_by: row.try_get("approved_by")?,
            approval_reason: row.try_get("approval_reason")?,
            created_at: safe(row.try_get("created_at")?)?,
            updated_at: safe(row.try_get("updated_at")?)?,
            activated_at: row
                .try_get::<Option<i64>, _>("activated_at")?
                .map(safe)
                .transpose()?,
        })
    })
    .transpose()
}

async fn read_active_spec(
    c: &mut PgConnection,
    company: &CompanyId,
    product_id: &str,
) -> Result<Option<QualitySpecRecord>, Error> {
    let row = sqlx::query("SELECT id,company_id,product_id,name,status,version,created_by,approved_by,approval_reason,created_at,updated_at,activated_at FROM inventory_quality_specs WHERE company_id=$1 AND product_id=$2 AND status='active' FOR UPDATE")
        .bind(company.as_str())
        .bind(product_id)
        .fetch_optional(&mut *c)
        .await?;
    row.map(|row| {
        Ok(QualitySpecRecord {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            product_id: row.try_get("product_id")?,
            name: row.try_get("name")?,
            status: SpecStatus::Active,
            version: safe(row.try_get("version")?)?,
            created_by: row.try_get("created_by")?,
            approved_by: row.try_get("approved_by")?,
            approval_reason: row.try_get("approval_reason")?,
            created_at: safe(row.try_get("created_at")?)?,
            updated_at: safe(row.try_get("updated_at")?)?,
            activated_at: row
                .try_get::<Option<i64>, _>("activated_at")?
                .map(safe)
                .transpose()?,
        })
    })
    .transpose()
}

async fn spec_metrics(
    c: &mut PgConnection,
    company: &CompanyId,
    spec_id: &str,
) -> Result<Vec<QualityMetric>, Error> {
    read_specification_metrics(c, company.as_str(), Some(spec_id)).await
}

fn expected_metrics(command: &QualitySpecCommand, record: &RecordId) -> Vec<QualityMetric> {
    command
        .input
        .metrics
        .iter()
        .enumerate()
        .map(
            |(index, metric): (usize, &QualitySpecMetricInput)| QualityMetric {
                id: format!("{}-metric-{}", record.as_str(), index),
                code: metric.code.clone(),
                label: metric.label.clone(),
                unit: metric.unit.clone(),
                minimum: metric.minimum.clone(),
                maximum: metric.maximum.clone(),
                ordinal: index as i64,
            },
        )
        .collect()
}

/// Prepare a draft insert/update against the current scoped row.
pub struct PreparedQualitySpecSave {
    unit: Unit,
    company: CompanyId,
    source: Option<QualitySpecRecord>,
    pre_metrics: Vec<QualityMetric>,
    expected: QualitySpecRecord,
    expected_metrics: Vec<QualityMetric>,
    receipt: QualitySpecReceipt,
}

impl PreparedQualitySpecSave {
    pub fn receipt(&self) -> &QualitySpecReceipt {
        &self.receipt
    }

    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_command_locks(c).await?;
        if unit(c).await? != self.unit {
            return Err(spec_changed().into());
        }
        if read_spec_record(c, &self.company, &self.expected.id).await? != self.source
            || match &self.source {
                Some(source) => spec_metrics(c, &self.company, &source.id).await?,
                None => Vec::new(),
            } != self.pre_metrics
        {
            return Err(spec_changed().into());
        }
        Ok(())
    }

    pub async fn apply(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_before(c).await?;
        if let Some(source) = &self.source {
            sqlx::query("DELETE FROM inventory_quality_spec_metrics WHERE spec_id=$1")
                .bind(&source.id)
                .execute(&mut *c)
                .await?;
            let updated = sqlx::query("UPDATE inventory_quality_specs SET name=$2,version=$3,updated_at=$4 WHERE id=$1 AND company_id=$5 AND version=$6 AND status='draft'")
                .bind(&source.id)
                .bind(&self.expected.name)
                .bind(self.expected.version)
                .bind(self.expected.updated_at)
                .bind(self.company.as_str())
                .bind(source.version)
                .execute(&mut *c)
                .await?;
            if updated.rows_affected() != 1 {
                return Err(spec_changed().into());
            }
        } else {
            sqlx::query("INSERT INTO inventory_quality_specs(id,company_id,product_id,name,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7)")
                .bind(&self.expected.id)
                .bind(&self.expected.company_id)
                .bind(&self.expected.product_id)
                .bind(&self.expected.name)
                .bind(&self.expected.created_by)
                .bind(self.expected.created_at)
                .bind(self.expected.updated_at)
                .execute(&mut *c)
                .await?;
        }
        for metric in &self.expected_metrics {
            sqlx::query("INSERT INTO inventory_quality_spec_metrics(id,spec_id,code,label,unit,minimum_value,maximum_value,ordinal) VALUES($1,$2,$3,$4,$5,$6::numeric,$7::numeric,$8)")
                .bind(&metric.id)
                .bind(&self.expected.id)
                .bind(&metric.code)
                .bind(&metric.label)
                .bind(&metric.unit)
                .bind(&metric.minimum)
                .bind(&metric.maximum)
                .bind(metric.ordinal)
                .execute(&mut *c)
                .await?;
        }
        self.require_after(c).await
    }

    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_command_locks(c).await?;
        if unit(c).await? != self.unit
            || read_spec_record(c, &self.company, &self.expected.id)
                .await?
                .as_ref()
                != Some(&self.expected)
            || spec_metrics(c, &self.company, &self.expected.id).await? != self.expected_metrics
        {
            return Err(spec_changed().into());
        }
        Ok(())
    }
}

pub async fn prepare_quality_spec_save(
    c: &mut PgConnection,
    command: &QualitySpecCommand,
    company: &CompanyId,
    actor: &UserId,
    now: i64,
) -> Result<PreparedQualitySpecSave, Error> {
    require_quality_command_locks(c).await?;
    let command =
        crate::application::quality_commands::normalize_quality_spec_save(command.clone())?;
    if command.input.metrics.len() > MAX_SPEC_METRICS {
        return Err(invalid(
            "QUALITY_METRICS_REQUIRED",
            "Add at most fifty measurements.",
        )
        .into());
    }
    let record = command.record_id()?;
    let source = read_spec_record(c, company, record.as_str()).await?;
    if command.id.is_some() && source.is_none() {
        return Err(spec_not_found().into());
    }
    if let Some(source) = &source {
        if source.status != SpecStatus::Draft {
            return Err(conflict(
                "QUALITY_SPEC_LOCKED",
                "Create a new specification after activation.",
            )
            .into());
        }
        if source.product_id != command.input.product_id.as_str() {
            return Err(invalid(
                "QUALITY_SPEC_PRODUCT_LOCKED",
                "Create a separate specification for another product.",
            )
            .into());
        }
        let expected = command
            .input
            .expected_version
            .ok_or_else(version_required)?;
        if expected != source.version {
            return Err(spec_changed().into());
        }
    } else if command.id.is_none() && source.is_some() {
        return Err(spec_changed().into());
    }
    let pre_metrics = match &source {
        Some(source) => spec_metrics(c, company, &source.id).await?,
        None => Vec::new(),
    };
    let expected_metrics = expected_metrics(&command, &record);
    let next = source.as_ref().map_or(1, |source| source.version + 1);
    let expected = QualitySpecRecord {
        id: record.as_str().to_owned(),
        company_id: company.as_str().to_owned(),
        product_id: command.input.product_id.as_str().to_owned(),
        name: command.input.name.clone(),
        status: SpecStatus::Draft,
        version: next,
        created_by: source
            .as_ref()
            .map(|source| source.created_by.clone())
            .unwrap_or_else(|| actor.as_str().to_owned()),
        approved_by: None,
        approval_reason: None,
        created_at: source.as_ref().map_or(now, |source| source.created_at),
        updated_at: now,
        activated_at: None,
    };
    Ok(PreparedQualitySpecSave {
        unit: unit(c).await?,
        company: company.clone(),
        source,
        pre_metrics,
        receipt: QualitySpecReceipt {
            id: expected.id.clone(),
            status: expected.status,
            version: expected.version,
        },
        expected,
        expected_metrics,
    })
}

/// Prepare an activate/cancel decision against the current draft.
pub struct PreparedQualitySpecTransition {
    unit: Unit,
    company: CompanyId,
    source: QualitySpecRecord,
    pre_metrics: Vec<QualityMetric>,
    previous_active: Option<QualitySpecRecord>,
    expected: QualitySpecRecord,
    expected_active: Option<QualitySpecRecord>,
    receipt: QualitySpecReceipt,
}

impl PreparedQualitySpecTransition {
    pub fn receipt(&self) -> &QualitySpecReceipt {
        &self.receipt
    }

    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_command_locks(c).await?;
        if unit(c).await? != self.unit
            || read_spec_record(c, &self.company, &self.source.id)
                .await?
                .as_ref()
                != Some(&self.source)
            || spec_metrics(c, &self.company, &self.source.id).await? != self.pre_metrics
            || read_active_spec(c, &self.company, &self.source.product_id).await?
                != self.previous_active
        {
            return Err(spec_changed().into());
        }
        Ok(())
    }

    pub async fn apply(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_before(c).await?;
        if let Some(active) = &self.previous_active {
            let superseded = sqlx::query("UPDATE inventory_quality_specs SET status='superseded',version=$2,updated_at=$3 WHERE id=$1 AND company_id=$4 AND version=$5 AND status='active'")
                .bind(&active.id)
                .bind(active.version + 1)
                .bind(self.expected.updated_at)
                .bind(self.company.as_str())
                .bind(active.version)
                .execute(&mut *c)
                .await?;
            if superseded.rows_affected() != 1 {
                return Err(spec_changed().into());
            }
        }
        let updated = match self.expected.status {
            SpecStatus::Active => sqlx::query("UPDATE inventory_quality_specs SET status='active',approved_by=$2,approval_reason=$3,activated_at=$4,version=$5,updated_at=$6 WHERE id=$1 AND company_id=$7 AND version=$8 AND status='draft'")
                .bind(&self.expected.id)
                .bind(&self.expected.approved_by)
                .bind(&self.expected.approval_reason)
                .bind(self.expected.activated_at)
                .bind(self.expected.version)
                .bind(self.expected.updated_at)
                .bind(self.company.as_str())
                .bind(self.source.version)
                .execute(&mut *c)
                .await?,
            SpecStatus::Cancelled => sqlx::query("UPDATE inventory_quality_specs SET status='cancelled',approval_reason=$2,version=$3,updated_at=$4 WHERE id=$1 AND company_id=$5 AND version=$6 AND status='draft'")
                .bind(&self.expected.id)
                .bind(&self.expected.approval_reason)
                .bind(self.expected.version)
                .bind(self.expected.updated_at)
                .bind(self.company.as_str())
                .bind(self.source.version)
                .execute(&mut *c)
                .await?,
            _ => return Err(spec_changed().into()),
        };
        if updated.rows_affected() != 1 {
            return Err(spec_changed().into());
        }
        self.require_after(c).await
    }

    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_command_locks(c).await?;
        if unit(c).await? != self.unit
            || read_spec_record(c, &self.company, &self.expected.id)
                .await?
                .as_ref()
                != Some(&self.expected)
            || spec_metrics(c, &self.company, &self.expected.id).await? != self.pre_metrics
            || read_active_spec(c, &self.company, &self.source.product_id).await?
                != self.expected_active
        {
            return Err(spec_changed().into());
        }
        Ok(())
    }
}

pub async fn prepare_quality_spec_transition(
    c: &mut PgConnection,
    command: &QualitySpecActionCommand,
    company: &CompanyId,
    actor: &UserId,
    now: i64,
) -> Result<PreparedQualitySpecTransition, Error> {
    require_quality_command_locks(c).await?;
    let command =
        crate::application::quality_commands::normalize_quality_spec_action(command.clone())?;
    let source = read_spec_record(c, company, command.id.as_str())
        .await?
        .ok_or_else(spec_not_found)?;
    if source.status != SpecStatus::Draft {
        return Err(conflict(
            "QUALITY_SPEC_LOCKED",
            "Only a draft specification can be activated or cancelled.",
        )
        .into());
    }
    if source.version != command.expected_version {
        return Err(spec_changed().into());
    }
    let pre_metrics = spec_metrics(c, company, &source.id).await?;
    if pre_metrics.is_empty() {
        return Err(metrics_required().into());
    }
    if command.action == QualitySpecAction::Activate
        && same_person(c, actor.as_str(), &source.created_by, company.as_str()).await?
    {
        return Err(independent_spec_approval().into());
    }
    let previous_active = read_active_spec(c, company, &source.product_id).await?;
    let next = source.version + 1;
    let expected = QualitySpecRecord {
        id: source.id.clone(),
        company_id: source.company_id.clone(),
        product_id: source.product_id.clone(),
        name: source.name.clone(),
        status: match command.action {
            QualitySpecAction::Activate => SpecStatus::Active,
            QualitySpecAction::Cancel => SpecStatus::Cancelled,
        },
        version: next,
        created_by: source.created_by.clone(),
        approved_by: match command.action {
            QualitySpecAction::Activate => Some(actor.as_str().to_owned()),
            QualitySpecAction::Cancel => None,
        },
        approval_reason: Some(command.reason.clone()),
        created_at: source.created_at,
        updated_at: now,
        activated_at: match command.action {
            QualitySpecAction::Activate => Some(now),
            QualitySpecAction::Cancel => None,
        },
    };
    let expected_active = match command.action {
        // After activation the target row is the current active specification.
        QualitySpecAction::Activate => Some(expected.clone()),
        // A cancelled draft leaves the previously active row unchanged.
        QualitySpecAction::Cancel => previous_active.clone(),
    };
    Ok(PreparedQualitySpecTransition {
        unit: unit(c).await?,
        company: company.clone(),
        receipt: QualitySpecReceipt {
            id: expected.id.clone(),
            status: expected.status,
            version: expected.version,
        },
        source,
        pre_metrics,
        previous_active,
        expected,
        expected_active,
    })
}

async fn read_inspection(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<Option<QualityInspection>, Error> {
    let row = sqlx::query("SELECT i.id,i.hold_id,i.reviewed_version,i.result,i.evidence,i.inspected_by,i.created_at,i.spec_id,i.spec_version FROM inventory_quality_inspections i JOIN inventory_quality_holds h ON h.id=i.hold_id WHERE i.id=$1 AND h.company_id=$2 FOR SHARE OF i")
        .bind(id)
        .bind(company.as_str())
        .fetch_optional(&mut *c)
        .await?;
    row.map(|row| {
        Ok(QualityInspection {
            id: row.try_get("id")?,
            hold_id: row.try_get("hold_id")?,
            reviewed_version: safe(row.try_get("reviewed_version")?)?,
            result: InspectionResult::parse(&row.try_get::<String, _>("result")?).ok_or_else(
                || {
                    invalid(
                        "QUALITY_INSPECTION_INVALID",
                        "The inspection result is invalid.",
                    )
                },
            )?,
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
    .transpose()
}

async fn read_measurements(
    c: &mut PgConnection,
    inspection_id: &str,
) -> Result<Vec<QualityMeasurement>, Error> {
    let rows = sqlx::query("SELECT id,inspection_id,metric_id,code,label,unit,minimum_value::text minimum,maximum_value::text maximum,measured_value::text measured_value,passed,ordinal FROM inventory_quality_measurements WHERE inspection_id=$1 ORDER BY ordinal LIMIT 101")
        .bind(inspection_id)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > MAX_INSPECTION_MEASUREMENTS {
        return Err(invalid(
            "QUALITY_MEASUREMENTS_REQUIRED",
            "The inspection has too many measurements.",
        )
        .into());
    }
    rows.iter()
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
        .collect()
}

async fn read_inspection_attachments(
    c: &mut PgConnection,
    inspection_id: &str,
) -> Result<Vec<(String, i64)>, Error> {
    let rows = sqlx::query("SELECT attachment_id,ordinal FROM inventory_quality_inspection_attachments WHERE inspection_id=$1 ORDER BY ordinal LIMIT 11")
        .bind(inspection_id)
        .fetch_all(&mut *c)
        .await?;
    if rows.len() > 10 {
        return Err(evidence_invalid().into());
    }
    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("attachment_id")?,
                safe(row.try_get("ordinal")?)?,
            ))
        })
        .collect()
}

async fn evidence_in_use(c: &mut PgConnection, ids: &[String]) -> Result<bool, Error> {
    if ids.is_empty() {
        return Ok(false);
    }
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM inventory_quality_inspection_attachments WHERE attachment_id=ANY($1::text[]))",
    )
    .bind(ids)
    .fetch_one(&mut *c)
    .await?)
}

fn evidence_error(error: Error) -> Error {
    match error {
        PgTransactionError::Business(_) => evidence_changed().into(),
        other => other,
    }
}

/// Prepare immutable inspection and measurement rows against the current hold.
pub struct PreparedQualityInspection {
    unit: Unit,
    company: CompanyId,
    branch: Option<RecordId>,
    source: crate::application::quality::QualityHold,
    metrics: Vec<QualityMetric>,
    evidence: Option<CurrentQualityInspectionEvidence>,
    evidence_sets: Option<CurrentQualityInspectionEvidenceSets>,
    expected_attachments: Vec<(String, i64)>,
    expected_hold: crate::application::quality::QualityHold,
    expected_inspection: QualityInspection,
    expected_measurements: Vec<QualityMeasurement>,
    receipt: QualityInspectionReceipt,
}

impl PreparedQualityInspection {
    pub fn receipt(&self) -> &QualityInspectionReceipt {
        &self.receipt
    }

    pub fn hold(&self) -> &crate::application::quality::QualityHold {
        &self.expected_hold
    }

    async fn require_evidence(&self, c: &mut PgConnection) -> Result<(), Error> {
        if let Some(evidence) = &self.evidence {
            evidence.require_current(c).await.map_err(evidence_error)?;
        }
        if let Some(sets) = &self.evidence_sets {
            sets.require_current(c).await.map_err(evidence_error)?;
        }
        Ok(())
    }

    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_command_locks(c).await?;
        if unit(c).await? != self.unit
            || read_holds(
                c,
                self.company.as_str(),
                self.branch.as_ref().map(RecordId::as_str),
                Some(&self.source.id),
            )
            .await?
            .pop()
            .as_ref()
                != Some(&self.source)
            || match self.source.spec_id.as_deref() {
                Some(spec_id) => spec_metrics(c, &self.company, spec_id).await?,
                None => Vec::new(),
            } != self.metrics
            || read_inspection(c, &self.company, &self.receipt.inspection_id)
                .await?
                .is_some()
            || !read_inspection_attachments(c, &self.receipt.inspection_id)
                .await?
                .is_empty()
        {
            return Err(hold_changed().into());
        }
        if !self.expected_attachments.is_empty()
            && evidence_in_use(
                c,
                &self
                    .expected_attachments
                    .iter()
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>(),
            )
            .await?
        {
            return Err(evidence_changed().into());
        }
        self.require_evidence(c).await?;
        Ok(())
    }

    pub async fn apply(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_before(c).await?;
        sqlx::query("INSERT INTO inventory_quality_inspections(id,hold_id,reviewed_version,result,evidence,inspected_by,created_at,spec_id,spec_version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&self.expected_inspection.id)
            .bind(&self.expected_inspection.hold_id)
            .bind(self.expected_inspection.reviewed_version)
            .bind(self.expected_inspection.result.as_str())
            .bind(&self.expected_inspection.evidence)
            .bind(&self.expected_inspection.inspected_by)
            .bind(self.expected_inspection.created_at)
            .bind(&self.expected_inspection.spec_id)
            .bind(self.expected_inspection.spec_version)
            .execute(&mut *c)
            .await?;
        for (attachment_id, ordinal) in &self.expected_attachments {
            let inserted = sqlx::query(
                "INSERT INTO inventory_quality_inspection_attachments(inspection_id,attachment_id,ordinal) VALUES($1,$2,$3)",
            )
            .bind(&self.expected_inspection.id)
            .bind(attachment_id)
            .bind(ordinal)
            .execute(&mut *c)
            .await?;
            if inserted.rows_affected() != 1 {
                return Err(evidence_changed().into());
            }
        }
        for measurement in &self.expected_measurements {
            sqlx::query("INSERT INTO inventory_quality_measurements(id,inspection_id,metric_id,code,label,unit,minimum_value,maximum_value,measured_value,passed,ordinal) VALUES($1,$2,$3,$4,$5,$6,$7::numeric,$8::numeric,$9::numeric,$10,$11)")
                .bind(&measurement.id)
                .bind(&measurement.inspection_id)
                .bind(&measurement.metric_id)
                .bind(&measurement.code)
                .bind(&measurement.label)
                .bind(&measurement.unit)
                .bind(&measurement.minimum)
                .bind(&measurement.maximum)
                .bind(&measurement.measured_value)
                .bind(if measurement.passed { 1_i64 } else { 0_i64 })
                .bind(measurement.ordinal)
                .execute(&mut *c)
                .await?;
        }
        let updated = sqlx::query("UPDATE inventory_quality_holds SET status=$4,last_result=$5,last_inspector=$6,version=$7,updated_at=$8 WHERE id=$1 AND company_id=$2 AND version=$3 AND status=$9")
            .bind(&self.source.id)
            .bind(self.company.as_str())
            .bind(self.source.version)
            .bind(self.receipt.status.as_str())
            .bind(self.receipt.result.as_str())
            .bind(&self.expected_inspection.inspected_by)
            .bind(self.receipt.version)
            .bind(self.expected_hold.updated_at)
            .bind(self.source.status.as_str())
            .execute(&mut *c)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(hold_changed().into());
        }
        self.require_after(c).await
    }

    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_quality_command_locks(c).await?;
        if unit(c).await? != self.unit
            || read_holds(
                c,
                self.company.as_str(),
                self.branch.as_ref().map(RecordId::as_str),
                Some(&self.source.id),
            )
            .await?
            .pop()
            .as_ref()
                != Some(&self.expected_hold)
            || match self.source.spec_id.as_deref() {
                Some(spec_id) => spec_metrics(c, &self.company, spec_id).await?,
                None => Vec::new(),
            } != self.metrics
            || read_inspection(c, &self.company, &self.receipt.inspection_id)
                .await?
                .as_ref()
                != Some(&self.expected_inspection)
            || read_inspection_attachments(c, &self.receipt.inspection_id).await?
                != self.expected_attachments
            || read_measurements(c, &self.receipt.inspection_id).await?
                != self.expected_measurements
        {
            return Err(hold_changed().into());
        }
        self.require_evidence(c).await?;
        Ok(())
    }
}

pub async fn prepare_quality_inspection(
    c: &mut PgConnection,
    command: &QualityInspectionCommand,
    company: &CompanyId,
    branch: Option<&RecordId>,
    actor: &UserId,
    now: i64,
) -> Result<PreparedQualityInspection, Error> {
    require_quality_command_locks(c).await?;
    let command =
        crate::application::quality_commands::normalize_quality_inspection(command.clone())?;
    let source = read_holds(
        c,
        company.as_str(),
        branch.map(RecordId::as_str),
        Some(command.hold_id.as_str()),
    )
    .await?
    .pop()
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "QUALITY_HOLD_NOT_FOUND",
            "Quality hold not found.",
        )
    })?;
    if same_person(c, actor.as_str(), &source.created_by, company.as_str()).await? {
        return Err(independent_inspector().into());
    }
    if source.status == HoldStatus::CancelPending {
        return Err(cancellation_pending().into());
    }
    if !source.status.is_inspectable() {
        return Err(hold_released().into());
    }
    if source.version != command.expected_version {
        return Err(hold_changed().into());
    }
    let (evidence, evidence_sets, expected_attachments) =
        if command.attachment_ids.is_empty() && command.attachment_set_ids.is_empty() {
            (None, None, Vec::new())
        } else {
            let hold_branch = RecordId::new(source.branch_id.clone())?;
            let mut attachment_ids = command.attachment_ids.clone();
            let evidence_sets = if command.attachment_set_ids.is_empty() {
                None
            } else {
                match inspect_quality_inspection_evidence_sets(
                    c,
                    company,
                    &hold_branch,
                    &source.id,
                    command.expected_version,
                    &command.attachment_set_ids,
                )
                .await
                {
                    Ok(Some(sets)) => {
                        for attachment_id in sets.attachment_ids() {
                            if !attachment_ids.contains(attachment_id) {
                                attachment_ids.push(attachment_id.clone());
                            }
                        }
                        Some(sets)
                    }
                    Ok(None) | Err(PgTransactionError::Business(_)) => {
                        return Err(evidence_invalid().into());
                    }
                    Err(error) => return Err(error),
                }
            };
            if attachment_ids.is_empty() || attachment_ids.len() > MAX_INSPECTION_ATTACHMENTS {
                return Err(evidence_invalid().into());
            }
            if evidence_in_use(c, &attachment_ids).await? {
                return Err(evidence_invalid().into());
            }
            let evidence = match inspect_quality_inspection_evidence(
                c,
                company,
                &hold_branch,
                &source.id,
                actor.as_str(),
                &attachment_ids,
            )
            .await
            {
                Ok(Some(evidence)) => evidence,
                Ok(None) | Err(PgTransactionError::Business(_)) => {
                    return Err(evidence_invalid().into());
                }
                Err(error) => return Err(error),
            };
            let expected = attachment_ids
                .iter()
                .enumerate()
                .map(|(ordinal, attachment_id)| (attachment_id.clone(), ordinal as i64))
                .collect();
            (Some(evidence), evidence_sets, expected)
        };
    let metrics = match source.spec_id.as_deref() {
        Some(spec_id) => spec_metrics(c, company, spec_id).await?,
        None => Vec::new(),
    };
    let evaluation = evaluate_measurements(&metrics, &command.measurements, command.result)?;
    let inspection_id = format!("inspection-{}", command.key.as_str());
    let result = evaluation.result;
    let status = if result == InspectionResult::Pass {
        HoldStatus::AwaitingRelease
    } else {
        HoldStatus::Held
    };
    let by_metric: std::collections::BTreeMap<_, _> = metrics
        .iter()
        .map(|metric| (metric.id.as_str(), metric))
        .collect();
    let expected_measurements = metrics
        .iter()
        .enumerate()
        .map(|(index, metric)| {
            let value = evaluation
                .provided
                .get(&metric.id)
                .expect("evaluated metric");
            QualityMeasurement {
                id: format!("{}-metric-{}", inspection_id, index),
                inspection_id: inspection_id.clone(),
                metric_id: metric.id.clone(),
                code: metric.code.clone(),
                label: metric.label.clone(),
                unit: metric.unit.clone(),
                minimum: metric.minimum.clone(),
                maximum: metric.maximum.clone(),
                measured_value: value.clone(),
                passed: quality_measurement_passed(metric, value),
                ordinal: metric.ordinal,
            }
        })
        .collect::<Vec<_>>();
    // Metrics are required by evaluate_measurements when the hold has a spec;
    // keep the lookup local to make an inconsistent caller fail closed.
    if by_metric.len() != metrics.len() {
        return Err(invalid(
            "QUALITY_SPEC_INVALID",
            "The specification metrics are ambiguous.",
        )
        .into());
    }
    let expected_inspection = QualityInspection {
        id: inspection_id.clone(),
        hold_id: source.id.clone(),
        reviewed_version: source.version,
        result,
        evidence: command.evidence.clone(),
        inspected_by: actor.as_str().to_owned(),
        created_at: now,
        spec_id: source.spec_id.clone(),
        spec_version: source.spec_version,
    };
    let expected_hold = crate::application::quality::QualityHold {
        status,
        version: source.version + 1,
        last_result: Some(result),
        last_inspector: Some(actor.as_str().to_owned()),
        updated_at: now,
        inspection_count: source.inspection_count + 1,
        ..source.clone()
    };
    let receipt = QualityInspectionReceipt {
        id: expected_hold.id.clone(),
        inspection_id,
        status,
        result,
        version: expected_hold.version,
    };
    Ok(PreparedQualityInspection {
        unit: unit(c).await?,
        company: company.clone(),
        branch: branch.cloned(),
        source,
        metrics,
        evidence,
        evidence_sets,
        expected_attachments,
        expected_hold,
        expected_inspection,
        expected_measurements,
        receipt,
    })
}
