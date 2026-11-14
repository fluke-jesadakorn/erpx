//! Registered Holding Service Count/Parts Company isolation via real Account and SSO.
#[allow(dead_code)]
#[expect(unused_imports, reason = "Shared real Account fixture helpers")]
#[path = "../../support/field_service/parts.rs"]
mod facts;
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_shared::public::{Clock,CompanyId,HoldingId};
use erpx_service::{field_service_parts::FieldServicePartsEnvironment,field_service_reservation_read::{FieldServiceReservationReadConfiguration,FieldServiceReservationReadService},field_service_reservation_read_http::{field_service_reservation_read_router,FieldServiceReservationReadTarget,FieldServiceReadScopeHeaders},service_initial_physical_stock::CountBindingFeatures};
use facts::*;
use std::sync::Arc;
use tower::ServiceExt;

fn request(path:&str,cookie:&str,company:&str,assignment:&str,revision:&str)->Request<Body>{
 Request::builder().method("GET").uri(path).header("host","acme.example.test").header("cookie",cookie)
 .header("x-erpx-context-company",company).header("x-erpx-context-assignment",assignment)
 .header("x-erpx-context-revision",revision).header("x-erpx-context-assignment-version","3").body(Body::empty()).unwrap()
}
async fn call(router:&axum::Router,request:Request<Body>)->(StatusCode,serde_json::Value){
 let response=router.clone().oneshot(request).await.unwrap();let status=response.status();
 let bytes=to_bytes(response.into_body(),2*1024*1024).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap())
}
#[tokio::test]
async fn selected_account_isolates_two_companies_and_rejects_stale_or_foreign_context(){
 account::with_system(|system| async move {
  let p=setup_workspace(system).await;let db=p.pool().raw();let now=p.review.system.scenario.clock.now().get();
  sqlx::raw_sql(r#"
   INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company-b','company','company-b','Second synthetic Company',NULL,'company-b',NULL,'active');
   INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','count_company_test','Synthetic Count operator',false,1,'active');
   INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','count_company_test',1,'["inventory.read","inventory.prepare","inventory.approve","finance.read","finance.write","finance.approve","organization.manage","module.inventory.read","module.field_service.read","module.finance.read","workflow.warehouse_manager","workflow.finance_approver"]','[]','["holding","branch"]','fixture',0);
   UPDATE assignments SET role='count_company_test',valid_from=0 WHERE id='field-fixture-a';
   INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES('company-b-person',NULL,'company-b','Second Company Person','TH','active','fixture',0,0);
   INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('company-b-engagement','company-b-person','company-b','employee','salaried','active','2020-01-01','Operator','THB','fixture',0,0);
   INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) VALUES('company-b','fixture-a','local:fixture-a','company-b-person','active','local',1);
   INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,version,reason,evidence,updated_at) SELECT 'company-b',id,1,'active',0,1,'Synthetic Company B grant','fixture',0 FROM unnest(ARRAY['service','service-parts-assets','supply','finance']) id;
   INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) SELECT 'company-b-grant-'||package_id,company_id,package_id,'fixture-a','company-b-grant',NULL,jsonb_build_object('companyId',company_id,'packageId',package_id,'packageVersion',package_version,'state',state,'startsAt',starts_at,'endsAt',ends_at,'version',version,'reason',reason,'evidence',evidence),0 FROM company_package_grants WHERE company_id='company-b';
   INSERT INTO commerce_products(id,company_id,sku,name,price_minor,tracking_mode) VALUES('company-b-product','company-b','SECOND-ONLY','Second Company Product',0,'none');
  "#).execute(db).await.unwrap();
  sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('company-b-assignment','fixture-a','count_company_test','[\"field_service\"]','company-b',$1,$2,3,'holding')").bind(now-1000).bind(now+3_600_000).execute(db).await.unwrap();
  let central=account::login(&p.review.system,"a@example.test",None).await;let child=account::child(&p.review.system,&central,account::ACME).await;
  for (company,assignment) in [("company","field-fixture-a"),("company-b","company-b-assignment")] {
   sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES($1,'fixture-a',$2,$3,3,8,$4,$5)").bind(&child.result.session_id).bind(company).bind(assignment).bind(now+3_600_000).bind(now).execute(db).await.unwrap();
  }
  let cookie=format!("__Host-erpx-product={}",child.token.expose());
  let config=FieldServiceReservationReadConfiguration::new(FieldServicePartsEnvironment::Development,HoldingId::new("h001").unwrap(),CompanyId::new("company").unwrap(),Arc::new(current_package_catalogue().unwrap())).unwrap()
   .with_parts_posting_setup().with_stock_setup().with_parts_prerequisites().with_initial_count(CountBindingFeatures{enabled:true,read_only:false,uploads_frozen:false});
  let service=Arc::new(FieldServiceReservationReadService::new(p.review.authorizer.clone(),config,p.review.system.scenario.clock.clone()));
  let router=field_service_reservation_read_router(Arc::new(account::fixture::account::policy()),account::fixture::account::holding_registry(),Arc::new(FieldServiceReadScopeHeaders),vec![FieldServiceReservationReadTarget{host:"acme.example.test".into(),holding_slug:"acme".into(),service}]).unwrap();
  let base="/api/v1/holdings/acme/field-service";
  for (company,assignment) in [("company","field-fixture-a"),("company-b","company-b-assignment")] {
   for suffix in ["initial-physical-stock","parts-setup","parts-setup/prerequisites","parts-setup/stock"] {
    let (status,result)=call(&router,request(&format!("{base}/{suffix}"),&cookie,company,assignment,"8")).await;
    assert_eq!(status,StatusCode::OK,"{company} {suffix}: {result}");assert_eq!(result["context"]["companyId"],company);
    let text=result.to_string();if company=="company" {assert!(!text.contains("SECOND-ONLY"));} else {assert!(!text.contains("SYNTHETIC"));}
   }
  }
  for (company,assignment,key) in [("company","field-fixture-a","company-a-chart"),("company-b","company-b-assignment","company-b-chart")] {
  let (_,prerequisites)=call(&router,request(&format!("{base}/parts-setup/prerequisites"),&cookie,company,assignment,"8")).await;
  let context=prerequisites["data"]["contextKey"].as_str().expect("selected Company prerequisites must expose a review context");
  let (resource,input)=if company=="company" {("periods",serde_json::json!({"name":"Company A new period","startDate":"2032-01-01","endDate":"2032-12-31"}))} else {("charts",serde_json::json!({"name":"Synthetic new chart","baseCurrency":"THB"}))};
  let body=serde_json::json!({"expectedVersion":0,"contextKey":context,"input":input}).to_string();
  let mut create=request(&format!("{base}/parts-setup/{resource}"),&cookie,company,assignment,"8");*create.method_mut()=axum::http::Method::POST;create.headers_mut().insert("origin",axum::http::HeaderValue::from_static("https://acme.example.test"));create.headers_mut().insert("idempotency-key",axum::http::HeaderValue::from_str(key).unwrap());*create.body_mut()=Body::from(body);
  let (status,created)=call(&router,create).await;assert_eq!(status,StatusCode::OK,"{company} chart: {created}");assert_eq!(created["context"]["companyId"],company);
  let chart_id=created["data"]["id"].as_str().unwrap();let query=if company=="company" {"SELECT company_id FROM finance_periods WHERE id=$1"} else {"SELECT company_id FROM finance_charts WHERE id=$1"};let actual_company:String=sqlx::query_scalar(query).bind(chart_id).fetch_one(db).await.unwrap();assert_eq!(actual_company,company);
  }
  // Synthetic committed Service mapping supplies prerequisites for a real Company B Count command.
  let chart:String=sqlx::query_scalar("SELECT id FROM finance_charts WHERE company_id='company-b' LIMIT 1").fetch_one(db).await.unwrap();
  for (id,code,category,control) in [("b-ar","111","asset","receivable"),("b-ap","211","liability","payable"),("b-revenue","411","revenue","none"),("b-inventory","121","asset","none"),("b-clearing","221","liability","none"),("b-cogs","511","expense","none"),("b-input","131","asset","none"),("b-output","231","liability","none"),("b-cash","141","asset","bank"),("b-variance","5400","expense","none")] {
   sqlx::query("INSERT INTO finance_accounts(id,company_id,chart_id,code,name,category,currency,control_kind,created_at,updated_at) VALUES($1,'company-b',$2,$3,$1,$4,'THB',$5,0,0)").bind(id).bind(&chart).bind(code).bind(category).bind(control).execute(db).await.unwrap();
  }
  sqlx::query("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('b-period','company-b','Synthetic January','2026-01-01','2026-01-31',0)").execute(db).await.unwrap();
  sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('b-branch','branch','b-branch','Second Company Branch','company-b','company-b','b-branch','active')").execute(db).await.unwrap();
  sqlx::query("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES('b-warehouse','company-b','b-branch','BWH','Second Company Warehouse')").execute(db).await.unwrap();
  sqlx::query("INSERT INTO commerce_settings(company_id,chart_id,currency,receivable_account_id,payable_account_id,revenue_account_id,inventory_account_id,clearing_account_id,cogs_account_id,input_tax_account_id,output_tax_account_id,cash_account_id) VALUES('company-b',$1,'THB','b-ar','b-ap','b-revenue','b-inventory','b-clearing','b-cogs','b-input','b-output','b-cash')").bind(&chart).execute(db).await.unwrap();
  let mut tx=db.begin().await.unwrap();
  sqlx::raw_sql(r#"INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) SELECT 'b-setup-key','commerce.service-parts-posting-setup.confirm-existing','fixture-a','synthetic-b-hash','company-b',jsonb_build_object('id','company-b','status','confirmed-existing','version',1,'_servicePartsPostingSetup',jsonb_build_object('kind','service-parts-posting-setup','companyId','company-b','commandId','b-setup-key','mode','confirm-existing','mapping',to_jsonb(s)))::text,0,'{"companyId":"company-b"}' FROM commerce_settings s WHERE company_id='company-b';
  INSERT INTO commerce_service_parts_posting_sources(company_id,command_id,mode,mapping_json,proof_json,created_by,created_at) SELECT 'company-b','b-setup-key','confirm-existing',to_jsonb(s),jsonb_build_object('kind','service-parts-posting-setup','companyId','company-b','commandId','b-setup-key','mode','confirm-existing','mapping',to_jsonb(s)),'fixture-a',0 FROM commerce_settings s WHERE company_id='company-b'"#).execute(&mut *tx).await.unwrap();tx.commit().await.unwrap();
  let selection=format!("{base}/initial-physical-stock?warehouseId=b-warehouse&productId=company-b-product&countDate=2026-01-10");
  let (status,view)=call(&router,request(&selection,&cookie,"company-b","company-b-assignment","8")).await;assert_eq!(status,StatusCode::OK,"Company B Count selection: {view}");
  let context=view["data"]["contextKey"].as_str().expect("Company B selected Count context");
  let body=serde_json::json!({"expectedVersion":0,"contextKey":context,"input":{"warehouseId":"b-warehouse","countDate":"2026-01-10","reason":"Synthetic Company B physical count","lines":[{"productId":"company-b-product","countedQuantity":"1","unitCost":"1"}]}}).to_string();
  let mut create=request(&format!("{base}/initial-physical-stock"),&cookie,"company-b","company-b-assignment","8");*create.method_mut()=axum::http::Method::POST;create.headers_mut().insert("origin",axum::http::HeaderValue::from_static("https://acme.example.test"));create.headers_mut().insert("idempotency-key",axum::http::HeaderValue::from_static("company-b-count"));*create.body_mut()=Body::from(body);
  let (status,result)=call(&router,create).await;assert_eq!(status,StatusCode::CREATED,"Company B Count create: {result}");assert_eq!(result["context"]["companyId"],"company-b");let count_id=result["data"]["id"].as_str().unwrap();
  let owner:String=sqlx::query_scalar("SELECT company_id FROM inventory_counts WHERE id=$1").bind(count_id).fetch_one(db).await.unwrap();assert_eq!(owner,"company-b");
  let marker:i64=sqlx::query_scalar("SELECT count(*) FROM inventory_service_initial_physical_sources WHERE count_id=$1 AND company_id='company-b'").bind(count_id).fetch_one(db).await.unwrap();assert_eq!(marker,1);
  let (status,_)=call(&router,request(&format!("{base}/initial-physical-stock/{count_id}"),&cookie,"company","field-fixture-a","8")).await;assert_ne!(status,StatusCode::OK,"foreign marked Count must not be readable");
  let mut foreign=request(&format!("{base}/initial-physical-stock/{count_id}/submit"),&cookie,"company","field-fixture-a","8");*foreign.method_mut()=axum::http::Method::POST;foreign.headers_mut().insert("origin",axum::http::HeaderValue::from_static("https://acme.example.test"));foreign.headers_mut().insert("idempotency-key",axum::http::HeaderValue::from_static("foreign-count-submit"));*foreign.body_mut()=Body::from(r#"{"expectedVersion":1}"#);
  assert_ne!(call(&router,foreign).await.0,StatusCode::OK);let foreign_receipts:i64=sqlx::query_scalar("SELECT count(*) FROM command_receipts WHERE command_id='foreign-count-submit'").fetch_one(db).await.unwrap();assert_eq!(foreign_receipts,0);
  for (company,assignment,revision) in [("company-b","field-fixture-a","8"),("company-b","company-b-assignment","9")] {
   let (status,_)=call(&router,request(&format!("{base}/initial-physical-stock"),&cookie,company,assignment,revision)).await;assert_ne!(status,StatusCode::OK);
  }
  let mut absent=request(&format!("{base}/initial-physical-stock"),&cookie,"company-b","company-b-assignment","8");absent.headers_mut().remove("x-erpx-context-assignment-version");assert_ne!(call(&router,absent).await.0,StatusCode::OK);
  let wrong=request("/api/v1/holdings/northstar/field-service/initial-physical-stock",&cookie,"company-b","company-b-assignment","8");assert_ne!(call(&router,wrong).await.0,StatusCode::OK);
  sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='company-b-assignment'").bind(now).execute(db).await.unwrap();
  assert_ne!(call(&router,request(&format!("{base}/initial-physical-stock"),&cookie,"company-b","company-b-assignment","8")).await.0,StatusCode::OK);
 }).await;
}
