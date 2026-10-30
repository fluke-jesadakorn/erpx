//! Actual Account/selected Holding unit and Workflow receipt. The prior Service
//! setup is an explicitly synthetic committed prerequisite, not a setup test.
#[allow(dead_code)]
#[expect(
    unused_imports,
    reason = "Shared Account fixture reexports other owner helpers"
)]
#[path = "../../support/field_service/parts.rs"]
mod facts;
use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
use erpx_server_pms::public::initial_count_modules::InitialCountModuleMode;
use erpx_server_shared::public::{BusinessError, IdentifierSource, RecordId};
#[cfg(feature = "field-service-http")]
use erpx_server_shared::public::{CompanyId, HoldingId};
use erpx_service::service_initial_physical_stock::*;
#[cfg(feature = "field-service-http")]
use erpx_service::service_initial_physical_stock_transport::*;
use erpx_service::service_initial_physical_stock_update::{
    ServiceCountUpdateCommand, execute_service_initial_count_update,
};
use facts::*;
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
fn environment() -> ServiceInitialPhysicalStockEnvironment {
    static ENV: std::sync::OnceLock<ServiceInitialPhysicalStockEnvironment> =
        std::sync::OnceLock::new();
    ENV.get_or_init(|| {
        ServiceInitialPhysicalStockEnvironment::new(
            InitialCountModuleMode::Development,
            "synthetic-field-test".into(),
            None,
        )
    })
    .clone()
}
#[cfg(feature = "field-service-http")]
fn target(
    clock: std::sync::Arc<account::fixture::MutableClock>,
) -> ServiceInitialCountTarget<account::fixture::MutableClock> {
    ServiceInitialCountTarget {
        holding: HoldingId::new("h001").unwrap(),
        company: CompanyId::new("company").unwrap().into(),
        clock,
        environment: environment(),
        files_features: CountBindingFeatures {
            enabled: true,
            read_only: false,
            uploads_frozen: false,
        },
    }
}
#[cfg(feature = "field-service-http")]
fn count_http(
    method: &str,
    path: &str,
    cookie: &str,
    key: Option<&str>,
    body: String,
) -> axum::http::Request<axum::body::Body> {
    let mut r = axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header("host", "acme.example.test")
        .header("origin", "https://acme.example.test")
        .header("cookie", cookie)
        .header("x-erpx-context-company", "company")
        .header("x-erpx-context-revision", "8")
        .header("x-erpx-context-assignment", "field-fixture-a")
        .header("x-erpx-context-assignment-version", "3");
    if let Some(k) = key {
        r = r.header("idempotency-key", k);
    }
    r.body(axum::body::Body::from(body)).unwrap()
}
#[cfg(feature = "field-service-http")]
async fn count_http_raw(
    router: &axum::Router,
    request: axum::http::Request<axum::body::Body>,
) -> (axum::http::StatusCode, String) {
    use tower::ServiceExt;
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("private, no-store")
    );
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}
#[cfg(feature = "field-service-http")]
async fn count_http_response(
    router: &axum::Router,
    request: axum::http::Request<axum::body::Body>,
) -> (axum::http::StatusCode, serde_json::Value) {
    let (status, body) = count_http_raw(router, request).await;
    (status, serde_json::from_str(&body).unwrap())
}
#[cfg(feature = "field-service-http")]
fn legacy_detail(database: &str, actor: &serde_json::Value, id: &str) -> serde_json::Value {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child=Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .arg("scripts/testing/fixtures/service-count-live-race.ts")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut stdin=child.stdin.take().unwrap();
    writeln!(stdin,"{}",serde_json::json!({"database":database,"actor":actor,"id":id,"key":"detail-read","body":{},"mode":"generic-detail"})).unwrap();
    writeln!(stdin,"GO").unwrap();drop(stdin);
    let output=child.wait_with_output().unwrap();
    assert!(output.status.success(),"legacy detail fixture failed: {}",String::from_utf8_lossy(&output.stderr));
    let result=String::from_utf8(output.stdout).unwrap();
    serde_json::from_str(result.strip_prefix("READY\n").expect("fixture ready")).unwrap()
}
#[cfg(feature = "field-service-http")]
fn assert_detail_parity(mut native: serde_json::Value, mut legacy: serde_json::Value) {
    for detail in [&mut native, &mut legacy] {
        let history=detail["history"].as_array_mut().unwrap();
        assert!(history.windows(2).all(|w|w[0]["created_at"].as_i64()>=w[1]["created_at"].as_i64()));
        // The existing query orders by timestamp only; equal-time rows have no
        // specified relative order across distinct PostgreSQL query plans.
        history.sort_by(|a,b|b["created_at"].as_i64().cmp(&a["created_at"].as_i64()).then_with(||a["id"].as_str().cmp(&b["id"].as_str())));
    }
    assert_eq!(native,legacy);
}

#[cfg(feature = "field-service-http")]
struct HeldBody(std::sync::Arc<std::sync::atomic::AtomicUsize>);
#[cfg(feature = "field-service-http")]
impl tokio::io::AsyncRead for HeldBody {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::task::Poll::Pending
    }
}
fn access() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "inventory.read".into(),
        module: None,
        destination: "services/field-service".into(),
        api: true,
    }
}
fn sources() -> ServiceInitialPhysicalStockSources {
    environment().sources()
}

#[tokio::test]
async fn selected_account_updates_and_replays_real_service_count() {
    for tracking in ["none", "lot"] {
        account::with_system(move|system|async move{
  let p=setup_workspace(system).await;let db=p.pool().raw();
  sqlx::raw_sql(r#"INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('count-product','company','COUNT-SKU','Synthetic count product',0,'none');
  INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','count_maker','Synthetic Count maker',false,1,'active');
  INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','count_maker',1,'["inventory.read","inventory.prepare","inventory.approve","workflow.warehouse_manager","workflow.finance_approver","organization.manage","module.inventory.read","module.field_service.read"]','[]','["holding","branch"]','fixture',0);
  INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','count_reviewer','Synthetic Count reviewer',false,1,'active');
  INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','count_reviewer',1,'["inventory.read","inventory.approve","workflow.finance_approver","module.inventory.read","module.field_service.read"]','[]','["holding"]','fixture',0);
  UPDATE assignments SET role='count_maker',valid_from=0 WHERE id='field-fixture-a';
  UPDATE assignments SET role='count_reviewer',valid_from=0 WHERE id='finance-fixture-b';
  INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES('count-reviewer-person','fixture-b','company','Synthetic Reviewer','TH','active','fixture',0,0);
  INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('count-reviewer-engagement','count-reviewer-person','company','employee','salaried','active','2020-01-01','Reviewer','THB','fixture',0,0);
  INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES('company','fixture-b','local:fixture-b','count-reviewer-person','active','local',1);
  INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,control_kind,created_at,updated_at) VALUES
  ('count-ar','company','chart','111','AR','asset','THB','receivable',0,0),('count-ap','company','chart','211','AP','liability','THB','payable',0,0),('count-revenue','company','chart','411','Revenue','revenue','THB','none',0,0),('count-inventory','company','chart','121','Inventory','asset','THB','none',0,0),('count-clearing','company','chart','221','Clearing','liability','THB','none',0,0),('count-cogs','company','chart','511','COGS','expense','THB','none',0,0),('count-input','company','chart','131','Input','asset','THB','none',0,0),('count-output','company','chart','231','Output','liability','THB','none',0,0),('count-cash','company','chart','141','Cash','asset','THB','bank',0,0),('count-variance','company','chart','5400','Variance','expense','THB','none',0,0);
  UPDATE commerce_settings SET currency='THB',receivable_account_id='count-ar',payable_account_id='count-ap',revenue_account_id='count-revenue',inventory_account_id='count-inventory',clearing_account_id='count-clearing',cogs_account_id='count-cogs',input_tax_account_id='count-input',output_tax_account_id='count-output',cash_account_id='count-cash' WHERE company_id='company';"#).execute(db).await.unwrap();
  sqlx::query("UPDATE commerce_products SET tracking_mode=$1 WHERE id='count-product'").bind(tracking).execute(db).await.unwrap();
  let mut tx=db.begin().await.unwrap();
  sqlx::raw_sql(r#"INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) SELECT 'fixture-setup-key','commerce.service-parts-posting-setup.confirm-existing','fixture-a','synthetic-setup-hash','company',jsonb_build_object('id','company','status','confirmed-existing','version',1,'_servicePartsPostingSetup',jsonb_build_object('kind','service-parts-posting-setup','companyId','company','commandId','fixture-setup-key','mode','confirm-existing','mapping',to_jsonb(s)))::text,0,'{"companyId":"company"}' FROM commerce_settings s WHERE company_id='company';
  INSERT INTO commerce_service_parts_posting_sources(company_id,command_id,mode,mapping_json,proof_json,created_by,created_at) SELECT 'company','fixture-setup-key','confirm-existing',to_jsonb(s),jsonb_build_object('kind','service-parts-posting-setup','companyId','company','commandId','fixture-setup-key','mode','confirm-existing','mapping',to_jsonb(s)),'fixture-a',0 FROM commerce_settings s WHERE company_id='company'"#).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
  { let mut tx=db.begin().await.unwrap();sqlx::query("INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at) VALUES('company','supply',1,'active',0,1,'Synthetic Supply gate','synthetic evidence',1)").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'supply-update-fixture',company_id,package_id,'fixture-a','supply-fixture',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),1 FROM company_package_grants WHERE company_id='company' AND package_id='supply'").execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();}
  let actor={let selection=p.selection.clone();let proof=p.review.maker.clone();p.review.authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|_,current,_|Box::pin(async move{let w=current.selection().workspace();let a=current.selection().assignment();let iam=erpx_server_identity_access::public::command_metadata::project_command_iam(current)?;Ok(serde_json::json!({"userId":current.user_id().as_str(),"name":"Synthetic maker","email":"a@example.test","assignmentId":a.id.as_str(),"role":a.role,"companyId":w.company_id.as_str(),"branchId":w.branch_id.as_ref().map(|v|v.as_str()),"departmentId":w.department_id.as_ref().map(|v|v.as_str()),"holdingId":current.holding_id().as_str(),"holdingSlug":"acme","holdingName":"Synthetic","scope":{"kind":"holding","holdingId":current.holding_id().as_str(),"holdingSlug":"acme","holdingName":"Synthetic","companyId":w.company_id.as_str()},"assignmentScope":"holding","inherited":false,"modules":["field_service","inventory"],"assignments":[],"workspaces":[],"authority":{"sessionId":current.session_id().as_str(),"revision":current.selection().revision(),"assignmentVersion":a.version},"iam":iam}))})).await.unwrap().unwrap()};
  #[cfg(feature="field-service-http")]
  {
   use erpx_server_commercial::public::purchases::current_package_catalogue;
   use erpx_service::{field_service_parts::FieldServicePartsEnvironment,field_service_reservation_read::{FieldServiceReservationReadConfiguration,FieldServiceReservationReadService},field_service_reservation_read_http::{field_service_reservation_read_router,FieldServiceReservationReadTarget,FieldServiceReadScopeHeaders}};
   use std::sync::Arc;use axum::http::{StatusCode,header::HeaderValue};
   sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('http-count-product','company','HTTP-COUNT','Synthetic HTTP count',0,$1)").bind(tracking).execute(db).await.unwrap();
   let central=account::login(&p.review.system,"a@example.test",None).await;let child=account::child(&p.review.system,&central,account::ACME).await;
   let now=erpx_server_shared::public::Clock::now(p.review.system.scenario.clock.as_ref()).get();sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
   let base=FieldServiceReservationReadConfiguration::new(FieldServicePartsEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap(),Arc::new(current_package_catalogue().unwrap())).unwrap();
   let mkrouter=|config|{let service=Arc::new(FieldServiceReservationReadService::new(p.review.authorizer.clone(),config,p.review.system.scenario.clock.clone()));field_service_reservation_read_router(Arc::new(account::fixture::account::policy()),account::fixture::account::holding_registry(),Arc::new(FieldServiceReadScopeHeaders),vec![FieldServiceReservationReadTarget{host:"acme.example.test".into(),holding_slug:"acme".into(),service}]).unwrap()};
   let disabled=mkrouter(base.clone());let router=mkrouter(base.with_initial_count(CountBindingFeatures{enabled:true,read_only:false,uploads_frozen:false}));
   let base_path="/api/v1/holdings/acme/field-service/initial-physical-stock";let selected=format!("{base_path}?warehouseId=warehouse&productId=http-count-product&countDate=2026-01-10");
   let (status,off)=count_http_response(&disabled,count_http("GET",&selected,&cookie,None,String::new())).await;assert_eq!(status,StatusCode::FORBIDDEN,"{off}");
   for path in [format!("{base_path}/masters"),format!("{base_path}/masters/flow")] {let (status,body)=count_http_response(&router,count_http("GET",&path,&cookie,None,String::new())).await;assert_eq!(status,StatusCode::NOT_FOUND,"{body}");assert_eq!(body["error"]["code"],"NOT_FOUND","{body}");}
   for path in [format!("{base_path}/masters/flow"),format!("{base_path}/masters/cancel")] {let (status,body)=count_http_response(&router,count_http("POST",&path,&cookie,Some("http-masters"),"{}".into())).await;assert_eq!(status,StatusCode::NOT_FOUND,"{body}");assert_eq!(body["error"]["code"],"NOT_FOUND","{body}");}
   let (status,view)=count_http_response(&router,count_http("GET",&selected,&cookie,None,String::new())).await;assert_eq!(status,StatusCode::OK,"{view}");assert_eq!(view["data"]["canPrepare"],true);let context=view["data"]["contextKey"].as_str().unwrap();
   let body=serde_json::json!({"expectedVersion":0,"contextKey":context,"input":{"warehouseId":"warehouse","countDate":"2026-01-10","reason":"HTTP first count","lines":[{"productId":"http-count-product","countedQuantity":"1","unitCost":"2.00","allocations":if tracking=="lot" {serde_json::json!([{"lotCode":"HTTP-LOT","countedQuantity":"1","unitCost":"2.00"}])}else{serde_json::json!([])} }]}}).to_string();
   let (status,off)=count_http_response(&disabled,count_http("POST",base_path,&cookie,Some("http-disabled"),body.clone())).await;assert_eq!(status,StatusCode::FORBIDDEN,"{off}");
   for (host,origin,revision,missing,expected) in [("wrong.example.test",None,"8",false,StatusCode::NOT_FOUND),("acme.example.test",Some("https://wrong.example.test"),"8",false,StatusCode::FORBIDDEN),("acme.example.test",None,"9",false,StatusCode::CONFLICT),("acme.example.test",None,"8",true,StatusCode::CONFLICT)] {let mut req=count_http("POST",base_path,&cookie,Some("http-denied"),body.clone());req.headers_mut().insert("host",HeaderValue::from_str(host).unwrap());if let Some(origin)=origin{req.headers_mut().insert("origin",HeaderValue::from_str(origin).unwrap());}req.headers_mut().insert("x-erpx-context-revision",HeaderValue::from_str(revision).unwrap());if missing{req.headers_mut().remove("x-erpx-context-assignment");}let (status,result)=count_http_response(&router,req).await;assert_eq!(status,expected,"{result}");}
   let denied:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('http-disabled','http-denied','http-masters')),(SELECT count(*) FROM inventory_counts WHERE id IN ('count-http-disabled','count-http-denied')),(SELECT count(*) FROM outbox_events WHERE command_id IN ('http-disabled','http-denied','http-masters'))").fetch_one(db).await.unwrap();assert_eq!(denied,(0,0,0));
   let (status,created)=count_http_raw(&router,count_http("POST",base_path,&cookie,Some("http-count-create"),body.clone())).await;assert_eq!(status,StatusCode::CREATED,"{created}");let created_data:serde_json::Value=serde_json::from_str(&created).unwrap();assert_eq!(created_data["data"]["id"],"count-http-count-create");let (status,retry)=count_http_raw(&router,count_http("POST",base_path,&cookie,Some("http-count-create"),body)).await;assert_eq!(status,StatusCode::CREATED,"{retry}");assert_eq!(created,retry);
   let record=format!("{base_path}/count-http-count-create");let (status,workspace)=count_http_response(&router,count_http("GET",&record,&cookie,None,String::new())).await;assert_eq!(status,StatusCode::OK,"{workspace}");assert_eq!(workspace["data"]["record"]["id"],"count-http-count-create");assert!(workspace["data"]["selection"]["warehouseId"].is_null());let (status,flow)=count_http_response(&router,count_http("GET",&format!("{record}/flow"),&cookie,None,String::new())).await;assert_eq!(status,StatusCode::OK,"{flow}");assert_eq!(flow["data"]["kind"],"stock_count");
   let detail_path=format!("{record}/stock-count");let database:String=sqlx::query_scalar("SELECT current_database()").fetch_one(db).await.unwrap();
   let (status,detail)=count_http_response(&router,count_http("GET",&detail_path,&cookie,None,String::new())).await;assert_eq!(status,StatusCode::OK,"{detail}");assert_detail_parity(detail["data"].clone(),legacy_detail(&database,&actor,"count-http-count-create"));
   // A same-ID event from another record type has no valid Count receipt and
   // must not leak into this Company's native history (legacy read is broader).
   sqlx::query("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES('foreign-type-audit','foreign-type-command','foreign-user','foreign-assignment','unrelated','foreign_type','count-http-count-create',1,'foreign',0)").execute(db).await.unwrap();
   let (status,protected)=count_http_response(&router,count_http("GET",&detail_path,&cookie,None,String::new())).await;assert_eq!(status,StatusCode::OK);assert_eq!(protected["data"],detail["data"]);
   sqlx::query("DELETE FROM audit_events WHERE id='foreign-type-audit'").execute(db).await.unwrap();
   let before:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)").fetch_one(db).await.unwrap();
   for (path,revision,company,expected) in [(detail_path.clone(),"9","company",StatusCode::CONFLICT),(detail_path.clone(),"8","foreign",StatusCode::NOT_FOUND),(format!("{base_path}/missing/stock-count"),"8","company",StatusCode::NOT_FOUND)] {let mut req=count_http("GET",&path,&cookie,None,String::new());req.headers_mut().insert("x-erpx-context-revision",HeaderValue::from_str(revision).unwrap());req.headers_mut().insert("x-erpx-context-company",HeaderValue::from_str(company).unwrap());let (status,result)=count_http_response(&router,req).await;assert_eq!(status,expected,"{result}");}
   let after:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts),(SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)").fetch_one(db).await.unwrap();assert_eq!(before,after);
   let assign=r#"{"stepKey":"submitted","userId":"fixture-b","expectedVersion":0,"recordVersion":1,"reason":"Assign HTTP review"}"#;let (status,result)=count_http_raw(&router,count_http("POST",&format!("{record}/flow"),&cookie,Some("http-count-assign"),assign.into())).await;assert_eq!(status,StatusCode::OK,"{result}");let (status,retry)=count_http_raw(&router,count_http("POST",&format!("{record}/flow"),&cookie,Some("http-count-assign"),assign.into())).await;assert_eq!(status,StatusCode::OK,"{retry}");assert_eq!(result,retry);
   let generic=r#"{"stepKey":"submitted","userId":"fixture-b","expectedVersion":2,"recordVersion":1,"reason":"Save HTTP \ud800 default","saveDefault":true,"ignored":"stripped"}"#;let generic_path=format!("{record}/stock-count/flow");
   let mut missing=count_http("POST",&generic_path,&cookie,Some("http-generic-denied"),generic.into());missing.headers_mut().remove("x-erpx-context-assignment");assert_eq!(count_http_response(&router,missing).await.0,StatusCode::CONFLICT);
   let (status,result)=count_http_raw(&router,count_http("POST",&generic_path,&cookie,Some("http-generic-flow"),generic.into())).await;assert_eq!(status,StatusCode::OK,"{result}");let (status,retry)=count_http_raw(&router,count_http("POST",&generic_path,&cookie,Some("http-generic-flow"),generic.into())).await;assert_eq!(status,StatusCode::OK,"{retry}");assert_eq!(result,retry);
   let effects:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_defaults WHERE company_id='company' AND kind='stock_count'),(SELECT count(*) FROM flow_events WHERE command_id='http-generic-flow'),(SELECT count(*) FROM command_receipts WHERE command_id='http-generic-denied')").fetch_one(db).await.unwrap();assert_eq!(effects,(1,1,0));let saved_reason:String=sqlx::query_scalar("SELECT reason FROM flow_defaults WHERE company_id='company' AND kind='stock_count'").fetch_one(db).await.unwrap();assert_eq!(saved_reason,"Save HTTP \u{fffd} default");
   let cancel=r#"{"expectedVersion":1,"reason":"Cancel HTTP count"}"#;let (status,result)=count_http_raw(&router,count_http("POST",&format!("{record}/cancel"),&cookie,Some("http-count-cancel"),cancel.into())).await;assert_eq!(status,StatusCode::OK,"{result}");let result_data:serde_json::Value=serde_json::from_str(&result).unwrap();assert_eq!(result_data["data"]["status"],"cancelled");let (status,retry)=count_http_raw(&router,count_http("POST",&format!("{record}/cancel"),&cookie,Some("http-count-cancel"),cancel.into())).await;assert_eq!(status,StatusCode::OK,"{retry}");assert_eq!(result,retry);
   let (status,detail)=count_http_response(&router,count_http("GET",&detail_path,&cookie,None,String::new())).await;assert_eq!(status,StatusCode::OK,"{detail}");assert_eq!(detail["data"]["count"]["status"],"cancelled");assert_detail_parity(detail["data"].clone(),legacy_detail(&database,&actor,"count-http-count-create"));
  }
  let authorizer=p.review.authorizer.clone();let clock=p.review.system.scenario.clock.clone();let proof=p.review.maker.clone();let selection=p.selection.clone();
  let view=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof.clone(),move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),{let clock=clock.clone();move|c,current,_|{let clock=clock.clone();Box::pin(async move{let v=read_initial_count_selection(c,current,clock.as_ref(),InitialCountSelection{warehouse_id:Some("warehouse".into()),product_id:Some("count-product".into()),count_date:Some("2026-01-10".into())},&environment()).await?;Ok(serde_json::to_value(v).unwrap())})}}).await.unwrap().unwrap();
  assert_eq!(view["canPrepare"],true,"{view}");let context=view["contextKey"].as_str().unwrap();
  #[cfg(feature="field-service-http")]
  {let selection=p.selection.clone();let response=read_selected_service_initial_count(&authorizer,"acme.example.test",proof.clone(),move||Ok(selection),target(clock.clone()),ServiceInitialCountReadRoute::Selection{query:Some("warehouseId=warehouse&productId=count-product&countDate=2026-01-10".into())}).await.unwrap();let data=serde_json::to_value(response.data).unwrap();assert_eq!(data["contextKey"],context);}
  #[cfg(feature="field-service-http")]
  {let mut stale=p.selection.clone();stale.expected.as_mut().unwrap().revision+=1;let result=read_selected_service_initial_count(&authorizer,"acme.example.test",proof.clone(),move||Ok(stale),target(clock.clone()),ServiceInitialCountReadRoute::Selection{query:None}).await;assert!(result.is_err());}

  #[cfg(feature="field-service-http")]
  for (key,body,expected) in [(Ok("bad-json".to_owned()),"{".to_owned(),"INVALID_JSON"),(Ok(String::new()),"{}".to_owned(),"COMMAND_KEY_REQUIRED"),(Ok("oversized".to_owned())," ".repeat(2*1024*1024+1),"VALIDATION_FAILED")] {let selection=p.selection.clone();let result=execute_selected_service_initial_count(&authorizer,"acme.example.test",proof.clone(),move||Ok(selection),target(clock.clone()),ServiceInitialCountTransportCommand{key,body:axum::body::Body::from(body),route:ServiceInitialCountCommandRoute::Create}).await;assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code==expected));}
  #[cfg(feature="field-service-http")]
  {let mut stale=p.selection.clone();stale.expected.as_mut().unwrap().revision+=1;let result=execute_selected_service_initial_count(&authorizer,"acme.example.test",proof.clone(),move||Ok(stale),target(clock.clone()),ServiceInitialCountTransportCommand{key:Ok("stale-invalid-body".into()),body:axum::body::Body::from("{"),route:ServiceInitialCountCommandRoute::Create}).await;assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code!="INVALID_JSON"));}
  for mismatched_mode in [true,false] {let other=ServiceInitialPhysicalStockEnvironment::new(if mismatched_mode{InitialCountModuleMode::Production}else{InitialCountModuleMode::Development},"other-environment".into(),None);let (fences,used)=if mismatched_mode{(sources(),other)}else{(other.sources(),environment())};let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let denial=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),fences),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let used=used.clone();Box::pin(async move{read_initial_count_selection(c,current,clock.as_ref(),InitialCountSelection::default(),&used).await.map(|_|())})}).await;assert!(matches!(denial,Err(erpx_server_runtime::public::postgres::PgTransactionError::Persistence(e)) if e.code=="SERVICE_INITIAL_COUNT_FENCE_REQUIRED"));}
  let allocations=if tracking=="lot"{serde_json::json!([{"lotCode":"LOT-COUNT","countedQuantity":"2","unitCost":"3.00"}])}else{serde_json::json!([])};
  let raw=serde_json::json!({"expectedVersion":0,"contextKey":context,"input":{"warehouseId":"warehouse","countDate":"2026-01-10","reason":"Initial actual count","lines":[{"productId":"count-product","countedQuantity":"2","unitCost":"3.00","allocations":allocations}]}}).to_string();
  {let input=normalize_initial_count_create(&raw).unwrap();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let input=input.clone();Box::pin(async move{create_service_initial_count(c,current,clock.as_ref(),&mut Ids,"native-update-create",&input,&environment()).await})}).await.unwrap().unwrap();}
  // Actual committed native Count birth must be accepted by the shared owner predicate.
  { let mut q=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT ");
    q.push(erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_ELIGIBLE)
      .push(" FROM outbox_events e WHERE e.command_id='native-update-create'");
    let valid:bool=q.build_query_scalar().fetch_one(db).await.unwrap();assert!(valid);
  }


  // The Inventory-owned predicate recognizes the actual native source while
  // quarantining a same-source event with a forged Company.
  {
   use erpx_server_inventory::public::service_count_outbox::{SERVICE_COUNT_OUTBOX_ELIGIBLE,SERVICE_COUNT_OUTBOX_QUARANTINE};
   let query=|id| { let mut q=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT (");q.push(SERVICE_COUNT_OUTBOX_ELIGIBLE).push("),(").push(SERVICE_COUNT_OUTBOX_QUARANTINE).push(") FROM outbox_events e WHERE e.command_id=").push_bind(id);q };
   let actual:(bool,bool)=query("native-update-create").build_query_as().fetch_one(db).await.unwrap();assert_eq!(actual,(true,true));
   let mut tx=db.begin().await.unwrap();sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES('fixture-forged-company','fixture-forged-company','inventory.count.create','inventory_count','count-native-update-create',1,'{\"companyId\":\"foreign\"}',0)").execute(&mut *tx).await.unwrap();
   let forged:(bool,bool)=query("fixture-forged-company").build_query_as().fetch_one(&mut *tx).await.unwrap();assert_eq!(forged,(false,true));tx.rollback().await.unwrap();
  }
  let db_name:String=sqlx::query_scalar("SELECT current_database()").fetch_one(db).await.unwrap();
  // Current TS update and Flow entrypoints must reject a genuine native source.
  for (mode,body) in [("create",serde_json::json!({"warehouseId":"warehouse","countDate":"2026-01-10","reason":"Blocked legacy replay","lines":[{"productId":"count-product","countedQuantity":"0","allocations":[]}]})),("update",serde_json::json!({"warehouseId":"warehouse","countDate":"2026-01-10","reason":"Blocked live update","expectedVersion":1,"lines":[{"productId":"count-product","countedQuantity":"0","allocations":[]}]})),("flow",serde_json::json!({"stepKey":"submitted","userId":"fixture-b","expectedVersion":0,"recordVersion":1,"reason":"Blocked live assignment","saveDefault":false})),("files",serde_json::json!({})),("files-tamper",serde_json::json!({}))] {
   let payload=serde_json::json!({"database":db_name,"actor":actor,"key":if mode=="create" {"native-update-create".to_owned()} else {format!("native-live-{mode}")},"id":"count-native-update-create","body":body,"mode":mode});
   let result=tokio::task::spawn_blocking(move||{use std::io::{BufRead,BufReader,Read,Write};use std::process::{Command,Stdio};let bun=std::env::var("ERPX_BUN_BIN").unwrap();let mut child=Command::new(bun).current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../..")).arg("scripts/testing/fixtures/service-count-live-race.ts").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();let mut input=child.stdin.take().unwrap();writeln!(input,"{}",payload).unwrap();input.flush().unwrap();let mut output=BufReader::new(child.stdout.take().unwrap());let mut line=String::new();output.read_line(&mut line).unwrap();assert_eq!(line,"READY\n");writeln!(input,"GO").unwrap();input.flush().unwrap();drop(input);let mut body=String::new();output.read_to_string(&mut body).unwrap();assert!(child.wait().unwrap().success());serde_json::from_str::<serde_json::Value>(&body).unwrap()}).await.unwrap();
   if mode=="files" {assert_eq!(result["outcome"],"created","Service Files source boundary: {result}");} else if mode=="files-tamper" {assert_eq!(result["outcome"],"denied");assert_eq!(result["code"],"23514");} else {assert_eq!(result["outcome"],"denied");assert_eq!(result["code"],"COUNT_NOT_FOUND");}
  }
  let denied:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('native-live-update','native-live-flow')),(SELECT count(*) FROM flow_events WHERE command_id IN ('native-live-update','native-live-flow'))").fetch_one(db).await.unwrap();assert_eq!(denied,(0,0));let file_assertions:i64=sqlx::query_scalar("SELECT count(*) FROM command_assertions WHERE step LIKE 'count-file-%'").fetch_one(db).await.unwrap();assert_eq!(file_assertions,0);
  // Native birth races the live generic TS writer against the same Count id.
  {
   sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('race-count-product','company','RACE-COUNT','Race count',0,'none')").execute(db).await.unwrap();
   let selection=p.selection.clone();let proof=proof.clone();let race_clock=clock.clone();let seed=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof.clone(),move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let race_clock=race_clock.clone();Box::pin(async move{read_initial_count_selection(c,current,race_clock.as_ref(),InitialCountSelection{warehouse_id:Some("warehouse".into()),product_id:Some("race-count-product".into()),count_date:Some("2026-01-10".into())},&environment()).await.map(|v|serde_json::to_value(v).unwrap())})}).await.unwrap().unwrap();
   let body=serde_json::json!({"warehouseId":"warehouse","countDate":"2026-01-10","reason":"Race original Count owner","lines":[{"productId":"race-count-product","countedQuantity":"0","allocations":[]}]});
   let native_raw=serde_json::json!({"expectedVersion":0,"contextKey":seed["contextKey"],"input":body}).to_string();let input=normalize_initial_count_create(&native_raw).unwrap();
   let child_input=serde_json::json!({"database":db_name.clone(),"actor":actor.clone(),"key":"native-birth-race","body":body});
   let spawn_live=|payload:serde_json::Value| {
    let (ready,started)=tokio::sync::oneshot::channel();let (release,go)=std::sync::mpsc::channel::<()>();
    let task=tokio::task::spawn_blocking(move||{use std::io::{BufRead,BufReader,Read,Write};use std::process::{Command,Stdio};let bun=std::env::var("ERPX_BUN_BIN").unwrap();let mut child=Command::new(bun).current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../..")).arg("scripts/testing/fixtures/service-count-live-race.ts").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();let mut input=child.stdin.take().unwrap();writeln!(input,"{}",payload).unwrap();input.flush().unwrap();let mut output=BufReader::new(child.stdout.take().unwrap());let mut line=String::new();output.read_line(&mut line).unwrap();assert_eq!(line,"READY\n");ready.send(()).unwrap();go.recv().unwrap();writeln!(input,"GO").unwrap();input.flush().unwrap();drop(input);let mut body=String::new();output.read_to_string(&mut body).unwrap();let status=child.wait().unwrap();assert!(status.success(),"live race helper failed");serde_json::from_str::<serde_json::Value>(&body).unwrap()});(started,release,task)
   };
   let (started,release,live)=spawn_live(child_input);
   let update_input=serde_json::json!({"database":db_name,"actor":actor,"key":"native-race-update","id":"count-native-birth-race","body":{"warehouseId":"warehouse","countDate":"2026-01-10","reason":"Race legacy update","expectedVersion":1,"lines":[{"productId":"race-count-product","countedQuantity":"0","allocations":[]}]},"mode":"update"});
   let flow_input=serde_json::json!({"database":db_name,"actor":actor,"key":"native-race-flow","id":"count-native-birth-race","body":{"stepKey":"submitted","userId":"fixture-b","expectedVersion":0,"recordVersion":1,"reason":"Race legacy flow","saveDefault":false},"mode":"flow"});
   let (update_started,update_release,update)=spawn_live(update_input);let (flow_started,flow_release,flow)=spawn_live(flow_input);
   started.await.unwrap();update_started.await.unwrap();flow_started.await.unwrap();release.send(()).unwrap();update_release.send(()).unwrap();flow_release.send(()).unwrap();
   let selection=p.selection.clone();let proof=proof.clone();let race_clock=clock.clone();let native=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let race_clock=race_clock.clone();let input=input.clone();Box::pin(async move{create_service_initial_count(c,current,race_clock.as_ref(),&mut Ids,"native-birth-race",&input,&environment()).await})}).await;
   let live=live.await.unwrap();let update=update.await.unwrap();let flow=flow.await.unwrap();let native_ok=matches!(native,Ok(Ok(_)));let live_ok=live["outcome"]=="created";assert_ne!(native_ok,live_ok,"exactly one Count birth must commit: {live}");if native_ok {assert_eq!(update["outcome"],"denied","legacy update race: {update}");assert_eq!(flow["outcome"],"denied","legacy Flow race: {flow}");assert!(matches!(live["code"].as_str(),Some("COUNT_NOT_FOUND"|"23505"|"40001"|"IDEMPOTENCY_CONFLICT")),"unexpected live writer failure: {live}");}
   let rows:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM inventory_counts WHERE id='count-native-birth-race'),(SELECT count(*) FROM command_receipts WHERE command_id='native-birth-race'),(SELECT count(*) FROM inventory_service_initial_physical_sources WHERE count_id='count-native-birth-race')").fetch_one(db).await.unwrap();assert_eq!(rows,(1,1,i64::from(native_ok)));let effects:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('native-race-update','native-race-flow')),(SELECT count(*) FROM flow_events WHERE command_id IN ('native-race-update','native-race-flow'))").fetch_one(db).await.unwrap();if native_ok {assert_eq!(effects,(0,0));}
  }
  // Force an old live TS SERIALIZABLE snapshot to observe no marker, then
  // commit native birth before allowing that transaction to write the same id.
  {
   sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('stale-count-product','company','STALE-COUNT','Stale count',0,'none')").execute(db).await.unwrap();
   let selection=p.selection.clone();let proof=proof.clone();let race_clock=clock.clone();let seed=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof.clone(),move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let race_clock=race_clock.clone();Box::pin(async move{read_initial_count_selection(c,current,race_clock.as_ref(),InitialCountSelection{warehouse_id:Some("warehouse".into()),product_id:Some("stale-count-product".into()),count_date:Some("2026-01-10".into())},&environment()).await.map(|v|serde_json::to_value(v).unwrap())})}).await.unwrap().unwrap();
   let body=serde_json::json!({"warehouseId":"warehouse","countDate":"2026-01-10","reason":"Stale Count owner snapshot","lines":[{"productId":"stale-count-product","countedQuantity":"0","allocations":[]}]});let native_raw=serde_json::json!({"expectedVersion":0,"contextKey":seed["contextKey"],"input":body}).to_string();let input=normalize_initial_count_create(&native_raw).unwrap();
   let create_payload=serde_json::json!({"database":db_name,"actor":actor,"key":"native-stale-race","id":"count-native-stale-race","body":body,"mode":"stale"});
   let update_payload=serde_json::json!({"database":db_name,"actor":actor,"key":"native-stale-update","id":"count-native-stale-race","body":{"warehouseId":"warehouse","countDate":"2026-01-10","reason":"Stale legacy update","expectedVersion":1,"lines":[{"productId":"stale-count-product","countedQuantity":"0","allocations":[]}]},"mode":"stale-update"});
   let flow_payload=serde_json::json!({"database":db_name,"actor":actor,"key":"native-stale-flow","id":"count-native-stale-race","body":{"stepKey":"submitted","userId":"fixture-b","expectedVersion":0,"recordVersion":1,"reason":"Stale legacy flow","saveDefault":false},"mode":"stale-flow"});
   let spawn_stale=|payload:serde_json::Value| {let (snapshot,seen)=tokio::sync::oneshot::channel();let (finish,release)=std::sync::mpsc::channel::<()>();
    let task=tokio::task::spawn_blocking(move||{use std::io::{BufRead,BufReader,Read,Write};use std::process::{Command,Stdio};let mut child=Command::new(std::env::var("ERPX_BUN_BIN").unwrap()).current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../..")).arg("scripts/testing/fixtures/service-count-live-race.ts").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();let mut stdin=child.stdin.take().unwrap();writeln!(stdin,"{}",payload).unwrap();stdin.flush().unwrap();let mut out=BufReader::new(child.stdout.take().unwrap());let mut line=String::new();out.read_line(&mut line).unwrap();assert_eq!(line,"READY\n");writeln!(stdin,"GO").unwrap();stdin.flush().unwrap();line.clear();out.read_line(&mut line).unwrap();assert_eq!(line,"SNAPSHOT\n");snapshot.send(()).unwrap();release.recv().unwrap();writeln!(stdin,"COMMIT").unwrap();stdin.flush().unwrap();drop(stdin);let mut body=String::new();out.read_to_string(&mut body).unwrap();assert!(child.wait().unwrap().success());serde_json::from_str::<serde_json::Value>(&body).unwrap()});(seen,finish,task)
   };
   let (seen,finish,live)=spawn_stale(create_payload);let (update_seen,update_finish,update)=spawn_stale(update_payload);let (flow_seen,flow_finish,flow)=spawn_stale(flow_payload);
   update_seen.await.unwrap();flow_seen.await.unwrap();
   seen.await.unwrap();let selection=p.selection.clone();let race_clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let race_clock=race_clock.clone();let input=input.clone();Box::pin(async move{create_service_initial_count(c,current,race_clock.as_ref(),&mut Ids,"native-stale-race",&input,&environment()).await})}).await.unwrap().unwrap();assert_eq!(result["id"],"count-native-stale-race");finish.send(()).unwrap();update_finish.send(()).unwrap();flow_finish.send(()).unwrap();let denied=live.await.unwrap();let denied_update=update.await.unwrap();let denied_flow=flow.await.unwrap();assert_eq!(denied_update["outcome"],"denied","{denied_update}");assert_eq!(denied_flow["outcome"],"denied","{denied_flow}");assert!(matches!(denied_update["code"].as_str(),Some("COUNT_NOT_FOUND"|"40001"|"RACE_SOURCE_ALREADY_PRESENT")),"{denied_update}");assert!(matches!(denied_flow["code"].as_str(),Some("COUNT_NOT_FOUND"|"FLOW_NOT_FOUND"|"COUNT_TRANSACTION_REQUIRED"|"40001"|"RACE_SOURCE_ALREADY_PRESENT")),"{denied_flow}");assert_eq!(denied["outcome"],"denied");assert!(matches!(denied["code"].as_str(),Some("COUNT_NOT_FOUND"|"23505"|"40001"|"IDEMPOTENCY_CONFLICT"|"RACE_SOURCE_ALREADY_PRESENT")),"unexpected stale writer failure: {denied}");
   let rows:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM inventory_counts WHERE id='count-native-stale-race'),(SELECT count(*) FROM command_receipts WHERE command_id='native-stale-race'),(SELECT count(*) FROM inventory_service_initial_physical_sources WHERE count_id='count-native-stale-race')").fetch_one(db).await.unwrap();assert_eq!(rows,(1,1,1));let effects:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM command_receipts WHERE command_id IN ('native-stale-update','native-stale-flow')),(SELECT count(*) FROM flow_events WHERE command_id IN ('native-stale-update','native-stale-flow'))").fetch_one(db).await.unwrap();assert_eq!(effects,(0,0));
  }
  let allocations=if tracking=="lot"{serde_json::json!([{"lotCode":"LOT-REVISED","countedQuantity":"3","unitCost":"3.00"}])}else{serde_json::json!([])};
  let revised=serde_json::json!({"warehouseId":"warehouse","countDate":"2026-01-11","reason":"Revised actual count","expectedVersion":1,"lines":[{"productId":"count-product","countedQuantity":"3","unitCost":"3.00","allocations":allocations}]}).to_string();
  let race=||{let raw=revised.clone();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let authorizer=authorizer.clone();async move{authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"native-update-command",id:"count-native-update-create",raw:&raw},&environment()).await})}).await}};
  let (one,two)=tokio::join!(race(),race());for result in [one,two]{assert_eq!(result.unwrap().unwrap(),serde_json::json!({"id":"count-native-update-create","status":"draft","version":2}));}
  for _ in 0..2 {let raw=revised.clone();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"native-update-command",id:"count-native-update-create",raw:&raw},&environment()).await})}).await.unwrap().unwrap();assert_eq!(result,serde_json::json!({"id":"count-native-update-create","status":"draft","version":2}));}
  for (remove,restore) in [("UPDATE audit_events SET command_id='fixture-missing-audit' WHERE command_id='native-update-command'","UPDATE audit_events SET command_id='native-update-command' WHERE command_id='fixture-missing-audit'"),("UPDATE outbox_events SET command_id='fixture-missing-outbox' WHERE command_id='native-update-command'","UPDATE outbox_events SET command_id='native-update-command' WHERE command_id='fixture-missing-outbox'")] {
   sqlx::query(remove).execute(db).await.unwrap();let raw=revised.clone();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"native-update-command",id:"count-native-update-create",raw:&raw},&environment()).await})}).await;assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code=="SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"));sqlx::query(restore).execute(db).await.unwrap();
  }
  {let mut v:serde_json::Value=serde_json::from_str(&revised).unwrap();v["expectedVersion"]=serde_json::json!(2);let raw=v.to_string();let selection=p.review.reviewer_selection.clone();let proof=p.review.reviewer.clone();let clock=clock.clone();let denied=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"nonpreparer-update",id:"count-native-update-create",raw:&raw},&environment()).await})}).await;assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.kind==erpx_server_shared::public::FailureKind::Forbidden));}
  sqlx::query("UPDATE assignments SET role='count_maker' WHERE id='finance-fixture-b'").execute(db).await.unwrap();
  {let mut v:serde_json::Value=serde_json::from_str(&revised).unwrap();v["expectedVersion"]=serde_json::json!(2);let raw=v.to_string();let selection=p.review.reviewer_selection.clone();let proof=p.review.reviewer.clone();let clock=clock.clone();let denied=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"different-preparer",id:"count-native-update-create",raw:&raw},&environment()).await})}).await;assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code=="COUNT_LOCKED"));}
  sqlx::query("UPDATE assignments SET role='count_reviewer' WHERE id='finance-fixture-b'").execute(db).await.unwrap();
  sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode,item_type) VALUES('active-service-product','company','SERVICE-PRODUCT','Service product',0,'none','service')").execute(db).await.unwrap();
  for (key,raw,expected) in [
   ("nonstock-valuation",serde_json::json!({"warehouseId":"warehouse","countDate":"2026-01-11","reason":"Revised actual count","expectedVersion":999,"lines":[{"productId":"active-service-product","countedQuantity":"1"}]}).to_string(),"VALUATION_REQUIRED"),
   ("bad-product",serde_json::json!({"warehouseId":"warehouse","countDate":"2026-01-11","reason":"Revised actual count","expectedVersion":999,"lines":[{"productId":"missing-product","countedQuantity":"1","unitCost":"1"}]}).to_string(),"PRODUCT_INVALID"),
   ("duplicate-product",serde_json::json!({"warehouseId":"warehouse","countDate":"2026-01-11","reason":"Revised actual count","expectedVersion":999,"lines":[{"productId":"count-product","countedQuantity":"1","unitCost":"1"},{"productId":"count-product","countedQuantity":"1","unitCost":"1"}]}).to_string(),"DUPLICATE_PRODUCT"),
   ("wrong-warehouse",serde_json::json!({"warehouseId":"other","countDate":"2026-01-11","reason":"Revised actual count","expectedVersion":999,"lines":[{"productId":"count-product","countedQuantity":"1","unitCost":"1"},{"productId":"count-product","countedQuantity":"1","unitCost":"1"}]}).to_string(),"WAREHOUSE_LOCKED")
  ] {let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key,id:"count-native-update-create",raw:&raw},&environment()).await})}).await;assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code==expected),"{key}");}
  #[cfg(not(feature="field-service-http"))] let latest_version=2_i64;
  #[cfg(feature="field-service-http")]
  let latest_version={
   use erpx_server_commercial::public::purchases::current_package_catalogue;
   use erpx_service::{field_service_parts::FieldServicePartsEnvironment,field_service_reservation_read::{FieldServiceReservationReadConfiguration,FieldServiceReservationReadService},field_service_reservation_read_http::{field_service_reservation_read_router,FieldServiceReservationReadTarget,FieldServiceReadScopeHeaders}};
   use std::sync::Arc;use axum::http::{StatusCode,header::HeaderValue};use tower::ServiceExt;
   let central=account::login(&p.review.system,"a@example.test",None).await;let child=account::child(&p.review.system,&central,account::ACME).await;let now=erpx_server_shared::public::Clock::now(clock.as_ref()).get();sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a','company','field-fixture-a',3,8,$2,$3)").bind(&child.result.session_id).bind(now+3_600_000).bind(now).execute(db).await.unwrap();let cookie=format!("__Host-erpx-product={}",child.token.expose());
   let config=FieldServiceReservationReadConfiguration::new(FieldServicePartsEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap(),Arc::new(current_package_catalogue().unwrap())).unwrap().with_initial_count(CountBindingFeatures{enabled:true,read_only:false,uploads_frozen:false});let service=Arc::new(FieldServiceReservationReadService::new(authorizer.clone(),config,clock.clone()));let router=field_service_reservation_read_router(Arc::new(account::fixture::account::policy()),account::fixture::account::holding_registry(),Arc::new(FieldServiceReadScopeHeaders),vec![FieldServiceReservationReadTarget{host:"acme.example.test".into(),holding_slug:"acme".into(),service}]).unwrap();
   let path="/api/v1/holdings/acme/field-service/initial-physical-stock/count-native-update-create/update";let mut body:serde_json::Value=serde_json::from_str(&revised).unwrap();body["expectedVersion"]=serde_json::json!(2);body["reason"]=serde_json::json!("HTTP revised count");let body=body.to_string();
   let mut missing=count_http("POST",path,&cookie,Some("http-update-missing"),body.clone());missing.headers_mut().remove("x-erpx-context-assignment");assert_eq!(router.clone().oneshot(missing).await.unwrap().status(),StatusCode::CONFLICT);
   let mut foreign=count_http("POST",path,&cookie,Some("http-update-origin"),body.clone());foreign.headers_mut().insert("origin",HeaderValue::from_static("https://foreign.example.test"));assert_eq!(router.clone().oneshot(foreign).await.unwrap().status(),StatusCode::FORBIDDEN);
   let polls=Arc::new(std::sync::atomic::AtomicUsize::new(0));let held=axum::http::Request::builder().method("POST").uri(path).header("host","acme.example.test").header("idempotency-key","http-update-held").body(axum::body::Body::from_stream(tokio_util::codec::FramedRead::new(HeldBody(polls.clone()),tokio_util::codec::BytesCodec::new()))).unwrap();let denied=tokio::time::timeout(std::time::Duration::from_secs(5),router.clone().oneshot(held)).await.unwrap().unwrap();assert_eq!(denied.status(),StatusCode::UNAUTHORIZED);assert_eq!(polls.load(std::sync::atomic::Ordering::SeqCst),0);
   for _ in 0..2 {let (status,result)=count_http_response(&router,count_http("POST",path,&cookie,Some("http-native-update"),body.clone())).await;assert_eq!(status,StatusCode::OK,"{result}");assert_eq!(result["data"]["version"],3);}
   3_i64
  };
  for (key,mode,expected) in [("missing-version",0,"CONFLICT"),("valid-extra-line",1,"SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"),("changed-product",2,"SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED")] {
   let mut v:serde_json::Value=serde_json::from_str(&revised).unwrap();v["expectedVersion"]=serde_json::json!(latest_version);if mode==0{v.as_object_mut().unwrap().remove("expectedVersion");}else if mode==1{v["lines"].as_array_mut().unwrap().push(serde_json::json!({"productId":"active-service-product","countedQuantity":"1","unitCost":"1"}));}else{v["lines"]=serde_json::json!([{"productId":"active-service-product","countedQuantity":"1","unitCost":"1"}]);}let raw=v.to_string();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key,id:"count-native-update-create",raw:&raw},&environment()).await})}).await;assert!(matches!(result,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code==expected),"{key}");
  }
  sqlx::raw_sql("CREATE FUNCTION fixture_late_revision() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE audit_events SET actor_id='tampered' WHERE command_id='native-update-late'; RETURN NULL; END $$; CREATE TRIGGER fixture_late_revision AFTER DELETE ON command_assertions FOR EACH STATEMENT EXECUTE FUNCTION fixture_late_revision()").execute(db).await.unwrap();
  {let mut v:serde_json::Value=serde_json::from_str(&revised).unwrap();v["expectedVersion"]=serde_json::json!(latest_version);v["reason"]=serde_json::json!("Later revised count");let raw=v.to_string();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let denied=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"native-update-late",id:"count-native-update-create",raw:&raw},&environment()).await})}).await;assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code=="SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED"));}
  let late:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM inventory_counts WHERE id='count-native-update-create'),(SELECT count(*) FROM command_receipts WHERE command_id='native-update-late'),(SELECT count(*) FROM audit_events WHERE command_id='native-update-late'),(SELECT count(*) FROM outbox_events WHERE command_id='native-update-late')").fetch_one(db).await.unwrap();assert_eq!(late,(latest_version,0,0,0));sqlx::raw_sql("DROP TRIGGER fixture_late_revision ON command_assertions; DROP FUNCTION fixture_late_revision()").execute(db).await.unwrap();
  for (key,action,version,reviewer,status) in [("revision-submit",CountAction::Submit,latest_version,false,"submitted"),("revision-reject",CountAction::Reject,latest_version+1,true,"rejected")] {let input=normalize_initial_count_action(&serde_json::json!({"expectedVersion":version,"reason":"Review revised count"}).to_string()).unwrap();let selection=if reviewer{p.review.reviewer_selection.clone()}else{p.selection.clone()};let proof=if reviewer{p.review.reviewer.clone()}else{proof.clone()};let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let input=input.clone();Box::pin(async move{execute_service_count_action(c,current,clock.as_ref(),&mut Ids,ServiceCountActionCommand{key,id:"count-native-update-create",action,input:&input,files_features:CountBindingFeatures{enabled:true,read_only:false,uploads_frozen:false}},&environment()).await})}).await.unwrap().unwrap();assert_eq!(result["status"],status);}
  {let mut v:serde_json::Value=serde_json::from_str(&revised).unwrap();v["expectedVersion"]=serde_json::json!(latest_version+2);v["reason"]=serde_json::json!("Rework returned count");let raw=v.to_string();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"revision-after-reject",id:"count-native-update-create",raw:&raw},&environment()).await})}).await.unwrap().unwrap();assert_eq!(result["status"],"draft");assert_eq!(result["version"],latest_version+3);}
  let latest_version=latest_version+3;
  // Execute the original TypeScript generic owner itself, then ask Rust to
  // replay the actual committed legacy receipt on the same selected session.

  let mut body:serde_json::Value=serde_json::from_str(&revised).unwrap();body["expectedVersion"]=serde_json::json!(latest_version);body["reason"]=serde_json::json!("Actual legacy update");let raw=body.to_string();let child_input=serde_json::json!({"database":db_name,"actor":actor.clone(),"id":"count-native-update-create","key":"legacy-real-update","body":body});
  {use std::io::Write;use std::process::{Command,Stdio};let bun=std::env::var("ERPX_BUN_BIN").expect("pinned Bun required");let mut child=Command::new(bun).current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.." )).arg("scripts/testing/fixtures/service-count-legacy-update.ts").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("legacy writer process");child.stdin.take().unwrap().write_all(child_input.to_string().as_bytes()).unwrap();let result=child.wait_with_output().unwrap();assert!(result.status.success(),"legacy writer failed: {}",String::from_utf8_lossy(&result.stderr));let result:serde_json::Value=serde_json::from_slice(&result.stdout).unwrap();assert_eq!(result["version"],latest_version+1);}
  {let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"legacy-real-update",id:"count-native-update-create",raw:&raw},&environment()).await})}).await.unwrap().unwrap();assert_eq!(result["version"],latest_version+1);}
  let latest_version=latest_version+1;
  let legacy_flow_input;
  // Genuine generic TS Flow assignment, including its scoped default, must
  // replay from the native coordinator without appending a second default.
  {
   use erpx_service::service_initial_physical_stock::{assign_generic_service_count_flow, normalize_generic_stock_count_assignment, ServiceCountAssignmentCommand};
   let flow_version:i64=sqlx::query_scalar("SELECT version FROM flow_routes WHERE company_id='company' AND kind='stock_count' AND record_id='count-native-update-create'").fetch_one(db).await.unwrap();
   let body=serde_json::json!({"stepKey":"submitted","userId":"fixture-b","expectedVersion":flow_version,"recordVersion":latest_version,"reason":"Assign actual review","saveDefault":true,"ignored":"stripped"});
   let child_input=serde_json::json!({"database":db_name,"actor":actor,"id":"count-native-update-create","key":"legacy-flow-default","body":body,"mode":"flow"});
   {use std::io::Write;use std::process::{Command,Stdio};let bun=std::env::var("ERPX_BUN_BIN").expect("pinned Bun required");let mut child=Command::new(bun).current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).arg("scripts/testing/fixtures/service-count-legacy-update.ts").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();child.stdin.take().unwrap().write_all(child_input.to_string().as_bytes()).unwrap();let result=child.wait_with_output().unwrap();assert!(result.status.success(),"legacy flow failed: {}",String::from_utf8_lossy(&result.stderr));let result:serde_json::Value=serde_json::from_slice(&result.stdout).unwrap();assert_eq!(result["version"],flow_version+1);}
   let input=normalize_generic_stock_count_assignment(&body.to_string()).unwrap();legacy_flow_input=input.clone();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let input=input.clone();Box::pin(async move{assign_generic_service_count_flow(c,current,clock.as_ref(),&mut Ids,ServiceCountAssignmentCommand{key:"legacy-flow-default",id:"count-native-update-create",input:&input},&environment()).await})}).await.unwrap().unwrap();assert_eq!(result["version"],flow_version+1);
   {use erpx_server_inventory::public::service_count_outbox::{SERVICE_COUNT_OUTBOX_ELIGIBLE,SERVICE_COUNT_OUTBOX_QUARANTINE};let mut q=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT (");q.push(SERVICE_COUNT_OUTBOX_ELIGIBLE).push("),(").push(SERVICE_COUNT_OUTBOX_QUARANTINE).push(") FROM outbox_events e WHERE e.command_id=").push_bind("legacy-flow-default");let flow:(bool,bool)=q.build_query_as().fetch_one(db).await.unwrap();assert_eq!(flow,(true,true));}
   // Race native lease against the actual legacy claim predicate for the same
   // committed stock_count event. The legacy worker must never own this row.
   {
    const OWNED: &str=concat!("(",include_str!("../../../packages/server-inventory/src/public/service_count_outbox_quarantine.sql"),") AND e.command_id='legacy-flow-default'");
    let native=async {let mut c=db.acquire().await.unwrap();erpx_server_workflow::public::service_count_outbox::claim(&mut c,OWNED,"fixture-claim",1_000_000).await.unwrap()};
    let legacy=async {let mut q=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT e.id FROM outbox_events e WHERE e.command_id='legacy-flow-default' AND NOT ");q.push(erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_QUARANTINE);q.build_query_scalar::<String>().fetch_optional(db).await.unwrap()};
    let (claimed,other)=tokio::join!(native,legacy);assert!(claimed.is_some());assert!(other.is_none());
    let event=claimed.unwrap();let mut c=db.acquire().await.unwrap();assert!(erpx_server_workflow::public::service_count_outbox::eligible(&mut c,erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_ELIGIBLE,&event).await.unwrap());
    // Release this fixture lease so later assertions retain their original state.
    assert!(erpx_server_workflow::public::service_count_outbox::fail(&mut c,&event,1_000_000,"FIXTURE_RELEASE").await.unwrap());
   }

   // Race the actual Bun claimant against native on the retained Flow event.
   // Other fixture events are delayed only within this disposable database.
   {
    sqlx::query("UPDATE outbox_events SET next_attempt_at=CASE WHEN command_id='legacy-flow-default' THEN 0 ELSE 2000000 END").execute(db).await.unwrap();
    let (ready,started)=tokio::sync::oneshot::channel();let (release,go)=std::sync::mpsc::channel::<()>();let payload=serde_json::json!({"database":db_name});
    let live=tokio::task::spawn_blocking(move||{use std::io::{BufRead,BufReader,Read,Write};use std::process::{Command,Stdio};let mut child=Command::new(std::env::var("ERPX_BUN_BIN").unwrap()).current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../..")).arg("scripts/testing/fixtures/service-count-job-claim.ts").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();let mut input=child.stdin.take().unwrap();writeln!(input,"{}",payload).unwrap();input.flush().unwrap();let mut output=BufReader::new(child.stdout.take().unwrap());let mut line=String::new();output.read_line(&mut line).unwrap();assert_eq!(line,"READY\n");ready.send(()).unwrap();go.recv().unwrap();writeln!(input,"GO").unwrap();input.flush().unwrap();drop(input);let mut body=String::new();output.read_to_string(&mut body).unwrap();assert!(child.wait().unwrap().success());serde_json::from_str::<serde_json::Value>(&body).unwrap()});
    started.await.unwrap();release.send(()).unwrap();
    let mut c=db.acquire().await.unwrap();let claimed=erpx_server_workflow::public::service_count_outbox::claim(&mut c,erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_QUARANTINE,"native-claim",1_000_000).await.unwrap().unwrap();
    assert_eq!(live.await.unwrap()["id"],serde_json::Value::Null);
    assert!(erpx_server_workflow::public::service_count_outbox::eligible(&mut c,erpx_server_inventory::public::service_count_outbox::SERVICE_COUNT_OUTBOX_ELIGIBLE,&claimed).await.unwrap());
    assert!(erpx_server_workflow::public::service_count_outbox::fail(&mut c,&claimed,1_000_000,"FIXTURE_RELEASE").await.unwrap());
   }
   // Project and acknowledge the real eligible Flow on distinct leased databases.
   {
    sqlx::query("UPDATE outbox_events SET next_attempt_at=CASE WHEN command_id='legacy-flow-default' THEN 0 ELSE 2000000 END").execute(db).await.unwrap();
    let write=p.pool().clone();
    account::database::with_database(2,move|read|async move{
     let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
     let migrations=erpx_server_runtime::public::postgres::load_migrations(root,erpx_server_runtime::public::postgres::MigrationTrack::HoldingRead).await.unwrap();
     erpx_server_runtime::public::postgres::migrate_database(&read,erpx_server_runtime::public::postgres::MigrationTrack::HoldingRead,&migrations).await.unwrap();
     assert!(erpx_service::service_count_worker::run_once(write.raw(),read.raw(),"native-projection",1_000_000).await.unwrap());
     let (id,processed):(String,Option<i64>)=sqlx::query_as("SELECT id,processed_at FROM outbox_events WHERE command_id='legacy-flow-default'").fetch_one(write.raw()).await.unwrap();assert!(processed.is_some());
     let count:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id=$1").bind(&id).fetch_one(read.raw()).await.unwrap();assert_eq!(count,1);
     let checkpoint:String=sqlx::query_scalar("SELECT last_event_id FROM projection_checkpoints WHERE projector='work-items'").fetch_one(read.raw()).await.unwrap();assert_eq!(checkpoint,id);
    }).await;
   }
   let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM flow_defaults WHERE company_id='company' AND kind='stock_count'),(SELECT count(*) FROM flow_events WHERE command_id='legacy-flow-default')").fetch_one(db).await.unwrap();assert_eq!(counts,(if cfg!(feature="field-service-http"){2}else{1},1));
  }
  {let input=normalize_initial_count_action(&serde_json::json!({"expectedVersion":latest_version,"reason":"Cancel after revised count"}).to_string()).unwrap();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let input=input.clone();Box::pin(async move{execute_service_count_action(c,current,clock.as_ref(),&mut Ids,ServiceCountActionCommand{key:"native-update-later-cancel",id:"count-native-update-create",action:CountAction::Cancel,input:&input,files_features:CountBindingFeatures{enabled:true,read_only:false,uploads_frozen:false}},&environment()).await})}).await.unwrap().unwrap();assert_eq!(result["status"],"cancelled");}
  {let raw=revised.clone();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let result=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"native-update-command",id:"count-native-update-create",raw:&raw},&environment()).await})}).await.unwrap().unwrap();assert_eq!(result["version"],2);}
  {let mut tx=db.begin().await.unwrap();sqlx::query("UPDATE company_package_grants SET state='draining',version=version+1,reason='Synthetic drain' WHERE company_id='company' AND package_id='supply'").execute(&mut *tx).await.unwrap();sqlx::query("INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'supply-drain-fixture',company_id,package_id,'fixture-a','supply-drain',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),2 FROM company_package_grants WHERE company_id='company' AND package_id='supply'").execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();}
  {let raw=revised.clone();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let denied=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let raw=raw.clone();Box::pin(async move{execute_service_initial_count_update(c,current,clock.as_ref(),&mut Ids,ServiceCountUpdateCommand{key:"native-update-command",id:"count-native-update-create",raw:&raw},&environment()).await})}).await;assert!(matches!(denied,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.kind==erpx_server_shared::public::FailureKind::Forbidden));}
  {use erpx_service::service_initial_physical_stock::{assign_generic_service_count_flow,ServiceCountAssignmentCommand};let input=legacy_flow_input.clone();let selection=p.selection.clone();let proof=proof.clone();let clock=clock.clone();let denied=authorizer.execute_with_selected_ingress_deferred("acme.example.test",proof,move||Ok(selection),(access(),sources()),|_|Ok::<_,BusinessError>(()),move|c,current,_|{let clock=clock.clone();let input=input.clone();Box::pin(async move{assign_generic_service_count_flow(c,current,clock.as_ref(),&mut Ids,ServiceCountAssignmentCommand{key:"legacy-flow-default",id:"count-native-update-create",input:&input},&environment()).await})}).await;assert!(denied.is_err(),"saveDefault replay must recheck current Supply new-work");}
  let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT version FROM inventory_counts WHERE id='count-native-update-create'),(SELECT count(*) FROM command_receipts WHERE command_id='native-update-command'),(SELECT count(*) FROM audit_events WHERE command_id='native-update-command'),(SELECT count(*) FROM outbox_events WHERE command_id='native-update-command')").fetch_one(db).await.unwrap();assert_eq!(counts,(latest_version+1,1,1,1));
 }).await;
    }
}

#[tokio::test]
async fn utf16_replay_identity_is_distinct_while_postgres_binding_matches_node() {
    use erpx_service::service_initial_physical_stock_update::normalize_service_count_update;
    let raw = |unit: &str| {
        format!(
            r#"{{"warehouseId":"w","countDate":"2026-01-01","reason":"valid reason","lines":[{{"productId":"{unit}","countedQuantity":"0"}}]}}"#
        )
    };
    let high = normalize_service_count_update(&raw(r"\ud800")).unwrap();
    let low = normalize_service_count_update(&raw(r"\udc00")).unwrap();
    assert_ne!(high.normalized, low.normalized);
    assert!(high.contains_unpaired_surrogate && low.contains_unpaired_surrogate);
    account::database::with_database(2, move |pool| async move {
        for value in [
            &high.lines[0].product_id,
            &low.lines[0].product_id,
            "\u{fffd}",
        ] {
            let hex: String =
                sqlx::query_scalar("SELECT encode(convert_to($1::text,'UTF8'),'hex')")
                    .bind(value)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(hex, "efbfbd");
        }
        let nul = normalize_service_count_update(&raw(r"\u0000")).unwrap();
        let err = sqlx::query("SELECT $1::text")
            .bind(&nul.lines[0].product_id)
            .execute(pool.raw())
            .await
            .unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("22021")
        );
    })
    .await;
}
