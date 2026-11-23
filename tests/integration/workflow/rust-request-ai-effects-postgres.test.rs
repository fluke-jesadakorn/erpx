//! Caller-transaction coverage for the Workflow-owned `request.ai.prepare`
//! assertion-clear statements (`RequestAiWorkflowFacts` native port).
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::{DatabasePool, MigrationTrack, load_migrations, migrate_database};
use erpx_server_workflow::public::request_ai_effects::{
    REQUEST_AI_ASSERTION_CLEAR_SQL, clear_request_ai_assertions,
};

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
    // Seed the durable baseline table: since 1039 the `command_assertions`
    // interface is a transaction-local view over `command_assertion_baselines`
    // plus the per-transaction guard setting, so durable fixtures belong in
    // the baseline table and every assertion-write path under test goes
    // through the view exactly like the retained statement.
    sqlx::raw_sql(
        "INSERT INTO command_assertion_baselines(command_id,step,ok) VALUES \
         ('request-ai-run:key-1','request-ai-lease',1),\
         ('request-ai-run:key-1','queued-assignment',1),\
         ('request-ai-result:key-1','request-ai-lease',1),\
         ('other-command:key-9','request-ai-lease',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn rows(pool: &DatabasePool, command_id: &str) -> Vec<(String, i64)> {
    sqlx::query_as(
        "SELECT step,ok FROM command_assertions WHERE command_id=$1 ORDER BY step",
    )
    .bind(command_id)
    .fetch_all(pool.raw())
    .await
    .unwrap()
}

#[tokio::test]
async fn request_ai_effects_clear_only_the_caller_key_on_the_caller_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        assert_eq!(
            REQUEST_AI_ASSERTION_CLEAR_SQL,
            "DELETE FROM command_assertions WHERE command_id=$1"
        );
        // Both retained effects share one statement shape; the run
        // authorization key and the result command key clear independently
        // inside a single caller unit, with no commit inside the adapter.
        let mut unit = pool.raw().begin().await.unwrap();
        clear_request_ai_assertions(&mut unit, "request-ai-run:key-1")
            .await
            .unwrap();
        clear_request_ai_assertions(&mut unit, "request-ai-result:key-1")
            .await
            .unwrap();
        unit.commit().await.unwrap();
        assert!(rows(&pool, "request-ai-run:key-1").await.is_empty());
        assert!(rows(&pool, "request-ai-result:key-1").await.is_empty());
        assert_eq!(
            rows(&pool, "other-command:key-9").await,
            vec![("request-ai-lease".to_owned(), 1)]
        );
        // Clearing an absent key stays a no-op success, matching the retained
        // unconditional DELETE (the drain never branches on row presence).
        let mut unit = pool.raw().begin().await.unwrap();
        clear_request_ai_assertions(&mut unit, "request-ai-run:key-1")
            .await
            .unwrap();
        unit.rollback().await.unwrap();
        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM command_assertions")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(total, 1);
    })
    .await;
}
