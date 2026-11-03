//! Scoped Service Count outbox effects. The caller owns worker activation and
//! ensures the legacy claimant excludes exactly the same Company scope.
use sqlx::{Connection, PgConnection, Row};

/// Privileged worker-process recovery, never an end-user authorization claim.
pub struct DeadLetterRetry<'a> {
    pub event_id: &'a str,
    pub dead_letter_at: i64,
    pub now: i64,
    pub holding_id: &'a str,
    pub reason: &'a str,
    pub audit_id: &'a str,
}

/// The outer worker supplies static owner predicates after validating its
/// private configuration and exact Holding bindings. Repeated or stale ETags
/// do nothing; an ineligible source cannot be rescued by resetting its queue.
pub async fn retry_owned_dead_letter(
    c: &mut PgConnection,
    owned: &str,
    eligible: &str,
    input: DeadLetterRetry<'_>,
) -> Result<bool, sqlx::Error> {
    if input.reason.trim().chars().count() < 3 || input.reason.len() > 2000
        || input.event_id.is_empty() || input.event_id.len() > 180
        || input.holding_id.is_empty() || input.audit_id.is_empty()
        || !(0..=9_007_199_254_740_991).contains(&input.now)
        || !(0..=input.now).contains(&input.dead_letter_at)
    {
        return Err(sqlx::Error::Protocol("Invalid native job recovery input".into()));
    }
    let mut tx = c.begin().await?;
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT e.aggregate_version,e.attempts,e.last_error,e.payload_json::jsonb->>'companyId' company_id FROM outbox_events e WHERE e.id=",
    );
    query.push_bind(input.event_id).push(" AND e.dead_letter_at=")
        .push_bind(input.dead_letter_at)
        .push(" AND e.processed_at IS NULL AND e.published_at IS NULL AND e.lease_token IS NULL AND e.claim_until IS NULL AND (")
        .push(owned).push(") AND (").push(eligible).push(") FOR UPDATE OF e");
    let Some(row) = query.build().fetch_optional(&mut *tx).await? else {
        tx.rollback().await?;
        return Ok(false);
    };
    let attempts: i64 = row.try_get("attempts")?;
    let error: Option<String> = row.try_get("last_error")?;
    let company: Option<String> = row.try_get("company_id")?;
    let version: i64 = row.try_get("aggregate_version")?;
    let purpose = serde_json::json!({
        "reason": input.reason.trim(), "previousAttempts": attempts,
        "previousError": error, "deadLetterAt": input.dead_letter_at,
    }).to_string();
    let authority = serde_json::json!({
        "kind":"runtime_service", "runtime":"native-rust",
        "worker":"field-dependency-worker", "holdingId":input.holding_id,
        "companyId":company,
    }).to_string();
    sqlx::query("WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events SET next_attempt_at=$1,last_error=NULL,dead_letter_at=NULL,attempts=0 FROM runtime_owner WHERE id=$2")
        .bind(input.now).bind(input.event_id).execute(&mut *tx).await?;
    // Preserve the old failure in the audit before clearing retry metadata.
    // This service identity deliberately contains no invented Person/session.
    sqlx::query("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES($1,$1,'runtime:field-dependency-worker','runtime:field-dependency-worker','runtime.job.retry','runtime_job',$2,$3,$4,$5,$6)")
        .bind(input.audit_id).bind(input.event_id).bind(version).bind(purpose)
        .bind(input.now).bind(authority).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

#[derive(Debug, Clone)]
pub struct ServiceCountEvent {
    pub id: String,
    pub aggregate_id: String,
    pub aggregate_version: i64,
    pub attempts: i64,
    pub lease_token: String,
}

/// Claim only events whose immutable create receipt identifies a Service Count
/// in an explicitly enabled Company. This is intentionally opt-in.
pub async fn claim(
    c: &mut PgConnection,
    owned: &str,
    token: &str,
    now: i64,
) -> Result<Option<ServiceCountEvent>, sqlx::Error> {
    let mut q = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events o SET lease_token=",
    );
    q.push_bind(token).push(",claim_until=").push_bind(now.saturating_add(90_000)).push(",attempts=attempts+1 FROM runtime_owner WHERE o.id=(SELECT e.id FROM outbox_events e WHERE e.processed_at IS NULL AND e.dead_letter_at IS NULL AND e.next_attempt_at<=").push_bind(now).push(" AND (e.claim_until IS NULL OR e.claim_until<").push_bind(now).push(") AND (").push(owned).push(") ORDER BY e.created_at,e.id FOR UPDATE OF e SKIP LOCKED LIMIT 1) RETURNING o.id,o.aggregate_id,o.aggregate_version,o.attempts,o.lease_token");
    let row = q.build().fetch_optional(c).await?;
    row.map(|r| {
        Ok(ServiceCountEvent {
            id: r.try_get("id")?,
            aggregate_id: r.try_get("aggregate_id")?,
            aggregate_version: r.try_get("aggregate_version")?,
            attempts: r.try_get("attempts")?,
            lease_token: r.try_get("lease_token")?,
        })
    })
    .transpose()
}

pub async fn eligible(
    c: &mut PgConnection,
    predicate: &str,
    event: &ServiceCountEvent,
) -> Result<bool, sqlx::Error> {
    let mut q = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT EXISTS(SELECT 1 FROM outbox_events e WHERE e.id=",
    );
    q.push_bind(&event.id)
        .push(" AND e.lease_token=")
        .push_bind(&event.lease_token)
        .push(" AND e.processed_at IS NULL AND (")
        .push(predicate)
        .push("))");
    q.build_query_scalar().fetch_one(c).await
}

/// Projection receipt and checkpoint must be committed together on Holding read.
pub async fn project(
    c: &mut PgConnection,
    event: &ServiceCountEvent,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO projection_receipts(event_id,aggregate_id,aggregate_version,applied_at) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
        .bind(&event.id).bind(&event.aggregate_id).bind(event.aggregate_version).bind(now).execute(&mut *c).await?;
    let matches:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projection_receipts WHERE event_id=$1 AND aggregate_id=$2 AND aggregate_version=$3)")
        .bind(&event.id).bind(&event.aggregate_id).bind(event.aggregate_version).fetch_one(&mut *c).await?;
    if !matches {
        return Err(sqlx::Error::RowNotFound);
    }
    sqlx::query("INSERT INTO projection_checkpoints(projector,last_event_id,last_event_at,updated_at) VALUES('work-items',$1,$2,$2) ON CONFLICT(projector) DO UPDATE SET last_event_id=excluded.last_event_id,last_event_at=excluded.last_event_at,updated_at=excluded.updated_at")
        .bind(&event.id).bind(now).execute(c).await?;
    Ok(())
}

pub async fn complete(
    c: &mut PgConnection,
    event: &ServiceCountEvent,
    now: i64,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events SET processed_at=$1,published_at=$1,claim_until=NULL,lease_token=NULL,last_error=NULL FROM runtime_owner WHERE id=$2 AND lease_token=$3 AND processed_at IS NULL")
        .bind(now).bind(&event.id).bind(&event.lease_token).execute(c).await?.rows_affected()==1)
}

pub async fn fail(
    c: &mut PgConnection,
    event: &ServiceCountEvent,
    now: i64,
    code: &str,
) -> Result<bool, sqlx::Error> {
    let delay = (1000_i64 * (1_i64 << event.attempts.clamp(0, 6))).min(60_000);
    let bounded: String = code.chars().take(64).collect();
    Ok(sqlx::query("WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events SET claim_until=NULL,lease_token=NULL,next_attempt_at=$1,last_error=$2,dead_letter_at=$3 FROM runtime_owner WHERE id=$4 AND lease_token=$5 AND processed_at IS NULL")
        .bind(now.saturating_add(delay)).bind(bounded).bind((event.attempts>=6).then_some(now)).bind(&event.id).bind(&event.lease_token).execute(c).await?.rows_affected()==1)
}

pub async fn renew(
    c: &mut PgConnection,
    event: &ServiceCountEvent,
    now: i64,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) UPDATE outbox_events SET claim_until=$1 FROM runtime_owner WHERE id=$2 AND lease_token=$3 AND processed_at IS NULL AND dead_letter_at IS NULL")
        .bind(now.saturating_add(90_000)).bind(&event.id).bind(&event.lease_token).execute(c).await?.rows_affected()==1)
}
