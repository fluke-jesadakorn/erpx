//! Actual owner record-source reads for Operations staff tasks over real
//! migrated PostgreSQL. These are read facts, not command, purchase or browser
//! evidence.
#![allow(dead_code)]
#[path = "../../support/operations/rust-operations-database.rs"]
mod fixture;
use erpx_operations_server::public::record_source::{
    lock_record_sources, read_record_source, read_social_task_source,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::record_source_contract::SourceKind;
use erpx_platform_shared_server::public::{BusinessError, CompanyId};

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres");
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
async fn social_task_source_preserves_preview_and_handover_participation() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        sqlx::raw_sql(
            "UPDATE staff_tasks SET start_approvers_json='[\"approver\"]' WHERE id='task';
             INSERT INTO staff_task_delegations(id,company_id,task_id,delegator_user_id,delegate_user_id,
               starts_at,expires_at,reason,created_at,updated_at) VALUES
              ('older','company','task','creator','older-delegate',0,200,'Fixture',10,10),
              ('latest','company','task','creator','latest-delegate',0,200,'Fixture',20,20),
              ('expired','company','task','creator','expired-delegate',0,99,'Fixture',30,30);",
        ).execute(&mut *tx).await.unwrap();
        for (user, preview, handover) in [
            ("assignee", true, true), ("approver", true, true),
            ("older-delegate", false, true), ("latest-delegate", true, true),
            ("expired-delegate", false, false), ("unrelated", false, false),
        ] {
            let task = read_social_task_source(&mut tx, &company(), "task", user, 100).await.unwrap().unwrap();
            assert_eq!((task.preview_participant, task.handover_participant), (preview, handover), "{user}");
            assert_eq!((task.title.as_str(), task.state.as_str(), task.version), ("Synthetic task", "in_progress", 7));
        }
        assert!(read_social_task_source(&mut tx, &company(), "foreign-task", "creator", 100).await.unwrap().is_none());
        sqlx::raw_sql(
            "INSERT INTO social_posts(id,author_id,author_name,company_id,body,created_at,updated_at)
               VALUES('origin','creator','Creator','company','Original post',1,1);
             INSERT INTO social_task_origins(post_id,task_id,source_version,source_hash,created_by,created_at)
               VALUES('origin','task',1,'fixture','creator',1);",
        ).execute(&mut *tx).await.unwrap();
        let task = read_social_task_source(&mut tx, &company(), "task", "assignee", 100).await.unwrap().unwrap();
        assert_eq!(task.origin_post_id.as_deref(), Some("origin"));
        assert!(task.preview_participant);
        assert!(!task.handover_participant);
        tx.rollback().await.unwrap();
    }).await;
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
