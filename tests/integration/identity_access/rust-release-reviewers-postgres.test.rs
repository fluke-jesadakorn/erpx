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
