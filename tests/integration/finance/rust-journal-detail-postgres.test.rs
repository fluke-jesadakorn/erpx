#[allow(
    dead_code,
    reason = "actual Auth/Finance/Workflow fixture also serves command tests"
)]
#[path = "../../support/finance/review-composition.rs"]
mod facts;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderValue, Request, StatusCode},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_finance::public::{FinanceActor, journal_detail::*};
use erpx_server_people_payroll::public::finance_journal_names::*;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{CompanyId, HoldingId, RecordId, UserId};
use erpx_service::{
    finance_journal_detail::*, finance_journal_detail_http::*,
    finance_journal_review::FinanceJournalEnvironment,
};
use facts::{Prepared, account, *};
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;
fn service(p: &Prepared) -> FinanceJournalDetailService<TestClock> {
    FinanceJournalDetailService::new(
        p.authorizer.clone(),
        FinanceJournalReadConfiguration::new(
            FinanceJournalEnvironment::Development,
            HoldingId::new("h001").unwrap(),
            CompanyId::new("company").unwrap(),
            Arc::new(current_package_catalogue().unwrap()),
        )
        .unwrap(),
        p.system.scenario.clock.clone(),
    )
}
fn router(p: &Prepared) -> Router {
    finance_journal_read_router(
        Arc::new(account::fixture::account::policy()),
        account::fixture::account::holding_registry(),
        Arc::new(FinanceJournalReadScopeHeaders),
        vec![FinanceJournalReadTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            service: Arc::new(service(p)),
        }],
    )
    .unwrap()
}
fn request(p: &Prepared, id: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/api/v1/holdings/acme/finance/journals/{id}"))
        .header("host", "acme.example.test")
        .header(
            "cookie",
            format!("__Host-erpx-product={}", p.reviewer_token.expose()),
        )
        .body(Body::empty())
        .unwrap()
}
async fn invoke(app: &Router, r: Request<Body>) -> (StatusCode, serde_json::Value) {
    let r = app.clone().oneshot(r).await.unwrap();
    assert_eq!(r.headers()["cache-control"], "private, no-store");
    assert!(r.headers().get("access-control-allow-origin").is_none());
    let status = r.status();
    let body = to_bytes(r.into_body(), 2 * 1024 * 1024 + 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}
async fn read_only(p: &Prepared) {
    sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_finance_review',2,'[\"finance.read\",\"module.finance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','synthetic-fixture',1);UPDATE iam_roles SET published_revision=2 WHERE id='custom_finance_review'").execute(p.system.holdings[0].raw()).await.unwrap();
}
async fn waiting(observer: &DatabasePool, database: &str, pid: i32) {
    tokio::time::timeout(Duration::from_secs(5),async{loop{if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=$1 AND $2=ANY(pg_blocking_pids(pid)))").bind(database).bind(pid).fetch_one(observer.raw()).await.unwrap(){break;}tokio::time::sleep(Duration::from_millis(10)).await;}}).await.expect("actual read source lock wait");
}
#[tokio::test]
async fn actual_read_only_account_gets_b0_detail_and_original_render_context_without_post_or_reviewer_policy()
 {
    account::with_system(|system|async move{let p=setup(system).await;p.create("detail-original",false).await.unwrap();p.create("opening-detail",true).await.unwrap();
 p.review("journal-detail-original","detail-post",post()).await.unwrap();p.review("journal-detail-original","detail-reverse",reverse()).await.unwrap();
 let pool=&p.system.holdings[0];sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at)VALUES('inactive-author','fixture-a','company','Original inactive Company Person','TH','inactive','fixture-a',0,0);UPDATE finance_accounts SET status='inactive'WHERE id='cash'").execute(pool.raw()).await.unwrap();read_only(&p).await;
 let app=router(&p);for id in ["journal-detail-original","journal-opening-detail","reversal-detail-reverse"]{let(status,value)=invoke(&app,request(&p,id)).await;assert_eq!(status,StatusCode::OK,"{value}");assert_eq!(value["context"]["userId"],"fixture-b");assert_eq!(value["context"]["assignmentId"],"finance-fixture-b");assert_eq!(value["context"]["assignmentVersion"],3);assert_eq!(value["context"]["revision"],7);assert_eq!(value["context"]["permissions"],serde_json::json!(["finance.read","module.finance.read"]));assert_eq!(value["data"]["journal"]["id"],id);assert_eq!(value["data"]["journal"]["company_id"],value["context"]["companyId"]);assert_eq!(value["data"]["lines"].as_array().unwrap().len(),2);assert_eq!(value["data"]["lines"][0]["ordinal"],0);if id=="journal-detail-original"{assert_eq!(value["data"]["journal"]["created_by_name"],"Original inactive Company Person");assert_eq!(value["data"]["journal"]["posted_by_name"],"Unlinked account");}
 let expected:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(j)||jsonb_build_object('created_by_name',COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=j.created_by AND p.company_id=j.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked account'),'posted_by_name',COALESCE((SELECT p.legal_name FROM hr_people p WHERE p.user_id=j.posted_by AND p.company_id=j.company_id AND p.merged_into_id IS NULL LIMIT 1),'Unlinked account'))FROM finance_journals j WHERE id=$1").bind(id).fetch_one(pool.raw()).await.unwrap();assert_eq!(value["data"]["journal"],expected);
 }
 assert!(FinanceJournalReadConfiguration::new(FinanceJournalEnvironment::Production,HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap(),Arc::new(current_package_catalogue().unwrap())).is_err());
 }).await;
}
#[tokio::test]
async fn cookie_host_origin_and_optional_full_cas_are_checked_without_an_actor_dto() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        p.create("transport", false).await.unwrap();
        let app = router(&p);
        let mut r = request(&p, "journal-transport");
        r.headers_mut().remove("cookie");
        r.headers_mut()
            .insert("x-erpx-context-revision", HeaderValue::from_static("bad"));
        assert_eq!(invoke(&app, r).await.0, StatusCode::UNAUTHORIZED);
        let mut r = request(&p, "journal-transport");
        r.headers_mut()
            .insert("x-erpx-context-revision", HeaderValue::from_static("7"));
        assert_eq!(invoke(&app, r).await.0, StatusCode::CONFLICT);
        for version in ["3", "2"] {
            let mut r = request(&p, "journal-transport");
            for (k, v) in [
                ("x-erpx-context-company", "company"),
                ("x-erpx-context-assignment", "finance-fixture-b"),
                ("x-erpx-context-revision", "7"),
                ("x-erpx-context-assignment-version", version),
            ] {
                r.headers_mut().insert(k, HeaderValue::from_str(v).unwrap());
            }
            assert_eq!(
                invoke(&app, r).await.0,
                if version == "3" {
                    StatusCode::OK
                } else {
                    StatusCode::CONFLICT
                }
            );
        }
        let mut r = request(&p, "journal-transport");
        let cookie = r.headers()["cookie"].clone();
        r.headers_mut().append("cookie", cookie);
        let (status, value) = invoke(&app, r).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(value["error"]["code"], "AMBIGUOUS_COOKIE_HEADER");
        let mut r = request(&p, "journal-transport");
        r.headers_mut().insert(
            "origin",
            HeaderValue::from_static("https://foreign.example.test"),
        );
        assert_eq!(invoke(&app, r).await.0, StatusCode::FORBIDDEN);
        let mut r = request(&p, "journal-transport");
        r.headers_mut()
            .insert("host", HeaderValue::from_static("northstar.example.test"));
        assert_eq!(invoke(&app, r).await.0, StatusCode::NOT_FOUND);
        sqlx::query("UPDATE assignments SET revoked_at=valid_from+1 WHERE id='finance-fixture-b'")
            .execute(p.system.holdings[0].raw())
            .await
            .unwrap();
        assert_ne!(
            invoke(&app, request(&p, "journal-transport")).await.0,
            StatusCode::OK
        );
    })
    .await;
}
#[tokio::test]
async fn retained_commercial_history_allows_read_after_new_work_and_settlement_purchase_end() {
    account::with_system(|system|async move{let p=setup(system).await;p.create("history-read",false).await.unwrap();let pool=&p.system.holdings[0];
 let mut tx=pool.raw().begin().await.unwrap();
 let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence)FROM company_package_grants WHERE company_id='company'AND package_id='finance'").fetch_one(&mut*tx).await.unwrap();
 sqlx::query("UPDATE company_package_grants SET state='inactive',version=version+1 WHERE company_id='company'AND package_id='finance'").execute(&mut*tx).await.unwrap();
 sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at)SELECT 'detail-history-withdrawal','company','finance','fixture-a','synthetic-history-source',$1,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1800000000000 FROM company_package_grants WHERE company_id='company'AND package_id='finance'").bind(before).execute(&mut*tx).await.unwrap();
 tx.commit().await.unwrap();read_only(&p).await;
 let(status,value)=invoke(&router(&p),request(&p,"journal-history-read")).await;assert_eq!(status,StatusCode::OK,"{value}");assert_eq!(value["data"]["journal"]["status"],"draft");
 }).await;
}
#[tokio::test]
async fn source_writer_before_first_snapshot_is_observed_with_same_retained_account_context() {
    account::with_system(|system| async move {
        let p = setup(system).await;
        p.create("source-wait", false).await.unwrap();
        let pool = &p.system.holdings[0];
        let mut writer = pool.raw().begin().await.unwrap();
        let (db, pid): (String, i32) = sqlx::query_as("SELECT current_database(),pg_backend_pid()")
            .fetch_one(&mut *writer)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE finance_accounts SET name='Writer committed current account'WHERE id='cash'",
        )
        .execute(&mut *writer)
        .await
        .unwrap();
        let app = router(&p);
        let r = request(&p, "journal-source-wait");
        let reading = tokio::spawn(async move { invoke(&app, r).await });
        waiting(&p.system.control, &db, pid).await;
        assert!(!reading.is_finished());
        writer.commit().await.unwrap();
        let (status, value) = reading.await.unwrap();
        assert_eq!(status, StatusCode::OK, "{value}");
        assert_eq!(
            value["data"]["lines"][0]["name"],
            "Writer committed current account"
        );
        assert_eq!(value["context"]["userId"], "fixture-b");
    })
    .await;
}
#[tokio::test]
async fn unsupported_source_name_corruption_and_bounded_data_never_return_a_partial_detail() {
    account::with_system(|system|async move{let p=setup(system).await;p.create("unsupported-read",false).await.unwrap();p.create("oversize-read",false).await.unwrap();let pool=&p.system.holdings[0];sqlx::query("UPDATE finance_journals SET source_type='stock_count',source_id='missing-owner'WHERE id='journal-unsupported-read'").execute(pool.raw()).await.unwrap();let app=router(&p);assert_eq!(invoke(&app,request(&p,"journal-unsupported-read")).await.0,StatusCode::NOT_FOUND);
 sqlx::query("UPDATE finance_journals SET memo=repeat('x',262145)WHERE id='journal-oversize-read'").execute(pool.raw()).await.unwrap();assert_eq!(invoke(&app,request(&p,"journal-oversize-read")).await.0,StatusCode::SERVICE_UNAVAILABLE);
 p.create("oversize-person",false).await.unwrap();sqlx::query("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('large-name','fixture-a','company',repeat('x',16385),'TH','fixture-a',0,0)").execute(pool.raw()).await.unwrap();let(status,value)=invoke(&app,request(&p,"journal-oversize-person")).await;assert_ne!(status,StatusCode::OK);assert!(value.get("data").is_none());
 }).await;
}
#[tokio::test]
async fn owner_witnesses_reject_local_mutation_and_same_backend_new_transaction() {
    account::with_system(|system|async move{let p=setup(system).await;p.create("local-proof",false).await.unwrap();let pool=&p.system.holdings[0];let actor=FinanceActor{holding_id:HoldingId::new("h001").unwrap(),company_id:CompanyId::new("company").unwrap(),user_id:UserId::new("fixture-b").unwrap(),assignment_id:p.reviewer_selection.expected.as_ref().unwrap().assignment_id.clone()};let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_journal_detail_sources(&mut tx).await.unwrap();let source=read_journal_detail(&mut tx,&actor,&RecordId::new("journal-local-proof").unwrap()).await.unwrap();source.require_current(&mut tx).await.unwrap();sqlx::query("UPDATE finance_journals SET memo='changed locally'WHERE id='journal-local-proof'").execute(&mut*tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();let mut tx=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();assert!(source.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
 let mut tx=pool.raw().begin().await.unwrap();lock_finance_journal_name_sources(&mut tx).await.unwrap();let user=UserId::new("fixture-a").unwrap();let names=read_finance_journal_names(&mut tx,&CompanyId::new("company").unwrap(),std::slice::from_ref(&user)).await.unwrap();assert_eq!(names.name(&user).unwrap(),"Unlinked account");sqlx::query("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('late-name','fixture-a','company','New same transaction name','TH','fixture-a',0,0)").execute(&mut*tx).await.unwrap();assert!(names.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn current_control_lease_keeps_the_original_render_account_through_read_commit_before_normal_signout_ack()
 {
    account::with_system(|system|async move{
        let p=setup(system).await;p.create("retained-detail-account",false).await.unwrap();let pool=&p.system.holdings[0];
        let mut writer=pool.raw().begin().await.unwrap();let(database,pid):(String,i32)=sqlx::query_as("SELECT current_database(),pg_backend_pid()").fetch_one(&mut*writer).await.unwrap();sqlx::query("UPDATE finance_accounts SET name='Retained current account'WHERE id='cash'").execute(&mut*writer).await.unwrap();
        let app=router(&p);let first=app.clone();let r=request(&p,"journal-retained-detail-account");let reading=tokio::spawn(async move{invoke(&first,r).await});waiting(&p.system.control,&database,pid).await;
        let authentication=p.system.scenario.auth.clone();let central=erpx_server_identity_access::public::PresentedSecrets{browser:p.reviewer_central.browser.clone(),active:p.reviewer_central.active.clone(),saved:p.reviewer_central.saved.clone()};
        let retirement=tokio::spawn(async move{authentication.sign_out(&central,erpx_server_identity_access::public::RequestAudience::Account).await});
        tokio::time::timeout(Duration::from_secs(5),async{loop{if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database()AND cardinality(pg_blocking_pids(pid))>0)").fetch_one(p.system.control.raw()).await.unwrap(){break;}tokio::time::sleep(Duration::from_millis(10)).await;}}).await.expect("normal signout must actually wait for retained Control proof");
        assert!(!retirement.is_finished());writer.commit().await.unwrap();let(status,data)=reading.await.unwrap();assert_eq!(status,StatusCode::OK,"{data}");assert_eq!(data["context"]["userId"],"fixture-b");assert_eq!(data["data"]["lines"][0]["name"],"Retained current account");retirement.await.unwrap().unwrap();
        assert_eq!(invoke(&app,request(&p,"journal-retained-detail-account")).await.0,StatusCode::UNAUTHORIZED);
    }).await;
}

#[tokio::test]
async fn people_name_tuple_fence_preserves_actual_40001_after_an_older_snapshot() {
    account::with_system(|system|async move{
        let p=setup(system).await;p.create("person-fence-detail",false).await.unwrap();let pool=&p.system.holdings[0];
        sqlx::query("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('name-fence','fixture-a','company','Original name','TH','fixture-a',0,0)").execute(pool.raw()).await.unwrap();
        let mut old=pool.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();let creator:String=sqlx::query_scalar("SELECT created_by FROM finance_journals WHERE id='journal-person-fence-detail'").fetch_one(&mut*old).await.unwrap();
        sqlx::query("UPDATE hr_people SET legal_name='Committed current name',version=version+1 WHERE id='name-fence'").execute(pool.raw()).await.unwrap();
        lock_finance_journal_name_sources(&mut old).await.unwrap();let result=read_finance_journal_names(&mut old,&CompanyId::new("company").unwrap(),&[UserId::new(creator).unwrap()]).await;
        assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e))if e.sql_state()==Some("40001")));old.rollback().await.unwrap();
    }).await;
}

fn workspace_request(p: &Prepared, id: &str) -> Request<Body> {
    let mut r = request(p, id);
    *r.uri_mut() = format!("/api/v1/holdings/acme/finance/journals/{id}/workspace")
        .parse()
        .unwrap();
    r
}
#[tokio::test]
async fn full_record_workspace_preserves_b0_reference_scope_order_and_read_only_states() {
    account::with_system(|system|async move{
  let p=setup(system).await;p.create("workspace-original",false).await.unwrap();let pool=&p.system.holdings[0];
  // Historical reference fixtures are valid retained rows, not a claim that a
  // native period-close or chart-archive command has already been migrated.
  sqlx::raw_sql("UPDATE finance_accounts SET status='inactive'WHERE id='cash';UPDATE finance_charts SET status='archived'WHERE id='chart';INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,created_at)VALUES('older-closed','company','Retained closed period','2025-01-01','2025-01-31','closed',0),('later-open','company','Later period','2027-01-01','2027-01-31','open',0);INSERT INTO finance_dimension_definitions(id,company_id,code,name,required,status,created_by,created_at)VALUES('a-dimension','company','a_region','Region',0,'active','fixture-a',0),('b-dimension','company','b_project','Project',1,'active','fixture-a',0),('inactive-dimension','company','old','Old',0,'inactive','fixture-a',0);INSERT INTO finance_dimension_values(id,dimension_id,company_id,code,name,status,created_at)VALUES('active-value','b-dimension','company','a','Active value','active',0),('active-old-parent','inactive-dimension','company','b','Active value for inactive parent','active',0),('inactive-value','a-dimension','company','c','Inactive value','inactive',0)").execute(pool.raw()).await.unwrap();
  read_only(&p).await;let(status,value)=invoke(&router(&p),workspace_request(&p,"journal-workspace-original")).await;assert_eq!(status,StatusCode::OK,"{value}");
  assert_eq!(value["context"]["userId"],"fixture-b");assert_eq!(value["context"]["assignmentVersion"],3);assert_eq!(value["context"]["permissions"],serde_json::json!(["finance.read","module.finance.read"]));
  let d=&value["data"];assert_eq!(d.as_object().unwrap().len(),8);assert_eq!(d["chart"]["id"],"chart");assert_eq!(d["chart"]["status"],"archived");assert_eq!(d["accounts"].as_array().unwrap().iter().map(|r|r["id"].as_str().unwrap()).collect::<Vec<_>>(),vec!["cash","sales"]);assert_eq!(d["accounts"][0]["status"],"inactive");
  assert_eq!(d["periods"].as_array().unwrap().iter().map(|r|r["id"].as_str().unwrap()).collect::<Vec<_>>(),vec!["later-open","period","older-closed"]);assert_eq!(d["periods"][2]["state"],"closed");
  assert_eq!(d["dimensions"].as_array().unwrap().iter().map(|r|r["code"].as_str().unwrap()).collect::<Vec<_>>(),vec!["a_region","b_project"]);assert_eq!(d["dimensions"][1]["required"],1);
  assert_eq!(d["dimensionValues"].as_array().unwrap().iter().map(|r|r["id"].as_str().unwrap()).collect::<Vec<_>>(),vec!["active-value","active-old-parent"]);assert_eq!(d["dimensionValues"][1]["dimension_id"],"inactive-dimension");
 }).await;
}
#[tokio::test]
async fn new_reference_writer_must_finish_before_iam_snapshot_and_its_rows_are_current() {
    account::with_system(|system|async move{
  let p=setup(system).await;p.create("workspace-writer",false).await.unwrap();let mut writer=p.system.holdings[0].raw().begin().await.unwrap();let(database,pid):(String,i32)=sqlx::query_as("SELECT current_database(),pg_backend_pid()").fetch_one(&mut*writer).await.unwrap();
  sqlx::query("INSERT INTO finance_dimension_definitions(id,company_id,code,name,required,status,created_by,created_at)VALUES('committing-dimension','company','current','Committed current dimension',0,'active','fixture-a',0)").execute(&mut*writer).await.unwrap();let app=router(&p);let r=workspace_request(&p,"journal-workspace-writer");let task=tokio::spawn(async move{invoke(&app,r).await});waiting(&p.system.control,&database,pid).await;assert!(!task.is_finished());writer.commit().await.unwrap();let(status,data)=task.await.unwrap();assert_eq!(status,StatusCode::OK,"{data}");assert_eq!(data["data"]["dimensions"][0]["id"],"committing-dimension");assert_eq!(data["context"]["userId"],"fixture-b");
 }).await;
}
#[tokio::test]
async fn full_workspace_reference_proof_rejects_local_mutation_detachment_and_real_old_snapshot_40001()
 {
    account::with_system(|system|async move{
  use erpx_server_finance::public::journal_workspace::*;use sqlx::Connection;
  let p=setup(system).await;p.create("workspace-witness",false).await.unwrap();let pool=&p.system.holdings[0];let actor=FinanceActor{holding_id:HoldingId::new("h001").unwrap(),company_id:CompanyId::new("company").unwrap(),user_id:UserId::new("fixture-b").unwrap(),assignment_id:p.reviewer_selection.expected.as_ref().unwrap().assignment_id.clone()};let id=RecordId::new("journal-workspace-witness").unwrap();
  let mut connection=pool.raw().acquire().await.unwrap();let mut tx=connection.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_journal_workspace_sources(&mut tx).await.unwrap();let source=read_journal_record_workspace(&mut tx,&actor,&id).await.unwrap();source.require_current(&mut tx).await.unwrap();sqlx::query("UPDATE finance_periods SET name='Changed only in caller unit'WHERE id='period'").execute(&mut*tx).await.unwrap();assert!(source.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();let mut tx=connection.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();assert!(source.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
  let mut old=connection.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();sqlx::query("SELECT id FROM finance_periods WHERE id='period'").fetch_one(&mut*old).await.unwrap();sqlx::query("UPDATE finance_periods SET name='Committed after old snapshot'WHERE id='period'").execute(pool.raw()).await.unwrap();lock_journal_workspace_sources(&mut old).await.unwrap();let result=read_journal_record_workspace(&mut old,&actor,&id).await;assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(ref e))if e.sql_state()==Some("40001")));old.rollback().await.unwrap();
 }).await;
}
#[tokio::test]
async fn unrelated_reference_bodies_are_excluded_and_one_aggregate_budget_precedes_collection() {
    account::with_system(|system|async move{
  let p=setup(system).await;p.create("workspace-budget",false).await.unwrap();let pool=&p.system.holdings[0];
  sqlx::query("UPDATE finance_accounts SET name=repeat('unrelated-private-book-',1500000)WHERE id='book-cash'").execute(pool.raw()).await.unwrap();let app=router(&p);let(status,value)=invoke(&app,workspace_request(&p,"journal-workspace-budget")).await;assert_eq!(status,StatusCode::OK,"{value}");assert_eq!(value["data"]["accounts"].as_array().unwrap().len(),2);assert!(value.to_string().len()<20000);
  sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,created_at,updated_at)SELECT 'workspace-big-'||n,'company','chart','big-'||n,repeat('x',250000),'asset',0,0 FROM generate_series(1,9)n").execute(pool.raw()).await.unwrap();let(status,value)=invoke(&app,workspace_request(&p,"journal-workspace-budget")).await;assert_eq!(status,StatusCode::SERVICE_UNAVAILABLE,"{value}");assert!(value.get("data").is_none());
 }).await;
}
