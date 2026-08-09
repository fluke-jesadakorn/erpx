use std::{
    fmt::Write as _,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use erpx_infrastructure::SqlxControlPlane;

static HTTP_REQUESTS: AtomicU64 = AtomicU64::new(0);
static HTTP_IN_FLIGHT: AtomicU64 = AtomicU64::new(0);
static HTTP_DURATION_MILLISECONDS: AtomicU64 = AtomicU64::new(0);
static HTTP_RESPONSES_2XX: AtomicU64 = AtomicU64::new(0);
static HTTP_RESPONSES_4XX: AtomicU64 = AtomicU64::new(0);
static HTTP_RESPONSES_5XX: AtomicU64 = AtomicU64::new(0);
static AUTH_FAILURES: AtomicU64 = AtomicU64::new(0);
static READINESS_FAILURES: AtomicU64 = AtomicU64::new(0);
static DB_POOL_ACQUIRES: AtomicU64 = AtomicU64::new(0);
static DB_POOL_WAIT_MILLISECONDS: AtomicU64 = AtomicU64::new(0);
static DB_OBSERVATION_FAILURES: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(crate) struct RequestObservation {
    started: Instant,
}

impl RequestObservation {
    pub(crate) fn start() -> Self {
        HTTP_REQUESTS.fetch_add(1, Ordering::Relaxed);
        HTTP_IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
        Self {
            started: Instant::now(),
        }
    }
}

impl Drop for RequestObservation {
    fn drop(&mut self) {
        HTTP_IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
        HTTP_DURATION_MILLISECONDS.fetch_add(
            self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
    }
}

pub(crate) fn response(status: &str) {
    let status_code = status
        .split_ascii_whitespace()
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(500);
    match status_code {
        200..=299 => {
            HTTP_RESPONSES_2XX.fetch_add(1, Ordering::Relaxed);
        }
        400..=499 => {
            HTTP_RESPONSES_4XX.fetch_add(1, Ordering::Relaxed);
        }
        _ => {
            HTTP_RESPONSES_5XX.fetch_add(1, Ordering::Relaxed);
        }
    }
    if matches!(status_code, 401 | 403) {
        AUTH_FAILURES.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn readiness_failure() {
    READINESS_FAILURES.fetch_add(1, Ordering::Relaxed);
}

#[derive(Debug, Default)]
struct RuntimeSnapshot {
    heartbeat_total: i64,
    heartbeat_fresh: i64,
    outbox_backlog: i64,
    outbox_dead_letters: i64,
    outbox_retry_attempts: i64,
    projection_backlog: i64,
    projection_retryable: i64,
    projection_dead_letters: i64,
    projection_lag_seconds: f64,
    last_verified_backup_timestamp_seconds: f64,
    last_verified_pitr_timestamp_seconds: f64,
}

async fn runtime_snapshot(store: &SqlxControlPlane) -> Result<RuntimeSnapshot, sqlx::Error> {
    let pool = &store.pools().write;
    let wait_started = Instant::now();
    let connection = pool.acquire().await?;
    DB_POOL_ACQUIRES.fetch_add(1, Ordering::Relaxed);
    DB_POOL_WAIT_MILLISECONDS.fetch_add(
        wait_started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        Ordering::Relaxed,
    );
    drop(connection);

    let (heartbeat_total, heartbeat_fresh): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (
             WHERE status='ready' AND last_seen_at >= now() - interval '60 seconds'
         )
         FROM erpx_control.worker_heartbeats",
    )
    .fetch_one(pool)
    .await?;
    let (outbox_backlog, outbox_dead_letters, outbox_retry_attempts): (i64, i64, i64) =
        sqlx::query_as(
            "SELECT
                count(*) FILTER (
                    WHERE published_at IS NULL AND dead_lettered_at IS NULL
                ),
                count(*) FILTER (WHERE dead_lettered_at IS NOT NULL),
                coalesce(sum(attempts), 0)::bigint
             FROM erpx_control.outbox_events",
        )
        .fetch_one(pool)
        .await?;
    let (projection_backlog, projection_retryable, projection_dead_letters): (i64, i64, i64) =
        sqlx::query_as(
            "SELECT
                count(*) FILTER (
                    WHERE state IN ('pending', 'claimed', 'retryable', 'rebuild_pending')
                ),
                count(*) FILTER (WHERE state='retryable'),
                count(*) FILTER (WHERE state='dead_letter')
             FROM erpx_control.tenant_projection_jobs",
        )
        .fetch_one(pool)
        .await?;
    let projection_lag_seconds: f64 = sqlx::query_scalar(
        "SELECT coalesce(
             extract(epoch FROM (now() - min(available_at))),
             0
         )::double precision
         FROM erpx_control.tenant_projection_jobs
         WHERE state IN ('pending', 'claimed', 'retryable', 'rebuild_pending')",
    )
    .fetch_one(pool)
    .await?;
    let last_verified_backup_timestamp_seconds: f64 = sqlx::query_scalar(
        "SELECT coalesce(extract(epoch FROM max(completed_at)), 0)::double precision
         FROM erpx_control.tenant_backup_runs
         WHERE status='completed' AND verified AND completed_at IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;
    let last_verified_pitr_timestamp_seconds: f64 = sqlx::query_scalar(
        "SELECT coalesce(extract(epoch FROM max(created_at)), 0)::double precision
         FROM erpx_control.platform_pitr_recovery_points
         WHERE state='available' AND wal_continuity_verified",
    )
    .fetch_one(pool)
    .await?;
    Ok(RuntimeSnapshot {
        heartbeat_total,
        heartbeat_fresh,
        outbox_backlog,
        outbox_dead_letters,
        outbox_retry_attempts,
        projection_backlog,
        projection_retryable,
        projection_dead_letters,
        projection_lag_seconds: projection_lag_seconds.max(0.0),
        last_verified_backup_timestamp_seconds,
        last_verified_pitr_timestamp_seconds,
    })
}

pub(crate) async fn render(store: &SqlxControlPlane) -> String {
    let (runtime, observation_up) = match runtime_snapshot(store).await {
        Ok(runtime) => (runtime, 1),
        Err(error) => {
            DB_OBSERVATION_FAILURES.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(error_kind = %error.as_database_error().map_or("runtime", |_| "database"), "operational_metrics_observation_failed");
            (RuntimeSnapshot::default(), 0)
        }
    };
    render_snapshot(
        &runtime,
        observation_up,
        u64::from(store.pools().write.size()),
        store.pools().write.num_idle() as u64,
    )
}

fn render_snapshot(
    runtime: &RuntimeSnapshot,
    observation_up: u8,
    pool_connections: u64,
    pool_idle: u64,
) -> String {
    let mut body = String::with_capacity(2_048);
    writeln!(body, "erpx_server_up 1").expect("write to string");
    writeln!(
        body,
        "erpx_http_requests_total {}",
        HTTP_REQUESTS.load(Ordering::Relaxed)
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_http_requests_in_flight {}",
        HTTP_IN_FLIGHT.load(Ordering::Relaxed)
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_http_request_duration_milliseconds_total {}",
        HTTP_DURATION_MILLISECONDS.load(Ordering::Relaxed)
    )
    .expect("write to string");
    for (class, value) in [
        ("2xx", HTTP_RESPONSES_2XX.load(Ordering::Relaxed)),
        ("4xx", HTTP_RESPONSES_4XX.load(Ordering::Relaxed)),
        ("5xx", HTTP_RESPONSES_5XX.load(Ordering::Relaxed)),
    ] {
        writeln!(
            body,
            "erpx_http_responses_total{{class=\"{class}\"}} {value}"
        )
        .expect("write to string");
    }
    writeln!(
        body,
        "erpx_auth_failures_total {}",
        AUTH_FAILURES.load(Ordering::Relaxed)
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_readiness_failures_total {}",
        READINESS_FAILURES.load(Ordering::Relaxed)
    )
    .expect("write to string");
    writeln!(body, "erpx_runtime_metrics_observation_up {observation_up}")
        .expect("write to string");
    writeln!(body, "erpx_db_pool_connections {pool_connections}").expect("write to string");
    writeln!(body, "erpx_db_pool_idle_connections {pool_idle}").expect("write to string");
    writeln!(
        body,
        "erpx_db_pool_acquires_total {}",
        DB_POOL_ACQUIRES.load(Ordering::Relaxed)
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_db_pool_wait_milliseconds_total {}",
        DB_POOL_WAIT_MILLISECONDS.load(Ordering::Relaxed)
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_db_observation_failures_total {}",
        DB_OBSERVATION_FAILURES.load(Ordering::Relaxed)
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_worker_heartbeats{{state=\"fresh\"}} {}",
        runtime.heartbeat_fresh
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_worker_heartbeats{{state=\"stale_or_unavailable\"}} {}",
        runtime
            .heartbeat_total
            .saturating_sub(runtime.heartbeat_fresh)
    )
    .expect("write to string");
    writeln!(body, "erpx_outbox_backlog {}", runtime.outbox_backlog).expect("write to string");
    writeln!(
        body,
        "erpx_outbox_dead_letters {}",
        runtime.outbox_dead_letters
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_outbox_retry_attempts {}",
        runtime.outbox_retry_attempts
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_projection_backlog {}",
        runtime.projection_backlog
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_projection_retryable {}",
        runtime.projection_retryable
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_projection_dead_letters {}",
        runtime.projection_dead_letters
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_projection_lag_seconds {:.3}",
        runtime.projection_lag_seconds
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_last_verified_backup_timestamp_seconds {:.3}",
        runtime.last_verified_backup_timestamp_seconds
    )
    .expect("write to string");
    writeln!(
        body,
        "erpx_last_verified_pitr_timestamp_seconds {:.3}",
        runtime.last_verified_pitr_timestamp_seconds
    )
    .expect("write to string");
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rendered_metrics_use_only_bounded_operational_labels() {
        let body = render_snapshot(&RuntimeSnapshot::default(), 1, 4, 3);
        for metric in [
            "erpx_http_request_duration_milliseconds_total",
            "erpx_db_pool_wait_milliseconds_total",
            "erpx_auth_failures_total",
            "erpx_worker_heartbeats",
            "erpx_outbox_backlog",
            "erpx_projection_lag_seconds",
            "erpx_projection_dead_letters",
            "erpx_last_verified_backup_timestamp_seconds",
            "erpx_last_verified_pitr_timestamp_seconds",
        ] {
            assert!(body.contains(metric), "missing {metric}");
        }
        for forbidden in ["tenant=", "credential=", "payload=", "token="] {
            assert!(
                !body.contains(forbidden),
                "forbidden metric label {forbidden}"
            );
        }
    }
}
