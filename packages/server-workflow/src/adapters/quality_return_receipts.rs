//! Scoped Workflow receipt source for Quality customer returns.
//!
//! The caller supplies the exact Company, return and actor taken from the
//! trusted owner image. This port returns only matching receipts; it confers no
//! approval authority and must be followed by the full TypeScript-equivalent
//! join and current IAM/purchase checks.
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind, RecordId, UserId};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

#[derive(Clone, Copy)]
pub struct QualityReturnReceiptScope<'a> {
    pub company: &'a CompanyId,
    pub return_id: &'a RecordId,
    pub actor: &'a UserId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityReturnReceipt {
    pub command_id: String,
    pub record_id: String,
    pub actor_id: String,
    pub authority_json: Option<String>,
}

type Unit = (String, i32, String, String, String, String, i64);

pub struct CurrentQualityReturnReceipts {
    unit: Unit,
    company: CompanyId,
    return_id: RecordId,
    actor: UserId,
    rows: Vec<QualityReturnReceipt>,
}

impl CurrentQualityReturnReceipts {
    pub fn rows(&self) -> &[QualityReturnReceipt] {
        &self.rows
    }

    pub fn command_ids(&self) -> impl Iterator<Item = &str> {
        self.rows.iter().map(|row| row.command_id.as_str())
    }

    /// Re-reads the exact scoped matching rows inside the same unit. Absence,
    /// multiplicity and full authority bytes must still be re-evaluated by the
    /// consumer after effects.
    pub async fn require_current(&self, c: &mut PgConnection) -> Result<(), Error> {
        let scope = QualityReturnReceiptScope {
            company: &self.company,
            return_id: &self.return_id,
            actor: &self.actor,
        };
        if unit(&mut *c).await? != self.unit
            || rows(&mut *c, &scope).await? != self.rows
            || unit(c).await? != self.unit
        {
            return Err(changed());
        }
        Ok(())
    }
}

/// Acquire before the first protected snapshot; composition owns global order.
pub async fn lock_quality_return_receipts(c: &mut PgConnection) -> Result<(), Error> {
    if !c.is_in_transaction() {
        return Err(PersistenceError::new("QUALITY_RETURN_RECEIPT_TRANSACTION_REQUIRED").into());
    }
    sqlx::query("LOCK TABLE public.command_receipts IN SHARE MODE NOWAIT")
        .execute(c)
        .await
        .map_err(PersistenceError::pre_snapshot_lock_conflict)?;
    Ok(())
}

pub async fn inspect_quality_return_receipts(
    c: &mut PgConnection,
    scope: QualityReturnReceiptScope<'_>,
) -> Result<CurrentQualityReturnReceipts, Error> {
    let identity = unit(&mut *c).await?;
    let rows = rows(&mut *c, &scope).await?;
    if unit(c).await? != identity {
        return Err(changed());
    }
    Ok(CurrentQualityReturnReceipts {
        unit: identity,
        company: scope.company.clone(),
        return_id: scope.return_id.clone(),
        actor: scope.actor.clone(),
        rows,
    })
}

async fn rows(
    c: &mut PgConnection,
    scope: &QualityReturnReceiptScope<'_>,
) -> Result<Vec<QualityReturnReceipt>, Error> {
    bounded(c, scope).await?;
    let rows = sqlx::query(
        "SELECT command_id,record_id,actor_id,\
         CASE WHEN authority_json IS NULL OR octet_length(authority_json)<=2097152 \
         THEN authority_json END authority_json \
         FROM public.command_receipts \
         WHERE operation='commerce.return.approve' AND record_id=$1 AND actor_id=$2 \
         AND authority_json::jsonb->>'companyId'=$3 \
         ORDER BY command_id LIMIT 10001",
    )
    .bind(scope.return_id.as_str())
    .bind(scope.actor.as_str())
    .bind(scope.company.as_str())
    .fetch_all(c)
    .await?;
    rows.iter()
        .map(|row| {
            Ok(QualityReturnReceipt {
                command_id: row.try_get("command_id")?,
                record_id: row.try_get("record_id")?,
                actor_id: row.try_get("actor_id")?,
                authority_json: row.try_get("authority_json")?,
            })
        })
        .collect()
}

async fn bounded(c: &mut PgConnection, scope: &QualityReturnReceiptScope<'_>) -> Result<(), Error> {
    let valid: bool = sqlx::query_scalar(
        "SELECT count(*)<=10000 AND COALESCE(sum(\
         octet_length(command_id)::bigint+octet_length(record_id)::bigint+\
         octet_length(actor_id)::bigint+COALESCE(octet_length(authority_json),0)::bigint),0)<=2097152 \
         FROM public.command_receipts \
         WHERE operation='commerce.return.approve' AND record_id=$1 AND actor_id=$2 \
         AND authority_json::jsonb->>'companyId'=$3",
    )
    .bind(scope.return_id.as_str())
    .bind(scope.actor.as_str())
    .bind(scope.company.as_str())
    .fetch_one(c)
    .await?;
    if !valid {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "QUALITY_RETURN_RECEIPT_LIMIT",
            "Quality receipt source exceeds the bounded read.",
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
        return Err(PersistenceError::new("QUALITY_RETURN_RECEIPT_SERIALIZABLE_REQUIRED").into());
    }
    let valid: bool = sqlx::query_scalar(
        "SELECT c.relnamespace='public'::regnamespace AND c.relkind IN ('r','p') \
         AND c.relpersistence='p' AND to_regclass('command_receipts')=c.oid \
         AND NOT row_security_active(c.oid) AND EXISTS(SELECT 1 FROM pg_locks l \
         WHERE l.pid=pg_backend_pid() AND l.locktype='relation' AND l.granted \
         AND l.mode IN ('ShareLock','ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') \
         AND l.relation=c.oid) FROM pg_class c WHERE c.oid=to_regclass('public.command_receipts')",
    )
    .fetch_optional(&mut *c)
    .await?
    .unwrap_or(false);
    if !valid {
        return Err(PersistenceError::new("QUALITY_RETURN_RECEIPT_SOURCE_FENCE_REQUIRED").into());
    }
    Ok(sqlx::query_as(
        "SELECT current_database(),pg_backend_pid(),pg_current_xact_id()::text,\
         pg_postmaster_start_time()::text,current_user::text,current_setting('search_path'),\
         'public.command_receipts'::regclass::oid::bigint",
    )
    .fetch_one(c)
    .await?)
}

fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "QUALITY_RETURN_RECEIPT_UNIT_CHANGED",
        "Quality receipt source changed.",
    )
    .into()
}
