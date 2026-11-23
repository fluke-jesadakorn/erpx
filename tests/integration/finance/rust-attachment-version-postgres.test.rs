//! Postgres proof of the native Finance attachment version-guard port.
//!
//! The guard, the single version step and the receipt/audit/outbox effects all
//! run on the caller's serializable unit. This suite proves the owner
//! allow-list, the exactly-once bump, the stale no-op, cross-Company
//! fail-closed behaviour and that a caller rollback stages nothing.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_finance::public::attachment_version::{
    AttachmentVersionActor, AttachmentVersionStep, assert_attachment_version,
    bump_attachment_version,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CommandKey};
use sqlx::Row;

type Error = PgTransactionError<BusinessError>;

async fn seed(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES \
           ('chart','co','Main','THB','user-1',10,10); \
         INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES \
           ('period','co','2026-11','2026-11-01','2026-11-30',10); \
         INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES \
           ('debit','chart','co','EXP','Expense','expense','THB',10,10), \
           ('credit','chart','co','CASH','Cash','asset','THB',10,10); \
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES \
           ('party','co','supplier','SUP','Supplier','THB',10,10); \
         INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,memo,status,created_by,created_at,updated_at) VALUES \
           ('journal-co','co','chart','period','J-CO','2026-11-23','THB','Version guard','draft','user-1',10,10), \
           ('journal-other','other','chart','period','J-OTHER','2026-11-23','THB','Foreign target','draft','user-2',10,10); \
         INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,created_by,created_at,updated_at) VALUES \
           ('invoice-co','co','party','payable','INV-CO','2026-11-01','2026-11-08','THB',1000,1000,1000,'debit','credit','draft','user-1',10,10)",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn actor() -> AttachmentVersionActor {
    AttachmentVersionActor {
        user_id: "user-1".to_owned(),
        assignment_id: "grant-1".to_owned(),
        company_id: "co".to_owned(),
        role: "finance_writer".to_owned(),
        scope_kind: "company".to_owned(),
    }
}

async fn guard(
    pool: &DatabasePool,
    target_type: &str,
    target_id: &str,
    expected_version: i64,
) -> Result<(), Error> {
    let target_type = target_type.to_owned();
    let target_id = target_id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let target_type = target_type.clone();
            let target_id = target_id.clone();
            Box::pin(async move {
                assert_attachment_version(
                    connection,
                    &actor(),
                    &target_type,
                    &target_id,
                    expected_version,
                )
                .await
            })
        })
        .await
}

async fn step(
    pool: &DatabasePool,
    key: &str,
    target_type: &str,
    target_id: &str,
    expected_version: i64,
    now: i64,
) -> Result<AttachmentVersionStep, Error> {
    let key = key.to_owned();
    let target_type = target_type.to_owned();
    let target_id = target_id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let key = key.clone();
            let target_type = target_type.clone();
            let target_id = target_id.clone();
            Box::pin(async move {
                let command = CommandKey::new(&key).map_err(PgTransactionError::Business)?;
                bump_attachment_version(
                    connection,
                    &actor(),
                    &command,
                    &target_type,
                    &target_id,
                    expected_version,
                    now,
                )
                .await
            })
        })
        .await
}

fn denied<T: std::fmt::Debug>(result: &Result<T, Error>) -> &'static str {
    match result {
        Err(PgTransactionError::Business(error)) => error.code,
        _ => panic!("expected an owner denial, got {result:?}"),
    }
}

#[tokio::test]
async fn allow_list_and_company_scope_fail_closed() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        for (index, target) in [
            "finance_unknown",
            "",
            "invoice_payment; DROP TABLE finance_journals",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                denied(&guard(&pool, target, "journal-co", 1).await),
                "ATTACHMENT_TARGET_UNSUPPORTED"
            );
            assert_eq!(
                denied(
                    &step(
                        &pool,
                        &format!("unsupported-key-{index}"),
                        target,
                        "journal-co",
                        1,
                        100
                    )
                    .await
                ),
                "ATTACHMENT_TARGET_UNSUPPORTED"
            );
        }
        // A journal that belongs to another Company is not visible to the
        // caller's Finance scope, and no version is fabricated for it.
        assert_eq!(
            denied(&guard(&pool, "finance_journal", "journal-other", 1).await),
            "ATTACHMENT_TARGET_NOT_FOUND"
        );
        assert_eq!(
            denied(
                &step(
                    &pool,
                    "cross-scope-key",
                    "finance_journal",
                    "journal-other",
                    1,
                    100
                )
                .await
            ),
            "ATTACHMENT_TARGET_NOT_FOUND"
        );
        let foreign: i64 =
            sqlx::query_scalar("SELECT version FROM finance_journals WHERE id='journal-other'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(foreign, 1);
    })
    .await;
}

#[tokio::test]
async fn allow_listed_target_bumps_once_and_a_stale_step_is_a_no_op() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        guard(&pool, "finance_journal", "journal-co", 1)
            .await
            .unwrap();
        let first = step(&pool, "step-key-1", "finance_journal", "journal-co", 1, 111)
            .await
            .unwrap();
        assert_eq!(first.changed, true);
        assert_eq!(first.version, 2);
        let replay = step(&pool, "step-key-1", "finance_journal", "journal-co", 1, 111)
            .await
            .unwrap();
        assert_eq!(replay, first);
        let row =
            sqlx::query("SELECT version,updated_at FROM finance_journals WHERE id='journal-co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(row.get::<i64, _>("version"), 2);
        assert_eq!(row.get::<i64, _>("updated_at"), 111);

        let stale = step(
            &pool,
            "step-key-stale",
            "finance_journal",
            "journal-co",
            1,
            112,
        )
        .await
        .unwrap();
        assert_eq!(stale.changed, false);
        assert_eq!(stale.version, 2);
        let unchanged: (i64, i64) =
            sqlx::query_as("SELECT version,updated_at FROM finance_journals WHERE id='journal-co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(unchanged, (2, 111));
        assert_eq!(
            denied(&guard(&pool, "finance_journal", "journal-co", 1).await),
            "ATTACHMENT_TARGET_CHANGED"
        );

        let second = step(&pool, "step-key-2", "finance_journal", "journal-co", 2, 113)
            .await
            .unwrap();
        assert_eq!(second.changed, true);
        assert_eq!(second.version, 3);
        let final_version: i64 =
            sqlx::query_scalar("SELECT version FROM finance_journals WHERE id='journal-co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(final_version, 3);
    })
    .await;
}

#[tokio::test]
async fn service_invoice_evidence_keeps_the_same_version_contract() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let first = step(
            &pool,
            "invoice-key-1",
            "invoice_payment",
            "invoice-co",
            1,
            200,
        )
        .await
        .unwrap();
        assert_eq!((first.changed, first.version), (true, 2));
        let stale = step(
            &pool,
            "invoice-key-stale",
            "supplier_bill",
            "invoice-co",
            1,
            201,
        )
        .await
        .unwrap();
        assert_eq!((stale.changed, stale.version), (false, 2));
        let second = step(
            &pool,
            "invoice-key-2",
            "supplier_bill",
            "invoice-co",
            2,
            202,
        )
        .await
        .unwrap();
        assert_eq!((second.changed, second.version), (true, 3));
        let row =
            sqlx::query("SELECT version,updated_at FROM finance_invoices WHERE id='invoice-co'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(row.get::<i64, _>("version"), 3);
        assert_eq!(row.get::<i64, _>("updated_at"), 202);
    })
    .await;
}

#[tokio::test]
async fn receipt_audit_and_outbox_share_the_callers_unit() {
    fixture::with_database(2, |pool| async move {
        seed(&pool).await;
        let rolled_back: Result<(), Error> = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let command = CommandKey::new("rollback-key").map_err(PgTransactionError::Business)?;
                    bump_attachment_version(connection, &actor(), &command, "finance_journal", "journal-co", 1, 300).await?;
                    Err(BusinessError::new(
                        erpx_server_shared::public::FailureKind::Conflict,
                        "CALLER_ROLLBACK",
                        "Caller forced rollback.",
                    )
                    .into())
                })
            })
            .await;
        assert!(rolled_back.is_err());
        let after_rollback: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(after_rollback, (0, 0, 0));
        let version: i64 = sqlx::query_scalar("SELECT version FROM finance_journals WHERE id='journal-co'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(version, 1);

        let result = step(&pool, "commit-key", "finance_journal", "journal-co", 1, 301)
            .await
            .unwrap();
        assert_eq!(result.changed, true);
        let committed: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts WHERE command_id='commit-key'),(SELECT count(*) FROM audit_events WHERE command_id='commit-key'),(SELECT count(*) FROM outbox_events WHERE command_id='commit-key')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(committed, (1, 1, 1));
    })
    .await;
}
