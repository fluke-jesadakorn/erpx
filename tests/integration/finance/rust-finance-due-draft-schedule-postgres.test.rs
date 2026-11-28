//! Postgres proof of the native Finance due-draft schedule command.
//!
//! The schedule path writes the draft row plus the command receipt, audit
//! event and outbox event inside the caller's serializable transaction:
//! the happy path resolves the exact record id, replay on the same
//! idempotency key returns the retained result, unknown/inactive/stale
//! sources fail with the retained oracle codes, and a caller-transaction
//! rollback stages nothing.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_finance_server::public::due_draft_schedule::{
    DueDraftCommandActor, DueDraftScheduleResult, decide_due_draft, schedule_due_draft,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};
use sqlx::Row;

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
          ('*','finance_writer','Finance writer',false,1,'active'); \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
          ('*','finance_writer',1,'[\"finance.write\"]','[]','[\"holding\"]','fixture',0); \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
          ('grant-1','user-1','finance_writer','[]','co',0,9007199254740991,3,'holding'); \
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
          ('cust-1','co','customer','C1','Customer','THB',0,0); \
         INSERT INTO finance_charts(id,company_id,name,base_currency,status,version,created_by,created_at,updated_at) VALUES \
          ('chart-1','co','Main','THB','active',1,'maker',0,0); \
         INSERT INTO finance_business_contracts(id,company_id,root_id,number,name,chart_id,party_id,start_date,end_date,edition,version,status,content_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES \
          ('contract-active','co','contract-active','C-1','Contract','chart-1','cust-1','2026-01-01','2026-12-31',1,5,'active','{}','hash','maker','reason',0,0), \
          ('contract-draft','co','contract-draft','C-2','Draft','chart-1','cust-1','2026-01-01','2026-12-31',1,2,'draft','{}','hash','maker','reason',0,0)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn actor() -> DueDraftCommandActor {
    DueDraftCommandActor {
        user_id: "user-1".to_owned(),
        assignment_id: "grant-1".to_owned(),
        role: "finance_writer".to_owned(),
        company_id: "co".to_owned(),
        holding_id: "holding-1".to_owned(),
        scope_kind: "holding".to_owned(),
    }
}

async fn schedule(
    pool: &DatabasePool,
    key: &str,
    source_id: &str,
    payload: &str,
) -> Result<DueDraftScheduleResult, PgTransactionError<BusinessError>> {
    let key = key.to_owned();
    let source_id = source_id.to_owned();
    let payload = payload.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let key = key.clone();
            let source_id = source_id.clone();
            let payload = payload.clone();
            Box::pin(async move {
                let command =
                    CommandKey::new(key).map_err(PgTransactionError::Business)?;
                schedule_due_draft(
                    connection,
                    &actor(),
                    &command,
                    "billing",
                    &source_id,
                    "2026-11-30",
                    &payload,
                    "Quarter close draft",
                    1_000,
                )
                .await
            })
        })
        .await
}

fn denied(
    result: &Result<DueDraftScheduleResult, PgTransactionError<BusinessError>>,
) -> (&'static str, &'static str) {
    match result {
        Err(PgTransactionError::Business(error)) => (error.code, error.message),
        _ => panic!("expected an oracle denial, got {result:?}"),
    }
}

async fn decide(
    pool: &DatabasePool,
    key: &str,
    id: &str,
    action: &str,
    expected_version: i64,
) -> Result<DueDraftScheduleResult, PgTransactionError<BusinessError>> {
    let key = key.to_owned();
    let id = id.to_owned();
    let action = action.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let key = key.clone();
            let id = id.clone();
            let action = action.clone();
            Box::pin(async move {
                let command = CommandKey::new(key).map_err(PgTransactionError::Business)?;
                decide_due_draft(
                    connection,
                    &actor(),
                    &command,
                    &id,
                    &action,
                    expected_version,
                    "Collector recovered",
                    2_000,
                )
                .await
            })
        })
        .await
}

#[tokio::test]
async fn schedule_resolves_the_retained_record_and_receipt() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let result = schedule(&pool, "idem-potent-key-1", "contract-active", "{}")
            .await
            .unwrap();
        assert_eq!(result.id, "finance-due-idem-potent-key-1");
        assert_eq!(result.status, "scheduled");
        assert_eq!(result.version, 1);
        let row = sqlx::query("SELECT source_version,input_hash,delegated_by,assignment_version FROM finance_due_drafts WHERE id='finance-due-idem-potent-key-1'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        let version: i64 = row.try_get("source_version").unwrap();
        assert_eq!(version, 5);
        let delegated: String = row.try_get("delegated_by").unwrap();
        assert_eq!(delegated, "user-1");
        let assignment_version: i64 = row.try_get("assignment_version").unwrap();
        assert_eq!(assignment_version, 3);
        let counts: (i64, i64, i64) = sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id='idem-potent-key-1'),(SELECT count(*)FROM audit_events WHERE command_id='idem-potent-key-1'),(SELECT count(*)FROM outbox_events WHERE command_id='idem-potent-key-1')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts, (1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn replay_on_the_same_key_returns_the_retained_result() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let first = schedule(&pool, "idem-potent-key-2", "contract-active", "{}")
            .await
            .unwrap();
        let again = schedule(&pool, "idem-potent-key-2", "contract-active", "{}")
            .await
            .unwrap();
        assert_eq!(first, again);
        let drafts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM finance_due_drafts WHERE company_id='co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(drafts, 1);
    })
    .await;
}

#[tokio::test]
async fn unknown_inactive_and_stale_sources_keep_the_oracle_denial() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert_eq!(
            denied(&schedule(&pool, "idem-potent-key-3", "missing", "{}").await),
            (
                "FINANCE_SCHEDULE_SOURCE_CHANGED",
                "Choose the current active source edition and a current Company Finance assignment."
            )
        );
        assert_eq!(
            denied(&schedule(&pool, "idem-potent-key-4", "contract-draft", "{}").await),
            (
                "FINANCE_SCHEDULE_SOURCE_CHANGED",
                "Choose the current active source edition and a current Company Finance assignment."
            )
        );
        assert_eq!(
            denied(
                &schedule(
                    &pool,
                    "idem-potent-key-5",
                    "contract-active",
                    r#"{"expectedVersion":4}"#
                )
                .await
            ),
            (
                "FINANCE_SCHEDULE_SOURCE_CHANGED",
                "Choose the current active source edition and a current Company Finance assignment."
            )
        );
        // An explicit matching version schedules normally.
        let ok = schedule(
            &pool,
            "idem-potent-key-6",
            "contract-active",
            r#"{"expectedVersion":5}"#,
        )
        .await
        .unwrap();
        assert_eq!(ok.id, "finance-due-idem-potent-key-6");
    })
    .await;
}

#[tokio::test]
async fn retry_reopens_the_draft_at_the_next_version() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let created = schedule(&pool, "decide-schedule-1", "contract-active", "{}")
            .await
            .unwrap();
        assert_eq!(created.version, 1);
        let retried = decide(&pool, "decide-retry-1", &created.id, "retry", 1)
            .await
            .unwrap();
        assert_eq!(retried.id, created.id);
        assert_eq!(retried.status, "scheduled");
        assert_eq!(retried.version, 2);
        let row: (String, i64) = sqlx::query_as(
            "SELECT status,version FROM finance_due_drafts WHERE id=$1",
        )
        .bind(&created.id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, ("scheduled".to_owned(), 2));
        // Same-key replay returns the retained decision without advancing.
        let again = decide(&pool, "decide-retry-1", &created.id, "retry", 1)
            .await
            .unwrap();
        assert_eq!(again, retried);
        let drafts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM finance_due_drafts WHERE company_id='co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(drafts, 1);
    })
    .await;
}

#[tokio::test]
async fn cancel_closes_the_draft_at_the_next_version() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let created = schedule(&pool, "decide-schedule-2", "contract-active", "{}")
            .await
            .unwrap();
        let cancelled = decide(&pool, "decide-cancel-2", &created.id, "cancel", 1)
            .await
            .unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(cancelled.version, 2);
        let counts: (i64, i64) = sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id='decide-cancel-2'),(SELECT count(*)FROM outbox_events WHERE command_id='decide-cancel-2')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts, (1, 1));
    })
    .await;
}

#[tokio::test]
async fn stale_closed_and_missing_decisions_keep_the_oracle_denial() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let created = schedule(&pool, "decide-schedule-3", "contract-active", "{}")
            .await
            .unwrap();
        assert_eq!(
            denied(&decide(&pool, "decide-stale-3", &created.id, "retry", 2).await),
            (
                "FINANCE_SCHEDULE_CHANGED",
                "Refresh this pending Finance schedule before changing it."
            )
        );
        let cancelled = decide(&pool, "decide-cancel-3", &created.id, "cancel", 1)
            .await
            .unwrap();
        assert_eq!(cancelled.version, 2);
        assert_eq!(
            denied(&decide(&pool, "decide-after-close-3", &created.id, "retry", 2).await),
            (
                "FINANCE_SCHEDULE_CHANGED",
                "Refresh this pending Finance schedule before changing it."
            )
        );
        assert_eq!(
            denied(&decide(&pool, "decide-missing-3", "finance-due-absent", "cancel", 1).await),
            (
                "FINANCE_SCHEDULE_CHANGED",
                "Refresh this pending Finance schedule before changing it."
            )
        );
        // The denied attempts staged no decision receipts: only the schedule
        // and the single successful cancel retained one each.
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id LIKE 'decide-%-3'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 2);
    })
    .await;
}

#[tokio::test]
async fn branch_scope_cannot_decide_a_company_wide_draft() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let created = schedule(&pool, "decide-schedule-4", "contract-active", "{}")
            .await
            .unwrap();
        let mut scoped = actor();
        scoped.scope_kind = "branch".to_owned();
        let denied_call: Result<DueDraftScheduleResult, _> = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), move |connection| {
                let id = created.id.clone();
                let scoped = scoped.clone();
                Box::pin(async move {
                    let command = CommandKey::new("decide-branch-4").unwrap();
                    decide_due_draft(
                        connection,
                        &scoped,
                        &command,
                        &id,
                        "cancel",
                        1,
                        "Collector recovered",
                        2_000,
                    )
                    .await
                })
            })
            .await;
        assert_eq!(
            denied(&denied_call),
            (
                "FINANCE_SCHEDULE_HOLDING_REQUIRED",
                "Schedule Company-wide Finance drafts with a Holding Finance assignment."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn port_writes_nothing_outside_the_caller_transaction() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let rolled_back: Result<Option<DueDraftScheduleResult>, _> =
            PgUnitOfWork::new(pool.clone())
                .run(TransactionOptions::default(), move |connection| {
                    Box::pin(async move {
                        let command = CommandKey::new("idem-potent-key-7").unwrap();
                        schedule_due_draft(
                            connection,
                            &actor(),
                            &command,
                            "billing",
                            "contract-active",
                            "2026-11-30",
                            "{}",
                            "Quarter close draft",
                            1_000,
                        )
                        .await?;
                        Err(BusinessError::new(
                            FailureKind::Conflict,
                            "SYNTHETIC_ROLLBACK",
                            "Synthetic caller failure.",
                        )
                        .into())
                    })
                })
                .await;
        assert!(matches!(
            rolled_back,
            Err(PgTransactionError::Business(ref error)) if error.code == "SYNTHETIC_ROLLBACK"
        ));
        let drafts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM finance_due_drafts WHERE company_id='co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(drafts, 0);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}
