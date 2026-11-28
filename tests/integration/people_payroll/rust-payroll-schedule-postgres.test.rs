//! Postgres proof of the native payroll schedule save command.
//!
//! The save path writes the pay-group schedule row plus the command receipt,
//! audit event and outbox event inside the caller's serializable transaction:
//! the happy path resolves the retained record at the next schedule version,
//! replay on the same idempotency key returns the retained result,
//! unknown/stale groups and missing anchor dates fail with the retained
//! oracle codes, and a caller-transaction rollback stages nothing.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_people_payroll_server::public::payroll_schedule::{
    PayrollScheduleCommandActor, PayrollScheduleResult, get_payroll_schedule, save_payroll_schedule,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind};

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES \
          ('*','payroll_writer','Payroll writer',false,1,'active'); \
         INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES \
          ('*','payroll_writer',1,'[\"payroll.prepare\"]','[]','[\"holding\"]','fixture',0); \
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES \
          ('grant-1','user-1','payroll_writer','[]','co',0,9007199254740991,3,'holding'); \
         INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,status,created_by,created_at,updated_at,schedule_enabled,pay_delay_days,schedule_anchor_date,schedule_version) VALUES \
          ('grp-weekly','co','Weekly','TH','THB','weekly','Asia/Bangkok','active','maker',0,0,false,1,NULL,2), \
          ('grp-monthly','co','Monthly','TH','THB','monthly','Asia/Bangkok','active','maker',0,0,false,1,NULL,4), \
          ('grp-archived','co','Archived','TH','THB','monthly','Asia/Bangkok','archived','maker',0,0,false,1,NULL,1)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn actor() -> PayrollScheduleCommandActor {
    PayrollScheduleCommandActor {
        user_id: "user-1".to_owned(),
        assignment_id: "grant-1".to_owned(),
        role: "payroll_writer".to_owned(),
        company_id: "co".to_owned(),
        holding_id: "holding-1".to_owned(),
        scope_kind: "holding".to_owned(),
    }
}

async fn save(
    pool: &DatabasePool,
    key: &str,
    group_id: &str,
    enabled: bool,
    pay_delay_days: i64,
    anchor_date: Option<&str>,
    expected_version: i64,
) -> Result<PayrollScheduleResult, PgTransactionError<BusinessError>> {
    let key = key.to_owned();
    let group_id = group_id.to_owned();
    let anchor_date = anchor_date.map(str::to_owned);
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let key = key.clone();
            let group_id = group_id.clone();
            let anchor_date = anchor_date.clone();
            Box::pin(async move {
                let command = CommandKey::new(key).map_err(PgTransactionError::Business)?;
                save_payroll_schedule(
                    connection,
                    &actor(),
                    &command,
                    &group_id,
                    enabled,
                    pay_delay_days,
                    anchor_date.as_deref(),
                    expected_version,
                    1_000,
                )
                .await
            })
        })
        .await
}

fn denied(
    result: &Result<PayrollScheduleResult, PgTransactionError<BusinessError>>,
) -> (&'static str, &'static str) {
    match result {
        Err(PgTransactionError::Business(error)) => (error.code, error.message),
        _ => panic!("expected an oracle denial, got {result:?}"),
    }
}

#[tokio::test]
async fn save_resolves_the_retained_record_and_receipt() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let result = save(&pool, "sched-key-1", "grp-weekly", true, 7, Some("2026-11-30"), 2)
            .await
            .unwrap();
        assert_eq!(result.id, "grp-weekly");
        assert!(result.enabled);
        assert_eq!(result.pay_delay_days, 7);
        assert_eq!(result.anchor_date.as_deref(), Some("2026-11-30"));
        assert_eq!(result.version, 3);
        let row: (bool, i32, Option<String>, i64) = sqlx::query_as(
            "SELECT schedule_enabled,pay_delay_days,schedule_anchor_date,schedule_version FROM hr_pay_groups WHERE id='grp-weekly'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, (true, 7, Some("2026-11-30".to_owned()), 3));
        let counts: (i64, i64, i64) = sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id='sched-key-1'),(SELECT count(*)FROM audit_events WHERE command_id='sched-key-1'),(SELECT count(*)FROM outbox_events WHERE command_id='sched-key-1')").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts, (1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn replay_on_the_same_key_returns_the_retained_result() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let first = save(&pool, "sched-key-2", "grp-monthly", true, 5, None, 4)
            .await
            .unwrap();
        let again = save(&pool, "sched-key-2", "grp-monthly", true, 5, None, 4)
            .await
            .unwrap();
        assert_eq!(first, again);
        assert_eq!(again.version, 5);
        let versions: i64 = sqlx::query_scalar(
            "SELECT schedule_version FROM hr_pay_groups WHERE id='grp-monthly'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(versions, 5);
    })
    .await;
}

#[tokio::test]
async fn unknown_stale_and_anchorless_saves_keep_the_oracle_denials() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        assert_eq!(
            denied(&save(&pool, "sched-key-3", "missing", true, 7, Some("2026-11-30"), 1).await),
            ("PAY_GROUP_NOT_FOUND", "Active pay group not found.")
        );
        assert_eq!(
            denied(&save(&pool, "sched-key-4", "grp-archived", true, 7, Some("2026-11-30"), 1).await),
            ("PAY_GROUP_NOT_FOUND", "Active pay group not found.")
        );
        assert_eq!(
            denied(&save(&pool, "sched-key-5", "grp-weekly", true, 7, Some("2026-11-30"), 9).await),
            (
                "VERSION_CONFLICT",
                "Pay group schedule changed. Refresh before saving."
            )
        );
        assert_eq!(
            denied(&save(&pool, "sched-key-6", "grp-weekly", true, 7, None, 2).await),
            (
                "PAYROLL_ANCHOR_REQUIRED",
                "Set an anchor date for weekly or fortnightly payroll."
            )
        );
        // Monthly payroll needs no anchor date.
        let monthly = save(&pool, "sched-key-7", "grp-monthly", true, 5, None, 4)
            .await
            .unwrap();
        assert_eq!(monthly.version, 5);
        // Denied attempts staged no receipts.
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN ('sched-key-3','sched-key-4','sched-key-5','sched-key-6')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}

#[tokio::test]
async fn branch_scope_cannot_save_a_company_wide_schedule() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let mut scoped = actor();
        scoped.scope_kind = "branch".to_owned();
        let denied_call: Result<PayrollScheduleResult, _> = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), move |connection| {
                let scoped = scoped.clone();
                Box::pin(async move {
                    let command = CommandKey::new("sched-key-8").unwrap();
                    save_payroll_schedule(
                        connection,
                        &scoped,
                        &command,
                        "grp-weekly",
                        true,
                        7,
                        Some("2026-11-30"),
                        2,
                        1_000,
                    )
                    .await
                })
            })
            .await;
        assert_eq!(
            denied(&denied_call),
            (
                "COMPANY_PAYROLL_SCOPE_REQUIRED",
                "Company-wide payroll requires a holding-level assignment."
            )
        );
    })
    .await;
}

#[tokio::test]
async fn read_resolves_the_retained_row_without_a_status_filter() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let weekly = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    get_payroll_schedule(connection, &actor(), "grp-weekly").await
                })
            })
            .await
            .unwrap();
        assert_eq!(weekly.id, "grp-weekly");
        assert_eq!(weekly.frequency, "weekly");
        assert_eq!(weekly.timezone, "Asia/Bangkok");
        assert_eq!(weekly.currency, "THB");
        assert!(!weekly.enabled);
        assert_eq!(weekly.pay_delay_days, 1);
        assert_eq!(weekly.anchor_date, None);
        assert_eq!(weekly.version, 2);
        // The read applies no status filter, exactly like the retained query:
        // archived groups still resolve here; the save path gates them.
        let archived = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    get_payroll_schedule(connection, &actor(), "grp-archived").await
                })
            })
            .await
            .unwrap();
        assert_eq!(archived.version, 1);
        // The read writes nothing: no receipt, audit or outbox rows.
        let writes: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM command_receipts)+(SELECT count(*) FROM audit_events)+(SELECT count(*) FROM outbox_events)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(writes, 0);
    })
    .await;
}

#[tokio::test]
async fn read_of_a_missing_group_keeps_the_oracle_denial() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let missing: Result<
            erpx_people_payroll_server::public::payroll_schedule::PayrollScheduleView,
            _,
        > = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    get_payroll_schedule(connection, &actor(), "absent").await
                })
            })
            .await;
        assert!(matches!(
            missing,
            Err(PgTransactionError::Business(ref error)) if error.code == "PAY_GROUP_NOT_FOUND"
        ));
    })
    .await;
}

#[tokio::test]
async fn port_writes_nothing_outside_the_caller_transaction() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let rolled_back: Result<Option<PayrollScheduleResult>, _> = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    let command = CommandKey::new("sched-key-9").unwrap();
                    save_payroll_schedule(
                        connection,
                        &actor(),
                        &command,
                        "grp-weekly",
                        true,
                        7,
                        Some("2026-11-30"),
                        2,
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
        let versions: i64 = sqlx::query_scalar(
            "SELECT schedule_version FROM hr_pay_groups WHERE id='grp-weekly'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(versions, 2);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM command_receipts")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}
