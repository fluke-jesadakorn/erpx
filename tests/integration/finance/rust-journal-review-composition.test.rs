#[allow(
    dead_code,
    reason = "actual Account lifecycle helper serves several owner suites"
)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use erpx_server_finance::public::journal_review::*;
use erpx_server_identity_access::public::RequestAudience;
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::Clock;
use erpx_service::finance_journal_review::*;
use facts::*;
use sqlx::PgConnection;

async fn counts(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    key: &str,
) -> (i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts WHERE command_id=$1),(SELECT count(*)FROM audit_events WHERE command_id=$1),(SELECT count(*)FROM outbox_events WHERE command_id=$1)").bind(key).fetch_one(pool.raw()).await.unwrap()
}
async fn journal(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    id: &str,
) -> (String, i64) {
    sqlx::query_as("SELECT status,version::bigint FROM finance_journals WHERE id=$1")
        .bind(id)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}
async fn inactive(c: &mut PgConnection, now: i64) {
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence)FROM company_package_grants WHERE company_id='company'AND package_id='finance'").fetch_one(&mut*c).await.unwrap();
    sqlx::query("UPDATE company_package_grants SET state='inactive',version=version+1 WHERE company_id='company'AND package_id='finance'").execute(&mut*c).await.unwrap();
    sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT $1,'company','finance','fixture-a','synthetic-current-withdrawal',$2,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$3 FROM company_package_grants WHERE company_id='company'AND package_id='finance'").bind(uuid::Uuid::new_v4().to_string()).bind(before).bind(now).execute(c).await.unwrap();
}

async fn blocked_by(
    observer: &erpx_server_runtime::public::postgres::DatabasePool,
    database: &str,
    blocker: i32,
) -> Result<(), tokio::time::error::Elapsed> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let blocked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=$1 AND $2=ANY(pg_blocking_pids(pid)))")
                .bind(database).bind(blocker).fetch_one(observer.raw()).await.unwrap();
            if blocked { return; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await
}
fn spawn_post(
    p: &Prepared,
    id: &str,
    key: &str,
) -> tokio::task::JoinHandle<Result<JournalReviewResult, Error>> {
    let administration = p.administration();
    let proof = p.reviewer.clone();
    let selection = p.reviewer_selection.clone();
    let command = command(id, key, post());
    tokio::spawn(async move {
        administration
            .review("acme.example.test", proof, selection, command)
            .await
    })
}

#[tokio::test]
async fn source_writer_committed_before_utility_lock_is_read_as_current_denial() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        p.create("before-snapshot", false).await.unwrap();
        let pool = &p.system.holdings[0];
        let mut writer = pool.raw().begin().await.unwrap();
        let (database, pid): (String, i32) =
            sqlx::query_as("SELECT current_database(),pg_backend_pid()")
                .fetch_one(&mut *writer)
                .await
                .unwrap();
        sqlx::query("UPDATE finance_accounts SET status='inactive'WHERE id='cash'")
            .execute(&mut *writer)
            .await
            .unwrap();
        let task = spawn_post(&p, "journal-before-snapshot", "post-before-snapshot");
        let waiting = blocked_by(&p.system.control, &database, pid).await;
        assert!(!task.is_finished());
        writer.commit().await.unwrap();
        waiting
            .expect("Finance utility lock must actually wait for the current configuration writer");
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(15), task)
            .await
            .unwrap()
            .unwrap();
        assert!(
            outcome.is_err(),
            "an old active-account snapshot must not authorize posting"
        );
        assert_eq!(
            journal(pool, "journal-before-snapshot").await,
            ("draft".into(), 1)
        );
        assert_eq!(counts(pool, "post-before-snapshot").await, (0, 0, 0));
    })
    .await;
}

#[tokio::test]
async fn purchase_revocation_after_iam_snapshot_retries_the_whole_unit_and_denies() {
    account::with_system(|system|async move {
        let p=setup(system).await;p.create("purchase-race",false).await.unwrap();
        let pool=&p.system.holdings[0];let mut writer=pool.raw().begin().await.unwrap();
        let (database,pid):(String,i32)=sqlx::query_as("SELECT current_database(),pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('commercial.package:company:company',0))").execute(&mut *writer).await.unwrap();
        let task=spawn_post(&p,"journal-purchase-race","post-purchase-race");
        let waiting=blocked_by(&p.system.control,&database,pid).await;
        assert!(!task.is_finished());
        // IAM and Finance facts have already been read by the blocked unit.
        // FOR SHARE of this updated known grant must raise 40001; its fresh
        // whole-unit replay then sees the inactive grant and returns Business.
        inactive(&mut writer,p.system.scenario.clock.now().get()).await;
        writer.commit().await.unwrap();waiting.expect("actual Company purchase advisory wait was not observed");
        let result=tokio::time::timeout(std::time::Duration::from_secs(15),task).await.unwrap().unwrap();
        business(result,"PACKAGE_INACTIVE");
        assert_eq!(journal(pool,"journal-purchase-race").await,("draft".into(),1));
        assert_eq!(counts(pool,"post-purchase-race").await,(0,0,0));
    }).await;
}

#[tokio::test]
async fn actual_account_retirement_waits_for_the_owning_financial_commit_then_denies_more_work() {
    account::with_system(|system|async move {
        let p=setup(system).await;p.create("retained-control",false).await.unwrap();
        let pool=&p.system.holdings[0];
        sqlx::raw_sql("CREATE FUNCTION hold_actual_finance_audit()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.action='finance.journal.post'THEN PERFORM pg_advisory_xact_lock(703391);END IF;RETURN NEW;END$$;CREATE TRIGGER hold_actual_finance_audit BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION hold_actual_finance_audit()")
            .execute(pool.raw()).await.unwrap();
        let mut gate=pool.raw().begin().await.unwrap();
        let (database,pid):(String,i32)=sqlx::query_as("SELECT current_database(),pg_backend_pid()").fetch_one(&mut *gate).await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(703391)").execute(&mut *gate).await.unwrap();
        let post=spawn_post(&p,"journal-retained-control","post-retained-control");
        let waiting=blocked_by(&p.system.control,&database,pid).await;
        if waiting.is_err(){gate.rollback().await.unwrap();}
        else {
            let auth=p.system.scenario.auth.clone();let central=erpx_server_identity_access::public::PresentedSecrets {
                browser:p.reviewer_central.browser.clone(),active:p.reviewer_central.active.clone(),saved:p.reviewer_central.saved.clone(),
            };
            let retire=tokio::spawn(async move{auth.sign_out(&central,RequestAudience::Account).await});
            // The session-first Control SHARE lease is retained through the
            // blocked audit. Observe the real Control command lock wait.
            let control_db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(p.system.control.raw()).await.unwrap();
            let control_wait=tokio::time::timeout(std::time::Duration::from_secs(5),async {
                loop {let waits:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=$1 AND cardinality(pg_blocking_pids(pid))>0)").bind(&control_db).fetch_one(p.system.control.raw()).await.unwrap();if waits{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
            }).await;
            assert!(!retire.is_finished());gate.commit().await.unwrap();
            control_wait.expect("actual non-retrying Account retirement must wait for the retained Control lease");
            tokio::time::timeout(std::time::Duration::from_secs(15),post).await.unwrap().unwrap().unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(15),retire).await.unwrap().unwrap().unwrap();
            assert_eq!(journal(pool,"journal-retained-control").await,("posted".into(),2));
            assert_eq!(counts(pool,"post-retained-control").await,(1,1,1));
            assert!(p.review("journal-retained-control","post-retained-control",facts::post()).await.is_err());
            return;
        }
        waiting.expect("actual audit wait was not observed");
    }).await;
}
#[tokio::test]
async fn actual_account_authority_purchase_and_workflow_post_reverse_and_replay_one_manual_origin()
{
    account::with_system(|system|async move{let p=setup(system).await;p.create("manual-origin",false).await.unwrap();let pool=&p.system.holdings[0];let id="journal-manual-origin";
 let posted=p.review(id,"post-manual",post()).await.unwrap();assert!(matches!(posted,JournalReviewResult::Posted{ref id,status:ref s,version:2}if id=="journal-manual-origin"&&s=="posted"));assert_eq!(p.review(id,"post-manual",post()).await.unwrap(),posted);assert_eq!(counts(pool,"post-manual").await,(1,1,1));assert_eq!(journal(pool,id).await,("posted".into(),2));
 p.review(id,"reverse-manual",reverse()).await.unwrap();assert_eq!(journal(pool,id).await,("reversed".into(),3));assert_eq!(journal(pool,"reversal-reverse-manual").await,("posted".into(),1));assert_eq!(counts(pool,"reverse-manual").await,(1,1,1));
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM money_transaction_snapshots WHERE source_type='finance_journal'").fetch_one(pool.raw()).await.unwrap(),2);
 let metadata:serde_json::Value=sqlx::query_scalar("SELECT authority_json::jsonb FROM command_receipts WHERE command_id='post-manual'").fetch_one(pool.raw()).await.unwrap();assert_eq!(metadata["assignmentId"],"finance-fixture-b");assert_eq!(metadata["scopeKind"],"holding");assert!(metadata["scopeNodeId"].is_null());assert!(metadata["iam"]["permissions"].as_array().unwrap().iter().any(|p|p=="finance.approve"));
 }).await;
}
#[tokio::test]
async fn opening_post_and_reversal_apply_real_unsettled_subledger_with_actual_reviewer() {
    account::with_system(|system|async move{let p=setup(system).await;p.create("opening-origin",true).await.unwrap();let pool=&p.system.holdings[0];let id="journal-opening-origin";
 sqlx::raw_sql("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)VALUES('party','company','customer','C1','Synthetic Customer','THB',0,0);INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,journal_id,created_by,created_at,updated_at)VALUES('opening-invoice','company','party','receivable','OPEN-1','2026-01-01','2026-01-31','THB',10000,10000,10000,10000,10000,'cash','sales','opening_import','journal-opening-origin','draft','journal-opening-origin','fixture-a',0,0)").execute(pool.raw()).await.unwrap();
 p.review(id,"post-opening",post()).await.unwrap();assert_eq!(sqlx::query_scalar::<_,String>("SELECT approved_by FROM finance_invoices WHERE id='opening-invoice'").fetch_one(pool.raw()).await.unwrap(),"fixture-b");
 p.review(id,"reverse-opening",reverse()).await.unwrap();let closed:(String,i64,i64)=sqlx::query_as("SELECT status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE id='opening-invoice'").fetch_one(pool.raw()).await.unwrap();assert_eq!(closed,("voided".into(),0,0));assert_eq!(counts(pool,"reverse-opening").await,(1,1,1));
 }).await;
}
#[tokio::test]
async fn actual_same_person_aliases_and_selected_mutation_cas_cannot_approve_their_own_source() {
    account::with_system(|system|async move{let p=setup(system).await;p.create("independent-origin",false).await.unwrap();let pool=&p.system.holdings[0];let id="journal-independent-origin";
 business(p.administration().review("acme.example.test",p.maker.clone(),p.maker_selection.clone(),command(id,"creator-denied",post())).await,"INDEPENDENT_APPROVER_REQUIRED");
 sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('same-person','fixture-a','company','Synthetic same Person','TH','fixture-a',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version)VALUES('company','fixture-a','local:fixture-a','same-person','active','local',1),('company','fixture-b','local:fixture-b','same-person','active','local',1)").execute(pool.raw()).await.unwrap();
 business(p.review(id,"alias-denied",post()).await,"INDEPENDENT_APPROVER_REQUIRED");let mut read=p.reviewer_selection.clone();read.mutation=false;read.expected=None;business(p.administration().review("acme.example.test",p.reviewer.clone(),read,command(id,"missing-cas",post())).await,"WORK_CONTEXT_CHANGED");
 assert_eq!(counts(pool,"creator-denied").await,(0,0,0));assert_eq!(counts(pool,"alias-denied").await,(0,0,0));assert_eq!(journal(pool,id).await,("draft".into(),1));
 }).await;
}
#[tokio::test]
async fn exact_replay_uses_retained_history_but_a_new_request_needs_purchase_and_current_identity()
{
    account::with_system(|system| async move {
        let p = setup(system).await;
        p.create("history-origin", false).await.unwrap();
        p.create("new-origin", false).await.unwrap();
        let pool = &p.system.holdings[0];
        let id = "journal-history-origin";
        let first = p.review(id, "post-history", post()).await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        inactive(&mut tx, p.system.scenario.clock.now().get()).await;
        tx.commit().await.unwrap();
        assert_eq!(p.review(id, "post-history", post()).await.unwrap(), first);
        business(
            p.review("journal-new-origin", "post-new-denied", post())
                .await,
            "PACKAGE_INACTIVE",
        );
        business(
            p.review(
                id,
                "post-history",
                JournalReviewInput::Post {
                    expected_version: 2,
                },
            )
            .await,
            "IDEMPOTENCY_CONFLICT",
        );
        sqlx::query("UPDATE iam_roles SET status='archived'WHERE id='custom_finance_review'")
            .execute(pool.raw())
            .await
            .unwrap();
        assert!(p.review(id, "post-history", post()).await.is_err());
        assert_eq!(counts(pool, "post-history").await, (1, 1, 1));
        assert_eq!(counts(pool, "post-new-denied").await, (0, 0, 0));
    })
    .await;
}
#[tokio::test]
async fn full_finance_purchase_cannot_grant_unproved_foreign_sources_or_production() {
    account::with_system(|system|async move{let p=setup(system).await;p.create("foreign-origin",false).await.unwrap();let pool=&p.system.holdings[0];sqlx::query("UPDATE finance_journals SET source_type='payroll',source_id='unproved-payroll'WHERE id='journal-foreign-origin'").execute(pool.raw()).await.unwrap();business(p.review("journal-foreign-origin","foreign-denied",post()).await,"FINANCE_JOURNAL_SOURCE_UNSUPPORTED");
 let denied=FinanceJournalConfiguration::new(FinanceJournalEnvironment::Production,erpx_server_shared::public::HoldingId::new("h001").unwrap(),erpx_server_shared::public::CompanyId::new("company").unwrap(),std::sync::Arc::new(erpx_server_commercial::public::purchases::current_package_catalogue().unwrap()));assert!(matches!(denied,Err(ref e)if e.code=="FINANCE_PRODUCTION_COMPOSITION_UNAVAILABLE"));assert_eq!(counts(pool,"foreign-denied").await,(0,0,0));
 }).await;
}
#[tokio::test]
async fn actual_audit_constraint_and_final_source_or_identity_withdrawal_roll_back_every_effect() {
    for mutation in ["audit", "source", "identity", "purchase"] {
        account::with_system(move|system|async move{let p=setup(system).await;p.create("rollback-origin",false).await.unwrap();let pool=&p.system.holdings[0];let body=match mutation{"audit"=>"RAISE EXCEPTION 'synthetic actual audit denial' USING ERRCODE='23514';","source"=>"UPDATE finance_journals SET memo='changed by actual trigger' WHERE id='journal-rollback-origin';","identity"=>"UPDATE iam_roles SET status='archived'WHERE id='custom_finance_review';",_=>"UPDATE company_package_grants SET state='inactive',version=version+1 WHERE company_id='company'AND package_id='finance';"};
 let sql=format!("CREATE FUNCTION actual_review_denial()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN IF NEW.action='finance.journal.post'THEN {body} END IF;RETURN NEW;END$$;CREATE TRIGGER actual_review_denial BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION actual_review_denial()");sqlx::raw_sql(sqlx::AssertSqlSafe(sql)).execute(pool.raw()).await.unwrap();
 let result=p.review("journal-rollback-origin","post-rollback",post()).await;
 match (mutation,result) {
  ("audit",Err(PgTransactionError::Persistence(e)))=>assert_eq!(e.sql_state(),Some("23514")),
  ("source",Err(PgTransactionError::Business(e)))=>assert!(["JOURNAL_REVIEW_SOURCE_CHANGED","FINANCE_JOURNAL_SOURCE_CHANGED"].contains(&e.code)),
  ("source",Err(PgTransactionError::Persistence(e)))=>assert_eq!(e.sql_state(),Some("23514"),"the immutable Finance source constraint must reject the audit-time mutation"),
  ("identity",Err(PgTransactionError::Business(e)))=>assert_eq!(e.code,"ROLE_UNAVAILABLE"),
  ("purchase",Err(PgTransactionError::Business(e)))=>assert_eq!(e.code,"COMMERCIAL_PURCHASE_CHANGED"),
  ("purchase",Err(PgTransactionError::Persistence(e)))=>assert_eq!(e.sql_state(),Some("23514"),"the unchanged deferred purchase audit constraint must reject the unreviewed mutation"),
  (_,unexpected)=>panic!("the actual final {mutation} guard did not deny with its expected failure: {unexpected:?}"),
 }
 assert_eq!(journal(pool,"journal-rollback-origin").await,("draft".into(),1));assert_eq!(counts(pool,"post-rollback").await,(0,0,0));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM money_transaction_snapshots WHERE source_type='finance_journal'").fetch_one(pool.raw()).await.unwrap(),0);
 }).await;
    }
}
