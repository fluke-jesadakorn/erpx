#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_customer_authority::public::portal_support_commands::{CloseInput,PgPortalSupportCommands,RespondInput,SupportActor};
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_runtime::public::postgres::{DatabasePool,MigrationTrack,PgTransactionError,PgUnitOfWork,TransactionOptions,load_migrations,migrate_database};
use erpx_server_shared::public::BusinessError;
const NOW:i64=1_800_000_000_000;const COMPANY:&str="company";
type Error=PgTransactionError<BusinessError>;
fn code(e:&Error)->&'static str{match e{PgTransactionError::Business(e)=>e.code,PgTransactionError::Persistence(_)=>"PERSISTENCE"}}
fn actor(user:&str)->SupportActor{SupportActor{user_id:user.into(),company_id:COMPANY.into(),branch_id:Some("branch".into()),department_id:None,assignment_id:format!("assignment-{user}"),role:"sales_manager".into(),session_id:None,assignment_version:None,permissions:vec!["commerce.read".into(),"commerce.sales".into(),"commerce.approve".into()]}}
async fn migrate(pool:&DatabasePool){let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(pool,MigrationTrack::HoldingWrite,&files).await.unwrap();}
async fn seed(pool:&DatabasePool){
 sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES('company','company','company','Company',NULL,'company',NULL,'active',1),('branch','branch','branch','Branch',NULL,'company',NULL,'active',1);\
 INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES('company','creator','local:creator','active','local',1),('company','manager','local:manager','active','local',1),('company','closer','local:closer','active','local',1);\
 INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,scope_kind,scope_node_id,source) VALUES('assignment-creator','creator','sales_manager','[]','company','branch',0,9000000000000,NULL,'holding',NULL,'manual'),('assignment-manager','manager','sales_manager','[]','company','branch',0,9000000000000,NULL,'holding',NULL,'manual'),('assignment-closer','closer','sales_manager','[]','company','branch',0,9000000000000,NULL,'holding',NULL,'manual');\
 INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES('party-1','company','customer','C1','Customer','THB',30,0,'active',1,1,1);\
 INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES('warehouse-1','company','branch','WH1','Warehouse','active',1);\
 INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,source_id,version,created_by,approved_by,created_at,updated_at) VALUES('order-1','company','branch',NULL,'quote','Q-1','party-1','warehouse-1','THB','2026-11-01','2026-11-08','','confirmed',1000,0,NULL,3,'creator','creator',100,300);\
 INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,reviewed_at,version,created_at,updated_at) VALUES('binding-1','company','order-1','party-1','customer@example.invalid',1800000600000,'approved','Seeded approval','creator','manager',100,2,100,100);\
 INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) VALUES('binding-1','customer','customer@example.invalid',100);\
 INSERT INTO customer_portal_actions(id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,request_key,request_hash,status,created_at) VALUES('request-1','company','binding-1','order-1','party-1','customer','request_support','Original customer request',3,'customer-request-key-0001','0','recorded',100);").execute(pool.raw()).await.unwrap();
}
async fn run<T,F>(pool:&DatabasePool,production:bool,action:F)->Result<T,Error> where T:Send+'static,F:for<'c> Fn(&'c mut PgPortalSupportCommands<'c>)->std::pin::Pin<Box<dyn std::future::Future<Output=Result<T,Error>>+Send+'c>>+Send+Clone+'static{PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(),move|connection|{let action=action.clone();Box::pin(async move{let mut repo=PgPortalSupportCommands::new(connection,PackageEnforcementMode::Disabled,production);action(&mut repo).await})}).await}
#[tokio::test]
async fn independent_response_replay_and_closure(){
 fixture::with_database(4,|pool| async move{migrate(&pool).await;seed(&pool).await;let manager=actor("manager");
 let receipt=run(&pool,false,{let manager=manager.clone();move|repo|{let manager=manager.clone();Box::pin(async move{repo.publish(&manager,"request-1",RespondInput{expected_binding_version:2,expected_response_version:0,message:"Independent reviewed response".into(),reason:"Reviewed the customer request".into()},"portal-support-0001",NOW).await})}}).await.unwrap();
 assert_eq!(receipt.status,"published");assert_eq!(receipt.version,1);
 eprintln!("receipts={:?}", sqlx::query_scalar::<_,i64>("SELECT count(*) FROM command_receipts WHERE command_id='portal-support-0001'").fetch_one(pool.raw()).await);
 let replay=run(&pool,false,{let manager=manager.clone();move|repo|{let manager=manager.clone();Box::pin(async move{repo.publish(&manager,"request-1",RespondInput{expected_binding_version:2,expected_response_version:0,message:"Independent reviewed response".into(),reason:"Reviewed the customer request".into()},"portal-support-0001",NOW).await})}}).await.unwrap();
 assert_eq!(replay,receipt);
 let independence=run(&pool,false,move|repo|Box::pin(async move{repo.publish(&actor("creator"),"request-1",RespondInput{expected_binding_version:2,expected_response_version:1,message:"Second response".into(),reason:"Try as preparer".into()},"portal-support-0002",NOW).await})).await.unwrap_err();
 assert_eq!(code(&independence),"INDEPENDENT_PORTAL_RESPONDER_REQUIRED");
 sqlx::query("UPDATE customer_access_bindings SET status='revoked',version=version+1 WHERE id='binding-1'").execute(pool.raw()).await.unwrap();
 let closed=run(&pool,false,{let closer=actor("closer");move|repo|{let closer=closer.clone();Box::pin(async move{repo.close(&closer,"request-1",CloseInput{expected_binding_version:3,expected_response_version:1,reason:"Customer cannot acknowledge the response".into()},"portal-support-close-0001",NOW).await})}}).await.unwrap();
 assert_eq!(closed.status,"closed_without_customer_acknowledgement");
 assert_eq!(closed.version,2);
 }).await;
}
