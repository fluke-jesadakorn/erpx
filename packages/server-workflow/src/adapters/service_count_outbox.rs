//! Scoped Service Count outbox effects. The caller owns worker activation and
//! ensures the legacy claimant excludes exactly the same Company scope.
use sqlx::{PgConnection, Row};

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
pub async fn claim(c: &mut PgConnection, owned: &'static str, token: &str, now: i64) -> Result<Option<ServiceCountEvent>, sqlx::Error> {
    let mut q=sqlx::QueryBuilder::<sqlx::Postgres>::new("UPDATE outbox_events o SET lease_token=");
    q.push_bind(token).push(",claim_until=").push_bind(now.saturating_add(90_000)).push(",attempts=attempts+1 WHERE o.id=(SELECT e.id FROM outbox_events e WHERE e.processed_at IS NULL AND e.dead_letter_at IS NULL AND e.next_attempt_at<=").push_bind(now).push(" AND (e.claim_until IS NULL OR e.claim_until<").push_bind(now).push(") AND ").push(owned).push(" ORDER BY e.created_at,e.id FOR UPDATE OF e SKIP LOCKED LIMIT 1) RETURNING o.id,o.aggregate_id,o.aggregate_version,o.attempts,o.lease_token");
    let row=q.build().fetch_optional(c).await?;
    row.map(|r| Ok(ServiceCountEvent { id:r.try_get("id")?, aggregate_id:r.try_get("aggregate_id")?, aggregate_version:r.try_get("aggregate_version")?, attempts:r.try_get("attempts")?, lease_token:r.try_get("lease_token")? })).transpose()
}

pub async fn eligible(c: &mut PgConnection, predicate: &'static str, event: &ServiceCountEvent) -> Result<bool, sqlx::Error> {
    let mut q=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT EXISTS(SELECT 1 FROM outbox_events e WHERE e.id=");
    q.push_bind(&event.id).push(" AND e.lease_token=").push_bind(&event.lease_token).push(" AND e.processed_at IS NULL AND ").push(predicate).push(")");
    q.build_query_scalar().fetch_one(c).await
}

/// Projection receipt and checkpoint must be committed together on Holding read.
pub async fn project(c: &mut PgConnection, event: &ServiceCountEvent, now: i64) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO projection_receipts(event_id,aggregate_id,aggregate_version,applied_at) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
        .bind(&event.id).bind(&event.aggregate_id).bind(event.aggregate_version).bind(now).execute(&mut *c).await?;
    sqlx::query("INSERT INTO projection_checkpoints(projector,last_event_id,last_event_at,updated_at) VALUES('work-items',$1,$2,$2) ON CONFLICT(projector) DO UPDATE SET last_event_id=excluded.last_event_id,last_event_at=excluded.last_event_at,updated_at=excluded.updated_at")
        .bind(&event.id).bind(now).execute(c).await?;
    Ok(())
}

pub async fn complete(c: &mut PgConnection, event: &ServiceCountEvent, now: i64) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE outbox_events SET processed_at=$1,published_at=$1,claim_until=NULL,lease_token=NULL,last_error=NULL WHERE id=$2 AND lease_token=$3 AND processed_at IS NULL")
        .bind(now).bind(&event.id).bind(&event.lease_token).execute(c).await?.rows_affected()==1)
}

pub async fn fail(c: &mut PgConnection, event: &ServiceCountEvent, now: i64, code: &str) -> Result<bool, sqlx::Error> {
    let delay = (1000_i64 * (1_i64 << event.attempts.clamp(0,6))).min(60_000);
    let bounded: String = code.chars().take(64).collect();
    Ok(sqlx::query("UPDATE outbox_events SET claim_until=NULL,lease_token=NULL,next_attempt_at=$1,last_error=$2,dead_letter_at=$3 WHERE id=$4 AND lease_token=$5 AND processed_at IS NULL")
        .bind(now.saturating_add(delay)).bind(bounded).bind((event.attempts>=6).then_some(now)).bind(&event.id).bind(&event.lease_token).execute(c).await?.rows_affected()==1)
}

pub async fn renew(c: &mut PgConnection, event: &ServiceCountEvent, now: i64) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE outbox_events SET claim_until=$1 WHERE id=$2 AND lease_token=$3 AND processed_at IS NULL AND dead_letter_at IS NULL")
        .bind(now.saturating_add(90_000)).bind(&event.id).bind(&event.lease_token).execute(c).await?.rows_affected()==1)
}
