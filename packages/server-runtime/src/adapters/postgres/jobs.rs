//! Durable PostgreSQL outbox queue on the caller's connection.
//!
//! Every mutation writes the native-runtime transaction setting in the same
//! statement as the queue update. The static owner predicate is supplied by the
//! worker composition and must never contain request, user or provider input.
use crate::domain::jobs::{
    DEFAULT_LEASE_MILLIS, MAX_ATTEMPTS, OutboxJob, retry_delay_millis, truncate_error_code,
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

/// Acknowledge the exact leased event after its durable effect committed.
pub async fn complete(
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
         UPDATE outbox_events SET processed_at=$1,published_at=$1,claim_until=NULL,\
         lease_token=NULL,last_error=NULL FROM runtime_owner \
         WHERE id=$2 AND lease_token=$3 AND processed_at IS NULL",
    )
    .bind(now)
    .bind(id)
    .bind(lease_token)
    .execute(connection)
    .await?
    .rows_affected()
        == 1)
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
    let dead_letter_at = (attempts >= MAX_ATTEMPTS).then_some(now);
    Ok(sqlx::query(
        "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) \
         UPDATE outbox_events SET claim_until=NULL,lease_token=NULL,next_attempt_at=$1,\
         last_error=$2,dead_letter_at=$3 FROM runtime_owner \
         WHERE id=$4 AND lease_token=$5 AND processed_at IS NULL",
    )
    .bind(now.saturating_add(retry_delay_millis(attempts)))
    .bind(truncate_error_code(code))
    .bind(dead_letter_at)
    .bind(id)
    .bind(lease_token)
    .execute(connection)
    .await?
    .rows_affected()
        == 1)
}
