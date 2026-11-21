//! Native general outbox supervisor on disposable PostgreSQL databases:
//! exact lease ownership, expiry, skip locked, idempotent completion, retained
//! retry/dead-letter, attestation proof/row, one heavy slot, per-Holding
//! independence and graceful shutdown/restart logging.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, LeaseCompletion, MigrationTrack, OutboxAttestation, OutboxHandlerRegistry,
    OutboxHolding, OutboxHoldingOutcome, OutboxJob, OutboxJobHandler, OutboxSupervisor,
    OutboxSupervisorPolicy, WorkerFuture, WorkerOutcome, attest_outbox_holding, claim, complete,
    complete_lease, load_migrations, migrate_database, record_outbox_attestation,
    run_outbox_supervisor, worker_failure,
};
use erpx_server_shared::public::{Clock, TimestampMillis};
use sqlx::PgPool;
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

const NOW: i64 = 1_800_000_000_000;

type EventState = (
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<i64>,
    i64,
    Option<String>,
);

struct FixedClock(i64);
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0)
    }
}

fn policy() -> OutboxSupervisorPolicy {
    OutboxSupervisorPolicy {
        idle_interval: Duration::from_millis(10),
        busy_interval: Duration::from_millis(5),
        lease_refresh: Duration::from_millis(50),
        restart_backoff: Duration::from_millis(5),
        max_events_per_pass: 64,
    }
}

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres")
}

async fn migrate_holding(pool: &DatabasePool) {
    let files = load_migrations(root(), MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn seed(pool: &DatabasePool, id: &str, event_type: &str, attempts: i64, created_at: i64) {
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at,attempts,next_attempt_at) \
         VALUES($1,$2,$3,'synthetic','aggregate',1,'{}',$4,$5,0)",
    )
    .bind(id)
    .bind(format!("command-{id}"))
    .bind(event_type)
    .bind(created_at)
    .bind(attempts)
    .execute(pool.raw())
    .await
    .unwrap();
}

fn attestation(holding_id: &str) -> OutboxAttestation {
    OutboxAttestation::new(
        holding_id,
        "erpx_dev_rust_contract",
        "erpx_test_admin",
        true,
        1_097,
        &"a".repeat(64),
    )
    .unwrap()
}

fn build_supervisor(
    holding_id: &str,
    pool: &DatabasePool,
    owned: &str,
    handler: Arc<dyn OutboxJobHandler>,
) -> OutboxSupervisor {
    supervisor_with_policy(holding_id, pool, owned, handler, policy())
}

fn supervisor_with_policy(
    holding_id: &str,
    pool: &DatabasePool,
    owned: &str,
    handler: Arc<dyn OutboxJobHandler>,
    policy: OutboxSupervisorPolicy,
) -> OutboxSupervisor {
    let holding = OutboxHolding::new(
        holding_id,
        pool.raw().clone(),
        owned,
        attestation(holding_id),
    )
    .unwrap();
    let mut registry = OutboxHandlerRegistry::new();
    registry.register(holding_id, handler).unwrap();
    OutboxSupervisor::new(vec![holding], &registry, policy).unwrap()
}

struct RecordingHandler {
    seen: Arc<Mutex<Vec<String>>>,
}
impl OutboxJobHandler for RecordingHandler {
    fn handle<'a>(&'a self, _: &'a PgPool, job: &'a OutboxJob, _: i64) -> WorkerFuture<'a> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(job.id.clone());
            Ok(WorkerOutcome::Complete)
        })
    }
}

struct FailHandler;
impl OutboxJobHandler for FailHandler {
    fn handle<'a>(&'a self, _: &'a PgPool, _: &'a OutboxJob, _: i64) -> WorkerFuture<'a> {
        Box::pin(async { Ok(WorkerOutcome::Fail("TRANSIENT")) })
    }
}

struct ErrorHandler;
impl OutboxJobHandler for ErrorHandler {
    fn handle<'a>(&'a self, _: &'a PgPool, _: &'a OutboxJob, _: i64) -> WorkerFuture<'a> {
        Box::pin(async { Err(worker_failure("HANDLER_FAILED")) })
    }
}

/// Simulates a process death after the durable effect committed but before the
/// supervisor acknowledges the lease.
struct AlreadyCompletingHandler;
impl OutboxJobHandler for AlreadyCompletingHandler {
    fn handle<'a>(&'a self, write: &'a PgPool, job: &'a OutboxJob, now: i64) -> WorkerFuture<'a> {
        Box::pin(async move {
            let mut connection = write
                .acquire()
                .await
                .map_err(|_| worker_failure("COMPLETION_CONNECTION_FAILED"))?;
            if !complete(&mut connection, &job.id, &job.lease_token, now)
                .await
                .map_err(|_| worker_failure("COMPLETION_FAILED"))?
            {
                return Err(worker_failure("COMPLETION_LEASE_LOST"));
            }
            Ok(WorkerOutcome::Complete)
        })
    }
}

struct HeavyHandler {
    in_flight: Arc<AtomicUsize>,
    max_in_flight: Arc<AtomicUsize>,
}
impl OutboxJobHandler for HeavyHandler {
    fn handle<'a>(&'a self, _: &'a PgPool, _: &'a OutboxJob, _: i64) -> WorkerFuture<'a> {
        Box::pin(async move {
            let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_in_flight.fetch_max(current, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            Ok(WorkerOutcome::Complete)
        })
    }
}

#[tokio::test]
async fn claims_completes_with_exact_lease_and_is_idempotent_after_effect_commit() {
    fixture::with_database(2, |pool| async move {
        migrate_holding(&pool).await;
        seed(&pool, "event-1", "owned", 0, 10).await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let supervisor = build_supervisor(
            "h001",
            &pool,
            "event_type='owned'",
            Arc::new(RecordingHandler { seen: seen.clone() }),
        );
        let report = supervisor.run_once(NOW).await;
        assert_eq!(report.completed, 1);
        assert_eq!(report.rejected, 0);
        assert_eq!(seen.lock().unwrap().as_slice(), ["event-1"]);

        let state: EventState = sqlx::query_as(
            "SELECT processed_at,published_at,lease_token,claim_until,attempts,last_error \
                 FROM outbox_events WHERE id='event-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(state.0.is_some_and(|value| value >= NOW));
        assert_eq!(state.0, state.1);
        assert_eq!(state.2, None);
        assert_eq!(state.3, None);
        assert_eq!(state.4, 1);
        assert_eq!(state.5, None);

        // The supervisor claims nothing terminal on the next pass.
        assert_eq!(supervisor.run_once(NOW + 1).await.completed, 0);

        // An effect committed before acknowledgement is recovered as success.
        seed(&pool, "event-2", "owned", 0, 20).await;
        let replay = build_supervisor(
            "h001",
            &pool,
            "event_type='owned'",
            Arc::new(AlreadyCompletingHandler),
        )
        .run_once(NOW + 2)
        .await;
        assert_eq!(replay.completed, 1);
        assert!(matches!(
            replay.holdings[0].1,
            OutboxHoldingOutcome::Completed { replayed: true, .. }
        ));

        // Explicit completion remains idempotent for callers awaiting a crash window.
        seed(&pool, "event-3", "owned", 0, 30).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let claimed = claim(
            &mut connection,
            "event_type='owned'",
            "token-idempotent",
            NOW + 3,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(claimed.id, "event-3");
        assert_eq!(
            complete_lease(&mut connection, &claimed.id, &claimed.lease_token, NOW + 4)
                .await
                .unwrap(),
            LeaseCompletion::Applied
        );
        assert_eq!(
            complete_lease(&mut connection, &claimed.id, &claimed.lease_token, NOW + 5)
                .await
                .unwrap(),
            LeaseCompletion::AlreadyCompleted
        );
        assert!(
            complete(&mut connection, &claimed.id, &claimed.lease_token, NOW + 6)
                .await
                .unwrap()
        );
        assert_eq!(
            complete_lease(&mut connection, &claimed.id, "forged", NOW + 7)
                .await
                .unwrap(),
            LeaseCompletion::AlreadyCompleted,
            "a terminal event is never reopened by a forged token"
        );
    })
    .await;
}

#[tokio::test]
async fn retries_with_retained_backoff_and_dead_letters_exactly_the_sixth_attempt() {
    fixture::with_database(2, |pool| async move {
        migrate_holding(&pool).await;
        seed(&pool, "retry", "owned", 0, 10).await;
        let supervisor = build_supervisor("h001", &pool, "event_type='owned'", Arc::new(FailHandler));
        let report = supervisor.run_once(NOW).await;
        assert_eq!(report.retried, 1);
        assert_eq!(report.dead_lettered, 0);
        assert_eq!(report.rejected, 0);
        let planned: (i64, Option<i64>, Option<String>, Option<String>, Option<i64>) =
            sqlx::query_as(
                "SELECT next_attempt_at,dead_letter_at,last_error,lease_token,claim_until \
                 FROM outbox_events WHERE id='retry'",
            )
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert!(
            (NOW + 2_000..=NOW + 3_000).contains(&planned.0),
            "failure time is claim start plus elapsed handler time"
        );
        assert_eq!(planned.1, None);
        assert_eq!(planned.2.as_deref(), Some("TRANSIENT"));
        assert_eq!(planned.3, None);
        assert_eq!(planned.4, None);

        let mut connection = pool.raw().acquire().await.unwrap();
        assert!(claim(
            &mut connection,
            "event_type='owned'",
            "too-early",
            planned.0 - 1
        )
        .await
        .unwrap()
        .is_none());
        assert_eq!(
            claim(&mut connection, "event_type='owned'", "on-time", planned.0)
                .await
                .unwrap()
                .unwrap()
                .attempts,
            2
        );

        seed(&pool, "dead", "terminal", 5, 20).await;
        let terminal = build_supervisor(
            "h002",
            &pool,
            "event_type='terminal'",
            Arc::new(FailHandler),
        )
        .run_once(NOW)
        .await;
        assert_eq!(terminal.retried, 1);
        assert_eq!(terminal.dead_lettered, 1);
        let dead: (i64, Option<i64>, Option<i64>, Option<String>) = sqlx::query_as(
            "SELECT attempts,dead_letter_at,lease_token,last_error FROM outbox_events WHERE id='dead'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(dead.0, 6);
        assert!(dead.1.is_some_and(|value| value >= NOW));
        assert_eq!(dead.2, None);
        assert_eq!(dead.3.as_deref(), Some("TRANSIENT"));
        assert!(
            claim(&mut connection, "event_type='terminal'", "after-dead", NOW + 10_000_000)
                .await
                .unwrap()
                .is_none()
        );

        // A handler error is recorded as a failed attempt with its stable code.
        seed(&pool, "handler-error", "handler-error", 0, 30).await;
        let errored = build_supervisor(
            "h003",
            &pool,
            "event_type='handler-error'",
            Arc::new(ErrorHandler),
        )
        .run_once(NOW)
        .await;
        assert_eq!(errored.retried, 1);
        let code: String =
            sqlx::query_scalar("SELECT last_error FROM outbox_events WHERE id='handler-error'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(code, "HANDLER_FAILED");
    })
    .await;
}

#[tokio::test]
async fn live_lease_is_never_stolen_and_an_expired_lease_is_reclaimed() {
    fixture::with_database(2, |pool| async move {
        migrate_holding(&pool).await;
        seed(&pool, "leased", "owned", 0, 10).await;
        let mut connection = pool.raw().acquire().await.unwrap();
        let manual = claim(&mut connection, "event_type='owned'", "manual", NOW)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(manual.attempts, 1);
        drop(connection);

        let supervisor = build_supervisor(
            "h001",
            &pool,
            "event_type='owned'",
            Arc::new(RecordingHandler {
                seen: Arc::new(Mutex::new(Vec::new())),
            }),
        );
        let live = supervisor.run_once(NOW + 1).await;
        assert_eq!(live.completed, 0);
        assert_eq!(live.holdings[0].1, OutboxHoldingOutcome::Idle);
        let attempts: i64 =
            sqlx::query_scalar("SELECT attempts FROM outbox_events WHERE id='leased'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(attempts, 1, "a live lease is not re-claimed");

        let expired = supervisor.run_once(NOW + 90_001).await;
        assert_eq!(expired.completed, 1);
        let attempts: i64 =
            sqlx::query_scalar("SELECT attempts FROM outbox_events WHERE id='leased'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(
            attempts, 2,
            "the expired lease is claimed exactly once more"
        );
    })
    .await;
}

#[tokio::test]
async fn skip_locked_moves_past_a_held_row_without_double_claiming() {
    fixture::with_database(2, |pool| async move {
        migrate_holding(&pool).await;
        seed(&pool, "first", "owned", 0, 10).await;
        seed(&pool, "second", "owned", 0, 20).await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let supervisor = build_supervisor(
            "h001",
            &pool,
            "event_type='owned'",
            Arc::new(RecordingHandler { seen: seen.clone() }),
        );

        let mut held = pool.raw().begin().await.unwrap();
        sqlx::query("SELECT id FROM outbox_events WHERE id='first' FOR UPDATE")
            .execute(&mut *held)
            .await
            .unwrap();
        let first_pass = supervisor.run_once(NOW).await;
        assert_eq!(first_pass.completed, 1);
        assert_eq!(seen.lock().unwrap().as_slice(), ["second"]);
        let first_attempts: i64 =
            sqlx::query_scalar("SELECT attempts FROM outbox_events WHERE id='first'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(first_attempts, 0, "the locked row is skipped, not claimed");
        held.rollback().await.unwrap();

        let second_pass = supervisor.run_once(NOW + 1).await;
        assert_eq!(second_pass.completed, 1);
        let mut found = seen.lock().unwrap().clone();
        found.sort();
        assert_eq!(found, ["first", "second"]);
    })
    .await;
}

#[tokio::test]
async fn one_heavy_slot_and_per_holding_independence_are_preserved() {
    fixture::with_database(3, |pool| async move {
        migrate_holding(&pool).await;
        seed(&pool, "a", "a", 0, 10).await;
        seed(&pool, "b", "b", 0, 20).await;

        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_in_flight = Arc::new(AtomicUsize::new(0));
        let handler = Arc::new(HeavyHandler {
            in_flight: in_flight.clone(),
            max_in_flight: max_in_flight.clone(),
        });
        let mut registry = OutboxHandlerRegistry::new();
        registry.register("h001", handler.clone()).unwrap();
        registry.register("h002", handler.clone()).unwrap();
        let supervisor = OutboxSupervisor::new(
            vec![
                OutboxHolding::new(
                    "h001",
                    pool.raw().clone(),
                    "event_type='a'",
                    attestation("h001"),
                )
                .unwrap(),
                OutboxHolding::new(
                    "h002",
                    pool.raw().clone(),
                    "event_type='b'",
                    attestation("h002"),
                )
                .unwrap(),
            ],
            &registry,
            policy(),
        )
        .unwrap();
        let report = supervisor.run_once(NOW).await;
        assert_eq!(report.completed, 2);
        assert_eq!(max_in_flight.load(Ordering::SeqCst), 1);
        assert_eq!(in_flight.load(Ordering::SeqCst), 0);

        // A handler failure in one Holding does not stop another Holding.
        seed(&pool, "fail", "fail", 0, 30).await;
        seed(&pool, "ok", "ok", 0, 40).await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut independent = OutboxHandlerRegistry::new();
        independent.register("h001", Arc::new(FailHandler)).unwrap();
        independent
            .register("h002", Arc::new(RecordingHandler { seen: seen.clone() }))
            .unwrap();
        let independent = OutboxSupervisor::new(
            vec![
                OutboxHolding::new(
                    "h001",
                    pool.raw().clone(),
                    "event_type='fail'",
                    attestation("h001"),
                )
                .unwrap(),
                OutboxHolding::new(
                    "h002",
                    pool.raw().clone(),
                    "event_type='ok'",
                    attestation("h002"),
                )
                .unwrap(),
            ],
            &independent,
            policy(),
        )
        .unwrap();
        let report = independent.run_once(NOW + 1).await;
        assert_eq!(report.retried, 1);
        assert_eq!(report.completed, 1);
        assert_eq!(report.rejected, 0);
        assert_eq!(seen.lock().unwrap().as_slice(), ["ok"]);
    })
    .await;
}

#[tokio::test]
async fn attestation_proof_is_recorded_as_one_control_health_row() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let control_files = load_migrations(root(), MigrationTrack::Control)
                .await
                .unwrap();
            migrate_database(&control, MigrationTrack::Control, &control_files)
                .await
                .unwrap();
            migrate_holding(&holding).await;
            let database: String = sqlx::query_scalar("SELECT current_database()::text")
                .fetch_one(holding.raw())
                .await
                .unwrap();
            let files = load_migrations(root(), MigrationTrack::HoldingWrite)
                .await
                .unwrap();
            let attestation = attest_outbox_holding(
                &holding,
                "h001",
                Some(&database),
                MigrationTrack::HoldingWrite,
                &files,
            )
            .await
            .unwrap();
            assert_eq!(attestation.database, database);
            assert!(attestation.primary);
            assert_eq!(attestation.migration_count, files.len());
            let expected_sha = attestation.migration_sha256.clone();

            let mismatch = attest_outbox_holding(
                &holding,
                "h001",
                Some("other-database"),
                MigrationTrack::HoldingWrite,
                &files,
            )
            .await
            .unwrap_err();
            assert_eq!(mismatch.code, "WORKER_DATABASE_IDENTITY_MISMATCH");

            let mut registry = OutboxHandlerRegistry::new();
            registry
                .register(
                    "h001",
                    Arc::new(RecordingHandler {
                        seen: Arc::new(Mutex::new(Vec::new())),
                    }),
                )
                .unwrap();
            let supervisor = OutboxSupervisor::new(
                vec![
                    OutboxHolding::new(
                        "h001",
                        holding.raw().clone(),
                        "event_type='owned'",
                        attestation,
                    )
                    .unwrap(),
                ],
                &registry,
                policy(),
            )
            .unwrap();
            record_outbox_attestation(control.raw(), "outbox-supervisor", &supervisor, NOW)
                .await
                .unwrap();
            let details: String = sqlx::query_scalar(
                "SELECT details_json FROM runtime_health WHERE service='outbox-supervisor'",
            )
            .fetch_one(control.raw())
            .await
            .unwrap();
            let details: serde_json::Value = serde_json::from_str(&details).unwrap();
            assert_eq!(details["runtime"], "native-rust");
            assert_eq!(details["supervisor"], "outbox");
            assert_eq!(details["attestations"][0]["holdingId"], "h001");
            assert_eq!(details["attestations"][0]["database"], database);
            assert_eq!(details["attestations"][0]["primary"], true);
            assert_eq!(details["attestations"][0]["migrationSha256"], expected_sha);
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn graceful_shutdown_drains_a_completed_pass_and_reports_it() {
    fixture::with_database(2, |pool| async move {
        migrate_holding(&pool).await;
        seed(&pool, "shutdown", "owned", 0, 10).await;
        let supervisor = supervisor_with_policy(
            "h001",
            &pool,
            "event_type='owned'",
            Arc::new(RecordingHandler {
                seen: Arc::new(Mutex::new(Vec::new())),
            }),
            OutboxSupervisorPolicy {
                idle_interval: Duration::from_millis(20),
                ..policy()
            },
        );
        let summary = tokio::time::timeout(
            Duration::from_secs(5),
            run_outbox_supervisor(supervisor, Arc::new(FixedClock(NOW)), async {
                tokio::time::sleep(Duration::from_millis(60)).await;
            }),
        )
        .await
        .expect("supervisor shutdown timed out")
        .unwrap();
        assert!(summary.graceful_shutdown);
        assert!(summary.completed >= 1);
        assert!(summary.passes >= 1);
        let processed: Option<i64> =
            sqlx::query_scalar("SELECT processed_at FROM outbox_events WHERE id='shutdown'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert!(processed.is_some());
    })
    .await;
}

#[tokio::test]
async fn a_closed_pool_logs_restarts_and_shutdown_still_returns_cleanly() {
    fixture::with_database(2, |pool| async move {
        migrate_holding(&pool).await;
        let supervisor = build_supervisor(
            "h001",
            &pool,
            "event_type='owned'",
            Arc::new(RecordingHandler {
                seen: Arc::new(Mutex::new(Vec::new())),
            }),
        );
        pool.close().await;
        let summary = tokio::time::timeout(
            Duration::from_secs(5),
            run_outbox_supervisor(supervisor, Arc::new(FixedClock(NOW)), async {
                tokio::time::sleep(Duration::from_millis(30)).await;
            }),
        )
        .await
        .expect("restarting supervisor shutdown timed out")
        .unwrap();
        assert!(summary.graceful_shutdown);
        assert!(summary.restarts >= 1);
        assert!(summary.rejected >= 1);
        assert_eq!(summary.completed, 0);
    })
    .await;
}
