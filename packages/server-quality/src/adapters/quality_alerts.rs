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
use erpx_server_runtime::public::postgres::{
    DatabasePool, PersistenceError, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use sqlx::{Connection, PgConnection, Row};

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

/// The retained escalation window: one business day ahead of the sweep clock.
pub const QUALITY_ALERT_WARNING_WINDOW_MS: i64 = 86_400_000;

/// Due-soon escalations are inserted from the hold's own reviewed version. The
/// `UNIQUE(hold_id,phase)` source key plus `ON CONFLICT DO NOTHING` make a
/// repeated or restarted sweep idempotent.
const DUE_SOON_ALERT: &str = "INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at) \
 SELECT 'quality-alert-due-soon-'||h.id,h.id,'due_soon',h.version,h.due_date::date,$1 \
 FROM inventory_quality_holds h \
 WHERE h.status NOT IN ('released','cancelled','disposed') AND h.company_id=$2 \
   AND h.due_date>=$3 AND h.due_date<=$4 \
 ON CONFLICT DO NOTHING";

const OVERDUE_ALERT: &str = "INSERT INTO inventory_quality_alerts(id,hold_id,phase,source_version,due_date,detected_at) \
 SELECT 'quality-alert-overdue-'||h.id,h.id,'overdue',h.version,h.due_date::date,$1 \
 FROM inventory_quality_holds h \
 WHERE h.status NOT IN ('released','cancelled','disposed') AND h.company_id=$2 \
   AND h.due_date<$3 \
 ON CONFLICT DO NOTHING";

/// The two escalation phases one scheduled sweep evaluates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QualityAlertPhase {
    DueSoon,
    Overdue,
}

impl QualityAlertPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DueSoon => "due_soon",
            Self::Overdue => "overdue",
        }
    }

    /// The retained deterministic alert identity for one hold and phase.
    pub fn alert_id(self, hold_id: &str) -> String {
        format!("quality-alert-{}-{hold_id}", self.as_str().replace('_', "-"))
    }
}

/// The business-day boundaries one scheduled sweep evaluates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityAlertSweepPlan {
    today: String,
    horizon: String,
}

impl QualityAlertSweepPlan {
    /// The UTC business date of the sweep clock.
    pub fn today(&self) -> &str {
        &self.today
    }

    /// The last inclusive due date that is still "due soon".
    pub fn due_soon_horizon(&self) -> &str {
        &self.horizon
    }

    pub fn phases(&self) -> [QualityAlertPhase; 2] {
        [QualityAlertPhase::DueSoon, QualityAlertPhase::Overdue]
    }
}

fn clock_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_ALERT_CLOCK_INVALID",
        "The scheduled Quality clock is unavailable.",
    )
}

fn utc_date(now: i64) -> Result<String, BusinessError> {
    const DATE: &[time::format_description::FormatItem<'static>] =
        time::macros::format_description!("[year]-[month]-[day]");
    let moment = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(now) * 1_000_000)
        .map_err(|_| clock_invalid())?;
    moment.date().format(DATE).map_err(|_| clock_invalid())
}

/// Plan one sweep from the caller's business clock. Dates are the UTC business
/// dates the retained sweep used; the warning window is inclusive of the
/// horizon date and never reorders the two stages.
pub fn quality_alert_sweep_plan(
    now: i64,
    window_ms: i64,
) -> Result<QualityAlertSweepPlan, BusinessError> {
    if !(0..=crate::domain::quality::MAX_SAFE_INTEGER).contains(&now) || window_ms < 0 {
        return Err(clock_invalid());
    }
    let horizon = now
        .checked_add(window_ms)
        .filter(|value| *value <= crate::domain::quality::MAX_SAFE_INTEGER)
        .ok_or_else(clock_invalid)?;
    Ok(QualityAlertSweepPlan {
        today: utc_date(now)?,
        horizon: utc_date(horizon)?,
    })
}

async fn due_soon_stage(
    c: &mut PgConnection,
    company: &str,
    plan: &QualityAlertSweepPlan,
    now: i64,
) -> Result<u64, Error> {
    Ok(sqlx::query(DUE_SOON_ALERT)
        .bind(now)
        .bind(company)
        .bind(plan.today())
        .bind(plan.due_soon_horizon())
        .execute(&mut *c)
        .await?
        .rows_affected())
}

async fn overdue_stage(
    c: &mut PgConnection,
    company: &str,
    plan: &QualityAlertSweepPlan,
    now: i64,
) -> Result<u64, Error> {
    Ok(sqlx::query(OVERDUE_ALERT)
        .bind(now)
        .bind(company)
        .bind(plan.today())
        .execute(&mut *c)
        .await?
        .rows_affected())
}

/// Sweep the due-soon and overdue escalations for every Company that still has
/// an active Quality hold. Each Company keeps its own bounded insert and the
/// caller's unit of work keeps the sweep replayable.
async fn sweep(
    c: &mut PgConnection,
    plan: &QualityAlertSweepPlan,
    now: i64,
) -> Result<(u64, u64), Error> {
    serializable(c).await?;
    let companies: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT company_id FROM inventory_quality_holds \
         WHERE status NOT IN ('released','cancelled','disposed') ORDER BY company_id",
    )
    .fetch_all(&mut *c)
    .await?;
    let (mut due_soon, mut overdue) = (0, 0);
    for company in companies {
        due_soon += due_soon_stage(c, &company, plan, now).await?;
        overdue += overdue_stage(c, &company, plan, now).await?;
    }
    Ok((due_soon, overdue))
}

/// Native scheduled Quality alert sweep. Returns the due-soon and overdue
/// escalations actually created; a replay returns `(0, 0)`.
pub async fn drain_quality_alerts(
    pool: DatabasePool,
    now: i64,
    window_ms: i64,
) -> Result<(u64, u64), BusinessError> {
    let plan = quality_alert_sweep_plan(now, window_ms)?;
    PgUnitOfWork::new(pool)
        .run(TransactionOptions::default(), move |c| {
            let plan = plan.clone();
            Box::pin(async move { sweep(c, &plan, now).await })
        })
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            PgTransactionError::Persistence(_) => BusinessError::new(
                FailureKind::Unavailable,
                "QUALITY_ALERT_UNAVAILABLE",
                "The scheduled Quality escalation sweep is unavailable.",
            ),
        })
}

/// Native Quality alert-notification sweep. This replaces the TS-only job
/// writer with the owner-owned statement on the caller's serializable unit.
/// `production` mirrors scheduledFeaturePredicate('quality'): quality is not a
/// release module, so the production predicate is FALSE.
pub async fn route_quality_alert_notifications(
    c: &mut PgConnection,
    now: i64,
    production: bool,
) -> Result<u64, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(PersistenceError::new("QUALITY_ALERT_NOTICE_UNIT_REQUIRED").into());
    }
    let feature = if production { "FALSE" } else { "TRUE" };
    let recipient = format!(
        "m.company_id=h.company_id \
 AND iam_assignment_active(m.id,$2::bigint) \
 AND iam_role_allows(m.role,m.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END) \
 AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=h.branch_id)) \
 AND NOT iam_same_person(m.user_id,h.created_by,h.company_id) \
 AND (h.status<>'cancel_pending' OR NOT iam_same_person(m.user_id,h.cancel_requested_by,h.company_id)) \
 AND (h.status<>'awaiting_release' OR NOT iam_same_person(m.user_id,h.last_inspector,h.company_id))"
    );
    // The retained TS predicate uses `iam_assignment_active(m.id, <ms>)`; the
    // native sweep passes the same bounded now value as a double-precision ms.
    let sql = format!(
        "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,\
 record_version,message,created_at) \
 SELECT DISTINCT a.id||':'||m.user_id,h.company_id,m.user_id,'quality_alert',a.id,a.version, \
 CASE WHEN a.phase='overdue' THEN 'Quality hold is overdue: take responsibility for inspection or release follow-up' \
 ELSE 'Quality hold is due within 24 hours: prepare the next inspection or release action' END,$1::bigint \
 FROM (SELECT \"id\",\"hold_id\",\"phase\",\"source_version\",\"due_date\",\"detected_at\",\
 \"version\",\"acknowledged_by\",\"acknowledged_at\",\"reason\" FROM inventory_quality_alerts) a \
 JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"warehouse_id\",\"product_id\",\"reason\",\
 \"due_date\",\"status\",\"created_by\",\"last_inspector\",\"last_result\",\"released_by\",\
 \"release_reason\",\"released_at\",\"version\",\"created_at\",\"updated_at\",\
 \"cancel_requested_by\",\"cancel_reason\",\"cancelled_by\",\"cancelled_at\",\"lot_id\",\
 \"held_quantity_milli\",\"source_type\",\"source_id\",\"disposition_kind\",\
 \"disposition_reason\",\"disposition_requested_by\",\"disposition_count_id\",\"disposed_by\",\
 \"disposed_at\",\"spec_id\",\"spec_version\",\"quantity_scope\",\
 \"disposition_quantity_milli\" FROM inventory_quality_holds) h ON h.id=a.hold_id \
 JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\
 \"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\
 \"parent_id\",\"source\",\"source_id\" FROM assignments) m ON {recipient} \
 WHERE a.acknowledged_by IS NULL AND h.status NOT IN ('released','cancelled','disposed') \
 AND {feature} ON CONFLICT DO NOTHING"
    );
    Ok(sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(now)
        .bind(now as f64)
        .execute(&mut *c)
        .await?
        .rows_affected())
}

/// Pooled wrapper for the scheduled Quality alert-notification sweep.
pub async fn route_quality_alert_notifications_pooled(
    pool: DatabasePool,
    now: i64,
    production: bool,
) -> Result<u64, BusinessError> {
    PgUnitOfWork::new(pool)
        .run(TransactionOptions::default(), move |c| {
            Box::pin(async move { route_quality_alert_notifications(c, now, production).await })
        })
        .await
        .map_err(|error| match error {
            PgTransactionError::Business(error) => error,
            PgTransactionError::Persistence(_) => BusinessError::new(
                FailureKind::Unavailable,
                "QUALITY_ALERT_NOTICE_UNAVAILABLE",
                "The scheduled Quality alert notice sweep is unavailable.",
            ),
        })
}
