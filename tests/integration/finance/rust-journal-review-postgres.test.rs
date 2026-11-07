#[path = "../../support/finance/review-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_finance::public::journal_review::*;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgUnitOfWork, TransactionOptions};
use facts::*;
use std::sync::{Arc, atomic::Ordering};
use tokio::sync::Mutex;

#[tokio::test]
async fn matched_request_hash_cannot_replay_a_stored_result_with_another_finance_identity() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, false).await;
        let policy = Arc::new(Mutex::new(Policy::default()));
        run(
            &pool,
            policy.clone(),
            "journal",
            "result-identity-native",
            post(),
        )
        .await
        .unwrap();
        // Additional native receipt admission, not literal B0 tamper acceptance.
        sqlx::query(
            "UPDATE command_receipts SET result_json=$1 WHERE command_id='result-identity-native'",
        )
        .bind(r#"{"id":"another-journal","status":"posted","version":2}"#)
        .execute(pool.raw())
        .await
        .unwrap();
        business(
            run(&pool, policy, "journal", "result-identity-native", post()).await,
            "JOURNAL_REVIEW_SOURCE_CHANGED",
        );
        assert_eq!(counts(&pool, "result-identity-native").await, (1, 1, 1));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*)FROM money_transaction_snapshots")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
    })
    .await;
}

async fn seed_rule(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    id: &str,
    date: &str,
    reason_bytes: i32,
) {
    sqlx::query("INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,opening_period_id,created_at)VALUES($1,'company','chart','book-chart',$2,'maker',CASE WHEN $3=0 THEN 'Reviewed rule' ELSE repeat('r',$3) END,'[]','synthetic-opening-hash','period',0)").bind(id).bind(date).bind(reason_bytes).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id)VALUES($1,'cash','book-cash'),($1,'sales','book-sales')").bind(id).execute(pool.raw()).await.unwrap();
    sqlx::query("UPDATE finance_book_rules SET status='approved',reviewed_by='reviewer',reviewed_at=1 WHERE id=$1").bind(id).execute(pool.raw()).await.unwrap();
}

#[tokio::test]
async fn unrelated_historical_and_future_rule_bodies_are_excluded_but_earliest_start_predicates_remain()
 {
    fixture::with_database(2, |pool| async move {
        setup(&pool, true).await;
        seed_rule(&pool, "historical-large", "2025-01-01", 36 * 1024 * 1024).await;
        seed_rule(&pool, "future-large", "2027-01-01", 36 * 1024 * 1024).await;
        run(
            &pool,
            Arc::new(Mutex::new(Policy::default())),
            "journal",
            "scoped-rule-bodies",
            post(),
        )
        .await
        .unwrap();
        let rule: String = sqlx::query_scalar(
            "SELECT rule_id FROM finance_book_postings WHERE primary_journal_id='journal'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(rule, "rule");
        assert_eq!(counts(&pool, "scoped-rule-bodies").await, (1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn candidate_reversal_date_includes_its_actual_effective_rule_before_the_new_journal_exists()
{
    fixture::with_database(2,|pool|async move{
    setup(&pool,true).await;seed_rule(&pool,"later-rule","2026-01-05",0).await;
    let policy=Arc::new(Mutex::new(Policy::default()));run(&pool,policy.clone(),"journal","post-rule-candidate",post()).await.unwrap();
    run(&pool,policy,"journal","reverse-rule-candidate",reverse()).await.unwrap();
    let rule:String=sqlx::query_scalar("SELECT rule_id FROM finance_book_postings WHERE primary_journal_id='reversal-reverse-rule-candidate'").fetch_one(pool.raw()).await.unwrap();assert_eq!(rule,"later-rule");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM finance_financial_events WHERE journal_id='book-later-rule-reversal-reverse-rule-candidate'").fetch_one(pool.raw()).await.unwrap(),1);
}).await;
}

#[tokio::test]
async fn a_future_first_reviewed_opening_still_rejects_an_earlier_posting_atomically() {
    fixture::with_database(2,|pool|async move{
    setup(&pool,false).await;seed_rule(&pool,"future-first","2026-01-05",0).await;
    let failure=run(&pool,Arc::new(Mutex::new(Policy::default())),"journal","future-first-denial",post()).await;
    assert!(matches!(failure,Err(PgTransactionError::Persistence(ref e))if e.sql_state()==Some("23514")));
    assert_eq!(counts(&pool,"future-first-denial").await,(0,0,0));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM money_transaction_snapshots").fetch_one(pool.raw()).await.unwrap(),0);
}).await;
}

#[tokio::test]
async fn single_and_aggregate_compressed_source_bytes_are_rejected_before_owner_effects() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, false).await;
        let policy = Arc::new(Mutex::new(Policy::default()));
        // TOAST-compressible values still exceed actual encoded JSON admission;
        // pg_column_size alone would undercount these legitimate stored bytes.
        sqlx::query("UPDATE finance_journals SET memo=repeat('x',36*1024*1024) WHERE id='journal'")
            .execute(pool.raw())
            .await
            .unwrap();
        business(
            run(
                &pool,
                policy.clone(),
                "journal",
                "source-single-limit",
                post(),
            )
            .await,
            "JOURNAL_REVIEW_SOURCE_LIMIT",
        );
        sqlx::query("UPDATE finance_journals SET memo=repeat('x',18*1024*1024) WHERE id='journal'")
            .execute(pool.raw())
            .await
            .unwrap();
        sqlx::raw_sql("INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,opening_period_id,created_at)VALUES('large-rule','company','chart','book-chart','2026-01-01','maker',repeat('r',18*1024*1024),'[]','synthetic-opening-hash','period',0);INSERT INTO finance_book_account_mappings(rule_id,primary_account_id,secondary_account_id)VALUES('large-rule','cash','book-cash'),('large-rule','sales','book-sales');UPDATE finance_book_rules SET status='approved',reviewed_by='reviewer',reviewed_at=1 WHERE id='large-rule'").execute(pool.raw()).await.unwrap();
        business(
            run(&pool, policy, "journal", "source-total-limit", post()).await,
            "JOURNAL_REVIEW_SOURCE_LIMIT",
        );
        assert_eq!(counts(&pool, "source-single-limit").await, (0, 0, 0));
        assert_eq!(counts(&pool, "source-total-limit").await, (0, 0, 0));
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

#[tokio::test]
async fn unrelated_large_company_configuration_does_not_enter_the_source_closure() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, false).await;
        // A large unrelated draft configuration is outside the protected
        // accounting record/date closure and cannot deny this small journal.
        sqlx::raw_sql("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at)VALUES('other-primary','company','Other primary','THB','maker',0,0),('other-secondary','company','Other secondary','THB','maker',0,0);INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,prepared_by,reason,opening_json,opening_hash,opening_period_id,created_at)VALUES('unrelated-large-rule','company','other-primary','other-secondary','2026-01-01','maker',repeat('r',36*1024*1024),'[]','synthetic-opening-hash','period',0)")
        .execute(pool.raw())
        .await
        .unwrap();
        run(&pool,Arc::new(Mutex::new(Policy::default())),"journal","unrelated-limit-known",post()).await.unwrap();
        assert_eq!(counts(&pool, "unrelated-limit-known").await, (1, 1, 1));
    })
    .await;
}

#[tokio::test]
async fn account_withdrawal_committed_while_utility_fence_waits_is_seen_before_first_snapshot() {
    fixture::with_database(3,|pool|async move {
        setup(&pool,false).await;
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("UPDATE finance_accounts SET status='inactive' WHERE id='sales'").execute(&mut *writer).await.unwrap();
        let reader_pool=pool.clone();
        let reader=tokio::spawn(async move {run(&reader_pool,Arc::new(Mutex::new(Policy::default())),"journal","source-writer-before-snapshot",post()).await});
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {
                let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareLock' AND relation='finance_accounts'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();
                if blocked{break;}
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.unwrap();
        writer.commit().await.unwrap();
        business(reader.await.unwrap(),"JOURNAL_REVIEW_SOURCE_CHANGED");
        assert_eq!(counts(&pool,"source-writer-before-snapshot").await,(0,0,0));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM money_transaction_snapshots").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}

#[tokio::test]
async fn genuinely_legacy_uncaptured_journal_only_creates_new_reversal_money_evidence() {
    fixture::with_database(2,|pool|async move {
        setup(&pool,false).await;
        sqlx::query("UPDATE finance_journals SET status='posted',posted_by='reviewer',posted_at=1 WHERE id='journal'").execute(pool.raw()).await.unwrap();
        run(&pool,Arc::new(Mutex::new(Policy::default())),"journal","legacy-reversal-only",reverse()).await.unwrap();
        let money:(String,String,String)=sqlx::query_as("SELECT source_id,amount_units::text,usd_units::text FROM money_transaction_snapshots").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(money,("reversal-legacy-reversal-only".into(),"-10000".into(),"-5000".into()));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM money_transaction_snapshots WHERE source_id='journal'").fetch_one(pool.raw()).await.unwrap(),0);
    }).await;
}

async fn run(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    policy: Arc<Mutex<Policy>>,
    journal: &str,
    key: &str,
    input: JournalReviewInput,
) -> Result<JournalReviewResult, Error> {
    let journal = journal.to_owned();
    let key = key.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let policy = policy.clone();
            let journal = journal.clone();
            let key = key.clone();
            let input = input.clone();
            Box::pin(async move { invoke(c, policy.as_ref(), &journal, &key, input).await })
        })
        .await
}
async fn counts(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    key: &str,
) -> (i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id=$1),(SELECT count(*)FROM audit_events WHERE command_id=$1),(SELECT count(*)FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(pool.raw()).await.unwrap()
}
fn business(result: Result<JournalReviewResult, Error>, code: &str) {
    match result {
        Err(PgTransactionError::Business(e)) => assert_eq!(e.code, code),
        other => panic!("Expected {code}, got {other:?}"),
    }
}

#[tokio::test]
async fn post_retains_real_money_receipt_audit_outbox_secondary_book_and_financial_events() {
    fixture::with_database(2,|pool|async move{
    setup(&pool,true).await;let policy=Arc::new(Mutex::new(Policy::default()));
    let result=run(&pool,policy,"journal","post-journal-native",post()).await.unwrap();assert_eq!(serde_json::to_value(result).unwrap(),serde_json::json!({"id":"journal","status":"posted","version":2}));
    assert_eq!(counts(&pool,"post-journal-native").await,(1,1,1));
    let money:(String,String,String)=sqlx::query_as("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE source_id='journal'").fetch_one(pool.raw()).await.unwrap();assert_eq!(money,("10000".into(),"5000".into(),"old-rate".into()));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM finance_book_postings WHERE primary_journal_id='journal'").fetch_one(pool.raw()).await.unwrap(),1);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM finance_financial_events WHERE journal_id IN('journal','book-rule-journal')").fetch_one(pool.raw()).await.unwrap(),2);
    let lines:(i64,i64)=sqlx::query_as("SELECT sum(debit_minor)::bigint,sum(credit_minor)::bigint FROM finance_journal_lines WHERE journal_id='book-rule-journal'").fetch_one(pool.raw()).await.unwrap();assert_eq!(lines,(10000,10000));
}).await;
}

#[tokio::test]
async fn exact_history_replay_precedes_invalid_current_post_state_and_never_reprepares_money() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, true).await;
        let policy = Arc::new(Mutex::new(Policy::default()));
        let original = run(
            &pool,
            policy.clone(),
            "journal",
            "post-history-native",
            post(),
        )
        .await
        .unwrap();
        sqlx::raw_sql(
            "UPDATE finance_review_test_authority SET settle_existing=false,can_approve=false",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let replay = run(
            &pool,
            policy.clone(),
            "journal",
            "post-history-native",
            post(),
        )
        .await
        .unwrap();
        assert_eq!(replay, original);
        assert_eq!(counts(&pool, "post-history-native").await, (1, 1, 1));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*)FROM money_transaction_snapshots")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            1
        );
        business(
            run(
                &pool,
                policy.clone(),
                "journal",
                "post-history-native",
                JournalReviewInput::Post {
                    expected_version: 2,
                },
            )
            .await,
            "IDEMPOTENCY_CONFLICT",
        );
        sqlx::raw_sql("UPDATE finance_review_test_authority SET can_approve=true")
            .execute(pool.raw())
            .await
            .unwrap();
        seed_journal(&pool, "journal-new", false).await;
        business(
            run(
                &pool,
                policy.clone(),
                "journal-new",
                "denied-new-native",
                post(),
            )
            .await,
            "FINANCE_SOURCE_ACCESS_CHANGED",
        );
        assert_eq!(counts(&pool, "denied-new-native").await, (0, 0, 0));
        sqlx::raw_sql("UPDATE finance_review_test_authority SET active=false")
            .execute(pool.raw())
            .await
            .unwrap();
        business(
            run(&pool, policy, "journal", "post-history-native", post()).await,
            "FINANCE_SOURCE_ACCESS_CHANGED",
        );
    })
    .await;
}

#[tokio::test]
async fn reversal_preserves_original_signed_money_and_true_parent_chain_across_reversal_of_reversal()
 {
    fixture::with_database(2,|pool|async move{
    setup(&pool,true).await;let policy=Arc::new(Mutex::new(Policy::default()));run(&pool,policy.clone(),"journal","post-chain-native",post()).await.unwrap();
    let original:String=sqlx::query_scalar("SELECT to_jsonb(m)::text FROM money_transaction_snapshots m WHERE source_id='journal'").fetch_one(pool.raw()).await.unwrap();
    sqlx::raw_sql("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('new-rate','company','THB','2026-01-01',99,'reviewer',2)").execute(pool.raw()).await.unwrap();
    run(&pool,policy.clone(),"journal","reverse-first-native",reverse()).await.unwrap();
    let negative:(String,String,Option<String>)=sqlx::query_as("SELECT amount_units::text,usd_units::text,original_source_id FROM money_transaction_snapshots WHERE source_id='reversal-reverse-first-native'").fetch_one(pool.raw()).await.unwrap();assert_eq!(negative,("-10000".into(),"-5000".into(),Some("journal".into())));
    run(&pool,policy,"reversal-reverse-first-native","reverse-second-native",reverse()).await.unwrap();
    let positive:(String,String,Option<String>,String)=sqlx::query_as("SELECT amount_units::text,usd_units::text,original_source_id,fx_rate_id FROM money_transaction_snapshots WHERE source_id='reversal-reverse-second-native'").fetch_one(pool.raw()).await.unwrap();assert_eq!(positive,("10000".into(),"5000".into(),None,"old-rate".into()));
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT to_jsonb(m)::text FROM money_transaction_snapshots m WHERE source_id='journal'").fetch_one(pool.raw()).await.unwrap(),original);
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT status FROM finance_journals WHERE id='book-rule-journal'").fetch_one(pool.raw()).await.unwrap(),"reversed");
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM finance_book_postings").fetch_one(pool.raw()).await.unwrap(),3);
}).await;
}

#[tokio::test]
async fn audit_and_deferred_projection_failures_roll_back_all_native_owner_and_workflow_effects() {
    fixture::with_database(2,|pool|async move{
    setup(&pool,true).await;sqlx::raw_sql("CREATE FUNCTION reject_native_finance_audit() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.command_id='audit-fail-native' THEN RAISE EXCEPTION 'synthetic audit failure' USING ERRCODE='23514';END IF;RETURN NEW;END$$;CREATE TRIGGER reject_native_finance_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION reject_native_finance_audit()").execute(pool.raw()).await.unwrap();
    let policy=Arc::new(Mutex::new(Policy::default()));let failure=run(&pool,policy.clone(),"journal","audit-fail-native",post()).await;assert!(matches!(failure,Err(PgTransactionError::Persistence(ref e))if e.sql_state()==Some("23514")));
    assert_eq!(counts(&pool,"audit-fail-native").await,(0,0,0));assert_eq!(sqlx::query_scalar::<_,String>("SELECT status FROM finance_journals WHERE id='journal'").fetch_one(pool.raw()).await.unwrap(),"draft");
    sqlx::raw_sql("UPDATE finance_accounts SET status='inactive' WHERE id='book-sales'").execute(pool.raw()).await.unwrap();let failure=run(&pool,policy,"journal","book-fail-native",post()).await;assert!(matches!(failure,Err(PgTransactionError::Persistence(ref e))if e.sql_state()==Some("23514")));
    assert_eq!(counts(&pool,"book-fail-native").await,(0,0,0));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT(SELECT count(*)FROM money_transaction_snapshots)+(SELECT count(*)FROM finance_book_postings)+(SELECT count(*)FROM finance_financial_events)").fetch_one(pool.raw()).await.unwrap(),0);
}).await;
}

#[tokio::test]
async fn final_source_mutation_or_current_authority_denial_after_deferred_events_blocks_commit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool, true).await;
        for revoke in [false, true] {
            let policy = Arc::new(Mutex::new(Policy {
                mutate_after_event: !revoke,
                revoke_after_event: revoke,
                ..Policy::default()
            }));
            let key = if revoke {
                "final-role-native"
            } else {
                "final-source-native"
            };
            let failure = run(&pool, policy, "journal", key, post()).await;
            business(
                failure,
                if revoke {
                    "FINANCE_SOURCE_ACCESS_CHANGED"
                } else {
                    "JOURNAL_REVIEW_SOURCE_CHANGED"
                },
            );
            assert_eq!(counts(&pool, key).await, (0, 0, 0));
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT status FROM finance_journals WHERE id='journal'"
                )
                .fetch_one(pool.raw())
                .await
                .unwrap(),
                "draft"
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*)FROM money_transaction_snapshots")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
}

#[tokio::test]
async fn a_real_updated_tuple_conflict_retries_the_whole_receipt_before_effects_unit() {
    fixture::with_database(3,|pool|async move{
    setup(&pool,true).await;sqlx::raw_sql("CREATE TABLE finance_retry_probe(id bigint PRIMARY KEY,value bigint NOT NULL);INSERT INTO finance_retry_probe VALUES(1,0);CREATE FUNCTION native_finance_retry_probe()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN PERFORM value FROM finance_retry_probe WHERE id=1 FOR UPDATE;RETURN NEW;END$$;CREATE TRIGGER native_finance_retry_probe BEFORE UPDATE ON finance_journals FOR EACH ROW EXECUTE FUNCTION native_finance_retry_probe()").execute(pool.raw()).await.unwrap();
    let gate=Arc::new(RetryGate::new());let policy=Arc::new(Mutex::new(Policy{gate:Some(gate.clone()),..Policy::default()}));let reader_pool=pool.clone();let task=tokio::spawn(async move{run(&reader_pool,policy,"journal","real-retry-native",post()).await});
    tokio::time::timeout(std::time::Duration::from_secs(10),gate.ready.notified()).await.unwrap();sqlx::query("UPDATE finance_retry_probe SET value=1 WHERE id=1").execute(pool.raw()).await.unwrap();gate.release.notify_one();task.await.unwrap().unwrap();
    assert!(gate.attempts.load(Ordering::SeqCst)>=2);assert_eq!(counts(&pool,"real-retry-native").await,(1,1,1));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM money_transaction_snapshots WHERE source_id='journal'").fetch_one(pool.raw()).await.unwrap(),1);
}).await;
}

#[tokio::test]
async fn opening_post_and_reverse_retain_the_real_unsettled_subledger() {
    fixture::with_database(2,|pool|async move{
    setup(&pool,false).await;seed_journal(&pool,"opening",true).await;
    sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('party','company','customer','C1','Customer','THB',0,0);INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,journal_id,created_by,created_at,updated_at)VALUES('opening-invoice','company','party','receivable','OPEN-1','2026-01-01','2026-01-31','THB',10000,10000,10000,10000,10000,'cash','sales','opening_import','opening','draft','opening','preparer',0,0)").execute(pool.raw()).await.unwrap();
    let policy=Arc::new(Mutex::new(Policy::default()));run(&pool,policy.clone(),"opening","post-opening-native",post()).await.unwrap();
    let posted:(String,Option<String>)=sqlx::query_as("SELECT status,approved_by FROM finance_invoices WHERE id='opening-invoice'").fetch_one(pool.raw()).await.unwrap();assert_eq!(posted,("posted".into(),Some("reviewer".into())));
    run(&pool,policy,"opening","reverse-opening-native",reverse()).await.unwrap();
    let voided:(String,i64,i64)=sqlx::query_as("SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id='opening-invoice'").fetch_one(pool.raw()).await.unwrap();assert_eq!(voided,("voided".into(),0,0));
}).await;
}
