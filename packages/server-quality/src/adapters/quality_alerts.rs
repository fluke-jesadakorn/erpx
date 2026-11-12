//! Native Quality alert acknowledgement owner effect.
//!
//! The alert row is the serializable owner unit. Company/branch scope, the
//! exact reviewed version, the unacknowledged source, terminal hold exclusion
//! and the actor's current eligible recipient fact are all re-read from the
//! actual Quality/Identity sources in the caller's transaction.
use super::quality_read::{Unit, safe, serializable, unit};
use crate::application::quality_alerts::{
    QualityAlertAcknowledgeCommand, QualityAlertAcknowledgementReceipt,
    normalize_quality_alert_acknowledge,
};
use crate::domain::quality::HoldStatus;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use sqlx::{PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

fn invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_ALERT_INVALID",
        "The Quality escalation source is invalid.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "QUALITY_ALERT_NOT_FOUND",
        "Quality escalation is not available in this scope.",
    )
}

fn acknowledged() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_ALERT_ACKNOWLEDGED",
        "This escalation has already been acknowledged.",
    )
}

/// The retained owner assertion failure is a generic command conflict, not a
/// new authority error: a stale version, terminal hold or withdrawn recipient
/// all refuse the same unit after refresh.
fn assertion_failed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CONFLICT",
        "The record changed, a reference is unavailable, or this value already exists. Refresh and review again.",
    )
}

fn normalized(
    command: &QualityAlertAcknowledgeCommand,
) -> Result<QualityAlertAcknowledgeCommand, Error> {
    let normalized = normalize_quality_alert_acknowledge(command.clone())?;
    if normalized != *command {
        return Err(BusinessError::invalid(
            "QUALITY_ALERT_REASON",
            "Normalize the acknowledgement reason before fingerprinting.",
        )
        .into());
    }
    Ok(normalized)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AlertSource {
    id: String,
    hold_id: String,
    version: i64,
    acknowledged_by: Option<String>,
    company_id: String,
    branch_id: String,
    status: HoldStatus,
    created_by: String,
    cancel_requested_by: Option<String>,
    last_inspector: Option<String>,
}

async fn read_source(
    c: &mut PgConnection,
    company: &CompanyId,
    branch: Option<&RecordId>,
    id: &str,
) -> Result<Option<AlertSource>, Error> {
    let row = sqlx::query(
        "SELECT a.id,a.hold_id,a.version,a.acknowledged_by,h.company_id,h.branch_id,h.status,h.created_by,h.cancel_requested_by,h.last_inspector \
         FROM inventory_quality_alerts a \
         JOIN inventory_quality_holds h ON h.id=a.hold_id \
         WHERE a.id=$1 AND h.company_id=$2 AND ($3::text IS NULL OR h.branch_id=$3)",
    )
    .bind(id)
    .bind(company.as_str())
    .bind(branch.map(RecordId::as_str))
    .fetch_optional(&mut *c)
    .await?;
    row.map(|row| {
        Ok(AlertSource {
            id: row.try_get("id")?,
            hold_id: row.try_get("hold_id")?,
            version: safe(row.try_get("version")?)?,
            acknowledged_by: row.try_get("acknowledged_by")?,
            company_id: row.try_get("company_id")?,
            branch_id: row.try_get("branch_id")?,
            status: HoldStatus::parse(&row.try_get::<String, _>("status")?).ok_or_else(invalid)?,
            created_by: row.try_get("created_by")?,
            cancel_requested_by: row.try_get("cancel_requested_by")?,
            last_inspector: row.try_get("last_inspector")?,
        })
    })
    .transpose()
}

/// The retained alert recipient SQL, evaluated against the current Identity
/// assignment and the actual hold status.
async fn eligible(
    c: &mut PgConnection,
    source: &AlertSource,
    actor: &UserId,
) -> Result<bool, Error> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM assignments m \
         WHERE m.company_id=$1 \
           AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=$2)) \
           AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND iam_role_allows(m.role,m.company_id,CASE WHEN $3 IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END) \
           AND m.user_id=$4 \
           AND NOT iam_same_person(m.user_id,$5,m.company_id) \
           AND ($3<>'cancel_pending' OR NOT iam_same_person(m.user_id,$6,m.company_id)) \
           AND ($3<>'awaiting_release' OR NOT iam_same_person(m.user_id,$7,m.company_id)))",
    )
    .bind(&source.company_id)
    .bind(&source.branch_id)
    .bind(source.status.as_str())
    .bind(actor.as_str())
    .bind(&source.created_by)
    .bind(source.cancel_requested_by.as_deref())
    .bind(source.last_inspector.as_deref())
    .fetch_one(&mut *c)
    .await?)
}

async fn require_source_current(
    c: &mut PgConnection,
    prepared: &PreparedQualityAlertAcknowledge,
) -> Result<AlertSource, Error> {
    serializable(c).await?;
    if unit(c).await? != prepared.unit {
        return Err(assertion_failed().into());
    }
    let source = read_source(
        c,
        &prepared.company,
        prepared.branch.as_ref(),
        prepared.command.alert_id.as_str(),
    )
    .await?
    .ok_or_else(not_found)?;
    if source != prepared.source
        || source.version != prepared.command.expected_version
        || source.acknowledged_by.is_some()
        || source.status.is_terminal()
        || !eligible(c, &source, &prepared.actor).await?
    {
        return Err(assertion_failed().into());
    }
    Ok(source)
}

/// Prepared acknowledgement against one locked alert source.
pub struct PreparedQualityAlertAcknowledge {
    unit: Unit,
    command: QualityAlertAcknowledgeCommand,
    company: CompanyId,
    branch: Option<RecordId>,
    actor: UserId,
    source: AlertSource,
    now: i64,
    receipt: QualityAlertAcknowledgementReceipt,
}

impl PreparedQualityAlertAcknowledge {
    pub fn receipt(&self) -> &QualityAlertAcknowledgementReceipt {
        &self.receipt
    }

    /// The actual hold branch retained by the alert, used for the outbox event
    /// scope exactly as the retained acknowledgement did.
    pub fn branch_id(&self) -> &RecordId {
        &self
            .branch
            .as_ref()
            .expect("acknowledged alert has a scoped hold branch")
    }

    pub async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        require_source_current(c, self).await?;
        Ok(())
    }

    pub async fn apply(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_before(c).await?;
        let updated = sqlx::query(
            "UPDATE inventory_quality_alerts SET acknowledged_by=$1,acknowledged_at=$2,reason=$3,version=version+1 \
             WHERE id=$4 AND version=$5 AND acknowledged_by IS NULL",
        )
        .bind(&self.receipt.acknowledged_by)
        .bind(self.now)
        .bind(&self.command.reason)
        .bind(self.command.alert_id.as_str())
        .bind(self.command.expected_version)
        .execute(&mut *c)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(assertion_failed().into());
        }
        self.require_after(c).await
    }

    pub async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        serializable(c).await?;
        if unit(c).await? != self.unit {
            return Err(assertion_failed().into());
        }
        let saved: Option<(String, i64, Option<String>, Option<i64>, Option<String>)> =
            sqlx::query_as(
                "SELECT a.id,a.version,a.acknowledged_by,a.acknowledged_at,a.reason \
                 FROM inventory_quality_alerts a \
                 JOIN inventory_quality_holds h ON h.id=a.hold_id \
                 WHERE a.id=$1 AND h.company_id=$2 AND ($3::text IS NULL OR h.branch_id=$3)",
            )
            .bind(self.command.alert_id.as_str())
            .bind(self.company.as_str())
            .bind(self.branch.as_ref().map(RecordId::as_str))
            .fetch_optional(&mut *c)
            .await?;
        let expected = Some((
            self.receipt.id.clone(),
            self.receipt.version,
            Some(self.receipt.acknowledged_by.clone()),
            Some(self.now),
            Some(self.command.reason.clone()),
        ));
        if saved != expected {
            return Err(assertion_failed().into());
        }
        Ok(())
    }
}

/// Prepare one acknowledgement against the exact locked alert row. The caller
/// must already hold the Commercial/IAM source locks and validate current
/// permission, purchase admission and Workflow replay independently.
pub async fn prepare_quality_alert_acknowledge(
    c: &mut PgConnection,
    command: &QualityAlertAcknowledgeCommand,
    company: &CompanyId,
    branch: Option<&RecordId>,
    actor: &UserId,
    now: i64,
) -> Result<PreparedQualityAlertAcknowledge, Error> {
    let command = normalized(command)?;
    serializable(c).await?;
    // The retained owner unit begins with the exact alert row lock. The scoped
    // source and recipient assertion is then evaluated in this same statement
    // order before any write.
    let locked: Option<String> =
        sqlx::query_scalar("SELECT id FROM inventory_quality_alerts WHERE id=$1 FOR UPDATE")
            .bind(command.alert_id.as_str())
            .fetch_optional(&mut *c)
            .await?;
    if locked.is_none() {
        return Err(not_found().into());
    }
    let source = read_source(c, company, branch, command.alert_id.as_str())
        .await?
        .ok_or_else(not_found)?;
    if source.acknowledged_by.is_some() {
        return Err(acknowledged().into());
    }
    if source.version != command.expected_version
        || source.status.is_terminal()
        || !eligible(c, &source, actor).await?
    {
        return Err(assertion_failed().into());
    }
    let version = command
        .expected_version
        .checked_add(1)
        .ok_or_else(assertion_failed)?;
    let receipt = QualityAlertAcknowledgementReceipt {
        id: source.id.clone(),
        version,
        acknowledged_by: actor.as_str().to_owned(),
    };
    let unit = unit(c).await?;
    let branch = RecordId::new(source.branch_id.clone())?;
    Ok(PreparedQualityAlertAcknowledge {
        unit,
        command,
        company: company.clone(),
        branch: Some(branch),
        actor: actor.clone(),
        source,
        now,
        receipt,
    })
}
