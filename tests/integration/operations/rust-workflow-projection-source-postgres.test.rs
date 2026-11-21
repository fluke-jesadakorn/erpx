//! Actual Operations Workflow projection source reads over migrated PostgreSQL.
#[path = "../../support/operations/rust-operations-database.rs"]
mod fixture;

use erpx_server_operations::public::workflow_projection_source::{
    TaskProjectionSource, task_projection_source,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use sqlx::Connection;

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

#[tokio::test]
async fn task_projection_source_reads_the_current_owner_row_on_the_caller_connection() {
    fixture::with_database(1, |holding| async move {
        migrate(&holding).await;
        let mut connection = holding.raw().acquire().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,creator_user_id,\
             assignee_user_id,due_at,state,version,created_at,updated_at) VALUES\
             ('task-source','company','branch',NULL,'Inspect deliveries','creator','assignee',\
              1790000000000,'in_progress',7,0,0)",
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        // The uncommitted owner row is visible because the read shares the
        // caller's unit; it never opens an independent transaction or snapshot.
        assert_eq!(
            task_projection_source(&mut tx, "task-source").await.unwrap(),
            Some(TaskProjectionSource {
                state: "in_progress".into(),
                version: 7,
                company_id: "company".into(),
                branch_id: Some("branch".into()),
                department_id: None,
            })
        );
        assert_eq!(
            task_projection_source(&mut tx, "missing-task").await.unwrap(),
            None
        );
        tx.rollback().await.unwrap();

        // Rollback removed the uncommitted fact; the port fabricates no row.
        assert_eq!(
            task_projection_source(&mut connection, "task-source")
                .await
                .unwrap(),
            None
        );
    })
    .await;
}
