//! Real native Account session/IAM work-role response through TS transport and
//! mapper only. This does not exercise Next requireWorkContext or mount routes.
#[path = "../../support/finance/review-composition.rs"]
mod review;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use erpx_server_identity_access::public::work_context::HoldingWorkContextInspector;
use erpx_server_shared::public::{Clock, HoldingId};
use erpx_service::{
    field_service_reservation_read_http::FieldServiceReadScopeHeaders,
    native_work_roles_http::{NativeWorkRoleTarget, native_work_roles_router},
};
use std::{io::Write, sync::Arc};
use tower::ServiceExt;
#[tokio::test]
async fn real_account_work_role_response_reaches_typescript_mapper() {
    review::account::with_system(|system|async move {
  let p=review::setup_with_finance(system,false).await;
  let inspector=Arc::new(HoldingWorkContextInspector::new(p.system.control.clone(),p.system.holdings[0].clone(),HoldingId::new("h001").unwrap(),review::account::fixture::account::policy(),p.system.scenario.clock.clone(),review::account::fixture::account::holding_registry()).unwrap());
  let app=native_work_roles_router(Arc::new(review::account::fixture::account::policy()),review::account::fixture::account::holding_registry(),Arc::new(FieldServiceReadScopeHeaders),p.system.scenario.clock.clone(),vec![NativeWorkRoleTarget{control:p.system.control.clone(),host:"acme.example.test".into(),holding_slug:"acme".into(),inspector}]).unwrap();
  let db=p.system.holdings[0].raw();
  for (role,revision) in [("r".repeat(128),7i64),("r".repeat(129),7),("é".repeat(65),7),("r".repeat(128),0),("r".repeat(128),9_007_199_254_740_991),("r".repeat(128),9_007_199_254_740_992)] {
   sqlx::query("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*',$1,'Boundary role',false,1,'active') ON CONFLICT DO NOTHING").bind(&role).execute(db).await.unwrap();
   sqlx::query("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*',$1,1,'[\"finance.read\",\"finance.write\",\"module.finance.read\"]','[]','[\"holding\"]','fixture',0) ON CONFLICT DO NOTHING").bind(&role).execute(db).await.unwrap();
   sqlx::query("UPDATE assignments SET role=$1 WHERE id='finance-fixture-a'").bind(&role).execute(db).await.unwrap();sqlx::query("UPDATE active_work_contexts SET revision=$1 WHERE assignment_id='finance-fixture-a'").bind(revision).execute(db).await.unwrap();
   let req=Request::builder().uri("/api/v1/holdings/acme/field-service/work-roles/context").header("host","acme.example.test").header("cookie",format!("__Host-erpx-product={}",p.maker_token.expose())).header("x-erpx-requested-company","company").body(Body::empty()).unwrap();
   let response=app.clone().oneshot(req).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();if !status.is_success(){assert_eq!(revision,9_007_199_254_740_992,"unexpected native boundary denial");assert_eq!(status,axum::http::StatusCode::CONFLICT);eprintln!("native work-role boundary case denied status={status} revision={revision} role_bytes={}",role.len());continue;}let body:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let state=&body["data"];
   let (name,email):(String,String)=sqlx::query_as("SELECT name,email FROM \"user\" WHERE id=$1").bind(state["userId"].as_str().unwrap()).fetch_one(p.system.control.raw()).await.unwrap();
   let identity=serde_json::json!({"sessionId":state["sessionId"],"userId":state["userId"],"companyId":state["companyId"]});
   let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");let mut child=std::process::Command::new(std::env::var("ERPX_NODE_BIN").unwrap()).current_dir(root).args(["--conditions=react-server","--import=tsx","scripts/testing/fixtures/marketing-work-context-legacy-oracle.ts"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
   write!(child.stdin.take().unwrap(),"{}",serde_json::json!({"response":body,"identity":identity,"user":{"name":name,"email":email}})).unwrap();let out=child.wait_with_output().unwrap();assert!(out.status.success(),"TS mapper oracle failed");let actual:serde_json::Value=serde_json::from_slice(&out.stdout).unwrap();
   let mut selection=p.maker_selection.clone();selection.expected.as_mut().unwrap().revision=revision;
   let hash=p.authorizer.execute("acme.example.test",p.maker.clone(),selection,erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest{permission:"finance.write".into(),module:Some("finance".into()),destination:"finance/journals".into(),api:true},|_c,current|Box::pin(async move {
    use erpx_server_workflow::public::{commands::CommandField,source_ordered_json::{SourceOrderedFingerprintActor,ScopedWorkAuthority,SourceOrderedJson,source_ordered_fingerprint_metadata}};
    let sel=current.selection();let assignment=sel.assignment();let workspace=sel.workspace();let actor=SourceOrderedFingerprintActor{user_id:current.user_id().as_str(),assignment_id:assignment.id.as_str(),role:&assignment.role,company_id:workspace.company_id.as_str(),branch:workspace.branch_id.as_ref().map(|x|CommandField::Value(x.as_str())).unwrap_or(CommandField::Null),department:workspace.department_id.as_ref().map(|x|CommandField::Value(x.as_str())).unwrap_or(CommandField::Null),authority:CommandField::Value(ScopedWorkAuthority{session_id:current.session_id().as_str(),revision:sel.revision(),assignment_version:assignment.version})};
    let input=SourceOrderedJson::parse(r#"{"kind":"content","name":"Draft","configuration":{},"preview":{}}"#)?;Ok(source_ordered_fingerprint_metadata(&actor,"marketing.asset.create","marketing-content-fixture",&input))
   })).await.unwrap();assert_eq!(hash,actual["hash"].as_str().unwrap());
   eprintln!("boundary outcome revision={revision} role_bytes={} mapper_ok={} code={}",role.len(),actual["ok"],actual["code"]);assert_eq!(actual["ok"],true,"actual TS transport/mapper must succeed for tested native response");if actual["ok"]==true{assert_eq!(actual["role"],role);assert_eq!(actual["revision"].as_i64(),Some(revision));}
  }
  let _=p.system.scenario.clock.now();
 }).await;
}

#[tokio::test]
async fn marketing_command_uses_real_account_role_boundaries() {
    use erpx_server_marketing::public::lead_candidate_command_http::{
        MarketingLeadCommandHttpTarget, marketing_lead_command_http_router,
    };
    use erpx_server_shared::public::CompanyId;
    use erpx_service::marketing::{MarketingBackend, MarketingConfiguration, MarketingService};
    review::account::with_system(|system|async move {
  let p=review::setup_with_finance(system,false).await;let db=p.system.holdings[0].raw();let now=p.system.scenario.clock.now().get();
  sqlx::query("UPDATE assignments SET modules_json='[\"finance\",\"marketing\"]' WHERE id='finance-fixture-a'").execute(db).await.unwrap();
  let mut seed=p.system.holdings[0].raw().begin().await.unwrap();
  for package in ["sales-marketing","sales","inventory","finance"] {
   sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES('company',$1,1,'active',0,$2,1,'fixture','fixture',$3)").bind(package).bind(now+3_600_000).bind(now).execute(&mut *seed).await.unwrap();
   sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'grant-'||package_id,company_id,package_id,'fixture-a','fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),$1 FROM company_package_grants WHERE company_id='company' AND package_id=$2").bind(now).bind(package).execute(&mut *seed).await.unwrap();
  }
  seed.commit().await.unwrap();
  let service=MarketingService::new(p.authorizer.clone(),MarketingConfiguration::new(HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap()),p.system.scenario.clock.clone());let backend=MarketingBackend::new(Arc::new(review::account::fixture::account::policy()),Arc::new(FieldServiceReadScopeHeaders),Arc::new(service));let app=marketing_lead_command_http_router(vec![MarketingLeadCommandHttpTarget{host:"acme.example.test".into(),holding_slug:"acme".into(),origin:"https://acme.example.test".into(),backend:Arc::new(backend)}]).unwrap();
  for (i,(role,revision)) in [("r".repeat(128),7i64),("r".repeat(129),7),("é".repeat(65),7),("r".repeat(128),0),("r".repeat(128),9_007_199_254_740_991)].into_iter().enumerate(){
   sqlx::query("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*',$1,'Boundary role',false,1,'active') ON CONFLICT DO NOTHING").bind(&role).execute(db).await.unwrap();
   sqlx::query("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*',$1,1,'[\"records.read\",\"records.write\",\"module.marketing.read\"]','[]','[\"holding\"]','fixture',0) ON CONFLICT DO NOTHING").bind(&role).execute(db).await.unwrap();
   sqlx::query("UPDATE assignments SET role=$1 WHERE id='finance-fixture-a'").bind(&role).execute(db).await.unwrap();sqlx::query("UPDATE active_work_contexts SET revision=$1 WHERE assignment_id='finance-fixture-a'").bind(revision).execute(db).await.unwrap();
   let key=format!("boundary-{i}");let body=serde_json::json!({"kind":"content","name":"Boundary asset","configuration":{},"preview":{}}).to_string();
   let req=Request::builder().method("POST").uri("/api/v1/holdings/acme/marketing/assets").header("host","acme.example.test").header("origin","https://acme.example.test").header("cookie",format!("__Host-erpx-product={}",p.maker_token.expose())).header("content-type","application/json").header("idempotency-key",&key).header("x-erpx-context-company","company").header("x-erpx-context-assignment","finance-fixture-a").header("x-erpx-context-revision",revision.to_string()).header("x-erpx-context-assignment-version","3").body(Body::from(body)).unwrap();
   let response=app.clone().oneshot(req).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),1024*1024).await.unwrap();let body:serde_json::Value=serde_json::from_slice(&bytes).unwrap();let effects:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM marketing_assets WHERE id=$1),(SELECT count(*) FROM command_receipts WHERE command_id=$2)").bind(format!("marketing-content-{key}")).bind(&key).fetch_one(db).await.unwrap();eprintln!("marketing outcome revision={revision} role_bytes={} status={status} code={} effects={effects:?}",role.len(),body["error"]["code"]);assert!(status.is_success(),"observed boundary command must succeed");assert_eq!(effects,(1,1));
  }
 }).await;
}
