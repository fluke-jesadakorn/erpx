#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_identity_access::public::release_reviewers::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations,
    migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, RecordId, UserId};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status)VALUES('company','company','acme','Synthetic',NULL,'company','active');INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status)VALUES('*','custom_release_reviewer','Synthetic reviewer',false,1,'active');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_release_reviewer',1,'[\"legal.manage\"]','[]','[\"holding\"]','fixture',0);INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind)VALUES('grant','reviewer','custom_release_reviewer','[\"legal\"]','company',0,8000000000000000,1,'holding')").execute(pool.raw()).await.unwrap();
}
fn request() -> ReleaseReviewerRequest {
    ReleaseReviewerRequest {
        company_id: CompanyId::new("company").unwrap(),
        assignment_id: RecordId::new("grant").unwrap(),
        reviewer_id: UserId::new("reviewer").unwrap(),
        requested_by: UserId::new("requester").unwrap(),
        permission: "legal.manage".into(),
    }
}
async fn allowed(pool: &DatabasePool, request: ReleaseReviewerRequest) -> bool {
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let q = ReleaseReviewerRequest {
                company_id: request.company_id.clone(),
                assignment_id: request.assignment_id.clone(),
                reviewer_id: request.reviewer_id.clone(),
                requested_by: request.requested_by.clone(),
                permission: request.permission.clone(),
            };
            Box::pin(async move {
                lock_release_reviewer_sources(c).await?;
                PgReleaseReviewerReader::new(c).inspect(&q).await
            })
        })
        .await
        .unwrap()
        .allowed
}
#[tokio::test]
async fn original_review_is_bound_to_current_company_holding_assignment_permission_and_person() {
    fixture::with_database(2,|pool|async move{
 setup(&pool).await;assert!(allowed(&pool,request()).await);
 let mut q=request();q.requested_by=q.reviewer_id.clone();assert!(!allowed(&pool,q).await);
 let mut q=request();q.company_id=CompanyId::new("foreign").unwrap();assert!(!allowed(&pool,q).await);
 let mut q=request();q.permission="organization.manage".into();assert!(!allowed(&pool,q).await);
 sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('person','reviewer','company','Synthetic','TH','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('engagement','person','company','employee','salaried','active','2026-01-01','Synthetic','THB','fixture',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy)VALUES('company','reviewer','local:reviewer','person','active','local'),('company','requester','local:requester','person','suspended','local')").execute(pool.raw()).await.unwrap();
 // The reviewer still has an active principal; a suspended alias for the same
 // person nevertheless cannot independently accept that person's release.
 assert!(sqlx::query_scalar::<_,bool>("SELECT iam_principal_active('reviewer','company',floor(extract(epoch FROM clock_timestamp())*1000)::bigint)").fetch_one(pool.raw()).await.unwrap());assert!(!allowed(&pool,request()).await);
}).await;
}
#[tokio::test]
async fn current_reviewer_revoked_after_earlier_snapshot_retries_then_denies() {
    fixture::with_database(2,|pool|async move{
 setup(&pool).await;let attempts=Arc::new(AtomicUsize::new(0));let observed=attempts.clone();let writer=pool.clone();
 let result=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),move|c|{let attempts=attempts.clone();let writer=writer.clone();Box::pin(async move{if attempts.fetch_add(1,Ordering::SeqCst)==0{sqlx::query("SELECT id FROM assignments WHERE id='grant'").execute(&mut *c).await?;sqlx::query("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='grant'").execute(writer.raw()).await?;}lock_release_reviewer_sources(c).await?;PgReleaseReviewerReader::new(c).inspect(&request()).await})}).await.unwrap();
 assert!(!result.allowed);assert_eq!(observed.load(Ordering::SeqCst),2);assert_eq!(result.revision.len(),64);
}).await;
}

#[tokio::test]
async fn stock_count_flow_participants_match_typescript_without_size_caps() {
    use erpx_server_identity_access::public::service_count_flow_participants::{inspect_stock_count_flow_assignments,lock_stock_count_flow_assignment_sources,service_count_flow_active_members,service_count_flow_reviewers};
    fixture::with_database(2,|pool|async move {
        setup(&pool).await;
        sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','flow-test','Flow reviewer',false,1,'active'),('*','flow-no-role','No flow',false,1,'active'),('*','flow-owner','Owner only',false,1,'active'),('*','flow-both','Both roles',false,1,'active');
            INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','flow-test',1,'[\"workflow.finance_approver\"]','[]','[\"holding\",\"branch\"]','fixture',0),('*','flow-no-role',1,'[]','[]','[\"holding\"]','fixture',0),('*','flow-owner',1,'[\"workflow.holding_owner\"]','[]','[\"holding\"]','fixture',0),('*','flow-both',1,'[\"workflow.finance_approver\",\"workflow.holding_owner\"]','[]','[\"holding\"]','fixture',0);
            INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) SELECT 'flow-person-'||n,CASE WHEN n=1 THEN repeat('é',101) ELSE 'flow-user-'||n END,'company','Person','TH','fixture',0,0 FROM generate_series(1,501)n;
            INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) SELECT 'flow-engagement-'||n,'flow-person-'||n,'company','employee','salaried','active','2000-01-01','Worker','THB','fixture',0,0 FROM generate_series(1,501)n;
            INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) SELECT 'flow-assignment-'||n,CASE WHEN n=1 THEN repeat('é',101) ELSE 'flow-user-'||n END,'flow-test','[]','company',0,8000000000000000,1,'holding' FROM generate_series(1,501)n;
            INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('flow-owner-person','aa-owner','company','Owner','TH','fixture',0,0),('flow-both-person','zz-both','company','Both','TH','fixture',0,0);
            INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('flow-owner-engagement','flow-owner-person','company','employee','salaried','active','2000-01-01','Owner','THB','fixture',0,0),('flow-both-engagement','flow-both-person','company','employee','salaried','active','2000-01-01','Both','THB','fixture',0,0);
            INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('flow-owner-grant','aa-owner','flow-owner','[]','company',0,8000000000000000,1,'holding'),('flow-both-grant','zz-both','flow-both','[]','company',0,8000000000000000,1,'holding');
            INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('suspended-person','suspended','company','Suspended','TH','fixture',0,0),('norole-person','norole','company','No role','TH','fixture',0,0);
            INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('suspended-engagement','suspended-person','company','employee','salaried','active','2000-01-01','Worker','THB','fixture',0,0),('norole-engagement','norole-person','company','employee','salaried','active','2000-01-01','Worker','THB','fixture',0,0);
            INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy) VALUES('company','suspended','local:suspended','suspended','local');
            INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind,revoked_at) VALUES('suspended-grant','suspended','flow-test','[]','company',0,8000000000000000,1,'holding',NULL),('norole-grant','norole','flow-no-role','[]','company',0,8000000000000000,1,'holding',NULL),('revoked-grant','revoked','flow-test','[]','company',0,8000000000000000,1,'holding',1),('expired-grant','expired','flow-test','[]','company',0,1,1,'holding',NULL)")
            .execute(pool.raw()).await.unwrap();
        let db:String=sqlx::query_scalar("SELECT current_database()").fetch_one(pool.raw()).await.unwrap();
        let script=r#"
if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');
import pg from 'pg';import './packages/server-runtime/src/adapters/runtime/postgres.ts';
import {createFlowRoutingSourceRepository} from './packages/server-identity_access/src/adapters/workflow-flow-routing-facts.ts';
import {workflowStaffRoleUsers} from './packages/server-identity_access/src/adapters/workflow-role-facts.ts';
const u=new URL(process.env.ERPX_TEST_ADMIN_URL);u.pathname='/'+process.env.COUNT_TEST_DB;const c=new pg.Client({connectionString:u.toString()});await c.connect();
const db={prepare(sql){return{bind(...args){return{async all(){let i=0;const q=sql.replace(/\?/g,()=>'$'+(++i));return{results:(await c.query(q,args)).rows}}}}}}};
const now=Date.now();const members=(await createFlowRoutingSourceRepository(db).readReadFlowMembers('company',now,now)).results.map(r=>r.user_id).sort();
const scope={company_id:'company',branch_id:null,department_id:null};const finance=await workflowStaffRoleUsers(db,scope,'finance_approver');const owner=await workflowStaffRoleUsers(db,scope,'holding_owner');const reviewers=[...new Set([...finance,...owner])];
process.stdout.write(JSON.stringify({members,reviewers}));await c.end();
"#;
        let oracle=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required"))
          .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
          .args(["--conditions=react-server","-e",script]).env("COUNT_TEST_DB",db).output().unwrap();
        assert!(oracle.status.success(),"TS Flow oracle failed: {}",String::from_utf8_lossy(&oracle.stderr));
        let oracle:serde_json::Value=serde_json::from_slice(&oracle.stdout).unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
        lock_stock_count_flow_assignment_sources(&mut tx).await.unwrap();
        let now:i64=sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint").fetch_one(&mut *tx).await.unwrap();
        let company=CompanyId::new("company").unwrap();
        let projection_script=r#"if(Bun.version!=='1.4.3')throw Error('PINNED_BUN_REQUIRED');const {finalRootOwnerProjectionSources:s}=await import('./packages/server-identity_access/src/adapters/final-root-owner-projection-sources.ts');process.stdout.write(s.assignments);"#;
        let projection=std::process::Command::new(std::env::var("ERPX_BUN_BIN").expect("pinned Bun required")).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).args(["--conditions=react-server","-e",projection_script]).output().unwrap();assert!(projection.status.success());let ts_sql=String::from_utf8(projection.stdout).unwrap();
        let witness=inspect_stock_count_flow_assignments(&mut tx,&company,now).await.unwrap();
        {
            let mut bound=witness.bind_projection(&mut tx).await.unwrap();
            assert_eq!(bound.assignments_sql(),ts_sql.replace("FROM assignments","FROM public.assignments"));
            let native=format!("SELECT row_to_json(t)::text FROM ({}) t WHERE company_id=$1 ORDER BY id",bound.assignments_sql());
            let legacy=format!("SELECT row_to_json(t)::text FROM ({ts_sql}) t WHERE company_id=$1 ORDER BY id");
            let actual:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(native)).bind("company").fetch_all(bound.connection()).await.unwrap();
            let expected:Vec<String>=sqlx::query_scalar(sqlx::AssertSqlSafe(legacy)).bind("company").fetch_all(bound.connection()).await.unwrap();assert_eq!(actual,expected);
            bound.finish().await.unwrap();
        }
        let members=service_count_flow_active_members(&mut tx,&company,now).await.unwrap();
        let mut expected:Vec<String>=serde_json::from_value(oracle["members"].clone()).unwrap(); expected.sort();let mut actual=members.clone();actual.sort();assert_eq!(actual,expected);
        assert!(members.len()>500);assert!(members.iter().any(|v|v.len()>200&&v.encode_utf16().count()<=200));assert!(!members.contains(&"suspended".into()));assert!(!members.contains(&"revoked".into()));assert!(!members.contains(&"expired".into()));assert!(members.contains(&"norole".into()));
        let staff:Vec<String>=sqlx::query_scalar("SELECT user_id FROM hr_people WHERE company_id='company' ORDER BY user_id").fetch_all(&mut *tx).await.unwrap();
        let reviewers=service_count_flow_reviewers(&mut tx,&company,&staff,"flow-user-2","flow-user-3",now).await.unwrap();
        let expected:Vec<String>=serde_json::from_value(oracle["reviewers"].clone()).unwrap();let expected:Vec<_>=expected.into_iter().filter(|v|v!="flow-user-2"&&v!="flow-user-3").collect();assert_eq!(reviewers,expected);
        assert_eq!(reviewers.iter().filter(|v|v.as_str()=="zz-both").count(),1);
        assert!(reviewers.iter().position(|v|v=="aa-owner").unwrap()>reviewers.iter().position(|v|v=="zz-both").unwrap(),"owner-only comes after finance pool");
        assert!(!reviewers.contains(&"norole".into()));assert!(!reviewers.contains(&"suspended".into()));
        sqlx::query("SAVEPOINT flow_identity_drift").execute(&mut *tx).await.unwrap();
        let mut bound=witness.bind_projection(&mut tx).await.unwrap();
        sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy) VALUES('company','flow-user-4','local:flow-user-4','suspended','local')").execute(bound.connection()).await.unwrap();
        assert!(bound.finish().await.is_err(),"changed IAM predicate must invalidate bound source");
        sqlx::query("ROLLBACK TO SAVEPOINT flow_identity_drift").execute(&mut *tx).await.unwrap();witness.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE assignments SET revoked_at=1 WHERE id='flow-assignment-4'").execute(&mut *tx).await.unwrap();
        assert_ne!(service_count_flow_active_members(&mut tx,&company,now).await.unwrap(),members);
        assert_ne!(service_count_flow_reviewers(&mut tx,&company,&staff,"flow-user-2","flow-user-3",now).await.unwrap(),reviewers);
        sqlx::query("ROLLBACK TO SAVEPOINT flow_identity_drift").execute(&mut *tx).await.unwrap();
        assert_eq!(service_count_flow_active_members(&mut tx,&company,now).await.unwrap(),members);
        assert_eq!(service_count_flow_reviewers(&mut tx,&company,&staff,"flow-user-2","flow-user-3",now).await.unwrap(),reviewers);
        tx.rollback().await.unwrap();
        let mut other=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await.unwrap();lock_stock_count_flow_assignment_sources(&mut other).await.unwrap();assert!(witness.bind_projection(&mut other).await.is_err());other.rollback().await.unwrap();
        let mut lost=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *lost).await.unwrap();sqlx::query("SAVEPOINT before_identity_fences").execute(&mut *lost).await.unwrap();lock_stock_count_flow_assignment_sources(&mut lost).await.unwrap();let retained=inspect_stock_count_flow_assignments(&mut lost,&company,now).await.unwrap();sqlx::query("ROLLBACK TO SAVEPOINT before_identity_fences").execute(&mut *lost).await.unwrap();assert!(retained.require_current(&mut lost).await.is_err());lost.rollback().await.unwrap();
    }).await;
}
