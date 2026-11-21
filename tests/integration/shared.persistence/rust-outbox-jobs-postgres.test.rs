//! Generic Runtime durable outbox queue: exact lease ownership, retry backoff,
//! dead-letter placement and the native-runtime fence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, OutboxJob, OutboxJobHandler, PgTransactionError, WorkerFuture,
    WorkerOutcome, WorkerSupervisor, WorkerTarget, claim, complete, fail, load_migrations,
    migrate_database, renew, requeue_dead_letter, requeue_dead_letter_with_audit, worker_failure,
};
use erpx_server_shared::public::BusinessError;
use sqlx::PgPool;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

const OWNED: &str = "e.event_type LIKE 'field.order.%'";
const NOW: i64 = 1_800_000_000_000;

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,\
         aggregate_version,payload_json,created_at,next_attempt_at,attempts) VALUES \
         ('native-1','command-1','field.order.created','field_order','order-1',1,'{}',10,0,0),\
         ('future-1','command-2','field.order.created','field_order','order-2',1,'{}',20,10000,0),\
         ('retry-1','command-3','field.order.created','field_order','order-3',1,'{}',30,0,0),\
         ('dead-1','command-4','field.order.created','field_order','order-4',1,'{}',40,0,5),\
         ('foreign-1','command-5','billing.invoice.issue','billing_invoice','invoice-1',1,'{}',50,0,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn claims_renews_completes_exactly_one_owned_lease() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let only = "e.event_type LIKE 'field.order.%' AND e.aggregate_id='order-1'";
        let claimed = claim(&mut c, only, "token-1", 1_000).await.unwrap().unwrap();
        assert_eq!(claimed.id, "native-1");
        assert_eq!(claimed.attempts, 1);
        assert_eq!(claimed.lease_token, "token-1");
        assert_eq!(claimed.aggregate_id, "order-1");
        assert!(claim(&mut c, only, "token-2", 1_000).await.unwrap().is_none());
        assert!(renew(&mut c, &claimed.id, &claimed.lease_token, 2_000).await.unwrap());
        let mut stale = claimed.clone();
        stale.lease_token = "forged".into();
        assert!(!renew(&mut c, &stale.id, &stale.lease_token, 2_000).await.unwrap());
        let deadline: i64 =
            sqlx::query_scalar("SELECT claim_until FROM outbox_events WHERE id='native-1'")
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(deadline, 92_000);
        assert!(!complete(&mut c, &stale.id, &stale.lease_token, 3_000).await.unwrap());
        assert!(complete(&mut c, &claimed.id, &claimed.lease_token, 3_000).await.unwrap());
        let row: (Option<i64>, Option<i64>, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT processed_at,published_at,lease_token,claim_until FROM outbox_events WHERE id='native-1'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(row, (Some(3_000), Some(3_000), None, None));
        assert!(claim(&mut c, only, "token-3", 90_000).await.unwrap().is_none());
    })
    .await;
}

#[tokio::test]
async fn retries_with_bounded_backoff_and_dead_letters_the_sixth_attempt() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let retry_owned = "e.event_type LIKE 'field.order.%' AND e.aggregate_id='order-3'";
        let event = claim(&mut c, retry_owned, "retry-token", 2_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.id, "retry-1");
        assert_eq!(event.attempts, 1);
        assert!(fail(&mut c, &event.id, &event.lease_token, event.attempts, 3_000, "TRANSIENT").await.unwrap());
        let planned: (i64, Option<i64>, Option<String>) = sqlx::query_as(
            "SELECT next_attempt_at,dead_letter_at,last_error FROM outbox_events WHERE id='retry-1'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(planned, (5_000, None, Some("TRANSIENT".into())));
        assert!(claim(&mut c, retry_owned, "early", 4_999).await.unwrap().is_none());
        let claimed = claim(&mut c, retry_owned, "on-time", 5_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.attempts, 2);
        assert!(fail(&mut c, &claimed.id, &claimed.lease_token, claimed.attempts, 5_000, &("x".repeat(100))).await.unwrap());
        let error: String =
            sqlx::query_scalar("SELECT last_error FROM outbox_events WHERE id='retry-1'")
                .fetch_one(&mut *c)
                .await
                .unwrap();
        assert_eq!(error, "x".repeat(64));
        let dead_owned = "e.event_type LIKE 'field.order.%' AND e.aggregate_id='order-4'";
        let dead = claim(&mut c, dead_owned, "dead-token", 6_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dead.id, "dead-1");
        assert_eq!(dead.attempts, 6);
        assert!(fail(&mut c, &dead.id, &dead.lease_token, dead.attempts, 6_000, "EXHAUSTED").await.unwrap());
        let terminal: (Option<i64>, Option<i64>, Option<String>) = sqlx::query_as(
            "SELECT dead_letter_at,lease_token,last_error FROM outbox_events WHERE id='dead-1'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(terminal, (Some(6_000), None, Some("EXHAUSTED".into())));
        assert!(
            claim(&mut c, dead_owned, "after-dead", 10_000_000)
                .await
                .unwrap()
                .is_none()
        );
        let unicode = "😀".repeat(40);
        assert_eq!(
            erpx_server_runtime::public::postgres::truncate_error_code(&unicode),
            "😀".repeat(32)
        );
    })
    .await;
}

#[tokio::test]
async fn static_owner_predicate_is_required_and_the_native_fence_stays_closed() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        assert!(claim(&mut c, " ", "token", 1_000).await.is_err());
        assert!(claim(&mut c, OWNED, "", 1_000).await.is_err());
        assert!(claim(&mut c, OWNED, "token", -1).await.is_err());
        // The native fence refuses an ordinary update without the runtime GUC.
        let denied =
            sqlx::query("UPDATE outbox_events SET lease_token='plain' WHERE id='native-1'")
                .execute(&mut *c)
                .await
                .unwrap_err();
        assert_eq!(
            denied
                .as_database_error()
                .and_then(|error| error.constraint()),
            Some("native_outbox_runtime_required")
        );
        let native = claim(&mut c, OWNED, "native", 1_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(native.id, "native-1");
        assert!(
            complete(&mut c, &native.id, &native.lease_token, 1_100)
                .await
                .unwrap()
        );
    })
    .await;
}

struct Handler {
    seen: Arc<Mutex<Vec<String>>>,
    fail: bool,
}
impl OutboxJobHandler for Handler {
    fn handle<'a>(&'a self, _: &'a PgPool, job: &'a OutboxJob, _: i64) -> WorkerFuture<'a> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(job.id.clone());
            if self.fail {
                Err(worker_failure("SYNTHETIC_WORKER_FAILURE"))
            } else {
                Ok(WorkerOutcome::Complete)
            }
        })
    }
}

async fn migrate(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn seed(pool: &DatabasePool, id: &str, event_type: &str, attempts: i64) {
    sqlx::query(
        "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at,attempts,next_attempt_at) \
         VALUES($1,$2,$3,'synthetic','aggregate',1,'{}',0,$4,0)",
    )
    .bind(id)
    .bind(format!("command-{id}"))
    .bind(event_type)
    .bind(attempts)
    .execute(pool.raw())
    .await
    .unwrap();
}

fn target(name: &str, pool: &DatabasePool, handler: Arc<dyn OutboxJobHandler>) -> WorkerTarget {
    WorkerTarget {
        name: name.into(),
        write: pool.raw().clone(),
        owned: "event_type='owned'".into(),
        handler,
    }
}

#[tokio::test]
async fn drains_each_explicit_holding_and_ignores_foreign_events() {
    fixture::with_database(2, |first| async move {
        fixture::with_database(2, move |second| async move {
            migrate(&first).await;
            migrate(&second).await;
            seed(&first, "owned-1", "owned", 0).await;
            seed(&first, "foreign", "foreign", 0).await;
            seed(&second, "owned-2", "owned", 0).await;
            let seen = Arc::new(Mutex::new(Vec::<String>::new()));
            let handler = Arc::new(Handler {
                seen: seen.clone(),
                fail: false,
            });
            let supervisor = WorkerSupervisor::new(
                vec![
                    target("h001", &first, handler.clone()),
                    target("h002", &second, handler),
                ],
                Duration::from_millis(10),
            )
            .unwrap();
            assert_eq!(supervisor.run_once(NOW).await.unwrap(), 2);
            let mut found = seen.lock().unwrap().clone();
            found.sort();
            assert_eq!(found, vec!["owned-1", "owned-2"]);
            let first_state: (Option<i64>, Option<String>, i64) = sqlx::query_as(
                "SELECT processed_at,lease_token,attempts FROM outbox_events WHERE id='owned-1'",
            )
            .fetch_one(first.raw())
            .await
            .unwrap();
            assert!(first_state.0.is_some_and(|value| value >= NOW));
            assert_eq!(first_state.1, None);
            assert_eq!(first_state.2, 1);
            let foreign: (Option<i64>, i64) = sqlx::query_as(
                "SELECT processed_at,attempts FROM outbox_events WHERE id='foreign'",
            )
            .fetch_one(first.raw())
            .await
            .unwrap();
            assert_eq!(foreign, (None, 0));
            let second_state: (Option<i64>, i64) = sqlx::query_as(
                "SELECT processed_at,attempts FROM outbox_events WHERE id='owned-2'",
            )
            .fetch_one(second.raw())
            .await
            .unwrap();
            assert!(second_state.0.is_some_and(|value| value >= NOW));
            assert_eq!(second_state.1, 1);
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn records_a_failure_and_dead_letters_the_sixth_attempt() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool, "failing", "owned", 5).await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let handler = Arc::new(Handler {
            seen,
            fail: true,
        });
        let supervisor = WorkerSupervisor::new(
            vec![target("h001", &pool, handler)],
            Duration::from_millis(10),
        )
        .unwrap();
        let result = supervisor.run_once(NOW).await;
        assert!(matches!(
            result,
            Err(PgTransactionError::Business(BusinessError {
                code: "SYNTHETIC_WORKER_FAILURE",
                ..
            }))
        ));
        let state: (i64, Option<i64>, Option<i64>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT attempts,processed_at,dead_letter_at,last_error,lease_token FROM outbox_events WHERE id='failing'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(state.0, 6);
        assert_eq!(state.1, None);
        assert!(state.2.is_some_and(|value| value >= NOW));
        assert_eq!(state.3.as_deref(), Some("SYNTHETIC_WORKER_FAILURE"));
        assert_eq!(state.4, None);
        let next: i64 = sqlx::query_scalar("SELECT next_attempt_at FROM outbox_events WHERE id='failing'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert!(next > NOW);
    })
    .await;
}

#[tokio::test]
async fn rejects_empty_targets_and_owner_predicates() {
    assert!(WorkerSupervisor::new(Vec::new(), Duration::from_millis(1)).is_err());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused")
        .unwrap();
    let handler = Arc::new(Handler {
        seen: Arc::new(Mutex::new(Vec::new())),
        fail: false,
    });
    assert!(
        WorkerSupervisor::new(
            vec![WorkerTarget {
                name: "h001".into(),
                write: pool,
                owned: " ".into(),
                handler,
            }],
            Duration::from_millis(1),
        )
        .is_err()
    );
}

#[tokio::test]
async fn requeues_only_the_exact_owned_dead_letter() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true))              UPDATE outbox_events SET dead_letter_at=500,attempts=6,last_error='DEAD' FROM runtime_owner WHERE id='dead-1'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let mut c = pool.raw().acquire().await.unwrap();
        assert!(!requeue_dead_letter(
            &mut c,
            "e.event_type='billing.invoice.issue'",
            "dead-1",
            500,
            NOW,
        )
        .await
        .unwrap());
        assert!(!requeue_dead_letter(
            &mut c,
            "e.event_type LIKE 'field.order.%'",
            "dead-1",
            499,
            NOW,
        )
        .await
        .unwrap());
        assert!(requeue_dead_letter(
            &mut c,
            "e.event_type LIKE 'field.order.%'",
            "dead-1",
            500,
            NOW,
        )
        .await
        .unwrap());
        let state: (Option<i64>, i64, Option<i64>, Option<String>, i64) = sqlx::query_as(
            "SELECT dead_letter_at,attempts,processed_at,last_error,next_attempt_at FROM outbox_events WHERE id='dead-1'",
        )
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(state, (None, 0, None, None, NOW));
        assert!(!requeue_dead_letter(
            &mut c,
            "e.event_type LIKE 'field.order.%'",
            "dead-1",
            500,
            NOW,
        )
        .await
        .unwrap());
    })
    .await;
}

#[tokio::test]
async fn requeues_with_operator_audit_in_one_unit() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "WITH runtime_owner AS MATERIALIZED (SELECT set_config('erpx.outbox_runtime','native-rust',true)) \
             UPDATE outbox_events SET dead_letter_at=500,attempts=6,last_error='DEAD' FROM runtime_owner WHERE id='dead-1'",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        assert!(!requeue_dead_letter_with_audit(
            &mut tx,
            "e.event_type='billing.invoice.issue'",
            "dead-1",
            500,
            NOW,
            "Operator recovery",
            "audit-1",
            "h001",
        )
        .await
        .unwrap());
        assert!(requeue_dead_letter_with_audit(
            &mut tx,
            "e.event_type LIKE 'field.order.%'",
            "dead-1",
            500,
            NOW,
            "Operator recovery",
            "audit-1",
            "h001",
        )
        .await
        .unwrap());
        let state: (Option<i64>, i64, Option<String>) = sqlx::query_as(
            "SELECT dead_letter_at,attempts,last_error FROM outbox_events WHERE id='dead-1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(state, (None, 0, None));
        let audit: (String, String, String, i64, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT action,record_type,record_id,record_version,purpose::jsonb->>'reason',authority_json::jsonb->>'holdingId' FROM audit_events WHERE id='audit-1'",
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(audit.0, "runtime.job.retry");
        assert_eq!(audit.1, "runtime_job");
        assert_eq!(audit.2, "dead-1");
        assert_eq!(audit.3, 1);
        assert_eq!(audit.4.as_deref(), Some("Operator recovery"));
        assert_eq!(audit.5.as_deref(), Some("h001"));
        tx.commit().await.unwrap();
    })
    .await;
}
