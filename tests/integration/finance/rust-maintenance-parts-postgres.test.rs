#[allow(
    dead_code,
    reason = "actual Account/Finance fixture also serves other owner suites"
)]
#[path = "../../support/finance/maintenance-parts.rs"]
mod facts;
use erpx_server_finance::public::{FinanceActor, journal_review::*};
use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
use erpx_server_inventory::public::consumption_finance::*;
use erpx_server_maintenance::public::finance_lineage::*;
use erpx_server_shared::public::Clock;
use facts::*;

#[tokio::test]
async fn real_parts_issuer_without_finance_write_and_independent_finance_reviewer_without_operational_hr_settle_with_a_fresh_quote()
 {
    account::with_system(|system|async move{
        let p=setup_parts(system).await;let id=p.issue("parts-source-one",1).await;
        withdraw(&p,&["finance"]).await;
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE p.user_id='fixture-b'").fetch_one(p.pool().raw()).await.unwrap(),0);
        // A later dated rate applies to fresh finance_journal/debit_total. The
        // original immutable stock_cost remains attached to its original rate.
        sqlx::query("INSERT INTO company_fx_rates(id,company_id,currency,effective_date,currency_per_usd,confirmed_by,confirmed_at)VALUES('later-rate','company','THB','2026-01-09',4,'fixture-b',$1)").bind(p.reviewer.system.scenario.clock.now().get()).execute(p.pool().raw()).await.unwrap();
        assert!(legacy_source_exists(&p,&id).await);
        sqlx::query("UPDATE hr_engagements SET end_date='2026-01-02'WHERE id='parts-engagement'").execute(p.pool().raw()).await.unwrap();
        let outcome=p.post(&id,"parts-post-included").await.unwrap();
        assert!(matches!(outcome,JournalReviewResult::Posted{version:2,..}));
        assert_eq!(state(&p,&id).await,("posted".into(),2));assert_eq!(counts(&p,"parts-post-included").await,(1,1,1,1));
        let money:Vec<(String,String,String,String)>=sqlx::query_as("SELECT source_type,amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots ORDER BY source_type").fetch_all(p.pool().raw()).await.unwrap();
        assert_eq!(money,vec![("finance_journal".into(),"200".into(),"50".into(),"later-rate".into()),("operational_inventory_consumption".into(),"200".into(),"100".into(),"rate".into())]);
        let iam:serde_json::Value=sqlx::query_scalar("SELECT authority_json::jsonb->'iam' FROM command_receipts WHERE command_id='parts-post-included'").fetch_one(p.pool().raw()).await.unwrap();
        assert!(!iam["permissions"].as_array().unwrap().iter().any(|v|v=="records.write"));
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM command_receipts WHERE operation='finance.journal.create'").fetch_one(p.pool().raw()).await.unwrap(),0);
    }).await;
}

#[tokio::test]
async fn full_finance_purchase_is_an_alternative_but_parts_generic_reversal_still_belongs_at_source()
 {
    account::with_system(|system| async move {
        let p = setup_parts(system).await;
        let id = p.issue("parts-source-full-finance", 1).await;
        withdraw(&p, &["assets-parts-costs"]).await;
        p.post(&id, "parts-post-full-finance").await.unwrap();
        business(
            p.reviewer
                .review(&id, "parts-reverse-denied", reverse())
                .await,
            "REVERSE_AT_SOURCE",
        );
        assert_eq!(state(&p, &id).await, ("posted".into(), 2));
        assert_eq!(counts(&p, "parts-reverse-denied").await, (0, 0, 0, 1));
    })
    .await;
}

#[tokio::test]
async fn exact_history_replay_survives_inactive_purchase_but_new_work_and_revoked_current_iam_do_not()
 {
    account::with_system(|system| async move {
        let p = setup_parts(system).await;
        let first = p.issue("parts-history-one", 1).await;
        let second = p.issue("parts-history-two", 2).await;
        withdraw(&p, &["finance"]).await;
        let posted = p.post(&first, "parts-history-post").await.unwrap();
        withdraw(&p, &["assets-parts-costs"]).await;
        assert_eq!(p.post(&first, "parts-history-post").await.unwrap(), posted);
        business(
            p.post(&second, "parts-new-denied").await,
            "PACKAGE_INACTIVE",
        );
        business(
            p.reviewer
                .review(
                    &first,
                    "parts-history-post",
                    JournalReviewInput::Post {
                        expected_version: 2,
                    },
                )
                .await,
            "IDEMPOTENCY_CONFLICT",
        );
        sqlx::query("UPDATE iam_roles SET status='archived'WHERE id='custom_finance_review'")
            .execute(p.pool().raw())
            .await
            .unwrap();
        assert!(p.post(&first, "parts-history-post").await.is_err());
        assert_eq!(counts(&p, "parts-history-post").await, (1, 1, 1, 1));
        assert_eq!(state(&p, &second).await, ("draft".into(), 1));
    })
    .await;
}

#[tokio::test]
async fn actual_same_person_and_current_approve_permission_are_required_without_inheriting_issuer_rights()
 {
    account::with_system(|system|async move{
        let p=setup_parts(system).await;let id=p.issue("parts-independent-source",1).await;
        sqlx::raw_sql("INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-b','local:fixture-b','parts-worker','active','local',1)").execute(p.pool().raw()).await.unwrap();
        business(p.post(&id,"parts-alias-denied").await,"INDEPENDENT_APPROVER_REQUIRED");
        sqlx::raw_sql("DELETE FROM iam_members WHERE company_id='company'AND user_id='fixture-b';INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_finance_review',2,'[\"finance.read\",\"finance.write\",\"module.finance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_finance_review'").execute(p.pool().raw()).await.unwrap();
        business(p.post(&id,"parts-no-approve").await,"INDEPENDENT_APPROVER_REQUIRED");
        assert_eq!(state(&p,&id).await,("draft".into(),1));assert_eq!(counts(&p,"parts-alias-denied").await,(0,0,0,0));
    }).await;
}

#[tokio::test]
async fn actual_immutable_consumption_and_work_lineage_proofs_reject_cross_unit_and_used_fact_changes_without_transferring_private_work_text()
 {
    account::with_system(|system|async move{
        let p=setup_parts(system).await;let id=p.issue("parts-proof-source",1).await;
        let config=p.reviewer.configuration.clone();
        let (consumption,lineage,target)=p.reviewer.authorizer.execute_with_sources("acme.example.test",p.reviewer.reviewer.clone(),p.reviewer.reviewer_selection.clone(),WorkAuthorizationRequest{permission:"finance.write".into(),module:Some("finance".into()),destination:"finance/journals".into(),api:true},config,move|c,current|{let id=id.clone();Box::pin(async move{
            let actor=FinanceActor{holding_id:current.holding_id().clone(),company_id:current.selection().workspace().company_id.clone(),user_id:current.user_id().clone(),assignment_id:current.selection().assignment().id.clone()};
            let target=inspect_maintenance_parts_journal_target(c,&actor,&command(&id,"parts-proof-inspect",post())).await?.unwrap();
            let consumption=read_maintenance_consumption_finance(c,&actor.holding_id,&actor.company_id,&target.facts().consumption_id).await?.unwrap();let f=consumption.facts();
            let lineage=read_maintenance_finance_lineage(c,&MaintenanceFinanceLineageRequest{holding_id:f.holding_id.clone(),company_id:f.company_id.clone(),work_id:f.work_id.clone(),branch_id:f.branch_id.clone(),department_id:f.department_id.clone(),minimum_consumption_version:f.source_version}).await?.unwrap();
            // The actual source has a multi-megabyte private title. Irrelevant
            // text changes are not transferred or mistaken for scope changes.
            sqlx::query("UPDATE maintenance_work_orders SET title=repeat('OTHER_PRIVATE_TITLE',200000)WHERE id='work'").execute(&mut*c).await?;lineage.require_current(c).await?;
            sqlx::query("SAVEPOINT source_change").execute(&mut*c).await?;sqlx::query("UPDATE maintenance_work_orders SET version=1 WHERE id='work'").execute(&mut*c).await?;
            assert!(lineage.require_current(c).await.is_err());sqlx::query("ROLLBACK TO SAVEPOINT source_change").execute(&mut*c).await?;lineage.require_current(c).await?;
            Ok((consumption,lineage,target))
        })}).await.unwrap();
        let mut tx=p.pool().raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut*tx).await.unwrap();
        assert!(consumption.require_current(&mut tx).await.is_err());assert!(lineage.require_current(&mut tx).await.is_err());assert!(target.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
    }).await;
}

#[tokio::test]
async fn work_withdrawal_committed_before_first_snapshot_is_observed_after_the_actual_utility_wait()
{
    account::with_system(|system| async move {
        let p = setup_parts(system).await;
        let id = p.issue("parts-writer-source", 1).await;
        let mut writer = p.pool().raw().begin().await.unwrap();
        let (database, pid): (String, i32) =
            sqlx::query_as("SELECT current_database(),pg_backend_pid()")
                .fetch_one(&mut *writer)
                .await
                .unwrap();
        sqlx::query("UPDATE maintenance_work_orders SET version=1 WHERE id='work'")
            .execute(&mut *writer)
            .await
            .unwrap();
        let task = spawn_post(&p, id.clone(), "parts-writer-post");
        blocked_by(&p, &database, pid).await;
        assert!(!task.is_finished());
        writer.commit().await.unwrap();
        business(
            tokio::time::timeout(std::time::Duration::from_secs(15), task)
                .await
                .unwrap()
                .unwrap(),
            "FINANCE_MAINTENANCE_SOURCE_UNAVAILABLE",
        );
        assert_eq!(state(&p, &id).await, ("draft".into(), 1));
        assert_eq!(counts(&p, "parts-writer-post").await, (0, 0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn late_audit_source_tamper_rolls_back_journal_money_receipt_audit_and_outbox_together() {
    account::with_system(|system|async move{
        let p=setup_parts(system).await;let id=p.issue("parts-late-source",1).await;
        sqlx::raw_sql("CREATE FUNCTION synthetic_finance_parts_tamper()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.action='finance.journal.post'THEN UPDATE maintenance_work_orders SET version=1 WHERE id='work';END IF;RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER synthetic_finance_parts_tamper AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_finance_parts_tamper()").execute(p.pool().raw()).await.unwrap();
        business(p.post(&id,"parts-late-post").await,"MAINTENANCE_FINANCE_LINEAGE_CHANGED");
        assert_eq!(state(&p,&id).await,("draft".into(),1));assert_eq!(counts(&p,"parts-late-post").await,(0,0,0,0));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM maintenance_work_orders WHERE id='work'").fetch_one(p.pool().raw()).await.unwrap(),2);
    }).await;
}

#[tokio::test]
async fn a_source_label_without_actual_inventory_evidence_never_acquires_maintenance_purchase_or_authority()
 {
    account::with_system(|system|async move{
        let p=setup_parts(system).await;
        // The actual manual creation path is used to produce a target, then a
        // hostile direct row mutation claims a foreign source. No consumption
        // or original issuer proof is fabricated to satisfy the service.
        let mut maker=p.reviewer.maker_selection.clone();maker.expected=Some(erpx_server_identity_access::public::work_context::ExpectedWorkSelection{revision:9,assignment_id:erpx_server_shared::public::AssignmentId::new("finance-fixture-a").unwrap(),assignment_version:3});
        sqlx::query("UPDATE active_work_contexts SET assignment_id='finance-fixture-a',revision=9 WHERE user_id='fixture-a'").execute(p.pool().raw()).await.unwrap();
        let mut real=p.reviewer;real.maker_selection=maker;real.create("parts-fake-manual",false).await.unwrap();
        sqlx::query("UPDATE finance_journals SET source_type='maintenance_parts',source_id='no-actual-consumption'WHERE id='journal-parts-fake-manual'").execute(real.system.holdings[0].raw()).await.unwrap();
        business(real.review("journal-parts-fake-manual","parts-fake-denied",post()).await,"FINANCE_MAINTENANCE_SOURCE_UNAVAILABLE");
        let count:i64=sqlx::query_scalar("SELECT count(*)FROM command_receipts WHERE command_id='parts-fake-denied'").fetch_one(real.system.holdings[0].raw()).await.unwrap();assert_eq!(count,0);
    }).await;
}

#[tokio::test]
async fn actual_finance_review_and_next_parts_issuer_share_finance_first_fences_without_a_work_stock_lock_cycle()
 {
    account::with_system(|system|async move{
        let p=setup_parts(system).await;let id=p.issue("parts-concurrent-original",1).await;
        sqlx::raw_sql("CREATE FUNCTION synthetic_hold_parts_review()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.action='finance.journal.post'THEN PERFORM pg_advisory_xact_lock(734429);END IF;RETURN NEW;END$$;CREATE TRIGGER synthetic_hold_parts_review BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION synthetic_hold_parts_review()").execute(p.pool().raw()).await.unwrap();
        let mut gate=p.pool().raw().begin().await.unwrap();let(database,pid):(String,i32)=sqlx::query_as("SELECT current_database(),pg_backend_pid()").fetch_one(&mut*gate).await.unwrap();sqlx::query("SELECT pg_advisory_xact_lock(734429)").execute(&mut*gate).await.unwrap();
        let review=spawn_post(&p,id.clone(),"parts-concurrent-review");blocked_by(&p,&database,pid).await;
        let issue=spawn_issue(&p,"parts-concurrent-next",2);
        // Observe the issue unit waiting behind the actual review connection,
        // not a sleep or a synthetic lock-only substitute for either command.
        tokio::time::timeout(std::time::Duration::from_secs(5),async{loop{let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity a JOIN pg_stat_activity b ON b.pid=ANY(pg_blocking_pids(a.pid)) WHERE a.datname=$1 AND $2=ANY(pg_blocking_pids(b.pid)))").bind(&database).bind(pid).fetch_one(p.reviewer.system.control.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.expect("issuer waits behind the actual retained review");
        assert!(!issue.is_finished());gate.commit().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(15),review).await.unwrap().unwrap().unwrap();tokio::time::timeout(std::time::Duration::from_secs(15),issue).await.unwrap().unwrap().unwrap();
        assert_eq!(state(&p,&id).await,("posted".into(),2));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM maintenance_work_orders WHERE id='work'").fetch_one(p.pool().raw()).await.unwrap(),3);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM operational_inventory_consumptions").fetch_one(p.pool().raw()).await.unwrap(),2);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM command_receipts WHERE command_id IN ('parts-concurrent-original','parts-concurrent-next','parts-concurrent-review')").fetch_one(p.pool().raw()).await.unwrap(),3);
    }).await;
}
