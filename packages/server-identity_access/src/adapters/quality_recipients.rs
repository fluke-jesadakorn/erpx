//! Current Identity half of Quality notification routing. Quality must retain
//! the actual post-command hold source; these input facts grant no authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

#[derive(Clone)]
pub struct QualityRecipientRequest {
    pub company_id: CompanyId,
    pub branch_id: RecordId,
    pub creator: String,
    pub cancellation_requester: Option<String>,
    pub status: QualityRecipientStatus,
}
#[derive(Clone, Copy)]
pub enum QualityRecipientStatus {
    Held,
    CancellationPending,
    Cancelled,
}
pub struct QualityRecipientLockPreparation<'a> {
    connection: &'a mut PgConnection,
}
pub struct QualityRecipientSourceLocks {
    unit: Unit,
}
impl QualityRecipientLockPreparation<'_> {
    pub fn connection(&mut self) -> &mut PgConnection {
        self.connection
    }
    /// Bind only after all owning source locks have been acquired, before any
    /// owner/source read. The caller controls the global lock order.
    pub async fn bind(self) -> Result<QualityRecipientSourceLocks, Error> {
        Ok(QualityRecipientSourceLocks {
            unit: unit(self.connection).await?,
        })
    }
}
/// Utility-only source locks; call before the first serializable snapshot.
pub async fn lock_quality_recipient_sources(
    c: &mut PgConnection,
) -> Result<QualityRecipientLockPreparation<'_>, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    super::postgres_effective_authority::lock_effective_authority_sources(c).await?;
    // A catalog writer may already hold iam_permissions while waiting for an
    // IAM role lock above. Never wait here: the owning unit must roll back so
    // that writer can complete before a fresh snapshot is taken.
    super::permission_catalog_lock::lock_permission_catalog_sources(c).await?;
    Ok(QualityRecipientLockPreparation { connection: c })
}
pub struct CurrentQualityRecipients {
    request: QualityRecipientRequest,
    recipients: Vec<String>,
    unit: Unit,
}
impl CurrentQualityRecipients {
    pub fn recipients(&self) -> &[String] {
        &self.recipients
    }
    /// Re-evaluate at the actual current DB clock, so expiry during an owning
    /// transaction cannot leave an obsolete notification recipient set.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit || read(c, &self.request).await? != self.recipients {
            return Err(changed());
        }
        Ok(())
    }
}
/// A selected actor and current recipient set retained in one actual unit.
/// The Quality owner must separately retain its real hold and purchase source.
pub struct SelectedQualityRecipients<'a> {
    context: &'a crate::public::effective_authority::AuthorizedWorkContext,
    inner: CurrentQualityRecipients,
}
impl SelectedQualityRecipients<'_> {
    pub fn recipients(&self) -> &[String] {
        self.inner.recipients()
    }
    pub async fn require_current(&self, c: &mut PgConnection, now: i64) -> Result<(), Error> {
        require_catalog_lock(&mut *c).await?;
        super::selected_command_authority::require_selected_actor_current(
            &mut *c,
            self.context,
            now,
        )
        .await?;
        self.inner.require_current(c).await
    }
}
/// `QualitySources.prepare` must acquire recipient and all other owner locks
/// before the first snapshot. The real database lock check below proves the
/// catalog lock is held now; it cannot prove the caller's preparation order.
/// The request's status/creator/requester must come from the retained hold.
pub async fn inspect_quality_recipients_for_selected<'a>(
    c: &mut PgConnection,
    context: &'a crate::public::effective_authority::AuthorizedWorkContext,
    request: QualityRecipientRequest,
    now: i64,
) -> Result<SelectedQualityRecipients<'a>, Error> {
    require_catalog_lock(&mut *c).await?;
    super::selected_command_authority::require_selected_actor_current(&mut *c, context, now)
        .await?;
    context.require_permission("quality.read")?;
    let workspace = context.selection().workspace();
    if workspace.company_id != request.company_id
        || workspace.department_id.is_some()
        || workspace
            .branch_id
            .as_ref()
            .is_some_and(|branch| branch != &request.branch_id)
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "QUALITY_ACCESS_CHANGED",
            "Choose the current Company or Branch Quality assignment.",
        )
        .into());
    }
    let locks = QualityRecipientSourceLocks {
        unit: unit(c).await?,
    };
    let inner = inspect_quality_recipients(c, &locks, request).await?;
    Ok(SelectedQualityRecipients { context, inner })
}
async fn require_catalog_lock(c: &mut PgConnection) -> Result<(), Error> {
    let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='relation' AND pid=pg_backend_pid() AND relation='iam_permissions'::regclass AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND granted)").fetch_one(c).await?;
    if !held {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "QUALITY_RECIPIENT_LOCK_REQUIRED",
            "Quality recipient source preparation is required.",
        )
        .into());
    }
    Ok(())
}
pub async fn inspect_quality_recipients(
    c: &mut PgConnection,
    locks: &QualityRecipientSourceLocks,
    request: QualityRecipientRequest,
) -> Result<CurrentQualityRecipients, Error> {
    if request.creator.is_empty()
        || request.creator.len() > 1024
        || request
            .cancellation_requester
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 1024)
    {
        return Err(invalid());
    }
    let unit = unit(c).await?;
    if unit != locks.unit {
        return Err(invalid());
    }
    let recipients = read(c, &request).await?;
    Ok(CurrentQualityRecipients {
        request,
        recipients,
        unit,
    })
}
async fn read(c: &mut PgConnection, r: &QualityRecipientRequest) -> Result<Vec<String>, Error> {
    let (permission, terminal) = match r.status {
        QualityRecipientStatus::Held => ("quality.inspect", false),
        QualityRecipientStatus::CancellationPending => ("quality.release", false),
        QualityRecipientStatus::Cancelled => ("quality.read", true),
    };
    let pending = matches!(r.status, QualityRecipientStatus::CancellationPending);
    let rows:Vec<String>=sqlx::query_scalar("SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=$1 AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$2)) AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,$3) AND (($4::boolean AND a.user_id=$5) OR (NOT $4::boolean AND NOT iam_same_person(a.user_id,$5,a.company_id) AND (NOT $6::boolean OR NOT iam_same_person(a.user_id,$7,a.company_id)))) ORDER BY a.user_id")
        .bind(r.company_id.as_str()).bind(r.branch_id.as_str()).bind(permission).bind(terminal).bind(&r.creator).bind(pending).bind(r.cancellation_requester.as_deref()).fetch_all(c).await?;
    Ok(rows)
}
async fn unit(c: &mut PgConnection) -> Result<Unit, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    Ok(sqlx::query_as("SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,pg_postmaster_start_time()::text").fetch_one(c).await?)
}
fn invalid() -> Error {
    PersistenceError::new("IAM_QUALITY_RECIPIENT_TRANSACTION_REQUIRED").into()
}
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "IAM_QUALITY_RECIPIENTS_CHANGED",
        "The Quality notification recipients changed.",
    )
    .into()
}
