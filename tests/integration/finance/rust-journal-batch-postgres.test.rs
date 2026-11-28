#[path = "../../support/finance/review-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_finance_server::public::JournalError;
use erpx_finance_server::public::journal_batch::*;
use erpx_finance_server::public::journal_batch_wire::normalize_journal_batch;
use erpx_platform_runtime_server::public::postgres::{DatabasePool, PgTransactionError};
use erpx_platform_shared_server::public::CommandKey;
use facts::*;
use tokio::sync::Mutex;

type Error = PgTransactionError<JournalError>;

impl PgJournalBatchPolicy for facts::Policy {}

fn body(ids: &[&str], version: i64) -> Vec<u8> {
    let rows = ids
        .iter()
        .map(|id| format!(r#"{{"id":"{id}","expectedVersion":{version}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    format!(r#"{{"journals":[{rows}],"reason":"Reviewed by finance controller"}}"#).into_bytes()
}

async fn run(
    pool: &DatabasePool,
    key: &str,
    ids: &[&str],
    version: i64,
) -> Result<JournalBatchResult, Error> {
    let wire = normalize_journal_batch(&body(ids, version)).unwrap();
    let command = wire.into_command(CommandKey::new(key).unwrap()).unwrap();
    let policy = Mutex::new(facts::Policy::default());
    let mut identifiers = facts::Ids;
    run_journal_batch(
        pool,
        &policy,
        &facts::FixedClock,
        &mut identifiers,
        &facts::actor(),
        &command,
    )
    .await
}

async fn counts(pool: &DatabasePool, key: &str) -> (i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id=$1),(SELECT count(*)FROM audit_events WHERE command_id=$1),(SELECT count(*)FROM outbox_events WHERE command_id=$1)")
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

#[tokio::test]
async fn posts_both_drafts_with_one_serializable_receipt_audit_outbox_and_retained_evidence() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, false).await;
        facts::seed_journal(&pool, "journal2", false).await;
        let result = run(&pool, "batch-native-post", &["journal", "journal2"], 1)
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({
                "id":"journal-batch-batch-native-post",
                "status":"posted",
                "journals":[{"id":"journal","version":2},{"id":"journal2","version":2}],
                "lineCount":4,
            })
        );
        assert_eq!(counts(&pool, "batch-native-post").await, (1, 1, 1));
        // Child posts run through the existing review port on this same unit.
        assert_eq!(counts(&pool, "batch-native-post:0").await, (1, 1, 1));
        assert_eq!(counts(&pool, "batch-native-post:1").await, (1, 1, 1));
        let evidence: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_batch_review_items WHERE command_id='batch-native-post'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(evidence, 2);
        let statuses: Vec<String> = sqlx::query_scalar(
            "SELECT status FROM finance_journals WHERE id IN('journal','journal2') ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(statuses, vec!["posted", "posted"]);
    })
    .await;
}

#[tokio::test]
async fn exact_batch_receipt_replays_without_any_new_post_effects() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, false).await;
        facts::seed_journal(&pool, "journal2", false).await;
        run(&pool, "batch-native-replay", &["journal", "journal2"], 1)
            .await
            .unwrap();
        // Retained current rows are no longer eligible drafts; only the exact
        // history receipt can satisfy a second request with the same key/body.
        let replayed = run(&pool, "batch-native-replay", &["journal", "journal2"], 1)
            .await
            .unwrap();
        assert_eq!(replayed.line_count, 4);
        assert_eq!(counts(&pool, "batch-native-replay").await, (1, 1, 1));
        assert_eq!(counts(&pool, "batch-native-replay:0").await, (1, 1, 1));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM finance_batch_review_items WHERE command_id='batch-native-replay'")
                .fetch_one(pool.raw()).await.unwrap(),
            2
        );
    }).await;
}

#[tokio::test]
async fn duplicate_scope_and_version_failures_roll_back_the_entire_batch() {
    fixture::with_database(2, |pool| async move {
        facts::setup(&pool, false).await;
        facts::seed_journal(&pool, "journal2", false).await;

        let duplicate = run(&pool, "batch-native-duplicate", &["journal", "journal"], 1).await;
        match duplicate {
            Err(PgTransactionError::Business(e)) => assert_eq!(e.code, "DUPLICATE_JOURNAL"),
            other => panic!("Expected DUPLICATE_JOURNAL, got {other:?}"),
        }
        assert_eq!(counts(&pool, "batch-native-duplicate").await, (0, 0, 0));

        let stale = run(&pool, "batch-native-stale", &["journal", "journal2"], 2).await;
        match stale {
            Err(PgTransactionError::Business(e)) => assert_eq!(e.code, "VERSION_CONFLICT"),
            other => panic!("Expected VERSION_CONFLICT, got {other:?}"),
        }
        assert_eq!(counts(&pool, "batch-native-stale").await, (0, 0, 0));
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM finance_journals WHERE id='journal'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "draft"
        );
    })
    .await;
}
