//! Current Identity recipients of independent Finance review for a Quality
//! scrap Count. Quality must retain the actual count/stock after-image; this
//! input fact grants no authority.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
type Unit = (String, i32, String, String);

pub struct QualityScrapApproverSourceLocks {
    unit: Unit,
}

/// Utility-only source locks; call before the first serializable snapshot.
pub async fn lock_quality_scrap_approver_sources(
    c: &mut PgConnection,
) -> Result<QualityScrapApproverSourceLocks, Error> {
    if !c.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *c)
            .await?
            != "serializable"
    {
        return Err(invalid());
    }
    super::postgres_effective_authority::lock_effective_authority_sources(c).await?;
    super::permission_catalog_lock::lock_permission_catalog_sources(c).await?;
    Ok(QualityScrapApproverSourceLocks {
        unit: unit(c).await?,
    })
}

pub struct CurrentQualityScrapApprovers {
    company: CompanyId,
    actor: String,
    recipients: Vec<String>,
    unit: Unit,
}

impl CurrentQualityScrapApprovers {
    pub fn recipients(&self) -> &[String] {
        &self.recipients
    }

    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        if unit(c).await? != self.unit
            || read(c, &self.company, &self.actor).await? != self.recipients
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Current holding-scope users whose role allows `inventory.approve`, excluding
/// the current actor. An empty result is a fact, never a grant.
pub async fn inspect_quality_scrap_approvers(
    c: &mut PgConnection,
    locks: &QualityScrapApproverSourceLocks,
    company: &CompanyId,
    actor: &str,
) -> Result<CurrentQualityScrapApprovers, Error> {
    require_catalog_lock(c).await?;
    if actor.is_empty() || actor.len() > 1024 {
        return Err(invalid());
    }
    let unit = unit(c).await?;
    if unit != locks.unit {
        return Err(invalid());
    }
    let recipients = read(c, company, actor).await?;
    Ok(CurrentQualityScrapApprovers {
        company: company.clone(),
        actor: actor.to_owned(),
        recipients,
        unit,
    })
}

async fn read(
    c: &mut PgConnection,
    company: &CompanyId,
    actor: &str,
) -> Result<Vec<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT a.user_id FROM assignments a \
         WHERE a.company_id=$1 AND a.scope_kind='holding' \
           AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
           AND iam_role_allows(a.role,a.company_id,'inventory.approve') \
           AND NOT iam_same_person(a.user_id,$2,a.company_id) \
         ORDER BY a.user_id",
    )
    .bind(company.as_str())
    .bind(actor)
    .fetch_all(c)
    .await?)
}

async fn require_catalog_lock(c: &mut PgConnection) -> Result<(), Error> {
    let held: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='relation' AND pid=pg_backend_pid() AND relation='iam_permissions'::regclass AND mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND granted)").fetch_one(c).await?;
    if !held {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "QUALITY_SCRAP_APPROVER_LOCK_REQUIRED",
            "Quality scrap approver source preparation is required.",
        )
        .into());
    }
    Ok(())
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
    PersistenceError::new("IAM_QUALITY_SCRAP_APPROVER_TRANSACTION_REQUIRED").into()
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "IAM_QUALITY_SCRAP_APPROVERS_CHANGED",
        "The Quality scrap Finance approvers changed.",
    )
    .into()
}
