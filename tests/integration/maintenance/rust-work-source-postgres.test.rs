#[allow(
    dead_code,
    reason = "Shared actual retained Account/Holding fixture also serves Identity tests"
)]
#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use auth::*;
use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
use erpx_server_maintenance::public::{MaintenanceTransition, work_source::*};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{BusinessError, RecordId, TimestampMillis};
async fn source(pool: &DatabasePool) {
    sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at)VALUES('worker','user','company','PRIVATE_WORKER','TH','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at)VALUES('engagement','worker','company','employee','salaried','active','2026-01-01','PRIVATE_JOB','THB','fixture',0,0);INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy)VALUES('company','user','local:user','worker','active','local');INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',2,'[\"records.write\",\"records.review\",\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_native_authority';UPDATE assignments SET modules_json='[\"maintenance\"]'WHERE id='grant';INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('asset','company','branch','SYNTHETIC','PRIVATE_ASSET','synthetic','THB','fixture',0,0);INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at)VALUES('work','company','branch','asset',repeat('PRIVATE_TITLE',200000),'normal','in_progress','user','user',0,0)").execute(pool.raw()).await.unwrap();
}
fn maintenance() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "records.write".into(),
        module: Some("maintenance".into()),
        destination: "assets/maintenance/work".into(),
        api: true,
    }
}
fn id() -> RecordId {
    RecordId::new("work").unwrap()
}
fn error<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    assert!(
        matches!(result,Err(PgTransactionError::Business(ref e))if e.code==code),
        "expected {code}"
    );
}
#[tokio::test]
async fn real_retained_work_reads_minimal_owner_source_and_preserves_policy_then_version_order() {
    fixture::with_database(2, |control| async move {
        fixture::with_database(2, move |holding| async move {
            let (authorizer, _) = setup(&control, &holding).await;
            source(&holding).await;
            authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    input(),
                    maintenance(),
                    MaintenanceWorkSourcePreparation,
                    |c, current| {
                        Box::pin(async move {
                            let s = read_current_maintenance_work(
                                c,
                                current,
                                &id(),
                                TimestampMillis::new(NOW),
                            )
                            .await?
                            .unwrap();
                            assert_eq!(s.facts().id, id());
                            assert_eq!(s.facts().branch_id.as_str(), "branch");
                            assert!(!format!("{:?}", s.facts()).contains("PRIVATE_"));
                            s.require_action(&MaintenanceTransition::IssuePart)?;
                            s.require_expected_version(1)?;
                            assert_eq!(
                                s.require_expected_version(2).unwrap_err().code,
                                "MAINTENANCE_STATE_CHANGED"
                            );
                            s.require_current(c, TimestampMillis::new(NOW)).await?;
                            assert!(
                                read_current_maintenance_work(
                                    c,
                                    current,
                                    &RecordId::new("missing")?,
                                    TimestampMillis::new(NOW)
                                )
                                .await?
                                .is_none()
                            );
                            Ok(())
                        })
                    },
                )
                .await
                .unwrap();
            // An eligible independent reviewer may act for another owner, but only with
            // the selected current records.review AND that reviewer's People reference.
            sqlx::query(
                "UPDATE maintenance_work_orders SET owner_id='unlinked-owner'WHERE id='work'",
            )
            .execute(holding.raw())
            .await
            .unwrap();
            authorizer
                .execute_with_sources(
                    "acme.example.test",
                    proof(),
                    input(),
                    maintenance(),
                    MaintenanceWorkSourcePreparation,
                    |c, current| {
                        Box::pin(async move {
                            let s = read_current_maintenance_work(
                                c,
                                current,
                                &id(),
                                TimestampMillis::new(NOW),
                            )
                            .await?
                            .unwrap();
                            s.require_action(&MaintenanceTransition::IssuePart)?;
                            Ok(())
                        })
                    },
                )
                .await
                .unwrap();
        })
        .await;
    })
    .await;
}
#[tokio::test]
async fn current_worker_expiry_and_selected_review_permission_cannot_be_replaced_by_boolean_labels()
{
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
  sqlx::query("UPDATE maintenance_work_orders SET owner_id='unlinked-owner',status='open'WHERE id='work'").execute(holding.raw()).await.unwrap();
  sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',3,'[\"records.write\",\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
  error(authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move{let s=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();s.require_action(&MaintenanceTransition::IssuePart)?;Ok(())})).await,"MAINTENANCE_OWNER_REQUIRED");
  sqlx::raw_sql("UPDATE maintenance_work_orders SET owner_id='user',status='in_progress'WHERE id='work';UPDATE hr_engagements SET end_date='2026-01-02'WHERE id='engagement'").execute(holding.raw()).await.unwrap();
  error(authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move{let s=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();s.require_action(&MaintenanceTransition::IssuePart)?;Ok(())})).await,"MAINTENANCE_OWNER_REQUIRED");
 }).await;}).await;
}
#[tokio::test]
async fn final_source_check_refuses_local_mutation_backward_time_and_expired_retained_context() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
  authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance(),MaintenanceWorkSourcePreparation,move|c,current|{let holding=holding.clone();Box::pin(async move{
   let s=read_current_maintenance_work(c,current,&id(),TimestampMillis::new(NOW)).await?.unwrap();let mut other=holding.raw().begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await?;assert!(s.require_current(&mut other,TimestampMillis::new(NOW)).await.is_err());other.rollback().await?;assert!(s.require_current(c,TimestampMillis::new(NOW-1)).await.is_err());assert!(s.require_current(c,TimestampMillis::new(current.expires_at())).await.is_err());sqlx::query("UPDATE maintenance_work_orders SET version=version+1 WHERE id='work'").execute(&mut *c).await?;assert!(s.require_current(c,TimestampMillis::new(NOW)).await.is_err());Ok(())
  })}).await.unwrap();
 }).await;}).await;
}
#[tokio::test]
async fn actual_selected_branch_and_physical_asset_relationship_cannot_be_supplied_by_request() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;source(&holding).await;
  sqlx::raw_sql("INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES('foreign','company','other-branch','OTHER','Other','synthetic','THB','fixture',0,0),('foreign-2','company','other-branch','OTHER2','Other second','synthetic','THB','fixture',0,0);INSERT INTO maintenance_work_orders(id,company_id,branch_id,asset_id,title,priority,status,owner_id,created_by,created_at,updated_at)VALUES('other-work','company','other-branch','foreign','Other','normal','in_progress','user','user',0,0),('mismatched','company','branch','foreign-2','Mismatch','normal','in_progress','user','user',0,0)").execute(holding.raw()).await.unwrap();
  let mut branch=input();branch.branch_slug=Some("north".into());
  authorizer.execute_with_sources("acme.example.test",proof(),branch.clone(),maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move{assert!(read_current_maintenance_work(c,current,&RecordId::new("other-work")?,TimestampMillis::new(NOW)).await?.is_none());Ok(())})).await.unwrap();
  error(authorizer.execute_with_sources("acme.example.test",proof(),branch,maintenance(),MaintenanceWorkSourcePreparation,|c,current|Box::pin(async move{let _=read_current_maintenance_work(c,current,&RecordId::new("mismatched")?,TimestampMillis::new(NOW)).await?;Ok(())})).await,"MAINTENANCE_STATE_CHANGED");
 }).await;}).await;
}
