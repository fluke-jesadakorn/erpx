#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_identity_access::public::work_context::*;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CompanyId, HoldingId, RecordId, TimestampMillis, UserId,
};
struct FixedClock(i64);
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0)
    }
}
fn request(now: i64) -> WorkContextRequest {
    WorkContextRequest {
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_slug: Some("north".into()),
        department_slug: Some("engineering".into()),
        session_id: RecordId::new("session").unwrap(),
        user_id: UserId::new("user").unwrap(),
        session_expires_at: now + 3600000,
        expected: Some(ExpectedWorkSelection {
            revision: 7,
            assignment_id: AssignmentId::new("grant").unwrap(),
            assignment_version: 3,
        }),
        mutation: true,
    }
}
async fn seed_selection(pool: &erpx_server_runtime::public::postgres::DatabasePool, now: i64) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','acme','Acme',NULL,'company',NULL,'active'),('branch','branch','north','North','company','company','branch','active'),('department','department','engineering','Engineering','branch','company','branch','active'); INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','custom_rust_context','Finance preparer',true,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_rust_context',1,'[\"finance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0),('*','custom_rust_context',2,'[\"finance.read\"]','[]','[\"department\"]','fixture',0);").execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id) VALUES('grant','user','custom_rust_context','[\"finance\"]','company','branch',$1,$2,3,'branch','branch')").bind(now-1000).bind(now+3600000).execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES('session','user','company','grant',3,7,$1,$2)").bind(now+3600000).bind(now).execute(pool.raw()).await.unwrap();
}
#[tokio::test]
async fn selected_grant_inspection_rechecks_database_lineage_role_and_rendered_revision() {
    fixture::with_database(2,|pool|async move{
    let now:i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(pool.raw()).await.unwrap();
    seed_selection(&pool,now).await;
    let uow=PgUnitOfWork::new(pool.clone());
    let current=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move|conn|Box::pin(async move{let mut port=PgWorkContextReader::new(HoldingId::new("h001").unwrap(),conn);inspect_current_work_selection(&mut port,&FixedClock(now),&request(now)).await})).await.unwrap();
    assert_eq!(current.assignment().id.as_str(),"grant");assert_eq!(current.role_revision(),1);
    for change in 0..5{
       let result=uow.run::<_,BusinessError,_>(TransactionOptions::default(),move|conn|Box::pin(async move{
         match change {
          0=>{sqlx::query("UPDATE organization_nodes SET status='suspended' WHERE id='branch'").execute(&mut *conn).await?;}
          1=>{sqlx::query("UPDATE active_work_contexts SET revision=8 WHERE session_id='session'").execute(&mut *conn).await?;}
          2=>{sqlx::query("UPDATE assignments SET revoked_at=$1 WHERE id='grant'").bind(now).execute(&mut *conn).await?;}
          3=>{sqlx::query("UPDATE iam_roles SET published_revision=2 WHERE id='custom_rust_context'").execute(&mut *conn).await?;}
          _=>{sqlx::query("INSERT INTO iam_revoked_sessions(session_id,company_id,user_id,revoked_at) VALUES('session','company','user',$1)").bind(now).execute(&mut *conn).await?;}
         }
         let mut port=PgWorkContextReader::new(HoldingId::new("h001").unwrap(),conn);inspect_current_work_selection(&mut port,&FixedClock(now),&request(now)).await
       })).await;
       assert!(matches!(result,Err(PgTransactionError::Business(_))),"source change{change} unexpectedly accepted: {result:?}");
    }
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT revision FROM active_work_contexts WHERE session_id='session'").fetch_one(pool.raw()).await.unwrap(),7,"failed authority checks roll back tentative owner changes");
 }).await;
}

#[tokio::test]
async fn retained_control_proof_fences_parent_switch_until_the_holding_snapshot_finishes() {
    use erpx_server_identity_access::public::{
        AccountPolicy, DeviceProof, HoldingOrigin, SecretDigest,
    };
    use std::{sync::Arc, time::Duration};
    fixture::with_database(3,|control|async move{
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let files=load_migrations(root,MigrationTrack::Control).await.unwrap();migrate_database(&control,MigrationTrack::Control,&files).await.unwrap();
    let now:i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(control.raw()).await.unwrap();
    sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") VALUES('user','Synthetic','user@example.test',1,$1,$1)").bind(now).execute(control.raw()).await.unwrap();
    for(id,digest)in[("parent","b".repeat(64)),("session","a".repeat(64))]{
       sqlx::query("INSERT INTO session(id,\"userId\",token,\"expiresAt\",\"createdAt\",\"updatedAt\") VALUES($1,'user',$2,to_timestamp(($3::bigint+3600000)::double precision/1000)::text,to_timestamp($3::double precision/1000)::text,to_timestamp($3::double precision/1000)::text)").bind(id).bind(digest).bind(now).execute(control.raw()).await.unwrap();
    }
    sqlx::query("INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES('parent','account','development',NULL,$1,$2),('session','holding:development:h001:acme.example.test','development','parent',$1,$2)").bind(now).bind(now+3600000).execute(control.raw()).await.unwrap();
    sqlx::query("INSERT INTO account_device_browsers(id,active_session_id,revision,updated_at) VALUES('browser','parent',1,$1)").bind(now).execute(control.raw()).await.unwrap();
    sqlx::query("INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES('parent','browser','saved-parent')").execute(control.raw()).await.unwrap();
    fixture::with_database(2,move|holding|async move{
       seed_selection(&holding,now).await;
       let policy=AccountPolicy{account_origin:"https://account.example.test".into(),account_host:"account.example.test".into(),environment:"development".into(),require_email_verification:true,holdings:vec![HoldingOrigin{origin:"https://acme.example.test".into(),host:"acme.example.test".into(),audience:"holding:development:h001:acme.example.test".into()}]};
       use erpx_server_identity_access::public::account_holding_bindings::{ConfiguredAccountHolding,ConfiguredHoldingRegistry};
       let registry=Arc::new(ConfiguredHoldingRegistry::new(&policy,vec![ConfiguredAccountHolding{holding_id:HoldingId::new("h001").unwrap(),slug:"acme".into(),write_binding:"HOLDING_H001_DB".into(),read_binding:"HOLDING_H001_READ_DB".into(),files_binding:"HOLDING_H001_FILES".into()}]).unwrap());
       sqlx::query("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES('h001','acme','Synthetic','HOLDING_H001_DB','HOLDING_H001_READ_DB','HOLDING_H001_FILES','active')").execute(control.raw()).await.unwrap();
       let inspector=Arc::new(HoldingWorkContextInspector::new(control.clone(),holding.clone(),HoldingId::new("h001").unwrap(),policy,Arc::new(FixedClock(now)),registry).unwrap());
       let proof=DeviceProof{active:Some(SecretDigest::new("a".repeat(64)).unwrap()),..Default::default()};
       let input=HoldingWorkspaceRequest{company_id:CompanyId::new("company").unwrap(),branch_slug:Some("north".into()),department_slug:Some("engineering".into()),expected:request(now).expected,mutation:true};
       let mut blocker=holding.raw().acquire().await.unwrap();sqlx::raw_sql("BEGIN; LOCK TABLE assignments IN EXCLUSIVE MODE").execute(&mut *blocker).await.unwrap();
       let inspect={let inspector=inspector.clone();let proof=proof.clone();let input=input.clone();tokio::spawn(async move{inspector.inspect("acme.example.test",proof,input).await})};
       let mut retained=false;
       for _ in 0..100{if sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation='session'::regclass AND mode='ShareLock' AND granted)").fetch_one(control.raw()).await.unwrap(){retained=true;break;}tokio::time::sleep(Duration::from_millis(10)).await;}
       assert!(retained,"Control verification lease was not retained while Holding waited");
       let mut switch={let control=control.clone();tokio::spawn(async move{sqlx::query("UPDATE account_device_browsers SET active_session_id=NULL WHERE id='browser'").execute(control.raw()).await.unwrap();})};
       assert!(tokio::time::timeout(Duration::from_millis(100),&mut switch).await.is_err(),"parent switch passed the retained Control fence");
       sqlx::query("COMMIT").execute(&mut *blocker).await.unwrap();drop(blocker);
       let selection=inspect.await.unwrap().unwrap();assert_eq!(selection.assignment().id.as_str(),"grant");
       tokio::time::timeout(Duration::from_secs(3),switch).await.unwrap().unwrap();
       holding.close().await;
       assert!(matches!(inspector.inspect("acme.example.test",proof.clone(),input.clone()).await,Err(PgTransactionError::Business(ref e)) if e.code=="UNAUTHENTICATED"),"a parent switch must deny before reacquiring the Holding pool");
       control.close().await;
       assert!(matches!(inspector.inspect("acme.example.test.attacker.test",proof,input).await,Err(PgTransactionError::Business(ref e)) if e.code=="WORKSPACE_NOT_FOUND"),"foreign host must deny before either pool");
    }).await;
 }).await;
}
