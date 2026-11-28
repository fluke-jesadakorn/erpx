//! Durable PostgreSQL outbox queue on the caller's connection.
//!
//! Every mutation writes the native-runtime transaction setting in the same
//! statement as the queue update. The static owner predicate is supplied by the
//! worker composition and must never contain request, user or provider input.
use crate::domain::jobs::{
    DEFAULT_LEASE_MILLIS, OutboxJob, OutboxRetrySchedule, retry_schedule, truncate_error_code,
};
use sqlx::{AssertSqlSafe, PgConnection, Row};

fn invalid(code: &'static str) -> sqlx::Error {
    sqlx::Error::Protocol(code.into())
}

fn validate(owned: &str, token: &str, now: i64) -> Result<(), sqlx::Error> {
    if owned.trim().is_empty() || token.is_empty() || token.len() > 180 {
        return Err(invalid("INVALID_NATIVE_JOB_CLAIM"));
    }
    if !(0..=9_007_199_254_740_991).contains(&now) {
        return Err(invalid("INVALID_NATIVE_JOB_CLOCK"));
    }
    Ok(())
}

fn job(row: &sqlx::postgres::PgRow) -> Result<OutboxJob, sqlx::Error> {
    Ok(OutboxJob {
        id: row.try_get("id")?,
        event_type: row.try_get("event_type")?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        aggregate_version: row.try_get("aggregate_version")?,
        payload_json: row.try_get("payload_json")?,
        attempts: row.try_get("attempts")?,
        lease_token: row.try_get("lease_token")?,
    })
}

/// Claim one ready event owned by the supplied static predicate. An expired or
/// absent lease is eligible; a live lease is never stolen or renewed.
pub async fn claim(
    connection: &mut PgConnection,
    owned: &str,
    token: &str,
    now: i64,
) -> Result<Option<OutboxJob>, sqlx::Error> {
    validate(owned, token, now)?;
    let row = sqlx::query(AssertSqlSafe(format!(
        "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) \
         UPDATE outbox_events o SET lease_token=$1,claim_until=$2,attempts=o.attempts+1 \
         FROM runtime_owner \
         WHERE o.id=(SELECT e.id FROM outbox_events e \
           WHERE e.processed_at IS NULL AND e.dead_letter_at IS NULL \
             AND e.next_attempt_at<=$3 AND (e.claim_until IS NULL OR e.claim_until<$3) \
             AND ({owned}) \
           ORDER BY e.created_at,e.id FOR UPDATE OF e SKIP LOCKED LIMIT 1) \
         RETURNING o.id,o.event_type,o.aggregate_type,o.aggregate_id,o.aggregate_version,\
         o.payload_json,o.attempts,o.lease_token"
    )))
    .bind(token)
    .bind(now.saturating_add(DEFAULT_LEASE_MILLIS))
    .bind(now)
    .fetch_optional(connection)
    .await?;
    row.as_ref().map(job).transpose()
}

/// Extend the exact live lease. A stale token, completed event or dead letter
/// returns `false`; the caller must stop work rather than retry the update.
pub async fn renew(
    connection: &mut PgConnection,
    id: &str,
    lease_token: &str,
    now: i64,
) -> Result<bool, sqlx::Error> {
    if id.is_empty() || lease_token.is_empty() || now < 0 {
        return Err(invalid("INVALID_NATIVE_JOB_LEASE"));
    }
    Ok(sqlx::query(
        "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) \
         UPDATE outbox_events SET claim_until=$1 FROM runtime_owner \
         WHERE id=$2 AND lease_token=$3 AND processed_at IS NULL AND dead_letter_at IS NULL",
    )
    .bind(now.saturating_add(DEFAULT_LEASE_MILLIS))
    .bind(id)
    .bind(lease_token)
    .execute(connection)
    .await?
    .rows_affected()
        == 1)
}

/// The exact terminal outcome of an acknowledgement attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseCompletion {
    /// This call moved the exact leased event to processed/published.
    Applied,
    /// The event is already terminal. A crash after the durable effect commit
    /// and before this acknowledgement is recovered as success, never replayed.
    AlreadyCompleted,
    /// A live event belongs to another token or the row no longer exists.
    LeaseLost,
}

/// Acknowledge the exact leased event after its durable effect committed.
///
/// Completion is idempotent: an event already marked processed is returned as
/// `AlreadyCompleted` instead of looking like a stolen lease. Any other
/// zero-row result remains `LeaseLost` and must not be treated as success.
pub async fn complete_lease(
    connection: &mut PgConnection,
    id: &str,
    lease_token: &str,
    now: i64,
) -> Result<LeaseCompletion, sqlx::Error> {
    if id.is_empty() || lease_token.is_empty() || now < 0 {
        return Err(invalid("INVALID_NATIVE_JOB_LEASE"));
    }
    let applied = sqlx::query(
        "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) \
         UPDATE outbox_events SET processed_at=$1,published_at=$1,claim_until=NULL,\
         lease_token=NULL,last_error=NULL FROM runtime_owner \
         WHERE id=$2 AND lease_token=$3 AND processed_at IS NULL",
    )
    .bind(now)
    .bind(id)
    .bind(lease_token)
    .execute(&mut *connection)
    .await?
    .rows_affected()
        == 1;
    if applied {
        return Ok(LeaseCompletion::Applied);
    }
    let completed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM outbox_events WHERE id=$1 AND processed_at IS NOT NULL)",
    )
    .bind(id)
    .fetch_one(&mut *connection)
    .await?;
    Ok(if completed {
        LeaseCompletion::AlreadyCompleted
    } else {
        LeaseCompletion::LeaseLost
    })
}

/// Acknowledge the exact leased event after its durable effect committed.
///
/// Retained TypeScript ignored the affected-row count of this statement. This
/// boolean therefore reports both a fresh acknowledgement and an already
/// terminal event as success; `complete_lease` exposes the reason.
pub async fn complete(
    connection: &mut PgConnection,
    id: &str,
    lease_token: &str,
    now: i64,
) -> Result<bool, sqlx::Error> {
    Ok(matches!(
        complete_lease(connection, id, lease_token, now).await?,
        LeaseCompletion::Applied | LeaseCompletion::AlreadyCompleted
    ))
}

/// Release a failed attempt with bounded exponential backoff and dead-letter
/// the sixth failure attempt. The caller-supplied code is truncated exactly
/// like the retained worker's UTF-16 slice.
pub async fn fail(
    connection: &mut PgConnection,
    id: &str,
    lease_token: &str,
    attempts: i64,
    now: i64,
    code: &str,
) -> Result<bool, sqlx::Error> {
    if id.is_empty() || lease_token.is_empty() || now < 0 || attempts < 0 {
        return Err(invalid("INVALID_NATIVE_JOB_LEASE"));
    }
    let OutboxRetrySchedule {
        next_attempt_at,
        dead_letter_at,
    } = retry_schedule(attempts, now);
    Ok(sqlx::query(
        "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) \
         UPDATE outbox_events SET claim_until=NULL,lease_token=NULL,next_attempt_at=$1,\
         last_error=$2,dead_letter_at=$3 FROM runtime_owner \
         WHERE id=$4 AND lease_token=$5 AND processed_at IS NULL",
    )
    .bind(next_attempt_at)
    .bind(truncate_error_code(code))
    .bind(dead_letter_at)
    .bind(id)
    .bind(lease_token)
    .execute(connection)
    .await?
    .rows_affected()
        == 1)
}

/// Requeue exactly one dead-lettered event under a static owner predicate.
///
/// The caller must write its audit/receipt in the same unit. A different
/// `dead_letter_at`, a processed event or an event outside the predicate is
/// never requeued.
pub async fn requeue_dead_letter(
    connection: &mut PgConnection,
    owned: &str,
    id: &str,
    dead_letter_at: i64,
    now: i64,
) -> Result<bool, sqlx::Error> {
    if owned.trim().is_empty()
        || owned.len() > 16 * 1024
        || id.is_empty()
        || id.len() > 180
        || !(0..=9_007_199_254_740_991).contains(&dead_letter_at)
        || !(0..=9_007_199_254_740_991).contains(&now)
    {
        return Err(invalid("INVALID_NATIVE_JOB_REQUEUE"));
    }
    Ok(sqlx::query(AssertSqlSafe(format!(
        "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) \
         UPDATE outbox_events e SET dead_letter_at=NULL,next_attempt_at=$1,attempts=0,\
         claim_until=NULL,lease_token=NULL,last_error=NULL FROM runtime_owner \
         WHERE e.id=$2 AND e.dead_letter_at=$3 AND e.processed_at IS NULL AND ({owned})"
    )))
    .bind(now)
    .bind(id)
    .bind(dead_letter_at)
    .execute(connection)
    .await?
    .rows_affected()
        == 1)
}

/// Requeue one exact dead-lettered event and write one Runtime operator audit
/// row in the caller's current transaction.
///
/// `owned` remains a static SQL predicate. The caller commits or rolls back the
/// whole unit; this function never opens or commits a transaction.
pub async fn requeue_dead_letter_with_audit(
    connection: &mut PgConnection,
    owned: &str,
    id: &str,
    dead_letter_at: i64,
    now: i64,
    reason: &str,
    audit_id: &str,
    holding_id: &str,
) -> Result<bool, sqlx::Error> {
    let reason = reason.trim();
    if owned.trim().is_empty()
        || owned.len() > 16 * 1024
        || id.is_empty()
        || id.len() > 180
        || audit_id.is_empty()
        || audit_id.len() > 180
        || holding_id.trim().is_empty()
        || holding_id.len() > 128
        || reason.chars().count() < 3
        || reason.chars().count() > 2000
        || !(0..=9_007_199_254_740_991).contains(&dead_letter_at)
        || !(0..=9_007_199_254_740_991).contains(&now)
        || dead_letter_at > now
    {
        return Err(invalid("INVALID_NATIVE_JOB_REQUEUE"));
    }
    let row = sqlx::query(AssertSqlSafe(format!(
        "SELECT e.aggregate_version,e.attempts,e.last_error,e.payload_json::jsonb->>'companyId' company \
         FROM outbox_events e WHERE e.id=$1 AND e.dead_letter_at=$2 AND e.processed_at IS NULL \
           AND e.published_at IS NULL AND e.lease_token IS NULL AND e.claim_until IS NULL \
           AND ({owned}) FOR UPDATE OF e"
    )))
    .bind(id)
    .bind(dead_letter_at)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let version: i64 = row.try_get("aggregate_version")?;
    let attempts: i64 = row.try_get("attempts")?;
    let previous_error: Option<String> = row.try_get("last_error")?;
    let company: Option<String> = row.try_get("companyId").ok().flatten();
    if !requeue_dead_letter(connection, owned, id, dead_letter_at, now).await? {
        return Ok(false);
    }
    let purpose = serde_json::json!({
        "reason": reason,
        "previousAttempts": attempts,
        "previousError": previous_error,
        "deadLetterAt": dead_letter_at,
    })
    .to_string();
    let authority = serde_json::json!({
        "kind": "runtime_service",
        "runtime": "native-rust",
        "worker": "outbox-requeue",
        "holdingId": holding_id,
        "companyId": company,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
         VALUES($1,$1,'runtime:outbox-requeue','runtime:outbox-requeue','runtime.job.retry','runtime_job',$2,$3,$4,$5,$6)",
    )
    .bind(audit_id)
    .bind(id)
    .bind(version)
    .bind(purpose)
    .bind(now)
    .bind(authority)
    .execute(&mut *connection)
    .await?;
    Ok(true)
}
