#[path = "../../support/finance/journal-source-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_finance::public::journal_review::*;
use erpx_server_runtime::public::postgres::{PgUnitOfWork, TransactionOptions};
use erpx_server_shared::public::{BusinessError, CompanyId};
use facts::*;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn real_creation_origin_survives_canonical_post_and_exact_history_replay() {
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, false).await;
        create(&pool, "genuine-source", false, false).await.unwrap();
        PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_journal_review_sources(c).await?;
                    let scope = inspect_journal_review_source(
                        c,
                        &review::actor(),
                        &command("journal-genuine-source", "post-genuine"),
                    )
                    .await?;
                    assert_eq!(scope.holding_id().as_str(), "holding");
                    assert_eq!(scope.company_id().as_str(), "company");
                    assert_eq!(scope.creator_user_id(), "preparer");
                    assert_eq!(scope.root_journal_id(), "journal-genuine-source");
                    assert_eq!(scope.root_kind(), JournalReviewRootKind::Manual);
                    assert_eq!(scope.journal_status(), "draft");
                    assert_eq!(scope.journal_version(), 1);
                    review::invoke(
                        c,
                        &Mutex::new(review::Policy::default()),
                        "journal-genuine-source",
                        "post-genuine",
                        review::post(),
                    )
                    .await
                    .map_err(convert)?;
                    scope.require_current(c).await
                })
            })
            .await
            .unwrap();
        let policy = Arc::new(Mutex::new(review::Policy::default()));
        run(
            &pool,
            policy.clone(),
            "journal-genuine-source",
            "post-genuine",
            review::post(),
        )
        .await
        .unwrap();
        assert_eq!(
            inspect(&pool, "journal-genuine-source")
                .await
                .unwrap()
                .journal_status(),
            "posted"
        );
        assert_eq!(counts(&pool, "post-genuine").await, (1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn arbitrary_labels_missing_origin_and_mismatched_company_creator_or_source_do_not_prove_ownership()
 {
    fixture::with_database(2,|pool|async move{
        review::setup(&pool,false).await;
        denied(inspect(&pool,"journal").await,"FINANCE_JOURNAL_SOURCE_UNSUPPORTED");
        create(&pool,"real-mismatch",false,false).await.unwrap();
        for sql in ["UPDATE finance_journals SET source_type='inventory' WHERE id='journal-real-mismatch'","UPDATE finance_journals SET source_id='foreign-source' WHERE id='journal-real-mismatch'","UPDATE finance_journals SET created_by='other-creator' WHERE id='journal-real-mismatch'","UPDATE finance_journals SET journal_number='arbitrary-label' WHERE id='journal-real-mismatch'"]{
            let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
                lock_journal_review_sources(c).await?;sqlx::query(sql).execute(&mut *c).await?;
                let inspected=inspect_journal_review_source(c,&review::actor(),&command("journal-real-mismatch","mismatch-source")).await;
                denied(inspected,"FINANCE_JOURNAL_SOURCE_UNSUPPORTED");Err(changed())
            })).await;denied(result,"FIXTURE_ROLLBACK");
        }
        let oversized=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
            lock_journal_review_sources(c).await?;
            sqlx::query("UPDATE command_receipts SET result_json=(result_json::jsonb||jsonb_build_object('padding',repeat('r',20000)))::text WHERE command_id='real-mismatch'").execute(&mut *c).await?;
            denied(inspect_journal_review_source(c,&review::actor(),&command("journal-real-mismatch","oversized-result")).await,"FINANCE_JOURNAL_ORIGIN_LIMIT");Err(changed())
        })).await;denied(oversized,"FIXTURE_ROLLBACK");
        PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
            lock_journal_review_sources(c).await?;let mut actor=review::actor();actor.company_id=CompanyId::new("outside-company").unwrap();
            denied(inspect_journal_review_source(c,&actor,&command("journal-real-mismatch","outside-source")).await,"FINANCE_JOURNAL_SOURCE_UNSUPPORTED");Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn aggregate_origin_budget_rejects_a_real_chain_of_individually_bounded_receipts() {
    fixture::with_database(2,|pool|async move{
        review::setup(&pool,false).await;create(&pool,"origin-budget-root",false,false).await.unwrap();
        let policy=Arc::new(Mutex::new(review::Policy::default()));
        let mut journal="journal-origin-budget-root".to_owned();
        run(&pool,policy.clone(),&journal,"origin-budget-post",review::post()).await.unwrap();
        for index in 0..16 {
            let key=format!("origin-chain-{index}");
            run(&pool,policy.clone(),&journal,&key,review::reverse()).await.unwrap();
            journal=format!("reversal-{key}");
        }
        assert_eq!(inspect(&pool,&journal).await.unwrap().root_journal_id(),"journal-origin-budget-root");
        // Each persisted authority remains below Workflow's real 2 MiB SQL
        // admission. The whole positive chain must nevertheless have a limit.
        let rows=sqlx::query("UPDATE command_receipts SET authority_json=(authority_json::jsonb||jsonb_build_object('resourcePadding',repeat('r',2*1024*1024-4096)))::text WHERE command_id='origin-budget-root' OR command_id LIKE 'origin-chain-%'").execute(pool.raw()).await.unwrap().rows_affected();
        assert_eq!(rows,17);
        denied(inspect(&pool,&journal).await,"FINANCE_JOURNAL_ORIGIN_LIMIT");
    }).await;
}

#[tokio::test]
async fn actual_opening_chart_source_is_supported_but_an_opening_label_without_it_is_not() {
    fixture::with_database(2,|pool|async move{
        review::setup(&pool,false).await;create(&pool,"opening-source",true,false).await.unwrap();
        assert_eq!(inspect(&pool,"journal-opening-source").await.unwrap().root_kind(),JournalReviewRootKind::OpeningImport);
        let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
            lock_journal_review_sources(c).await?;sqlx::query("UPDATE finance_journals SET source_id=NULL WHERE id='journal-opening-source'").execute(&mut *c).await?;
            denied(inspect_journal_review_source(c,&review::actor(),&command("journal-opening-source","opening-wrong-source")).await,"FINANCE_JOURNAL_SOURCE_UNSUPPORTED");Err(changed())
        })).await;denied(result,"FIXTURE_ROLLBACK");
        run_with_source(&pool,"journal-opening-source","post-opening-source",review::post()).await.unwrap();
        run_with_source(&pool,"journal-opening-source","reverse-opening-source",review::reverse()).await.unwrap();
    }).await;
}

#[tokio::test]
async fn every_reversal_requires_its_actual_parent_bound_receipt_not_just_a_real_manual_root() {
    fixture::with_database(2,|pool|async move{
        review::setup(&pool,false).await;create(&pool,"reversal-root",false,false).await.unwrap();let policy=Arc::new(Mutex::new(review::Policy::default()));
        run(&pool,policy.clone(),"journal-reversal-root","post-reversal-root",review::post()).await.unwrap();
        run_with_source(&pool,"journal-reversal-root","genuine-reverse-1",review::reverse()).await.unwrap();
        run_with_source(&pool,"reversal-genuine-reverse-1","genuine-reverse-2",review::reverse()).await.unwrap();
        let source=inspect(&pool,"reversal-genuine-reverse-2").await.unwrap();assert_eq!(source.root_journal_id(),"journal-reversal-root");assert_eq!(source.root_kind(),JournalReviewRootKind::Manual);
        sqlx::query("UPDATE command_receipts SET result_json='{\"reversalJournalId\":\"reversal-other\",\"reversedJournalId\":\"journal-reversal-root\"}' WHERE command_id='genuine-reverse-1'").execute(pool.raw()).await.unwrap();
        denied(inspect(&pool,"reversal-genuine-reverse-2").await,"FINANCE_JOURNAL_SOURCE_UNSUPPORTED");
    }).await;
}

#[tokio::test]
async fn actual_secondary_chart_ownership_wins_over_a_genuine_manual_creation_label() {
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, true).await;
        create(&pool, "secondary-manual", false, true)
            .await
            .unwrap();
        denied(
            inspect(&pool, "journal-secondary-manual").await,
            "FINANCE_JOURNAL_SOURCE_UNSUPPORTED",
        );
    })
    .await;
}

#[tokio::test]
async fn source_witness_rejects_same_transaction_content_changes_and_another_transaction() {
    fixture::with_database(1, |pool| async move {
        review::setup(&pool, false).await;
        create(&pool, "witness-source", false, false).await.unwrap();
        let result = PgUnitOfWork::new(pool.clone())
            .run::<(), BusinessError, _>(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_journal_review_sources(c).await?;
                    let source = inspect_journal_review_source(
                        c,
                        &review::actor(),
                        &command("journal-witness-source", "witness-check"),
                    )
                    .await?;
                    sqlx::query(
                        "UPDATE finance_journals SET memo='changed' WHERE id='journal-witness-source'",
                    )
                    .execute(&mut *c)
                    .await?;
                    source.require_current(c).await
                })
            })
            .await;
        denied(result, "FINANCE_JOURNAL_SOURCE_CHANGED");
        let source = Arc::new(inspect(&pool, "journal-witness-source").await.unwrap());
        let result = PgUnitOfWork::new(pool.clone())
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
                let source = source.clone();
                Box::pin(async move {
                    lock_journal_review_sources(c).await?;
                    source.require_current(c).await
                })
            })
            .await;
        denied(result, "FINANCE_JOURNAL_SOURCE_CHANGED");
    })
    .await;
}

#[tokio::test]
async fn committed_source_withdrawal_while_utility_fence_waits_is_seen_before_first_snapshot() {
    fixture::with_database(3,|pool|async move{
        review::setup(&pool,false).await;create(&pool,"withdrawal",false,false).await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE finance_journals SET source_type='inventory' WHERE id='journal-withdrawal'").execute(&mut *writer).await.unwrap();
        let reader_pool=pool.clone();let reader=tokio::spawn(async move{inspect(&reader_pool,"journal-withdrawal").await});
        tokio::time::timeout(std::time::Duration::from_secs(10),async{
            loop{let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareRowExclusiveLock' AND relation='finance_journals'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();if waiting{break}tokio::task::yield_now().await;}
        }).await.unwrap();writer.commit().await.unwrap();
        denied(reader.await.unwrap(),"FINANCE_JOURNAL_SOURCE_UNSUPPORTED");
    }).await;
}
