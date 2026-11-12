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
async fn field_history_finance_candidates_follow_selected_payment_and_journal_roots() {
    use erpx_server_finance::public::attachment_target_source::*;
    use erpx_server_finance::public::field_history_candidates::*;
    use erpx_server_finance::public::field_history_money_lines::*;
    fixture::with_database(3, |pool| async move {
        review::setup(&pool,false).await;
        // Deliberately ordinary staged rows test Finance candidate enumeration,
        // not Field lineage, Workflow receipts, purchase, or valid posting.
        sqlx::raw_sql("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,created_by,created_at,updated_at) VALUES('graph-primary','company','chart','period','GRAPH-P','2026-01-10','THB','manual','fixture',0,0),('graph-secondary','company','chart','period','GRAPH-S','2026-01-10','THB','manual','fixture',0,0),('graph-other','company','chart','period','GRAPH-O','2026-01-10','THB','manual','fixture',0,0);
        INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,status,version,prepared_by,reason,opening_json,opening_hash,created_at) VALUES('graph-rule','company','chart','book-chart','2026-01-10','draft',1,'fixture','fixture','{}','fixture',0);
        INSERT INTO finance_book_postings(primary_journal_id,secondary_journal_id,rule_id,company_id,source_version,created_at) VALUES('graph-primary','graph-secondary','graph-rule','company',1,0);
        INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('graph-party','company','customer','GRAPH','Graph Party','THB',0,0),('graph-foreign-party','foreign','customer','GRAPH','Foreign','THB',0,0);
        INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,journal_id,created_by,created_at,updated_at) VALUES('graph-invoice','company','graph-party','receivable','GRAPH-I','2026-01-10','2026-01-10','THB',10,0,10,10,'cash','sales','graph-primary','fixture',0,0),('graph-unrelated-invoice','company','graph-party','receivable','GRAPH-U','2026-01-10','2026-01-10','THB',10,0,10,10,'cash','sales','graph-other','fixture',0,0),('graph-foreign-invoice','foreign','graph-foreign-party','receivable','GRAPH-F','2026-01-10','2026-01-10','THB',10,0,10,10,'cash','sales','graph-primary','fixture',0,0);
        INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,journal_id,fx_journal_id,created_by,created_at,updated_at) VALUES('graph-payment-direct','company','graph-party','receipt','GRAPH-D','2026-01-10','THB',10,10,'graph-secondary',NULL,'fixture',0,0),('graph-payment-fx','company','graph-party','receipt','GRAPH-X','2026-01-10','THB',10,10,NULL,'graph-primary','fixture',0,0),('graph-payment-reversal','company','graph-party','receipt','GRAPH-R','2026-01-10','THB',10,10,NULL,NULL,'fixture',0,0),('graph-payment-fx-reversal','company','graph-party','receipt','GRAPH-FX','2026-01-10','THB',10,10,NULL,NULL,'fixture',0,0),('graph-payment-other','company','graph-party','receipt','GRAPH-O','2026-01-10','THB',10,10,'graph-other',NULL,'fixture',0,0),('graph-payment-foreign','foreign','graph-foreign-party','receipt','GRAPH-F','2026-01-10','THB',10,10,'graph-primary',NULL,'fixture',0,0);
        INSERT INTO finance_payment_reversals(id,payment_id,reversal_journal_id,fx_reversal_journal_id,reason,reversed_by,reversed_at) VALUES('graph-reversal','graph-payment-reversal','graph-primary',NULL,'fixture','fixture',0),('graph-fx-reversal','graph-payment-fx-reversal','graph-other','graph-secondary','fixture','fixture',0)").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_canonical_finance_attachment_targets(&mut tx).await.unwrap();lock_canonical_finance_history_book_roots(&mut tx).await.unwrap();lock_field_history_finance_candidate_sources(&mut tx).await.unwrap();lock_field_history_money_line_sources(&mut tx).await.unwrap();
        let target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"graph-secondary").await.unwrap();
        let roots=inspect_canonical_finance_history_journal_roots(&mut tx,&target).await.unwrap();
        let candidates=inspect_field_history_finance_candidates(&mut tx,&target,Some(&roots)).await.unwrap();
        let root_query=format!("{} SELECT id,source_type,source_id,currency FROM finance_history_root_journals ORDER BY id",candidates.cte());
        let actual_roots:Vec<(String,Option<String>,Option<String>,String)>=sqlx::query_as(sqlx::AssertSqlSafe(root_query)).bind(candidates.selected_id()).bind(candidates.company_id()).bind(candidates.kind_parameter()).fetch_all(&mut *tx).await.unwrap();
        let legacy_roots:Vec<(String,Option<String>,Option<String>,String)>=sqlx::query_as("SELECT j.id,j.source_type,j.source_id,j.currency FROM public.finance_journals j JOIN (SELECT d.id,d.company_id FROM public.finance_journals d WHERE d.id='graph-secondary' AND d.company_id='company' UNION SELECT p.id,p.company_id FROM public.finance_journals d JOIN public.finance_book_postings b ON b.secondary_journal_id=d.id JOIN public.finance_journals p ON p.id=b.primary_journal_id AND p.company_id=d.company_id WHERE d.id='graph-secondary' AND d.company_id='company') r ON r.id=j.id AND r.company_id=j.company_id ORDER BY j.id").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(actual_roots,legacy_roots);assert_eq!(actual_roots.len(),2);
        let money_lines=inspect_field_history_money_lines(&mut tx,&candidates).await.unwrap();
        assert!(std::ptr::eq(money_lines.source(),&candidates));
        let separately_captured=inspect_field_history_finance_candidates(&mut tx,&target,Some(&roots)).await.unwrap();
        assert!(!std::ptr::eq(money_lines.source(),&separately_captured));
        let money_query=format!("{}, {} SELECT EXISTS(SELECT 1 FROM field_history_finance_money m WHERE m.company_id='company' AND m.source_type='field_service_billing_release' AND m.source_id='graph-billing' AND m.amount_kind='invoice_total' AND m.currency='THB' AND m.amount_units=10),(SELECT sum(debit_minor)=10 FROM field_history_finance_journal_lines WHERE journal_id='graph-primary'),(SELECT sum(credit_minor)=10 FROM field_history_finance_journal_lines WHERE journal_id='graph-primary')",candidates.cte(),money_lines.cte_sql());
        let before:(bool,Option<bool>,Option<bool>)=sqlx::query_as(sqlx::AssertSqlSafe(money_query.as_str())).bind(candidates.selected_id()).bind(candidates.company_id()).bind(candidates.kind_parameter()).fetch_one(&mut *tx).await.unwrap();assert_eq!(before,(false,None,None));
        // These are staged rows in a rolled-back unit, not a committed Field
        // release, stock consumption or native owner writer proof.
        sqlx::query("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,used_latest_fallback,captured_at) VALUES('company','field_service_billing_release','graph-billing','invoice_total','2026-01-10','THB',2,10,10,false,0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES('graph-line-debit','graph-primary','cash',10,0,10,0,0),('graph-line-credit','graph-primary','sales',0,10,0,10,1)").execute(&mut *tx).await.unwrap();
        let actual:(bool,Option<bool>,Option<bool>)=sqlx::query_as(sqlx::AssertSqlSafe(money_query.as_str())).bind(candidates.selected_id()).bind(candidates.company_id()).bind(candidates.kind_parameter()).fetch_one(&mut *tx).await.unwrap();
        let legacy:(bool,Option<bool>,Option<bool>)=sqlx::query_as("SELECT EXISTS(SELECT 1 FROM public.money_transaction_snapshots m WHERE m.company_id='company' AND m.source_type='field_service_billing_release' AND m.source_id='graph-billing' AND m.amount_kind='invoice_total' AND m.currency='THB' AND m.amount_units=10),(SELECT sum(debit_minor)=10 FROM public.finance_journal_lines WHERE journal_id='graph-primary'),(SELECT sum(credit_minor)=10 FROM public.finance_journal_lines WHERE journal_id='graph-primary')").fetch_one(&mut *tx).await.unwrap();assert_eq!(actual,legacy);assert_eq!(actual,(true,Some(true),Some(true)));
        money_lines.require_current(&mut tx).await.unwrap();
        let query=format!("{} SELECT 'invoice' kind,id FROM finance_history_direct_invoices UNION ALL SELECT 'payment',id FROM finance_history_payment_candidates ORDER BY kind,id",candidates.cte());
        let rows:Vec<(String,String)>=sqlx::query_as(sqlx::AssertSqlSafe(query)).bind(candidates.selected_id()).bind(candidates.company_id()).bind(candidates.kind_parameter()).fetch_all(&mut *tx).await.unwrap();
        assert_eq!(rows,vec![("invoice".into(),"graph-invoice".into()),("payment".into(),"graph-payment-direct".into()),("payment".into(),"graph-payment-fx".into()),("payment".into(),"graph-payment-fx-reversal".into()),("payment".into(),"graph-payment-reversal".into())]);
        candidates.require_current(&mut tx).await.unwrap();
        let payment=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"graph-payment-direct").await.unwrap();
        assert!(inspect_field_history_finance_candidates(&mut tx,&payment,Some(&roots)).await.is_err());
        let single=inspect_field_history_finance_candidates(&mut tx,&payment,None).await.unwrap();
        let query=format!("{} SELECT id FROM finance_history_payment_candidates ORDER BY id",single.cte());
        let rows:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(query)).bind(single.selected_id()).bind(single.company_id()).bind(single.kind_parameter()).fetch_all(&mut *tx).await.unwrap();assert_eq!(rows,["graph-payment-direct"]);
        let foreign=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"graph-payment-foreign").await.unwrap();let absent=inspect_field_history_finance_candidates(&mut tx,&foreign,None).await.unwrap();let query=format!("{} SELECT count(*) FROM finance_history_payment_candidates",absent.cte());let count:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(query)).bind(absent.selected_id()).bind(absent.company_id()).bind(absent.kind_parameter()).fetch_one(&mut *tx).await.unwrap();assert_eq!(count,0);
        // The seam intentionally doesn't retain candidate rows; consumers
        // must repeat their full joined decision after same-unit effects.
        sqlx::query("SAVEPOINT graph_change").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE finance_payments SET journal_id='graph-secondary' WHERE id='graph-payment-other'").execute(&mut *tx).await.unwrap();candidates.require_current(&mut tx).await.unwrap();let query=format!("{} SELECT count(*) FROM finance_history_payment_candidates",candidates.cte());let changed:i64=sqlx::query_scalar(sqlx::AssertSqlSafe(query)).bind(candidates.selected_id()).bind(candidates.company_id()).bind(candidates.kind_parameter()).fetch_one(&mut *tx).await.unwrap();assert_eq!(changed,5);sqlx::query("ROLLBACK TO SAVEPOINT graph_change").execute(&mut *tx).await.unwrap();candidates.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn assets_purchase_finance_source_retains_full_row_and_scoped_absence() {
    use erpx_server_finance::public::asset_purchase_source::{lock_asset_purchase_finance_source,observe_asset_purchase_finance_source};
    use sqlx::Connection;
    fixture::with_database(3, |pool| async move {
        review::setup(&pool, false).await;
        sqlx::query("INSERT INTO public.finance_assets(id,company_id,asset_number,name,account_id,cost_minor,residual_minor,useful_life_months,in_service_date,status,version,currency) VALUES('linked','company','LINKED','Original','cash',10,0,12,'2026-01-01','in_service',1,'THB'),('foreign-linked','foreign','FOREIGN','Foreign','cash',10,0,12,'2026-01-01','in_service',1,'THB')").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let mut conn=pool.raw().acquire().await.unwrap();
        // A shadow table must not substitute for the public owner source.
        sqlx::raw_sql("CREATE TEMP TABLE finance_assets(id text,company_id text,name text); INSERT INTO finance_assets VALUES('linked','company','Forged')").execute(&mut *conn).await.unwrap();
        let mut tx=conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_asset_purchase_finance_source(&mut tx).await.unwrap();
        let source=observe_asset_purchase_finance_source(&mut tx,&company,Some("linked")).await.unwrap();
        let legacy:String=sqlx::query_scalar("SELECT to_jsonb(f)::text FROM public.finance_assets f WHERE f.id='linked' AND f.company_id='company'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(source.image(),Some(legacy.as_str()));assert!(source.image().unwrap().contains("Original"));
        let absent=observe_asset_purchase_finance_source(&mut tx,&company,Some("missing-linked")).await.unwrap();assert!(absent.image().is_none());
        let foreign=observe_asset_purchase_finance_source(&mut tx,&company,Some("foreign-linked")).await.unwrap();assert!(foreign.image().is_none());
        let no_id=observe_asset_purchase_finance_source(&mut tx,&company,None).await.unwrap();assert!(no_id.image().is_none());
        sqlx::query("SAVEPOINT source_drift").execute(&mut *tx).await.unwrap();
        // A same-version change still changes the legacy full-row image.
        sqlx::query("UPDATE public.finance_assets SET name='Changed' WHERE id='linked'").execute(&mut *tx).await.unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        sqlx::query("INSERT INTO public.finance_assets(id,company_id,asset_number,name,account_id,cost_minor,residual_minor,useful_life_months,in_service_date,status,version,currency) VALUES('missing-linked','company','MISSING','Inserted','cash',10,0,12,'2026-01-01','in_service',1,'THB')").execute(&mut *tx).await.unwrap();
        assert!(absent.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT source_drift").execute(&mut *tx).await.unwrap();
        source.require_current(&mut tx).await.unwrap();absent.require_current(&mut tx).await.unwrap();foreign.require_current(&mut tx).await.unwrap();no_id.require_current(&mut tx).await.unwrap();
        // A separate writer cannot pass the read fence while this unit lives.
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("SET LOCAL lock_timeout='200ms'").execute(&mut *writer).await.unwrap();
        let blocked=sqlx::query("UPDATE public.finance_assets SET name='Writer' WHERE id='linked'").execute(&mut *writer).await.unwrap_err();
        assert_eq!(blocked.as_database_error().and_then(|e|e.code()).as_deref(),Some("55P03"));writer.rollback().await.unwrap();
        tx.rollback().await.unwrap();
        let mut fresh=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_asset_purchase_finance_source(&mut fresh).await.unwrap();assert!(source.require_current(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
        let mut save=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *save).await.unwrap();sqlx::query("SAVEPOINT before_source_lock").execute(&mut *save).await.unwrap();lock_asset_purchase_finance_source(&mut save).await.unwrap();let held=observe_asset_purchase_finance_source(&mut save,&company,Some("linked")).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_source_lock").execute(&mut *save).await.unwrap();assert!(held.require_current(&mut save).await.is_err());save.rollback().await.unwrap();
    }).await;
}

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
async fn forged_inventory_transit_loss_label_is_not_a_journal_source() {
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, false).await;
        create(&pool, "forged-label", false, false).await.unwrap();
        sqlx::query("UPDATE finance_journals SET source_type='inventory_transit_loss' WHERE id='journal-forged-label'")
            .execute(pool.raw())
            .await
            .unwrap();
        denied(
            inspect(&pool, "journal-forged-label").await,
            "FINANCE_JOURNAL_SOURCE_UNSUPPORTED",
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT status FROM finance_journals WHERE id='journal-forged-label'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "draft"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM money_transaction_snapshots")
                .fetch_one(pool.raw())
                .await
                .unwrap(),
            0
        );
    })
    .await;
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

#[tokio::test]
async fn global_raw_field_journal_marker_is_same_unit_and_retains_absence() {
    use erpx_server_finance::public::raw_field_journal_marker::*;
    use erpx_server_finance::public::field_attachment_routing::{inspect_global_field_journal_finance_source,lock_field_attachment_finance_routing};
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, false).await;
        let id=format!("field-journal-{}", "x".repeat(220));
        let mut unlocked=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(inspect_raw_field_journal_marker(&mut unlocked,&id).await.is_err());
        unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_raw_field_journal_marker_sources(&mut tx).await.unwrap();lock_field_attachment_finance_routing(&mut tx).await.unwrap();
        sqlx::query("LOCK TABLE public.operational_inventory_consumptions IN SHARE MODE NOWAIT").execute(&mut *tx).await.unwrap();
        let source=inspect_global_field_journal_finance_source(&mut tx).await.unwrap();
        let candidate_sql=format!("WITH {} SELECT EXISTS(SELECT 1 FROM finance_field_service_journals j WHERE j.id=$1::jsonb->>'target')",source.cte_sql());
        let input=serde_json::json!({"target":id.clone()}).to_string();
        let before:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(candidate_sql.as_str())).bind(&input).fetch_one(&mut *tx).await.unwrap();assert!(!before);
        let absent=inspect_raw_field_journal_marker(&mut tx,&id).await.unwrap();
        assert!(!absent.marked());absent.require_current(&mut tx).await.unwrap();
        // Deliberately different Company: the 1097 global discriminator has
        // no selected-company filter and does not authorize the caller.
        sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,created_by,created_at,updated_at) VALUES($1,'foreign','chart','period','FIELD-MARKER','2026-01-10','THB','field_service_parts','fixture',0,0)").bind(&id).execute(&mut *tx).await.unwrap();
        assert!(absent.require_current(&mut tx).await.is_err());
        // This source retains locks and identity, not rows. The full decision
        // must be recomputed when a same-unit write adds a candidate.
        source.require_current(&mut tx).await.unwrap();
        let after:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(candidate_sql.as_str())).bind(&input).fetch_one(&mut *tx).await.unwrap();assert!(after);
        for value in [input.clone(),serde_json::json!({"target":"missing"}).to_string(),serde_json::json!({"target":null}).to_string(),serde_json::json!({"target":123}).to_string(),"[]".to_owned()] {
            let actual:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(candidate_sql.as_str())).bind(&value).fetch_one(&mut *tx).await.unwrap();
            // Fixture has no Inventory consumption markers, so 1097's second
            // owner branch is false; no full classifier claim is made here.
            let oracle:bool=sqlx::query_scalar("SELECT erpx_finance_field_journal_owned($1::jsonb->>'target',NULL)").bind(&value).fetch_one(&mut *tx).await.unwrap();assert_eq!(actual,oracle,"{value}");
        }
        let field=inspect_raw_field_journal_marker(&mut tx,&id).await.unwrap();assert!(field.marked());field.require_current(&mut tx).await.unwrap();
        assert!(!inspect_raw_field_journal_marker(&mut tx,"unknown").await.unwrap().marked());
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_raw_field_journal_marker_sources(&mut other).await.unwrap();assert!(field.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT before_marker_lock").execute(&mut *saved).await.unwrap();lock_raw_field_journal_marker_sources(&mut saved).await.unwrap();let proof=inspect_raw_field_journal_marker(&mut saved,&id).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_marker_lock").execute(&mut *saved).await.unwrap();assert!(proof.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn global_payment_allocation_marker_retains_presence_and_absence() {
    use erpx_server_finance::public::raw_field_payment_allocation_marker::*;
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, false).await;
        let payment=format!("payment-{}","x".repeat(220));
        let mut unlocked=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(inspect_raw_field_payment_allocation_marker(&mut unlocked,&payment).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_raw_field_payment_allocation_sources(&mut tx).await.unwrap();
        let absent=inspect_raw_field_payment_allocation_marker(&mut tx,&payment).await.unwrap();assert!(!absent.marked());absent.require_current(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('marker-party','company','customer','MARKER','Synthetic','THB',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,created_by,created_at,updated_at) VALUES($1,'company','marker-party','receipt','MARKER-PAY','2026-01-10','THB',100,100,'fixture',0,0)").bind(&payment).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES('marker-invoice','company','marker-party','receivable','MARKER-INVOICE','2026-01-10','2026-01-10','THB',100,0,100,100,'cash','sales','field_service','synthetic-source','fixture',0,0)").execute(&mut *tx).await.unwrap();
        // Uncommitted staged rows exercise the raw predicate only; the normal
        // deferred Service writer guard remains active and is not bypassed.
        sqlx::query("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES('marker-allocation','foreign',$1,'marker-invoice',100,'fixture',0)").bind(&payment).execute(&mut *tx).await.unwrap();
        assert!(absent.require_current(&mut tx).await.is_err());
        let marked=inspect_raw_field_payment_allocation_marker(&mut tx,&payment).await.unwrap();assert!(marked.marked());marked.require_current(&mut tx).await.unwrap();
        assert!(!inspect_raw_field_payment_allocation_marker(&mut tx,"missing").await.unwrap().marked());
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_raw_field_payment_allocation_sources(&mut other).await.unwrap();assert!(marked.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT before_payment_lock").execute(&mut *saved).await.unwrap();lock_raw_field_payment_allocation_sources(&mut saved).await.unwrap();let proof=inspect_raw_field_payment_allocation_marker(&mut saved,&payment).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_payment_lock").execute(&mut *saved).await.unwrap();assert!(proof.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn payment_marker_fences_each_source_and_releases_on_conflict() {
    use erpx_server_finance::public::raw_field_payment_allocation_marker::*;
    use sqlx::Acquire;
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, false).await;
        let mut blocker_conn=pool.raw().acquire().await.unwrap();let blocker_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *blocker_conn).await.unwrap();
        let mut reader_conn=pool.raw().acquire().await.unwrap();let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader_conn).await.unwrap();assert_ne!(blocker_pid,reader_pid);
        let mut blocker=blocker_conn.begin().await.unwrap();sqlx::query("LOCK TABLE finance_allocations IN ACCESS EXCLUSIVE MODE").execute(&mut *blocker).await.unwrap();
        let mut reader=reader_conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
        let result=tokio::time::timeout(std::time::Duration::from_secs(5),lock_raw_field_payment_allocation_sources(&mut reader)).await.unwrap();
        assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        // PostgreSQL releases the preceding statement's locks on abort; do
        // not treat an already-failed unit as a lock-order observation.
        let invoice_held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='finance_invoices'::regclass AND mode='ShareLock' AND granted)").bind(reader_pid).fetch_one(&mut *blocker).await.unwrap();assert!(!invoice_held);
        reader.rollback().await.unwrap();blocker.rollback().await.unwrap();
        let mut invoice_blocker=blocker_conn.begin().await.unwrap();sqlx::query("LOCK TABLE finance_invoices IN ACCESS EXCLUSIVE MODE").execute(&mut *invoice_blocker).await.unwrap();
        let mut reader=reader_conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
        let result=tokio::time::timeout(std::time::Duration::from_secs(5),lock_raw_field_payment_allocation_sources(&mut reader)).await.unwrap();
        assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        reader.rollback().await.unwrap();invoice_blocker.rollback().await.unwrap();drop(reader_conn);drop(blocker_conn);
        let mut fresh=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_raw_field_payment_allocation_sources(&mut fresh).await.unwrap();fresh.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn global_raw_field_invoice_marker_retains_presence_and_absence() {
    use erpx_server_finance::public::raw_field_invoice_marker::*;
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, false).await;
        let id=format!("invoice-{}","x".repeat(220));
        let mut unlocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();assert!(inspect_raw_field_invoice_marker(&mut unlocked,&id).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_raw_field_invoice_marker_sources(&mut tx).await.unwrap();
        let absent=inspect_raw_field_invoice_marker(&mut tx,&id).await.unwrap();assert!(!absent.marked());absent.require_current(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('invoice-marker-party','company','customer','INVOICE-MARKER','Synthetic','THB',0,0)").execute(&mut *tx).await.unwrap();
        // Global discriminator intentionally sees this foreign Company source;
        // the staged rows are rolled back, not a claimed Field handoff.
        sqlx::query("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES($1,'foreign','invoice-marker-party','receivable','MARKER','2026-01-10','2026-01-10','THB',100,0,100,100,'cash','sales','field_service','synthetic','fixture',0,0)").bind(&id).execute(&mut *tx).await.unwrap();
        assert!(absent.require_current(&mut tx).await.is_err());let marked=inspect_raw_field_invoice_marker(&mut tx,&id).await.unwrap();assert!(marked.marked());marked.require_current(&mut tx).await.unwrap();assert!(!inspect_raw_field_invoice_marker(&mut tx,"missing").await.unwrap().marked());tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_raw_field_invoice_marker_sources(&mut other).await.unwrap();assert!(marked.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT before_invoice_lock").execute(&mut *saved).await.unwrap();lock_raw_field_invoice_marker_sources(&mut saved).await.unwrap();let proof=inspect_raw_field_invoice_marker(&mut saved,&id).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_invoice_lock").execute(&mut *saved).await.unwrap();assert!(proof.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn finance_attachment_target_uses_actual_company_and_record_version() {
    use erpx_server_finance::public::attachment_target_source::*;
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, false).await;
        let company=CompanyId::new("company").unwrap();let foreign=CompanyId::new("foreign").unwrap();
        sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,version,created_by,created_at,updated_at) VALUES('target-journal','company','chart','period','TARGET-J','2026-01-10','THB','manual',7,'fixture',0,0)").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,version,created_by,created_at,updated_at) VALUES('target-payment','company','receipt','TARGET-P','2026-01-10','THB',100,100,9,'fixture',0,0)").execute(pool.raw()).await.unwrap();
        let mut unlocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();assert!(inspect_finance_attachment_target(&mut unlocked,FinanceAttachmentTargetKind::Journal,&company,"target-journal").await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_finance_attachment_target_sources(&mut tx).await.unwrap();
        let journal=inspect_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"target-journal").await.unwrap().unwrap();
        let payment=inspect_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"target-payment").await.unwrap().unwrap();
        assert_eq!((journal.kind(),journal.company(),journal.id(),journal.version()),(FinanceAttachmentTargetKind::Journal,&company,"target-journal",7));assert_eq!((payment.id(),payment.version()),("target-payment",9));
        assert!(inspect_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&foreign,"target-journal").await.unwrap().is_none());assert!(inspect_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"target-journal").await.unwrap().is_none());
        journal.require_current(&mut tx).await.unwrap();payment.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE finance_journals SET version=version+1 WHERE id='target-journal'").execute(&mut *tx).await.unwrap();assert!(journal.require_current(&mut tx).await.is_err());payment.require_current(&mut tx).await.unwrap();tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_finance_attachment_target_sources(&mut other).await.unwrap();assert!(payment.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT before_target_lock").execute(&mut *saved).await.unwrap();lock_finance_attachment_target_sources(&mut saved).await.unwrap();let proof=inspect_finance_attachment_target(&mut saved,FinanceAttachmentTargetKind::Journal,&company,"target-journal").await.unwrap().unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_target_lock").execute(&mut *saved).await.unwrap();assert!(proof.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
        // Temp shadows deliberately precede this transaction; the canonical
        // owner port must lock and read public, never the forged versions.
        {use sqlx::Acquire;let mut conn=pool.raw().acquire().await.unwrap();sqlx::raw_sql("CREATE TEMP TABLE finance_journals(id text,company_id text,version bigint);CREATE TEMP TABLE finance_payments(id text,company_id text,version bigint);INSERT INTO pg_temp.finance_journals VALUES('target-journal','company',999);INSERT INTO pg_temp.finance_payments VALUES('target-payment','company',999)").execute(&mut *conn).await.unwrap();
         let mut tx=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_canonical_finance_attachment_targets(&mut tx).await.unwrap();
         let j=inspect_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"target-journal").await.unwrap().unwrap();let p=inspect_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"target-payment").await.unwrap().unwrap();assert_eq!((j.version(),p.version()),(7,9));assert!(inspect_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&foreign,"target-journal").await.unwrap().is_none());assert!(inspect_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"missing").await.unwrap().is_none());
         let absent=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"target-absent").await.unwrap();assert_eq!(absent.version(),None);absent.require_current(&mut tx).await.unwrap();
         let foreign_absent=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&foreign,"target-journal").await.unwrap();assert_eq!(foreign_absent.version(),None);foreign_absent.require_current(&mut tx).await.unwrap();
         sqlx::query("SAVEPOINT canonical_insert").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,version,created_by,created_at,updated_at) VALUES('target-absent','company','chart','period','TARGET-ABSENT','2026-01-10','THB','manual',1,'fixture',0,0)").execute(&mut *tx).await.unwrap();assert!(absent.require_current(&mut tx).await.is_err());foreign_absent.require_current(&mut tx).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT canonical_insert").execute(&mut *tx).await.unwrap();absent.require_current(&mut tx).await.unwrap();
         let missing_payment=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"target-payment-late").await.unwrap();assert_eq!(missing_payment.version(),None);sqlx::query("SAVEPOINT canonical_payment_insert").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,version,created_by,created_at,updated_at) VALUES('target-payment-late','company','receipt','TARGET-LATE-P','2026-01-10','THB',100,100,1,'fixture',0,0)").execute(&mut *tx).await.unwrap();assert!(missing_payment.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT canonical_payment_insert").execute(&mut *tx).await.unwrap();missing_payment.require_current(&mut tx).await.unwrap();
         sqlx::query("SAVEPOINT canonical_version_change").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.finance_journals SET version=version+1 WHERE id='target-journal'").execute(&mut *tx).await.unwrap();assert!(j.require_current(&mut tx).await.is_err());p.require_current(&mut tx).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT canonical_version_change").execute(&mut *tx).await.unwrap();j.require_current(&mut tx).await.unwrap();
         sqlx::query("SET LOCAL search_path TO pg_catalog,public,pg_temp").execute(&mut *tx).await.unwrap();assert!(p.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();sqlx::raw_sql("DROP TABLE pg_temp.finance_journals;DROP TABLE pg_temp.finance_payments").execute(&mut *conn).await.unwrap();}
        let mut saved=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();sqlx::query("SELECT pg_current_xact_id()").execute(&mut *saved).await.unwrap();sqlx::query("SAVEPOINT canonical_locks").execute(&mut *saved).await.unwrap();lock_canonical_finance_attachment_targets(&mut saved).await.unwrap();let canonical=inspect_canonical_finance_attachment_target(&mut saved,FinanceAttachmentTargetKind::Journal,&company,"target-journal").await.unwrap().unwrap();sqlx::query("ROLLBACK TO SAVEPOINT canonical_locks").execute(&mut *saved).await.unwrap();assert!(canonical.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
        // A retained payment absence is not portable to another transaction.
        let mut absence_unit=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *absence_unit).await.unwrap();lock_canonical_finance_attachment_targets(&mut absence_unit).await.unwrap();let payment_absent=observe_canonical_finance_attachment_target(&mut absence_unit,FinanceAttachmentTargetKind::Payment,&company,"target-absent-payment").await.unwrap();assert_eq!(payment_absent.version(),None);payment_absent.require_current(&mut absence_unit).await.unwrap();
        let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *absence_unit).await.unwrap();
        let (send_pid,recv_pid)=tokio::sync::oneshot::channel();let writer_pool=pool.clone();
        let mut inserting=tokio::spawn(async move {use sqlx::Acquire;let mut conn=writer_pool.raw().acquire().await?;let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *conn).await?;let _=send_pid.send(pid);let mut tx=conn.begin().await?;sqlx::query("SET LOCAL statement_timeout='12s'").execute(&mut *tx).await?;sqlx::query("INSERT INTO public.finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,version,created_by,created_at,updated_at) VALUES('target-absent-payment','company','receipt','TARGET-ABSENT-P','2026-01-10','THB',100,100,1,'fixture',0,0)").execute(&mut *tx).await?;tx.commit().await?;Ok::<(),sqlx::Error>(())});
        let blocked=tokio::time::timeout(std::time::Duration::from_secs(5),async{let pid=recv_pid.await.map_err(|_|"writer exited before PID")?;loop{let blocked:bool=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2)) AND EXISTS(SELECT 1 FROM pg_locks WHERE pid=$2 AND relation='public.finance_payments'::regclass AND mode='RowExclusiveLock' AND NOT granted)").bind(reader_pid).bind(pid).fetch_one(&mut *absence_unit).await.map_err(|_|"lock observation failed")?;if blocked{return Ok::<_,&str>(pid);}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await;
        // Release before every assertion; timeout aborts and joins the writer.
        let release=tokio::time::timeout(std::time::Duration::from_secs(5),absence_unit.rollback()).await;let finished=tokio::time::timeout(std::time::Duration::from_secs(10),&mut inserting).await;if finished.is_err(){inserting.abort();let _=inserting.await;}release.expect("reader rollback timed out").expect("reader rollback failed");let writer_pid=blocked.expect("writer failed to wait").expect("writer observation failed");assert_ne!(writer_pid,reader_pid);finished.expect("writer timed out").expect("writer task failed").expect("writer insert failed");
        let mut fresh=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_canonical_finance_attachment_targets(&mut fresh).await.unwrap();assert!(payment_absent.require_current(&mut fresh).await.is_err());let payment_now=observe_canonical_finance_attachment_target(&mut fresh,FinanceAttachmentTargetKind::Payment,&company,"target-absent-payment").await.unwrap();assert_eq!(payment_now.version(),Some(1));payment_now.require_current(&mut fresh).await.unwrap();fresh.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("UPDATE public.finance_payments SET version=version+1 WHERE id='target-payment'").execute(&mut *writer).await.unwrap();let mut blocked=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();let conflict=lock_canonical_finance_attachment_targets(&mut blocked).await;assert!(matches!(conflict,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn service_customer_sources_retain_finance_party_lock_across_the_unit() {
    use erpx_server_finance::public::field_service_workspace::*;
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, false).await;
        sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('service-lock-customer','company','customer','SERVICE-LOCK','Synthetic customer','THB',0,0)").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();let ids=vec!["service-lock-customer".to_owned()];
        let mut outside=pool.raw().acquire().await.unwrap();assert!(require_service_workspace_customers_held(&mut outside).await.is_err());drop(outside);
        let mut unlocked=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        assert!(read_service_workspace_customers(&mut unlocked,&company,false,&ids).await.is_err());
        assert!(read_service_settlement_customers(&mut unlocked,&company,&ids).await.is_err());
        unlocked.rollback().await.unwrap();
        let mut weak=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE finance_parties IN ROW SHARE MODE").execute(&mut *weak).await.unwrap();assert!(require_service_workspace_customers_held(&mut weak).await.is_err());weak.rollback().await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        sqlx::query("SELECT pg_current_xact_id()").execute(&mut *tx).await.unwrap();
        sqlx::query("SAVEPOINT before_customer_lock").execute(&mut *tx).await.unwrap();
        lock_service_workspace_customers(&mut tx).await.unwrap();
        require_service_workspace_customers_held(&mut tx).await.unwrap();
        let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND relation='finance_parties'::regclass AND mode='ShareLock' AND granted)").fetch_one(&mut *tx).await.unwrap();assert!(held);
        let workspace=read_service_workspace_customers(&mut tx,&company,false,&ids).await.unwrap();
        let settlement=read_service_settlement_customers(&mut tx,&company,&ids).await.unwrap();
        assert!(workspace.data().display.iter().any(|row|row.id==ids[0] && row.kind=="customer"));
        assert!(settlement.data().iter().any(|row|row.id==ids[0] && row.kind=="customer"));
        workspace.require_current(&mut tx).await.unwrap();settlement.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT before_customer_lock").execute(&mut *tx).await.unwrap();
        assert!(workspace.require_current(&mut tx).await.is_err());assert!(settlement.require_current(&mut tx).await.is_err());assert!(require_service_workspace_customers_held(&mut tx).await.is_err());
        sqlx::query("LOCK TABLE finance_parties IN EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();require_service_workspace_customers_held(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn payment_receipt_candidates_join_actual_field_invoice_and_revalidate() {
    use erpx_server_finance::public::raw_field_invoice_marker::{inspect_payment_receipt_field_origin,lock_raw_field_invoice_marker_sources};
    use erpx_server_finance::public::raw_field_payment_allocation_marker::{inspect_field_payment_origin,lock_raw_field_payment_allocation_sources};
    use erpx_server_workflow::public::record_origin::{inspect_payment_origin_candidates,lock_payment_origin_candidate_sources};
    use erpx_server_finance::public::field_attachment_routing::{inspect_global_field_payment_finance_sources,lock_field_attachment_finance_routing};
    fixture::with_database(2, |pool| async move {
        review::setup(&pool,false).await;
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_payment_origin_candidate_sources(&mut tx).await.unwrap();lock_raw_field_payment_allocation_sources(&mut tx).await.unwrap();lock_field_attachment_finance_routing(&mut tx).await.unwrap();
        sqlx::query("LOCK TABLE public.attachment_sets,public.attachment_links,public.attachments,public.upload_intents IN SHARE ROW EXCLUSIVE MODE NOWAIT").execute(&mut *tx).await.unwrap();
        let finance_sources=inspect_global_field_payment_finance_sources(&mut tx).await.unwrap();
        let absent=inspect_payment_origin_candidates(&mut tx,"global-payment").await.unwrap();
        let none=inspect_payment_receipt_field_origin(&mut tx,&absent).await.unwrap();assert!(!none.marked());none.require_current(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('join-party','company','customer','JOIN-PARTY','Synthetic','THB',0,0)").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES('global-invoice','foreign','join-party','receivable','JOIN-INVOICE','2026-01-10','2026-01-10','THB',100,0,100,100,'cash','sales','field_service','synthetic','fixture',0,0)").execute(&mut *tx).await.unwrap();
        for (key,record,result) in [("candidate-record","global-payment",r#"{"settlementInvoiceId":"global-invoice"}"#),("candidate-json","other",r#"{"id":"global-payment","settlementInvoiceId":"global-invoice"}"#),("candidate-null","global-payment",r#"{"settlementInvoiceId":null}"#),("candidate-number","global-payment",r#"{"settlementInvoiceId":123}"#)] {
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,'finance.payment.create','fixture','synthetic',$2,$3,0)").bind(key).bind(record).bind(result).execute(&mut *tx).await.unwrap();
        }
        assert!(none.require_current(&mut tx).await.is_err());
        for (key,record,result) in [("only-record","record-only",r#"{"settlementInvoiceId":"global-invoice"}"#),("only-json","unrelated",r#"{"id":"json-only","settlementInvoiceId":"global-invoice"}"#),("negative-null","negative-only",r#"{"settlementInvoiceId":null}"#),("negative-array","negative-only",r#"["global-invoice"]"#),("negative-number","negative-only",r#"{"settlementInvoiceId":123}"#),("duplicate-record","record-only",r#"{"settlementInvoiceId":"global-invoice"}"#)] {
            sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES($1,'finance.payment.create','fixture','synthetic',$2,$3,0)").bind(key).bind(record).bind(result).execute(&mut *tx).await.unwrap();
        }
        for payment in ["allocation-only","both-branches"] {
            sqlx::query("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,created_by,created_at,updated_at) VALUES($1,'company','join-party','receipt',$1,'2026-01-10','THB',100,100,'fixture',0,0)").bind(payment).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES($1,'foreign',$2,'global-invoice',100,'fixture',0)").bind(format!("allocation-{payment}")).bind(payment).execute(&mut *tx).await.unwrap();
        }
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('both-receipt','finance.payment.create','fixture','synthetic','both-branches',$1,0)").bind(r#"{"settlementInvoiceId":"global-invoice"}"#).execute(&mut *tx).await.unwrap();
        for (id,expected) in [("record-only",true),("allocation-only",true),("both-branches",true),("negative-only",false)] {
            let source=inspect_payment_origin_candidates(&mut tx,id).await.unwrap();let origin=inspect_field_payment_origin(&mut tx,&source).await.unwrap();assert_eq!(origin.marked(),expected,"{id}");origin.require_current(&mut tx).await.unwrap();
        }
        // Compose only test-side Workflow receipt inputs with the actual Finance
        // CTEs and compare the installed 1092 SQL function on global rows.
        // Invoice and allocation companies intentionally need not match.
        let composed=format!("WITH {} SELECT EXISTS(SELECT 1 FROM command_receipts r JOIN finance_field_service_invoices i ON i.id=r.result_json::jsonb->>'settlementInvoiceId' WHERE r.operation='finance.payment.create' AND (r.record_id=$1 OR r.result_json::jsonb->>'id'=$1)) OR EXISTS(SELECT 1 FROM finance_field_payment_allocations a WHERE a.payment_id=$1)",finance_sources.cte_sql());
        for id in ["global-payment","record-only","json-only","allocation-only","both-branches","negative-only","missing-global-payment"] {
            let actual:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(composed.as_str())).bind(id).fetch_one(&mut *tx).await.unwrap();
            let oracle:bool=sqlx::query_scalar("SELECT erpx_finance_field_payment_owned($1)").bind(id).fetch_one(&mut *tx).await.unwrap();
            assert_eq!(actual,oracle,"{id}");finance_sources.require_current(&mut tx).await.unwrap();
        }
        // One actual attachment is linked from an unrelated set and a
        // foreign Finance-payment set. The 1093/1097 predicate is global.
        sqlx::raw_sql("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at,company_id) VALUES('global-attach-upload','asset','asset','manual','file','text/plain',1,'global-attach-staging','uploaded','fixture',10000,0,0,'company'); INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id) VALUES('global-attach','global-attach-upload','asset','asset','manual','global-attach-object',1,'file','text/plain',1,'checksum','fixture',0,'company'); INSERT INTO attachment_sets(id,company_id,branch_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) VALUES('global-related-set','foreign','branch','finance_payment','allocation-only','record','manual',1,1,'internal','{}',1,10,'draft','fixture',0,0); INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('global-related-link','global-related-set','global-attach','draft','fixture',0,10000)").execute(&mut *tx).await.unwrap();
        let related_sql=format!("WITH {} SELECT EXISTS(SELECT 1 FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN finance_field_payment_allocations a ON a.payment_id=s.target_id WHERE l.attachment_id=$1 AND s.target_type='finance_payment')",finance_sources.cte_sql());
        let related:bool=sqlx::query_scalar(sqlx::AssertSqlSafe(related_sql.as_str())).bind("global-attach").fetch_one(&mut *tx).await.unwrap();
        let related_oracle:bool=sqlx::query_scalar("SELECT erpx_files_field_attachment_owned($1)").bind("global-attach").fetch_one(&mut *tx).await.unwrap();
        assert!(related);assert_eq!(related,related_oracle);finance_sources.require_current(&mut tx).await.unwrap();
        let retained_absence=inspect_payment_origin_candidates(&mut tx,"late-payment").await.unwrap();let before=inspect_field_payment_origin(&mut tx,&retained_absence).await.unwrap();assert!(!before.marked());
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('late-receipt','finance.payment.create','fixture','synthetic','late-payment',$1,0)").bind(r#"{"settlementInvoiceId":"global-invoice"}"#).execute(&mut *tx).await.unwrap();assert!(before.require_current(&mut tx).await.is_err());
        for (id,expected) in [("record-only",true),("json-only",true),("negative-only",false)] {
            let source=inspect_payment_origin_candidates(&mut tx,id).await.unwrap();let origin=inspect_payment_receipt_field_origin(&mut tx,&source).await.unwrap();assert_eq!(origin.marked(),expected,"{id}");origin.require_current(&mut tx).await.unwrap();
        }
        let candidates=inspect_payment_origin_candidates(&mut tx,"global-payment").await.unwrap();let marked=inspect_payment_receipt_field_origin(&mut tx,&candidates).await.unwrap();assert!(marked.marked());marked.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT candidate_tamper").execute(&mut *tx).await.unwrap();let tamper=format!("DELETE FROM {} WHERE command_id='candidate-null'",candidates.relation_identifier());sqlx::query(sqlx::AssertSqlSafe(tamper.as_str())).execute(&mut *tx).await.unwrap();assert!(marked.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT candidate_tamper").execute(&mut *tx).await.unwrap();marked.require_current(&mut tx).await.unwrap();
        // An independent nondeterministic collation oracle equates distinct UTF-8
        // bytes; the actual witness also rejects the byte substitution.
        sqlx::query("SAVEPOINT candidate_collation").execute(&mut *tx).await.unwrap();
        sqlx::query("CREATE COLLATION fixture_payment_nondeterministic (provider=icu, locale='und-u-ks-level2', deterministic=false)").execute(&mut *tx).await.unwrap();
        let (equal,text_same,bytes_same):(bool,bool,bool)=sqlx::query_as("WITH source(v) AS (VALUES ('candidate-null'::text COLLATE fixture_payment_nondeterministic)), retained(v) AS (VALUES ('CANDIDATE-NULL'::text COLLATE fixture_payment_nondeterministic)) SELECT (SELECT source.v=retained.v FROM source,retained),NOT EXISTS((SELECT v FROM source EXCEPT ALL SELECT v FROM retained) UNION ALL (SELECT v FROM retained EXCEPT ALL SELECT v FROM source)),NOT EXISTS((SELECT convert_to(v,'UTF8') FROM source EXCEPT ALL SELECT convert_to(v,'UTF8') FROM retained) UNION ALL (SELECT convert_to(v,'UTF8') FROM retained EXCEPT ALL SELECT convert_to(v,'UTF8') FROM source))").fetch_one(&mut *tx).await.unwrap();assert!(equal);assert!(text_same);assert!(!bytes_same);
        let collated=inspect_payment_origin_candidates(&mut tx,"global-payment").await.unwrap();collated.require_current(&mut tx).await.unwrap();
        let mutate=format!("UPDATE {} SET command_id='CANDIDATE-NULL' WHERE command_id='candidate-null'",collated.relation_identifier());sqlx::query(sqlx::AssertSqlSafe(mutate.as_str())).execute(&mut *tx).await.unwrap();
        assert!(collated.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT candidate_collation").execute(&mut *tx).await.unwrap();marked.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT candidate_shape").execute(&mut *tx).await.unwrap();let ddl=format!("ALTER TABLE {} ADD COLUMN injected text",candidates.relation_identifier());sqlx::query(sqlx::AssertSqlSafe(ddl.as_str())).execute(&mut *tx).await.unwrap();assert!(marked.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT candidate_shape").execute(&mut *tx).await.unwrap();marked.require_current(&mut tx).await.unwrap();
        sqlx::query("SAVEPOINT candidate_drop").execute(&mut *tx).await.unwrap();let ddl=format!("DROP TABLE {}",candidates.relation_identifier());sqlx::query(sqlx::AssertSqlSafe(ddl.as_str())).execute(&mut *tx).await.unwrap();assert!(marked.require_current(&mut tx).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT candidate_drop").execute(&mut *tx).await.unwrap();marked.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_payment_origin_candidate_sources(&mut other).await.unwrap();lock_raw_field_invoice_marker_sources(&mut other).await.unwrap();assert!(marked.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('receipt-writer','finance.payment.create','fixture','synthetic','writer-payment','{}',0)").execute(&mut *writer).await.unwrap();
        let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='command_receipts'::regclass AND granted AND mode='RowExclusiveLock')").bind(writer_pid).fetch_one(pool.raw()).await.unwrap();assert!(held);
        let mut contender=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *contender).await.unwrap();
        let conflict=lock_payment_origin_candidate_sources(&mut contender).await;assert!(matches!(conflict,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));contender.rollback().await.unwrap();writer.rollback().await.unwrap();
        let mut malformed=pool.raw().begin().await.unwrap();
        let bad=sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES('receipt-malformed','finance.payment.create','fixture','synthetic','bad-payment','{invalid',0)").execute(&mut *malformed).await.unwrap_err();assert_eq!(bad.as_database_error().and_then(|e|e.code()).as_deref(),Some("22P02"));malformed.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn raw_field_invoice_fence_returns_retryable_conflict_with_actual_writer() {
    use erpx_server_finance::public::raw_field_invoice_marker::lock_raw_field_invoice_marker_sources;
    use sqlx::Acquire;
    fixture::with_database(2, |pool| async move {
        review::setup(&pool,false).await;
        let mut writer_conn=pool.raw().acquire().await.unwrap();let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer_conn).await.unwrap();
        let mut reader_conn=pool.raw().acquire().await.unwrap();let reader_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *reader_conn).await.unwrap();assert_ne!(reader_pid,writer_pid);
        let mut writer=writer_conn.begin().await.unwrap();
        sqlx::query("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('invoice-writer-party','company','customer','INVOICE-WRITER','Synthetic','THB',0,0)").execute(&mut *writer).await.unwrap();
        sqlx::query("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES('invoice-writer','foreign','invoice-writer-party','receivable','WRITER-INVOICE','2026-01-10','2026-01-10','THB',100,0,100,100,'cash','sales','field_service','synthetic','fixture',0,0)").execute(&mut *writer).await.unwrap();
        let held:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation='finance_invoices'::regclass AND mode='RowExclusiveLock' AND granted)").bind(writer_pid).fetch_one(&mut *writer).await.unwrap();assert!(held);
        let mut reader=reader_conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
        let result=tokio::time::timeout(std::time::Duration::from_secs(5),lock_raw_field_invoice_marker_sources(&mut reader)).await.unwrap();
        assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));reader.rollback().await.unwrap();writer.rollback().await.unwrap();drop(reader_conn);drop(writer_conn);
        let mut fresh=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_raw_field_invoice_marker_sources(&mut fresh).await.unwrap();fresh.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn finance_history_journal_roots_match_book_projection_and_revalidate() {
    use erpx_server_finance::public::attachment_target_source::*;
    fixture::with_database(2, |pool| async move {
        review::setup(&pool, false).await;
        // Staged rows exercise Finance's historical root query, not an owner
        // command, purchase grant, posted book, or valid business lineage.
        sqlx::query("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,version,created_by,created_at,updated_at) VALUES('root-primary','company','chart','period','ROOT-PRIMARY','2026-01-10','THB','manual',1,'fixture',0,0),('root-secondary','company','chart','period','ROOT-SECONDARY','2026-01-10','THB','manual',1,'fixture',0,0),('root-foreign-primary','foreign','chart','period','ROOT-FOREIGN-P','2026-01-10','THB','manual',1,'fixture',0,0),('root-foreign-secondary','company','chart','period','ROOT-FOREIGN-S','2026-01-10','THB','manual',1,'fixture',0,0)").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,status,version,prepared_by,reason,opening_json,opening_hash,created_at) VALUES('root-rule','company','chart','book-chart','2026-01-10','draft',1,'fixture','fixture','{}','fixture',0)").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO finance_book_postings(primary_journal_id,secondary_journal_id,rule_id,company_id,source_version,created_at) VALUES('root-foreign-primary','root-foreign-secondary','root-rule','company',1,0)").execute(pool.raw()).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let foreign=CompanyId::new("foreign").unwrap();
        let mut conn=pool.raw().acquire().await.unwrap();
        sqlx::raw_sql("CREATE TEMP TABLE finance_book_postings(primary_journal_id text,secondary_journal_id text); INSERT INTO pg_temp.finance_book_postings VALUES('forged','root-secondary')").execute(&mut *conn).await.unwrap();
        use sqlx::Acquire;
        let mut unlocked=conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();
        lock_canonical_finance_attachment_targets(&mut unlocked).await.unwrap();
        let unlocked_target=observe_canonical_finance_attachment_target(&mut unlocked,FinanceAttachmentTargetKind::Journal,&company,"root-secondary").await.unwrap();
        assert!(inspect_canonical_finance_history_journal_roots(&mut unlocked,&unlocked_target).await.is_err());
        unlocked.rollback().await.unwrap();
        let mut tx=conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_canonical_finance_attachment_targets(&mut tx).await.unwrap();
        lock_canonical_finance_history_book_roots(&mut tx).await.unwrap();
        let target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"root-secondary").await.unwrap();
        let roots=inspect_canonical_finance_history_journal_roots(&mut tx,&target).await.unwrap();
        assert_eq!(roots.roots(),["root-secondary"]);
        let missing=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&foreign,"root-secondary").await.unwrap();
        assert!(inspect_canonical_finance_history_journal_roots(&mut tx,&missing).await.unwrap().roots().is_empty());
        let wrong_kind=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"root-secondary").await.unwrap();
        assert!(inspect_canonical_finance_history_journal_roots(&mut tx,&wrong_kind).await.is_err());
        let cross=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"root-foreign-secondary").await.unwrap();
        assert_eq!(inspect_canonical_finance_history_journal_roots(&mut tx,&cross).await.unwrap().roots(),["root-foreign-secondary"]);
        // The legacy History expression is correlated to the actual selected
        // Finance row; compare it directly, with the database's UNION
        // collation, before and after inserting the book relationship.
        let legacy_roots=r#"SELECT r.id FROM public.finance_journals d CROSS JOIN LATERAL (SELECT d.id,d.company_id UNION SELECT p.id,p.company_id FROM public.finance_book_postings b JOIN public.finance_journals p ON p.id=b.primary_journal_id AND p.company_id=d.company_id WHERE b.secondary_journal_id=d.id) r WHERE d.id=$1 AND d.company_id=$2 ORDER BY r.id COLLATE "C""#;
        for (id,scope) in [("root-secondary",&company),("root-foreign-secondary",&company),("root-secondary",&foreign),("root-missing",&company)] {
            let selected=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,scope,id).await.unwrap();
            let actual=inspect_canonical_finance_history_journal_roots(&mut tx,&selected).await.unwrap();
            let expected:Vec<String>=sqlx::query_scalar(legacy_roots).bind(id).bind(scope.as_str()).fetch_all(&mut *tx).await.unwrap();
            assert_eq!(actual.roots(),expected.as_slice(),"legacy History roots for {id}");
            actual.require_current(&mut tx).await.unwrap();
        }
        sqlx::query("SAVEPOINT history_root_change").execute(&mut *tx).await.unwrap();
        // Legacy roots intentionally ignore b.company_id while matching the
        // actual primary journal Company to the selected journal Company.
        sqlx::query("INSERT INTO public.finance_book_postings(primary_journal_id,secondary_journal_id,rule_id,company_id,source_version,created_at) VALUES('root-primary','root-secondary','root-rule','foreign',1,0)").execute(&mut *tx).await.unwrap();
        assert!(roots.require_current(&mut tx).await.is_err());
        let projected=inspect_canonical_finance_history_journal_roots(&mut tx,&target).await.unwrap();
        assert_eq!(projected.roots(),["root-primary","root-secondary"]);
        let expected:Vec<String>=sqlx::query_scalar(legacy_roots).bind("root-secondary").bind(company.as_str()).fetch_all(&mut *tx).await.unwrap();
        assert_eq!(projected.roots(),expected.as_slice(),"legacy History roots after book insertion");
        sqlx::query("ROLLBACK TO SAVEPOINT history_root_change").execute(&mut *tx).await.unwrap();
        roots.require_current(&mut tx).await.unwrap();
        assert!(projected.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        sqlx::query("DROP TABLE pg_temp.finance_book_postings").execute(&mut *conn).await.unwrap();
        let mut fresh=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *fresh).await.unwrap();lock_canonical_finance_attachment_targets(&mut fresh).await.unwrap();lock_canonical_finance_history_book_roots(&mut fresh).await.unwrap();assert!(roots.require_current(&mut fresh).await.is_err());fresh.rollback().await.unwrap();
        let mut saved=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();lock_canonical_finance_attachment_targets(&mut saved).await.unwrap();sqlx::query("SAVEPOINT history_book_lock").execute(&mut *saved).await.unwrap();lock_canonical_finance_history_book_roots(&mut saved).await.unwrap();let saved_target=observe_canonical_finance_attachment_target(&mut saved,FinanceAttachmentTargetKind::Journal,&company,"root-secondary").await.unwrap();let saved_roots=inspect_canonical_finance_history_journal_roots(&mut saved,&saved_target).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT history_book_lock").execute(&mut *saved).await.unwrap();assert!(saved_roots.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("INSERT INTO public.finance_book_postings(primary_journal_id,secondary_journal_id,rule_id,company_id,source_version,created_at) VALUES('root-primary','root-secondary','root-rule','company',1,0)").execute(&mut *writer).await.unwrap();
        let mut blocked=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();lock_canonical_finance_attachment_targets(&mut blocked).await.unwrap();let conflict=lock_canonical_finance_history_book_roots(&mut blocked).await;assert!(matches!(conflict,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn initial_history_marker_roots_match_actual_legacy_book_company_rule() {
    use erpx_server_finance::public::attachment_target_source::*;
    use erpx_server_finance::public::initial_history_marker_roots::*;
    use sqlx::Acquire;
    fixture::with_database(2,|pool|async move {
        review::setup(&pool,false).await;
        // These mismatched Company examples are staged to prove SQL behavior;
        // they are not claimed to be valid or committed accounting records.
        sqlx::query("INSERT INTO public.finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,version,created_by,created_at,updated_at) VALUES('initial-foreign-primary','foreign','chart','period','INIT-FOREIGN-P','2026-01-10','THB','manual',1,'fixture',0,0),('initial-book-company-secondary','company','chart','period','INIT-COMPANY-S','2026-01-10','THB','manual',1,'fixture',0,0),('initial-company-primary','company','chart','period','INIT-COMPANY-P','2026-01-10','THB','manual',1,'fixture',0,0),('initial-book-foreign-secondary','company','chart','period','INIT-FOREIGN-S','2026-01-10','THB','manual',1,'fixture',0,0),('initial-late-primary','company','chart','period','INIT-LATE-P','2026-01-10','THB','manual',1,'fixture',0,0),('initial-late-secondary','company','chart','period','INIT-LATE-S','2026-01-10','THB','manual',1,'fixture',0,0)").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO public.finance_book_rules(id,company_id,primary_chart_id,secondary_chart_id,effective_date,status,version,prepared_by,reason,opening_json,opening_hash,created_at) VALUES('initial-rule','company','chart','book-chart','2026-01-10','draft',1,'fixture','fixture','{}','fixture',0)").execute(pool.raw()).await.unwrap();
        let mut conn=pool.raw().acquire().await.unwrap();sqlx::query("CREATE TEMP TABLE finance_book_postings(primary_journal_id text,secondary_journal_id text,company_id text)").execute(&mut *conn).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let mut unlocked=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unlocked).await.unwrap();lock_canonical_finance_attachment_targets(&mut unlocked).await.unwrap();let unlocked_target=observe_canonical_finance_attachment_target(&mut unlocked,FinanceAttachmentTargetKind::Journal,&company,"initial-book-company-secondary").await.unwrap();assert!(inspect_finance_initial_marker_roots(&mut unlocked,&unlocked_target).await.is_err());unlocked.rollback().await.unwrap();
        let mut tx=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_canonical_finance_attachment_targets(&mut tx).await.unwrap();lock_canonical_finance_history_book_roots(&mut tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO public.finance_book_postings(primary_journal_id,secondary_journal_id,rule_id,company_id,source_version,created_at) VALUES('initial-foreign-primary','initial-book-company-secondary','initial-rule','company',1,0),('initial-company-primary','initial-book-foreign-secondary','initial-rule','foreign',1,0)").execute(&mut *tx).await.unwrap();
        let a=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"initial-book-company-secondary").await.unwrap();let initial_a=inspect_finance_initial_marker_roots(&mut tx,&a).await.unwrap();let generic_a=inspect_canonical_finance_history_journal_roots(&mut tx,&a).await.unwrap();
        assert_eq!(initial_a.roots(),["initial-book-company-secondary","initial-foreign-primary"]);assert_eq!(generic_a.roots(),["initial-book-company-secondary"]);
        let b=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"initial-book-foreign-secondary").await.unwrap();let initial_b=inspect_finance_initial_marker_roots(&mut tx,&b).await.unwrap();let generic_b=inspect_canonical_finance_history_journal_roots(&mut tx,&b).await.unwrap();
        assert_eq!(initial_b.roots(),["initial-book-foreign-secondary"]);assert_eq!(generic_b.roots(),["initial-book-foreign-secondary","initial-company-primary"]);
        let missing=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"initial-missing").await.unwrap();assert!(inspect_finance_initial_marker_roots(&mut tx,&missing).await.unwrap().roots().is_empty());
        let payment=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"initial-missing").await.unwrap();assert!(inspect_finance_initial_marker_roots(&mut tx,&payment).await.is_err());
        // Execute the exact roots function extracted from all three checked-in
        // TS owners; no runtime dependency on their unrelated JSON imports.
        sqlx::query("DROP TABLE pg_temp.finance_book_postings").execute(&mut *tx).await.unwrap();
        let node=std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into());let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/adapters");
        let script=r#"const fs=require('node:fs'),m=require('node:module');if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const out=process.argv.slice(1).map(p=>{const s=fs.readFileSync(p,'utf8'),a=s.indexOf('const roots='),b=s.indexOf('\n',a);if(a<0||b<0)throw Error('SOURCE_REQUIRED');return new Function(m.stripTypeScriptTypes(s.slice(a,b))+';return roots')()('d')});process.stdout.write(JSON.stringify(out));"#;
        let output=std::process::Command::new(node).arg("-e").arg(script).arg(dir.join("service-initial-stock-journal-sources.ts")).arg(dir.join("pos-initial-stock-journal-sources.ts")).arg(dir.join("manufacturing-initial-stock-journal-sources.ts")).output().expect("pinned Node legacy roots oracle");assert!(output.status.success(),"Node roots oracle: {}",String::from_utf8_lossy(&output.stderr));
        let sources:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();assert_eq!(sources[0],sources[1]);assert_eq!(sources[0],sources[2]);
        let oracle=format!("SELECT r.id FROM public.finance_journals d CROSS JOIN LATERAL ({}) r WHERE d.id=$1 AND d.company_id=$2 ORDER BY r.id COLLATE \"C\"",sources[0].as_str().unwrap());
        for (id,witness) in [("initial-book-company-secondary",&initial_a),("initial-book-foreign-secondary",&initial_b)] {let expected:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(oracle.as_str())).bind(id).bind("company").fetch_all(&mut *tx).await.unwrap();assert_eq!(witness.roots(),expected.as_slice());witness.require_current(&mut tx).await.unwrap();}
        let late_target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"initial-late-secondary").await.unwrap();let late=inspect_finance_initial_marker_roots(&mut tx,&late_target).await.unwrap();assert_eq!(late.roots(),["initial-late-secondary"]);
        sqlx::query("SAVEPOINT initial_marker_book_change").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO public.finance_book_postings(primary_journal_id,secondary_journal_id,rule_id,company_id,source_version,created_at) VALUES('initial-late-primary','initial-late-secondary','initial-rule','company',1,0)").execute(&mut *tx).await.unwrap();assert!(late.require_current(&mut tx).await.is_err());let next=inspect_finance_initial_marker_roots(&mut tx,&late_target).await.unwrap();let expected:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(oracle.as_str())).bind("initial-late-secondary").bind("company").fetch_all(&mut *tx).await.unwrap();assert_eq!(next.roots(),expected.as_slice());sqlx::query("ROLLBACK TO SAVEPOINT initial_marker_book_change").execute(&mut *tx).await.unwrap();late.require_current(&mut tx).await.unwrap();
        tx.rollback().await.unwrap();sqlx::query("DROP TABLE pg_temp.finance_book_postings").execute(&mut *conn).await.unwrap();let mut other=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_canonical_finance_attachment_targets(&mut other).await.unwrap();lock_canonical_finance_history_book_roots(&mut other).await.unwrap();assert!(initial_a.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut saved=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *saved).await.unwrap();lock_canonical_finance_attachment_targets(&mut saved).await.unwrap();sqlx::query("SAVEPOINT initial_book_lock").execute(&mut *saved).await.unwrap();lock_canonical_finance_history_book_roots(&mut saved).await.unwrap();let saved_target=observe_canonical_finance_attachment_target(&mut saved,FinanceAttachmentTargetKind::Journal,&company,"initial-book-company-secondary").await.unwrap();let saved_roots=inspect_finance_initial_marker_roots(&mut saved,&saved_target).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT initial_book_lock").execute(&mut *saved).await.unwrap();assert!(saved_roots.require_current(&mut saved).await.is_err());saved.rollback().await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE public.finance_book_postings IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();let mut blocked=conn.begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *blocked).await.unwrap();lock_canonical_finance_attachment_targets(&mut blocked).await.unwrap();let conflict=lock_canonical_finance_history_book_roots(&mut blocked).await;assert!(matches!(conflict,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));blocked.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn asset_finance_join_source_matches_actual_asset_ts_left_join() {
    use erpx_server_finance::public::asset_purchase_source::{inspect_asset_purchase_finance_join,lock_asset_purchase_finance_source};
    fixture::with_database(2,|pool|async move {
        review::setup(&pool,false).await;
        let script=r#"import fs from 'node:fs';if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const s=fs.readFileSync(process.argv[1],'utf8');const scope=s.match(/const scope=\"([^\"]+)\";/);const image=s.match(/if\(target\.kind===\"asset\"\)return `([^`]+)`;/);if(!scope||!image)throw Error('ASSET_SOURCE_MISSING');let n=0;const sql=new Function('file','scope','return `'+image[1]+'`')(false,scope[1]).replace(/\?/g,()=>'$'+(++n));process.stdout.write(JSON.stringify(sql));"#;
        let path=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../server-assets/src/adapters/purchases.ts");let output=std::process::Command::new(std::env::var("ERPX_BUN_BIN").unwrap_or_else(|_|"bun".into())).arg("-e").arg(script).arg(path).output().unwrap();assert!(output.status.success(),"actual Asset TS SQL: {}",String::from_utf8_lossy(&output.stderr));let actual:String=serde_json::from_slice(&output.stdout).unwrap();let replaced=actual.replace("LEFT JOIN finance_assets f","LEFT JOIN finance_asset_purchase_rows f");assert_ne!(actual,replaced);
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();lock_asset_purchase_finance_source(&mut tx).await.unwrap();let source=inspect_asset_purchase_finance_join(&mut tx).await.unwrap();
        // Staged actual rows exercise the real SQL; deferred owner guards stay
        // enabled and this unit rolls back, so no production lineage is claimed.
        sqlx::query("INSERT INTO public.finance_assets(id,company_id,asset_number,name,account_id,cost_minor,residual_minor,useful_life_months,in_service_date,status,version,currency) VALUES('join-finance','company','JOIN-F','Original','cash',10,0,12,'2026-01-01','in_service',1,'THB'),('join-foreign','foreign','JOIN-X','Foreign','cash',10,0,12,'2026-01-01','in_service',1,'THB')").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO public.operational_assets(id,company_id,branch_id,code,name,category,currency,finance_asset_id,finance_asset_version,created_by,created_at,updated_at) VALUES('join-asset','company','branch','JOIN-A','Fixture','equipment','THB','join-finance',1,'fixture',0,0),('join-no-finance','company','branch','JOIN-N','Fixture','equipment','THB',NULL,NULL,'fixture',0,0),('join-other-company','company','branch','JOIN-O','Fixture','equipment','THB','join-foreign',1,'fixture',0,0)").execute(&mut *tx).await.unwrap();
        let candidate=format!("WITH {} {}",source.cte_sql(),replaced);
        async fn query(c:&mut sqlx::PgConnection,sql:&str,id:&str,company:&str)->Option<String>{sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(id).bind(company).bind(None::<&str>).bind(None::<&str>).bind(None::<&str>).bind(None::<&str>).fetch_optional(c).await.unwrap()}
        for id in ["join-asset","join-no-finance","join-other-company","join-missing"] {let original=query(&mut tx,&actual,id,"company").await;let projected=query(&mut tx,&candidate,id,"company").await;assert_eq!(original,projected,"{id}");if id=="join-asset"{assert!(projected.unwrap().contains("Original"));}else if id!="join-missing"{let value:serde_json::Value=serde_json::from_str(projected.as_deref().unwrap()).unwrap();assert!(value["finance"].is_null());}}
        assert_eq!(query(&mut tx,&actual,"join-asset","foreign").await,query(&mut tx,&candidate,"join-asset","foreign").await);
        source.require_current(&mut tx).await.unwrap();let before=query(&mut tx,&candidate,"join-asset","company").await;sqlx::query("SAVEPOINT finance_join_drift").execute(&mut *tx).await.unwrap();sqlx::query("UPDATE public.finance_assets SET name='Changed' WHERE id='join-finance'").execute(&mut *tx).await.unwrap();let after=query(&mut tx,&candidate,"join-asset","company").await;assert_ne!(before,after);assert_eq!(after,query(&mut tx,&actual,"join-asset","company").await);source.require_current(&mut tx).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT finance_join_drift").execute(&mut *tx).await.unwrap();assert_eq!(before,query(&mut tx,&candidate,"join-asset","company").await);tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_asset_purchase_finance_source(&mut other).await.unwrap();assert!(source.require_current(&mut other).await.is_err());other.rollback().await.unwrap();
    }).await;
}

// Test-only projections of the checked-in Field/Workflow/Inventory owner CTEs
// and the unmounted composition's join shape. The Finance CTEs themselves come
// from the actual Finance public owner ports below. No production SQL is
// accepted from a request and no authority or purchase is claimed.
const FIELD_HISTORY_FIELD_CTE:&str=r#"field_history_orders AS (SELECT id,company_id,branch_id,department_id,agreement_id,version FROM public.field_service_orders),field_history_order_events AS (SELECT id,order_id,version,action,actor_id,status,details_json FROM public.field_service_order_events),field_history_agreements AS (SELECT id,company_id,branch_id,department_id,customer_id FROM public.field_service_agreements),field_history_billing_releases AS (SELECT id,order_id,agreement_id,customer_id,currency,amount_minor,source_order_version,status,released_by,pricing_date FROM public.field_service_billing_releases),field_history_part_reservations AS (SELECT id,order_id,warehouse_id,product_id FROM public.field_service_part_reservations)"#;
const FIELD_HISTORY_WORKFLOW_CTE:&str=r#"field_history_receipts AS (SELECT command_id,operation,record_id,actor_id,authority_json,result_json FROM public.command_receipts WHERE operation IN ('field.order.release-billing','field.order.issue-part','finance.payment.create'))"#;
const FIELD_HISTORY_INVENTORY_CTE:&str=r#"field_history_consumptions AS (SELECT id,company_id,branch_id,department_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,journal_id,created_by FROM public.operational_inventory_consumptions WHERE source_kind='field_service' AND quantity_milli>0 AND value_minor>0)"#;
const FIELD_HISTORY_COMPOSED_JOINS:&str=r#", owned_field_invoice AS (
 SELECT i.company_id,i.id,i.journal_id FROM field_history_billing_releases b
 JOIN field_history_orders s ON s.id=b.order_id AND s.agreement_id=b.agreement_id AND s.version>=b.source_order_version
 JOIN field_history_agreements g ON g.id=s.agreement_id AND g.company_id=s.company_id AND g.branch_id=s.branch_id AND g.department_id IS NOT DISTINCT FROM s.department_id AND g.customer_id=b.customer_id
 JOIN field_history_order_events e ON e.order_id=s.id AND e.version=b.source_order_version AND e.action='release-billing' AND e.status='billed' AND e.actor_id=b.released_by
 JOIN field_history_receipts r ON b.id='field-billing-'||r.command_id AND e.id='service-order-event-'||r.command_id AND r.operation='field.order.release-billing' AND r.record_id=s.id AND r.actor_id=b.released_by AND r.authority_json::jsonb->>'companyId'=s.company_id
 JOIN finance_history_invoices i ON i.id='field-invoice-'||r.command_id AND i.company_id=s.company_id AND i.kind='receivable' AND i.source_type='field_service' AND i.source_id=s.id AND i.party_id=b.customer_id AND i.currency=b.currency AND i.total_minor=b.amount_minor AND i.invoice_date=b.pricing_date
 JOIN field_history_finance_money m ON m.company_id=s.company_id AND m.source_type='field_service_billing_release' AND m.source_id=b.id AND m.amount_kind='invoice_total' AND m.currency=b.currency AND m.amount_units=b.amount_minor
 WHERE b.status='released'
), owned_field_payment AS (
 SELECT p.company_id,p.id FROM finance_history_payment_candidates p
 JOIN field_history_receipts r ON r.record_id=p.id AND r.operation='finance.payment.create' AND r.actor_id=p.created_by AND r.authority_json::jsonb->>'companyId'=p.company_id AND r.result_json::jsonb->>'id'=p.id
 JOIN owned_field_invoice source ON source.id=r.result_json::jsonb->>'settlementInvoiceId' AND source.company_id=p.company_id
 JOIN finance_history_invoices original ON original.id=source.id AND original.company_id=p.company_id AND original.party_id=p.party_id AND original.currency=p.currency
 WHERE p.kind='receipt'
 AND NOT EXISTS(SELECT 1 FROM finance_history_allocations a WHERE a.payment_id=p.id AND (a.company_id<>p.company_id OR a.invoice_id<>source.id))
 AND (p.status NOT IN ('settled','voided') OR EXISTS(SELECT 1 FROM finance_history_allocations a WHERE a.payment_id=p.id AND a.company_id=p.company_id AND a.invoice_id=source.id HAVING sum(a.amount_minor)=p.amount_minor))
), owned_field_parts AS (
 SELECT s.company_id,j.id FROM field_history_consumptions c
 JOIN field_history_orders s ON s.id=c.source_id AND s.company_id=c.company_id AND s.branch_id=c.branch_id AND s.department_id IS NOT DISTINCT FROM c.department_id AND s.version>=c.source_version
 JOIN field_history_order_events e ON e.order_id=s.id AND e.version=c.source_version AND e.action='issue-part' AND e.details_json->>'consumptionId'=c.id AND e.details_json->>'journalId'=c.journal_id
 JOIN field_history_part_reservations v ON v.id=e.details_json->>'reservationId' AND v.order_id=s.id AND v.warehouse_id=c.warehouse_id AND v.product_id=c.product_id
 JOIN field_history_receipts r ON c.id='operational-part-'||r.command_id AND e.id='service-order-event-'||r.command_id AND r.operation='field.order.issue-part' AND r.record_id=s.id AND r.actor_id=e.actor_id AND r.actor_id=c.created_by AND r.authority_json::jsonb->>'companyId'=s.company_id
 JOIN finance_history_root_journals j ON j.id=c.journal_id AND j.company_id=s.company_id AND j.source_type='field_service_parts' AND j.source_id=c.id AND j.currency=c.currency
 JOIN field_history_finance_money m ON m.company_id=s.company_id AND m.source_type='operational_inventory_consumption' AND m.source_id=c.id AND m.amount_kind='stock_cost' AND m.currency=c.currency AND m.amount_units=c.value_minor
 WHERE e.details_json->>'quantityMilli'=c.quantity_milli::text AND e.details_json->>'costMinor'=c.value_minor::text
 AND (SELECT sum(debit_minor) FROM field_history_finance_journal_lines WHERE journal_id=j.id)=c.value_minor
 AND (SELECT sum(credit_minor) FROM field_history_finance_journal_lines WHERE journal_id=j.id)=c.value_minor
)"#;
const FIELD_HISTORY_PAYMENT_TAIL:&str=" SELECT EXISTS(SELECT 1 FROM owned_field_payment)";
const FIELD_HISTORY_PARTS_TAIL:&str=" SELECT EXISTS(SELECT 1 FROM owned_field_parts)";
const FIELD_HISTORY_JOURNAL_TAIL:&str=" SELECT EXISTS(SELECT company_id,id FROM owned_field_parts UNION SELECT i.company_id,r.id FROM owned_field_invoice i JOIN finance_history_roots r ON r.company_id=i.company_id AND r.id=i.journal_id UNION SELECT company_id,id FROM owned_field_payment)";
fn field_finance_phase_sql(c:&erpx_server_finance::public::field_history_candidates::CurrentFieldHistoryFinanceCandidates<'_>,m:&erpx_server_finance::public::field_history_money_lines::CurrentFieldHistoryMoneyLines<'_,'_>,tail:&str)->String{
    format!("{}, {}, {}, {}, {}{}{}",c.cte(),FIELD_HISTORY_WORKFLOW_CTE,m.cte_sql(),FIELD_HISTORY_INVENTORY_CTE,FIELD_HISTORY_FIELD_CTE,FIELD_HISTORY_COMPOSED_JOINS,tail)
}
async fn field_finance_composed_bool(c:&mut sqlx::PgConnection,sql:&str,id:&str,company:&str,kind:&str)->bool{
    sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(sql)).bind(id).bind(company).bind(kind).fetch_one(c).await.unwrap()
}
async fn field_finance_ts_bool(c:&mut sqlx::PgConnection,sql:&str,company:&str,id:&str)->bool{
    sqlx::query_scalar::<_,bool>(sqlx::AssertSqlSafe(sql)).bind(company).bind(id).fetch_one(c).await.unwrap()
}

/// Exercises the Finance owner's invoice/payment/parts candidate and Money
/// witnesses against the actual imported Field TS `fieldServiceFinanceRecordSources`
/// SQL. The fixture rows are staged in a rollback-only unit with deferred native
/// guards enabled; this proves source phase behavior only, not authority,
/// purchase, a committed Field command or the unmounted composition mount.
#[tokio::test]
async fn field_finance_history_phases_match_imported_typescript_sources_and_money_pointers() {
    use erpx_server_finance::public::attachment_target_source::*;
    use erpx_server_finance::public::field_history_candidates::*;
    use erpx_server_finance::public::field_history_money_lines::*;
    use erpx_server_workflow::public::record_origin::lock_field_history_receipt_join_sources;
    use sqlx::Acquire;
    fixture::with_database(2,|pool|async move{
        review::setup(&pool,false).await;
        let node=std::env::var("ERPX_NODE_BIN").unwrap_or_else(|_|"node".into());
        let source=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../server-field_service/src/adapters/finance-record-sources.ts");
        let output=std::process::Command::new(node).arg("--conditions=react-server").arg("--input-type=module").arg("-e").arg("if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');const m=await import(process.argv[1]);process.stdout.write(JSON.stringify({invoice:m.fieldServiceFinanceRecordSources.invoice,payment:m.fieldServiceFinanceRecordSources.payment,journal:m.fieldServiceFinanceRecordSources.journal,parts:m.fieldServiceFinanceRecordSources.partsJournal}))").arg(source).output().expect("pinned Node Field Finance source oracle");
        assert!(output.status.success(),"actual Field TS source: {}",String::from_utf8_lossy(&output.stderr));
        let sources:serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
        let invoice_ts=format!("SELECT EXISTS(SELECT 1 FROM ({}) owner_source WHERE owner_source.company_id=$1 AND owner_source.id=$2)",sources["invoice"].as_str().unwrap());
        let payment_ts=format!("SELECT EXISTS(SELECT 1 FROM ({}) owner_source WHERE owner_source.company_id=$1 AND owner_source.id=$2)",sources["payment"].as_str().unwrap());
        let journal_ts=format!("SELECT EXISTS(SELECT 1 FROM ({}) owner_source WHERE owner_source.company_id=$1 AND owner_source.id=$2)",sources["journal"].as_str().unwrap());
        let parts_ts=format!("SELECT EXISTS(SELECT 1 FROM ({}) owner_source WHERE owner_source.company_id=$1 AND owner_source.id=$2)",sources["parts"].as_str().unwrap());

        let mut conn=pool.raw().acquire().await.unwrap();
        let mut tx=conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        // Acquire every participating owner fence before the first data read.
        // The fixture inserts below are same-transaction writes; self-held SHARE
        // locks do not conflict and deferred native guards remain enabled.
        lock_canonical_finance_attachment_targets(&mut tx).await.unwrap();
        lock_canonical_finance_history_book_roots(&mut tx).await.unwrap();
        lock_field_history_finance_candidate_sources(&mut tx).await.unwrap();
        lock_field_history_money_line_sources(&mut tx).await.unwrap();
        lock_field_history_receipt_join_sources(&mut tx).await.unwrap();
        sqlx::query("LOCK TABLE public.operational_inventory_consumptions IN SHARE MODE NOWAIT").execute(&mut *tx).await.unwrap();
        sqlx::query("LOCK TABLE public.field_service_agreements,public.field_service_orders,public.field_service_order_events,public.field_service_part_reservations,public.field_service_billing_releases IN SHARE MODE NOWAIT").execute(&mut *tx).await.unwrap();

        sqlx::raw_sql(r#"
INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','company','Company',NULL,'company',NULL,'active'),('branch','branch','branch','Branch','company','company','branch','active');
INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES('customer','company','customer','CUSTOMER','Synthetic','THB',0,0),('foreign-customer','foreign','customer','FOREIGN','Foreign','THB',0,0);
INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES('warehouse','company','branch','WAREHOUSE','Synthetic');
INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES('product','company','PRODUCT','Product',100);
INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor) VALUES('warehouse','product',2000,200);
INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES('sale','company','branch','sales','SALE','customer','warehouse','THB','2026-01-01','2026-01-02',0,0,'fixture',0,0);
INSERT INTO support_tickets(id,company_id,branch_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at) VALUES('ticket','company','branch','sale','customer','Ticket','Issue','fixture','2026-01-02','fixture',0,0);
INSERT INTO field_service_agreements(id,company_id,branch_id,customer_id,title,start_date,end_date,currency,visit_rate_minor,status,created_by,created_at,updated_at) VALUES('agreement','company','branch','customer','Agreement','2026-01-01','2026-12-31','THB',100,'active','fixture',0,0);
INSERT INTO field_service_orders(id,company_id,branch_id,ticket_id,ticket_version,agreement_id,agreement_version,issue,site,technician_id,status,version,created_by,created_at,updated_at) VALUES('field-order','company','branch','ticket',1,'agreement',1,'Issue','Site','tech','open',3,'fixture',0,0);
INSERT INTO field_service_part_reservations(id,order_id,warehouse_id,product_id,quantity_milli,expires_at,created_by,reason,created_at) VALUES('reservation','field-order','warehouse','product',1000,10000,'issuer','fixture',0);
INSERT INTO field_service_order_events(id,order_id,version,action,actor_id,reason,status,technician_id,details_json,created_at) VALUES('service-order-event-billing-key','field-order',2,'release-billing','releaser','fixture','billed','tech','{}',0),('service-order-event-issue-key','field-order',3,'issue-part','issuer','fixture','open','tech','{"reservationId":"reservation","consumptionId":"operational-part-issue-key","journalId":"journal-parts","quantityMilli":1000,"costMinor":100}',0);
INSERT INTO field_service_billing_releases(id,order_id,agreement_id,customer_id,currency,amount_minor,source_order_version,status,released_by,reason,created_at,pricing_date) VALUES('field-billing-billing-key','field-order','agreement','customer','THB',100,2,'released','releaser','fixture',0,'2026-01-01');
INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,created_by,created_at,updated_at) VALUES('journal-invoice','company','chart','period','J-INVOICE','2026-01-01','THB','manual',NULL,'fixture',0,0),('journal-payment','company','chart','period','J-PAYMENT','2026-01-01','THB','manual',NULL,'fixture',0,0),('journal-payment-fx','company','chart','period','J-PAYMENT-FX','2026-01-01','THB','manual',NULL,'fixture',0,0),('journal-reversal','company','chart','period','J-REVERSAL','2026-01-01','THB','manual',NULL,'fixture',0,0),('journal-reversal-fx','company','chart','period','J-REVERSAL-FX','2026-01-01','THB','manual',NULL,'fixture',0,0),('journal-payment-invalid','company','chart','period','J-PAYMENT-INVALID','2026-01-01','THB','manual',NULL,'fixture',0,0),('journal-parts','company','chart','period','J-PARTS','2026-01-01','THB','field_service_parts','operational-part-issue-key','issuer',0,0);
INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES('journal-parts-debit','journal-parts','cash',100,0,100,0,0),('journal-parts-credit','journal-parts','sales',0,100,0,100,1);
INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,journal_id,created_by,created_at,updated_at) VALUES('field-invoice-billing-key','company','customer','receivable','FIELD-INVOICE','2026-01-01','2026-01-02','THB',100,0,100,100,'cash','sales','field_service','field-order','journal-invoice','fixture',0,0);
INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at) VALUES('billing-key','field.order.release-billing','releaser','synthetic','field-order','{}','{"companyId":"company"}',0),('issue-key','field.order.issue-part','issuer','synthetic','field-order','{}','{"companyId":"company"}',0),('payment-key','finance.payment.create','payer','synthetic','payment-valid','{"id":"payment-valid","settlementInvoiceId":"field-invoice-billing-key"}','{"companyId":"company"}',0);
INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,status,journal_id,fx_journal_id,version,created_by,created_at,updated_at) VALUES('payment-valid','company','customer','receipt','PAY-VALID','2026-01-01','THB',100,100,'draft','journal-payment',NULL,1,'payer',0,0),('payment-fx','company','customer','receipt','PAY-FX','2026-01-01','THB',100,100,'draft',NULL,'journal-payment-fx',1,'payer',0,0),('payment-reversal','company','customer','receipt','PAY-REVERSAL','2026-01-01','THB',100,100,'draft',NULL,NULL,1,'payer',0,0),('payment-invalid','company','customer','receipt','PAY-INVALID','2026-01-01','THB',100,100,'draft','journal-payment-invalid',NULL,1,'payer',0,0),('payment-foreign','foreign','foreign-customer','receipt','PAY-FOREIGN','2026-01-01','THB',100,100,'draft','journal-payment',NULL,1,'payer',0,0);
INSERT INTO finance_payment_reversals(id,payment_id,reversal_journal_id,fx_reversal_journal_id,reason,reversed_by,reversed_at) VALUES('payment-reversal-row','payment-reversal','journal-reversal','journal-reversal-fx','fixture','payer',0);
INSERT INTO operational_inventory_consumptions(id,company_id,branch_id,source_kind,source_id,source_version,warehouse_id,product_id,quantity_milli,value_minor,currency,movement_date,journal_id,reason,created_by,created_at) VALUES('operational-part-issue-key','company','branch','field_service','field-order',3,'warehouse','product',1000,100,'THB','2026-01-01','journal-parts','fixture','issuer',0);
"#).execute(&mut *tx).await.unwrap();
        // Wrong Money pointers are present first: a different source id and a
        // different amount kind must never satisfy the actual TS joins.
        sqlx::raw_sql("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,used_latest_fallback,captured_at) VALUES('company','field_service_billing_release','field-invoice-billing-key','invoice_total','2026-01-01','THB',2,100,100,false,0),('company','field_service_billing_release','field-billing-billing-key','wrong_kind','2026-01-01','THB',2,100,100,false,0),('company','operational_inventory_consumption','journal-parts','stock_cost','2026-01-01','THB',2,100,100,false,0),('company','operational_inventory_consumption','operational-part-issue-key','wrong_kind','2026-01-01','THB',2,100,100,false,0)").execute(&mut *tx).await.unwrap();

        let company=CompanyId::new("company").unwrap();
        let foreign=CompanyId::new("foreign").unwrap();
        // Payment target: the journal/root branches are empty and must not
        // contribute to payment inclusion. The candidate set is the selected
        // payment only, not every payment whose journal happens to exist.
        let payment_target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"payment-valid").await.unwrap();
        let payment_candidates=inspect_field_history_finance_candidates(&mut tx,&payment_target,None).await.unwrap();
        let payment_money=inspect_field_history_money_lines(&mut tx,&payment_candidates).await.unwrap();
        assert_eq!((payment_candidates.kind_parameter(),payment_candidates.selected_id(),payment_candidates.company_id()),("payment","payment-valid","company"));
        let roots_count=format!("{} SELECT (SELECT count(*) FROM finance_history_roots),(SELECT count(*) FROM finance_history_root_journals),(SELECT count(*) FROM finance_history_direct_invoices)",payment_candidates.cte());
        let (root_rows,journal_rows,invoice_rows):(i64,i64,i64)=sqlx::query_as(sqlx::AssertSqlSafe(roots_count.as_str())).bind("payment-valid").bind("company").bind("payment").fetch_one(&mut *tx).await.unwrap();
        assert_eq!((root_rows,journal_rows,invoice_rows),(0,0,0),"payment target must not populate journal branches");
        let payment_ids=format!("{} SELECT id FROM finance_history_payment_candidates ORDER BY id",payment_candidates.cte());
        let payment_rows:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(payment_ids.as_str())).bind("payment-valid").bind("company").bind("payment").fetch_all(&mut *tx).await.unwrap();
        assert_eq!(payment_rows,["payment-valid"]);
        let payment_phase=field_finance_phase_sql(&payment_candidates,&payment_money,FIELD_HISTORY_PAYMENT_TAIL);
        assert!(!field_finance_composed_bool(&mut tx,&payment_phase,"payment-valid","company","payment").await,"decoy Money must not satisfy the payment source");
        assert!(!field_finance_ts_bool(&mut tx,&payment_ts,"company","payment-valid").await);
        assert!(!field_finance_ts_bool(&mut tx,&journal_ts,"company","payment-valid").await);
        assert!(!field_finance_ts_bool(&mut tx,&parts_ts,"company","payment-valid").await);

        // The actual Money pointer (billing id for invoice, consumption id for
        // parts) is required. Add it in a savepoint, prove both decisions, then
        // roll the insert back and prove the seam retained no effects.
        sqlx::query("SAVEPOINT correct_money").execute(&mut *tx).await.unwrap();
        sqlx::raw_sql("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,used_latest_fallback,captured_at) VALUES('company','field_service_billing_release','field-billing-billing-key','invoice_total','2026-01-01','THB',2,100,100,false,0),('company','operational_inventory_consumption','operational-part-issue-key','stock_cost','2026-01-01','THB',2,100,100,false,0)").execute(&mut *tx).await.unwrap();
        assert!(field_finance_composed_bool(&mut tx,&payment_phase,"payment-valid","company","payment").await);
        assert!(field_finance_ts_bool(&mut tx,&payment_ts,"company","payment-valid").await);
        assert!(field_finance_ts_bool(&mut tx,&invoice_ts,"company","field-invoice-billing-key").await);
        payment_candidates.require_current(&mut tx).await.unwrap();
        payment_money.require_current(&mut tx).await.unwrap();
        sqlx::query("ROLLBACK TO SAVEPOINT correct_money").execute(&mut *tx).await.unwrap();
        assert!(!field_finance_composed_bool(&mut tx,&payment_phase,"payment-valid","company","payment").await);
        assert!(!field_finance_ts_bool(&mut tx,&payment_ts,"company","payment-valid").await);
        sqlx::raw_sql("INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,used_latest_fallback,captured_at) VALUES('company','field_service_billing_release','field-billing-billing-key','invoice_total','2026-01-01','THB',2,100,100,false,0),('company','operational_inventory_consumption','operational-part-issue-key','stock_cost','2026-01-01','THB',2,100,100,false,0)").execute(&mut *tx).await.unwrap();

        // Journal invoice phase: inclusion is true, parts supplement is false.
        let invoice_target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"journal-invoice").await.unwrap();
        let invoice_roots=inspect_canonical_finance_history_journal_roots(&mut tx,&invoice_target).await.unwrap();
        let invoice_candidates=inspect_field_history_finance_candidates(&mut tx,&invoice_target,Some(&invoice_roots)).await.unwrap();
        let invoice_money=inspect_field_history_money_lines(&mut tx,&invoice_candidates).await.unwrap();
        let invoice_journal_phase=field_finance_phase_sql(&invoice_candidates,&invoice_money,FIELD_HISTORY_JOURNAL_TAIL);
        let invoice_parts_phase=field_finance_phase_sql(&invoice_candidates,&invoice_money,FIELD_HISTORY_PARTS_TAIL);
        assert!(field_finance_composed_bool(&mut tx,&invoice_journal_phase,"journal-invoice","company","journal").await);
        assert!(!field_finance_composed_bool(&mut tx,&invoice_parts_phase,"journal-invoice","company","journal").await,"parts supplement must not match an invoice journal");
        assert!(field_finance_ts_bool(&mut tx,&journal_ts,"company","journal-invoice").await);
        assert!(!field_finance_ts_bool(&mut tx,&parts_ts,"company","journal-invoice").await);

        // Journal parts phase: inclusion and the service-parts supplement are
        // both true only when the selected journal is the actual parts journal.
        let parts_target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"journal-parts").await.unwrap();
        let parts_roots=inspect_canonical_finance_history_journal_roots(&mut tx,&parts_target).await.unwrap();
        let parts_candidates=inspect_field_history_finance_candidates(&mut tx,&parts_target,Some(&parts_roots)).await.unwrap();
        let parts_money=inspect_field_history_money_lines(&mut tx,&parts_candidates).await.unwrap();
        let parts_journal_phase=field_finance_phase_sql(&parts_candidates,&parts_money,FIELD_HISTORY_JOURNAL_TAIL);
        let parts_parts_phase=field_finance_phase_sql(&parts_candidates,&parts_money,FIELD_HISTORY_PARTS_TAIL);
        assert!(field_finance_composed_bool(&mut tx,&parts_journal_phase,"journal-parts","company","journal").await);
        assert!(field_finance_composed_bool(&mut tx,&parts_parts_phase,"journal-parts","company","journal").await);
        assert!(field_finance_ts_bool(&mut tx,&journal_ts,"company","journal-parts").await);
        assert!(field_finance_ts_bool(&mut tx,&parts_ts,"company","journal-parts").await);
        parts_candidates.require_current(&mut tx).await.unwrap();
        parts_money.require_current(&mut tx).await.unwrap();

        // Payment journal phase and an invalid payment journal: the latter has a
        // real matching journal root but no create receipt/source invoice, so it
        // must not leak into inclusion.
        let payment_journal_target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"journal-payment").await.unwrap();
        let payment_journal_roots=inspect_canonical_finance_history_journal_roots(&mut tx,&payment_journal_target).await.unwrap();
        let payment_journal_candidates=inspect_field_history_finance_candidates(&mut tx,&payment_journal_target,Some(&payment_journal_roots)).await.unwrap();
        let payment_journal_money=inspect_field_history_money_lines(&mut tx,&payment_journal_candidates).await.unwrap();
        let payment_journal_phase=field_finance_phase_sql(&payment_journal_candidates,&payment_journal_money,FIELD_HISTORY_JOURNAL_TAIL);
        assert!(field_finance_composed_bool(&mut tx,&payment_journal_phase,"journal-payment","company","journal").await);
        assert!(field_finance_ts_bool(&mut tx,&journal_ts,"company","journal-payment").await);
        let invalid_target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&company,"journal-payment-invalid").await.unwrap();
        let invalid_roots=inspect_canonical_finance_history_journal_roots(&mut tx,&invalid_target).await.unwrap();
        let invalid_candidates=inspect_field_history_finance_candidates(&mut tx,&invalid_target,Some(&invalid_roots)).await.unwrap();
        let invalid_money=inspect_field_history_money_lines(&mut tx,&invalid_candidates).await.unwrap();
        let invalid_phase=field_finance_phase_sql(&invalid_candidates,&invalid_money,FIELD_HISTORY_JOURNAL_TAIL);
        assert!(!field_finance_composed_bool(&mut tx,&invalid_phase,"journal-payment-invalid","company","journal").await);
        assert!(!field_finance_ts_bool(&mut tx,&journal_ts,"company","journal-payment-invalid").await);

        // Foreign and missing targets retain absence, not a global negative.
        let foreign_payment_target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&foreign,"payment-foreign").await.unwrap();
        let foreign_payment=inspect_field_history_finance_candidates(&mut tx,&foreign_payment_target,None).await.unwrap();
        let foreign_payment_money=inspect_field_history_money_lines(&mut tx,&foreign_payment).await.unwrap();
        let foreign_payment_phase=field_finance_phase_sql(&foreign_payment,&foreign_payment_money,FIELD_HISTORY_PAYMENT_TAIL);
        assert!(!field_finance_composed_bool(&mut tx,&foreign_payment_phase,"payment-foreign","foreign","payment").await);
        assert!(!field_finance_ts_bool(&mut tx,&payment_ts,"foreign","payment-foreign").await);
        let foreign_journal_target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Journal,&foreign,"journal-payment").await.unwrap();
        let foreign_roots=inspect_canonical_finance_history_journal_roots(&mut tx,&foreign_journal_target).await.unwrap();
        let foreign_journal=inspect_field_history_finance_candidates(&mut tx,&foreign_journal_target,Some(&foreign_roots)).await.unwrap();
        let foreign_journal_money=inspect_field_history_money_lines(&mut tx,&foreign_journal).await.unwrap();
        let foreign_journal_phase=field_finance_phase_sql(&foreign_journal,&foreign_journal_money,FIELD_HISTORY_JOURNAL_TAIL);
        assert!(!field_finance_composed_bool(&mut tx,&foreign_journal_phase,"journal-payment","foreign","journal").await);
        assert!(!field_finance_ts_bool(&mut tx,&journal_ts,"foreign","journal-payment").await);
        let missing_target=observe_canonical_finance_attachment_target(&mut tx,FinanceAttachmentTargetKind::Payment,&company,"payment-missing").await.unwrap();
        let missing=inspect_field_history_finance_candidates(&mut tx,&missing_target,None).await.unwrap();
        let missing_money=inspect_field_history_money_lines(&mut tx,&missing).await.unwrap();
        let missing_phase=field_finance_phase_sql(&missing,&missing_money,FIELD_HISTORY_PAYMENT_TAIL);
        assert!(!field_finance_composed_bool(&mut tx,&missing_phase,"payment-missing","company","payment").await);
        assert!(!field_finance_ts_bool(&mut tx,&payment_ts,"company","payment-missing").await);

        // A target version change invalidates the retained payment witness.
        sqlx::query("SAVEPOINT stale_payment_version").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE finance_payments SET version=version+1 WHERE id='payment-valid'").execute(&mut *tx).await.unwrap();
        assert!(payment_candidates.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT stale_payment_version").execute(&mut *tx).await.unwrap();
        payment_candidates.require_current(&mut tx).await.unwrap();
        // Same-unit invoice drift changes the joined decision but not the
        // target identity; the consumer must re-run the full joined decision.
        sqlx::query("SAVEPOINT stale_invoice_total").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE finance_invoices SET total_minor=101 WHERE id='field-invoice-billing-key'").execute(&mut *tx).await.unwrap();
        assert!(!field_finance_composed_bool(&mut tx,&payment_phase,"payment-valid","company","payment").await);
        assert!(!field_finance_ts_bool(&mut tx,&payment_ts,"company","payment-valid").await);
        assert!(payment_candidates.require_current(&mut tx).await.is_ok());
        sqlx::query("ROLLBACK TO SAVEPOINT stale_invoice_total").execute(&mut *tx).await.unwrap();
        assert!(field_finance_composed_bool(&mut tx,&payment_phase,"payment-valid","company","payment").await);
        assert!(field_finance_ts_bool(&mut tx,&payment_ts,"company","payment-valid").await);
        // Same-unit order drift makes the parts event stale for the selected
        // journal while the Finance target witness remains valid.
        sqlx::query("SAVEPOINT stale_order_version").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE field_service_orders SET version=2 WHERE id='field-order'").execute(&mut *tx).await.unwrap();
        assert!(!field_finance_composed_bool(&mut tx,&parts_parts_phase,"journal-parts","company","journal").await);
        assert!(!field_finance_ts_bool(&mut tx,&parts_ts,"company","journal-parts").await);
        assert!(parts_candidates.require_current(&mut tx).await.is_ok());
        sqlx::query("ROLLBACK TO SAVEPOINT stale_order_version").execute(&mut *tx).await.unwrap();
        assert!(field_finance_composed_bool(&mut tx,&parts_parts_phase,"journal-parts","company","journal").await);
        assert!(field_finance_ts_bool(&mut tx,&parts_ts,"company","journal-parts").await);

        tx.rollback().await.unwrap();
        // The rollback-only fixture leaves no payment, invoice, Money, receipt,
        // consumption or Field row behind, and the source seam wrote no effect.
        let counts:(i64,i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM finance_payments WHERE id IN ('payment-valid','payment-fx','payment-reversal','payment-invalid','payment-foreign')),(SELECT count(*) FROM finance_invoices WHERE id='field-invoice-billing-key'),(SELECT count(*) FROM money_transaction_snapshots WHERE source_id IN ('field-billing-billing-key','operational-part-issue-key')),(SELECT count(*) FROM command_receipts WHERE command_id IN ('billing-key','issue-key','payment-key')),(SELECT count(*) FROM operational_inventory_consumptions WHERE id='operational-part-issue-key'),(SELECT count(*) FROM field_service_orders WHERE id='field-order'),(SELECT count(*) FROM audit_events WHERE command_id IN ('billing-key','issue-key','payment-key'))").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(counts,(0,0,0,0,0,0,0));
        drop(conn);

        // Same-unit owner fences reject a real writer before the first snapshot.
        let mut writer_conn=pool.raw().acquire().await.unwrap();
        let mut reader_conn=pool.raw().acquire().await.unwrap();
        let mut writer=writer_conn.begin().await.unwrap();
        sqlx::query("LOCK TABLE public.finance_invoices IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
        let mut reader=reader_conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
        let conflict=lock_field_history_finance_candidate_sources(&mut reader).await;
        assert!(matches!(conflict,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        reader.rollback().await.unwrap();writer.rollback().await.unwrap();
        let mut writer=writer_conn.begin().await.unwrap();
        sqlx::query("LOCK TABLE public.money_transaction_snapshots IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
        let mut reader=reader_conn.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *reader).await.unwrap();
        let conflict=lock_field_history_money_line_sources(&mut reader).await;
        assert!(matches!(conflict,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e)) if e.code=="PRE_SNAPSHOT_LOCK_CONTENDED" && e.sql_state()==Some("55P03") && e.retryable()));
        reader.rollback().await.unwrap();writer.rollback().await.unwrap();
    }).await;
}
