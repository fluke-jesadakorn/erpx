//! Actual owner record-source reads for Operations staff tasks over real
//! migrated PostgreSQL. These are read facts, not command, purchase or browser
//! evidence.
#![allow(dead_code)]
#[path = "../../support/operations/rust-operations-database.rs"]
mod fixture;
use erpx_server_operations::public::record_source::{lock_record_sources, read_record_source};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::record_source_contract::SourceKind;
use erpx_server_shared::public::{BusinessError, CompanyId};

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn serializable(pool: &DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,creator_user_id,assignee_user_id,due_at,state,version,created_at,updated_at)VALUES\
         ('task','company','branch',NULL,'Synthetic task','creator','assignee',100,'in_progress',7,0,5),\
         ('foreign-task','other','branch',NULL,'Foreign task','creator','assignee',100,'in_progress',1,0,5),\
         ('oversize-task','company','branch',NULL,repeat('x',2100000),'creator','assignee',100,'in_progress',1,0,5);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

#[tokio::test]
async fn reads_staff_task_sources_scoped_to_the_company() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_record_sources(&mut tx).await.unwrap();

        let task = read_record_source(&mut tx, &company(), "task")
            .await
            .unwrap()
            .unwrap();
        let data = task.data();
        assert_eq!(data.kind, SourceKind::StaffTask);
        assert_eq!(data.id, "task");
        assert_eq!(data.company_id, "company");
        assert_eq!(data.branch_id.as_deref(), Some("branch"));
        assert_eq!(data.department_id, None);
        assert_eq!(data.version, 7);
        assert_eq!(data.title, "Synthetic task");
        assert_eq!(data.status, "in_progress");

        assert!(
            read_record_source(&mut tx, &company(), "foreign-task")
                .await
                .unwrap()
                .is_none(),
            "a foreign company row is not readable"
        );
        assert!(
            read_record_source(&mut tx, &company(), "missing")
                .await
                .unwrap()
                .is_none(),
            "a missing row is not fabricated"
        );

        sqlx::query("UPDATE staff_tasks SET title='Changed after capture' WHERE id='task'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            task.require_current(&mut tx).await.is_err(),
            "a changed owner row invalidates the captured witness"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn source_reads_require_a_serializable_transaction_and_respect_the_bound() {
    fixture::with_database(1, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;

        let mut plain = holding.raw().begin().await.unwrap();
        assert!(
            read_record_source(&mut plain, &company(), "task")
                .await
                .is_err(),
            "read-committed reads are refused"
        );
        plain.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        match read_record_source(&mut tx, &company(), "oversize-task").await {
            Err(PgTransactionError::Business(BusinessError {
                code: "RECORD_SOURCE_LIMIT",
                ..
            })) => {}
            _ => panic!("expected the bounded read to refuse a 2 MiB source"),
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn source_locks_require_an_open_transaction() {
    fixture::with_database(1, |holding| async move {
        let mut connection = holding.raw().acquire().await.unwrap();
        assert!(
            lock_record_sources(&mut *connection).await.is_err(),
            "the source table lock is fenced to a caller transaction"
        );
    })
    .await;
}
