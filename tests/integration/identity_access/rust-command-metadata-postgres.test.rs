#[allow(
    dead_code,
    reason = "shared real Account/SSO fixture exposes helpers for multiple suites"
)]
#[path = "../../support/rust-account-queued-authority.rs"]
mod fixture;
#[path = "../../support/rust-directory-provenance-facts.rs"]
mod source_facts;
use erpx_server_identity_access::public::{
    command_metadata::{project_command_iam, project_queued_command_iam},
    effective_authority::AuthorizedWorkContext,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::BusinessError;
use serde_json::Value;
use sqlx::Row;

// Captured by executing unchanged B0 createIamAuthority on pinned Node26.11.1,
// application/authority.ts SHA97ab001c31ea1b2c... with the same independent facts.
const LOCAL: &str = r#"{"personUserIds":["fixture-a"],"emergencyUntil":null,"roleRevision":1,"identityVersion":0,"permissions":["legal.read","module.legal.read"],"grantablePermissions":[],"principalId":"local:fixture-a","personId":null,"freshUntil":null}"#;
const DIRECTORY: &str = r#"{"personUserIds":["fixture-a","alias-a","fixture-b"],"emergencyUntil":null,"roleRevision":1,"identityVersion":7,"permissions":["legal.read","module.legal.read"],"grantablePermissions":[],"principalId":"directory:fixture-a","personId":"person","freshUntil":1800000299000}"#;
const EMERGENCY: &str = r#"{"personUserIds":["fixture-a"],"emergencyUntil":1800000010000,"roleRevision":1,"identityVersion":2,"permissions":["legal.read","module.legal.read"],"grantablePermissions":[],"principalId":"local:fixture-a","personId":null,"freshUntil":null}"#;
const HR: &str = r#"{"personUserIds":["fixture-a"],"emergencyUntil":null,"roleRevision":1,"identityVersion":0,"permissions":["legal.read","module.legal.read","people.read","people.manage","leave.policy.manage","module.people_payroll.read"],"hrAppointmentVersion":2,"grantablePermissions":[],"principalId":"local:fixture-a","personId":"person","freshUntil":null}"#;
fn encode(
    context: &AuthorizedWorkContext,
    queued: bool,
) -> Result<String, PgTransactionError<BusinessError>> {
    Ok(serde_json::to_string(&if queued {
        project_queued_command_iam(context)?
    } else {
        project_command_iam(context)?
    })
    .unwrap())
}
async fn projection(
    prepared: &fixture::PreparedAccountQueuedAuthority,
    queued: bool,
) -> Result<(String, i64), PgTransactionError<BusinessError>> {
    prepared
        .authorizer
        .execute(
            "acme.example.test",
            prepared.proof.clone(),
            prepared.selection.clone(),
            prepared.access.clone(),
            move |_, context| {
                Box::pin(async move { Ok((encode(context, queued)?, context.expires_at())) })
            },
        )
        .await
}
async fn directory(prepared: &fixture::PreparedAccountQueuedAuthority) {
    let pool = &prepared.system.holdings[0];
    sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('person','fixture-a','company','Synthetic Person','TH','fixture',0,0);INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES('metadata-connection','company','Synthetic Directory','univention','{\"mode\":\"ldaps\",\"secretProfile\":\"synthetic\",\"authority\":\"synthetic approved source facts\"}','active','fixture',0); INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,connection_id,identity_id,version) VALUES('company','fixture-a','directory:fixture-a','person','active','directory','metadata-connection','metadata-identity',7),('company','alias-a','local:alias-a','person','active','local',NULL,NULL,1),('company','fixture-b','local:fixture-b','person','active','local',NULL,NULL,1)").execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at,user_id,person_id) VALUES('metadata-identity','company','metadata-connection','synthetic-external','synthetic','Synthetic Directory User',true,$1,'fixture-a','person')").bind(fixture::account::fixture::account::NOW-1000).execute(pool.raw()).await.unwrap();
    source_facts::approve_synthetic_source(
        pool,
        "metadata-connection",
        "company",
        fixture::account::fixture::account::NOW,
    )
    .await;
    source_facts::stamp_synthetic_observation(
        pool,
        "metadata-connection",
        "company",
        fixture::account::fixture::account::NOW - 1000,
    )
    .await;
}

#[tokio::test]
async fn real_local_projection_matches_node_null_omission_order_and_queued_version_append() {
    fixture::account::with_system(|system|async move{
        let prepared=fixture::prepare(system).await;
        let (interactive,_)=projection(&prepared,false).await.unwrap();assert_eq!(interactive,LOCAL);
        let (queued,_)=projection(&prepared,true).await.unwrap();assert_eq!(queued,format!("{},\"queuedAssignmentVersion\":3}}",&LOCAL[..LOCAL.len()-1]));
        let node:Value=serde_json::from_str(&interactive).unwrap();assert!(node.get("hrAppointmentVersion").is_none());assert!(node.get("queuedAssignmentVersion").is_none());
        // Projection runs on the original borrowed backend; no checkout/query
        // exists in its portable implementation or its serializable data DTO.
        prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),|connection,context|Box::pin(async move{
            let before:(String,i32,String)=sqlx::query_as("SELECT current_database()::text,pg_backend_pid(),pg_current_xact_id()::text").fetch_one(&mut *connection).await?;
            assert_eq!(encode(context,false)?,LOCAL);
            let after:(String,i32,String)=sqlx::query_as("SELECT current_database()::text,pg_backend_pid(),pg_current_xact_id()::text").fetch_one(connection).await?;assert_eq!(before,after);Ok(())
        })).await.unwrap();
    }).await;
}

#[tokio::test]
async fn published_permission_and_grantable_vectors_keep_original_order_and_duplicates() {
    fixture::account::with_system(|system|async move{
        let prepared=fixture::prepare(system).await;let pool=&prepared.system.holdings[0];
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_queued_legal',2,'[\"module.legal.read\",\"legal.read\",\"legal.read\",\"legal.configure\"]','[\"legal.configure\",\"legal.read\"]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=2 WHERE id='custom_queued_legal'").execute(pool.raw()).await.unwrap();
        let (encoded,_)=projection(&prepared,false).await.unwrap();let data:Value=serde_json::from_str(&encoded).unwrap();
        assert_eq!(data["roleRevision"],2);assert_eq!(data["permissions"],serde_json::json!(["module.legal.read","legal.read","legal.read","legal.configure"]));assert_eq!(data["grantablePermissions"],serde_json::json!(["legal.configure","legal.read"]));
    }).await;
}

#[tokio::test]
async fn directory_aliases_and_exact_freshness_are_actual_facts_not_the_shorter_actor_expiry() {
    fixture::account::with_system(|system| async move {
        let prepared = fixture::prepare(system).await;
        directory(&prepared).await;
        sqlx::query("UPDATE active_work_contexts SET expires_at=$2 WHERE session_id=$1")
            .bind(&prepared.child_id)
            .bind(fixture::account::fixture::account::NOW + 1000)
            .execute(prepared.system.holdings[0].raw())
            .await
            .unwrap();
        let (data, expires) = projection(&prepared, false).await.unwrap();
        assert_eq!(data, DIRECTORY);
        assert_eq!(expires, fixture::account::fixture::account::NOW + 1000);
        prepared.system.scenario.clock.advance(300_000);
        assert!(
            projection(&prepared, false).await.is_err(),
            "stale source must not manufacture a new metadata snapshot"
        );
    })
    .await;
}

#[tokio::test]
async fn emergency_wire_fresh_until_is_null_while_actual_authorization_keeps_its_deadline() {
    fixture::account::with_system(|system|async move{
        let prepared=fixture::prepare(system).await;let pool=&prepared.system.holdings[0];let until=fixture::account::fixture::account::NOW+10_000;
        sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES('company','fixture-a','local:fixture-a','active','emergency',2)").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO iam_emergency_sessions(session_id,company_id,user_id,expires_at,reason) VALUES($1,'company','fixture-a',$2,'Synthetic reviewed elevation')").bind(&prepared.child_id).bind(until).execute(pool.raw()).await.unwrap();
        let (data,expires)=projection(&prepared,false).await.unwrap();assert_eq!(data,EMERGENCY);assert_eq!(expires,until);
    }).await;
}

#[tokio::test]
async fn current_hr_appointment_changes_the_real_permissions_and_only_then_emits_its_version() {
    fixture::account::with_system(|system|async move{
        let prepared=fixture::prepare(system).await;let pool=&prepared.system.holdings[0];
        sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('person','fixture-a','company','Synthetic Person','TH','fixture',0,0);INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('metadata-engagement','person','company','employee','salaried','active','2026-01-01','Synthetic HR','THB','fixture',0,0);INSERT INTO authority_seats(company_id,kind,scope_id,version,person_id,effective_at) VALUES('company','hr','company',2,'person',0) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=excluded.version,person_id=excluded.person_id,effective_at=excluded.effective_at").execute(pool.raw()).await.unwrap();
        assert_eq!(projection(&prepared,false).await.unwrap().0,HR);
        sqlx::query("UPDATE hr_engagements SET status='suspended',version=version+1 WHERE id='metadata-engagement'").execute(pool.raw()).await.unwrap();
        let (data,_)=projection(&prepared,false).await.unwrap();let data:Value=serde_json::from_str(&data).unwrap();assert!(data.get("hrAppointmentVersion").is_none());assert_eq!(data["permissions"],serde_json::json!(["legal.read","module.legal.read"]));
    }).await;
}

#[tokio::test]
async fn unsafe_javascript_metadata_numbers_are_rejected_instead_of_rounded_or_defaulted() {
    fixture::account::with_system(|system|async move{
        let prepared=fixture::prepare(system).await;let pool=&prepared.system.holdings[0];
        let unsafe_until=9_007_199_254_740_992_i64;
        // Keep real session/selection/source admission valid so this case
        // reaches the independent metadata wire-number guard.
        sqlx::query("INSERT INTO iam_members(company_id,user_id,principal_id,status,login_policy,version) VALUES('company','fixture-a','local:fixture-a','active','emergency',2)").execute(pool.raw()).await.unwrap();
        sqlx::query("INSERT INTO iam_emergency_sessions(session_id,company_id,user_id,expires_at,reason) VALUES($1,'company','fixture-a',$2,'Synthetic unsafe metadata boundary')").bind(&prepared.child_id).bind(unsafe_until).execute(pool.raw()).await.unwrap();
        let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let actual=reached.clone();
        let outcome=prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),move|_,context|{
            actual.store(true,std::sync::atomic::Ordering::SeqCst);assert_eq!(context.emergency_until(),Some(unsafe_until));assert!(context.expires_at()<unsafe_until);
            Box::pin(async move {encode(context,false).map(|_|())})
        }).await;
        let code=match &outcome {Err(PgTransactionError::Business(error))=>error.code,Err(PgTransactionError::Persistence(_))=>"PERSISTENCE_ERROR",Ok(_)=>"UNEXPECTED_ADMISSION"};
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst),"actual live context must reach the metadata projector");
        assert!(matches!(&outcome,Err(PgTransactionError::Business(error)) if error.code=="COMMAND_IAM_METADATA_UNAVAILABLE"),"actual sanitized denial: {code}");
    }).await;
}

#[tokio::test]
async fn future_directory_observation_denies_before_any_command_metadata_projection() {
    fixture::account::with_system(|system|async move{
        let prepared=fixture::prepare(system).await;directory(&prepared).await;
        sqlx::query("UPDATE iam_identities SET observed_at=$1 WHERE id='metadata-identity'").bind(9_007_199_254_740_992_i64).execute(prepared.system.holdings[0].raw()).await.unwrap();
        let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let actual=reached.clone();
        let outcome=prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),move|_,context|{
            actual.store(true,std::sync::atomic::Ordering::SeqCst);Box::pin(async move {encode(context,false).map(|_|())})
        }).await;
        let code=match &outcome {Err(PgTransactionError::Business(error))=>error.code,Err(PgTransactionError::Persistence(_))=>"PERSISTENCE_ERROR",Ok(_)=>"UNEXPECTED_ADMISSION"};
        // The ordinary coordinator's SQL principal check rejects the selected
        // grant before the later typed Directory source reader is reached.
        assert!(!reached.load(std::sync::atomic::Ordering::SeqCst),"invalid Directory observation must not reach projection");
        assert!(matches!(&outcome,Err(PgTransactionError::Business(error)) if error.code=="WORK_ROLE_UNAVAILABLE"),"actual sanitized denial: {code}");
    }).await;
}

#[tokio::test]
async fn copied_audit_metadata_cannot_survive_a_changed_current_identity_as_command_authority() {
    fixture::account::with_system(|system|async move{
        let prepared=fixture::prepare(system).await;directory(&prepared).await;
        let result=prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),|connection,context|Box::pin(async move{
            let data=encode(context,false)?;
            sqlx::query("UPDATE iam_members SET version=version+1 WHERE company_id='company' AND user_id='fixture-a'").execute(connection).await?;
            Ok(data)
        })).await;
        assert!(matches!(result,Err(PgTransactionError::Business(ref error)) if error.code=="WORK_AUTHORITY_CHANGED"));
        let row=sqlx::query("SELECT version::bigint version FROM iam_members WHERE company_id='company' AND user_id='fixture-a'").fetch_one(prepared.system.holdings[0].raw()).await.unwrap();assert_eq!(row.get::<i64,_>("version"),7);
    }).await;
}

#[tokio::test]
async fn synchronous_service_parts_uses_selected_account_and_actual_assignment_version() {
    use erpx_server_identity_access::public::selected_command_authority::{
        ServicePartsAuthorityPurpose, capture_service_parts_command_authority,
    };
    fixture::account::with_system(|system| async move {
        let prepared=fixture::prepare(system).await;
        let pool=prepared.system.holdings[0].clone();
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_queued_legal',2,'[\"legal.read\",\"module.legal.read\",\"finance.read\",\"finance.write\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE id='custom_queued_legal'").execute(pool.raw()).await.unwrap();
        let clock=prepared.system.scenario.clock.clone(); let other_pool=pool.clone();
        prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),move |c,context|{let clock=clock.clone();let other_pool=other_pool.clone();Box::pin(async move {
            let witness=capture_service_parts_command_authority(c,context,ServicePartsAuthorityPurpose::Write,clock.as_ref()).await?;
            let iam=serde_json::to_string(witness.iam()).unwrap();
            assert!(iam.ends_with(",\"queuedAssignmentVersion\":3}"));
            witness.require_current(c,clock.as_ref()).await?;
            let mut other=other_pool.raw().begin().await?; sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *other).await?;
            assert!(capture_service_parts_command_authority(&mut other,context,ServicePartsAuthorityPurpose::Write,clock.as_ref()).await.is_err());
            assert!(witness.require_current(&mut other,clock.as_ref()).await.is_err()); other.rollback().await?;
            Ok(())
        })}).await.unwrap();
        let clock=prepared.system.scenario.clock.clone();
        prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),move |c,context|{let clock=clock.clone();Box::pin(async move {
            let witness=capture_service_parts_command_authority(c,context,ServicePartsAuthorityPurpose::Write,clock.as_ref()).await?;
            let iam=serde_json::to_string(witness.iam()).unwrap();
            assert!(iam.ends_with(",\"queuedAssignmentVersion\":3}"));
            witness.require_current(c,clock.as_ref()).await?;
            sqlx::query("UPDATE active_work_contexts SET revision=revision+1 WHERE session_id=$1").bind(context.session_id().as_str()).execute(&mut *c).await?;
            assert!(witness.require_current(c,clock.as_ref()).await.is_err());
            Ok(())
        })}).await.expect_err("coordinator must reject the changed selected context");
        let mut stale=prepared.selection.clone(); stale.expected.as_mut().unwrap().assignment_version=4;
        let clock=prepared.system.scenario.clock.clone();
        let result=prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),stale,prepared.access.clone(),move |c,context|{let clock=clock.clone();Box::pin(async move { capture_service_parts_command_authority(c,context,ServicePartsAuthorityPurpose::Write,clock.as_ref()).await?; Ok(()) })}).await;
        assert!(result.is_err(),"stale selected assignment must not enter command");
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_queued_legal',3,'[\"legal.read\",\"module.legal.read\",\"finance.read\",\"finance.approve\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); UPDATE iam_roles SET published_revision=3 WHERE id='custom_queued_legal'").execute(pool.raw()).await.unwrap();
        let clock=prepared.system.scenario.clock.clone();
        prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),move |c,context|{let clock=clock.clone();Box::pin(async move { let witness=capture_service_parts_command_authority(c,context,ServicePartsAuthorityPurpose::Read,clock.as_ref()).await?; assert!(serde_json::to_string(witness.iam()).unwrap().ends_with(",\"queuedAssignmentVersion\":3}")); witness.require_current(c,clock.as_ref()).await?; Ok(()) })}).await.unwrap();
        let clock=prepared.system.scenario.clock.clone();
        let result=prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),move |c,context|{let clock=clock.clone();Box::pin(async move { capture_service_parts_command_authority(c,context,ServicePartsAuthorityPurpose::Write,clock.as_ref()).await?; Ok(()) })}).await;
        assert!(result.is_err(),"finance approve alone cannot perform setup");
        sqlx::raw_sql("UPDATE iam_roles SET published_revision=2 WHERE id='custom_queued_legal'; INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('service-branch','branch','service-branch','Service Branch','company','company','service-branch','active')").execute(pool.raw()).await.unwrap();
        let mut branch=prepared.selection.clone(); branch.branch_slug=Some("service-branch".into());
        let entered=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)); let observed=entered.clone(); let clock=prepared.system.scenario.clock.clone();
        let result=prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),branch,prepared.access.clone(),move |c,context|{let clock=clock.clone();let observed=observed.clone();Box::pin(async move { observed.store(true,std::sync::atomic::Ordering::SeqCst); capture_service_parts_command_authority(c,context,ServicePartsAuthorityPurpose::Write,clock.as_ref()).await?; Ok(()) })}).await;
        assert!(entered.load(std::sync::atomic::Ordering::SeqCst),"normal branch context must reach the purpose guard"); assert!(result.is_err(),"branch context cannot configure Company Finance");
        prepared.sign_out().await.unwrap();
        let clock=prepared.system.scenario.clock.clone();
        let result=prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),move |c,context|{let clock=clock.clone();Box::pin(async move { capture_service_parts_command_authority(c,context,ServicePartsAuthorityPurpose::Write,clock.as_ref()).await?; Ok(()) })}).await;
        assert!(result.is_err(),"revoked real Account child cannot enter command");
    }).await;
}

#[derive(Clone)]
struct MetadataReadSources;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for MetadataReadSources
{
    fn prepare<'c>(
        &'c self,
        _: &'c mut sqlx::PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}
#[derive(Debug)]
enum MetadataDenial {
    Unavailable(erpx_server_identity_access::public::effective_authority::SelectedContextRead),
    Error(BusinessError),
}
fn denied_metadata(
    value: &erpx_server_identity_access::public::effective_authority::SelectedContextRead,
    error: BusinessError,
) -> Result<MetadataDenial, BusinessError> {
    if error.code != "PERMISSION_REQUIRED" {
        return Err(error);
    }
    Ok(MetadataDenial::Unavailable(value.clone()))
}
#[tokio::test]
async fn genuine_selected_context_denial_exposes_only_current_read_metadata() {
    use erpx_server_identity_access::public::effective_authority::{
        SelectedContextRead, SelectedIngressFuture, SelectedTransportCallbacks, SelectedWorkIngress,
    };
    fixture::account::with_system(|system| async move {
        let prepared=fixture::prepare(system).await;
        let pool=prepared.system.holdings[0].clone();
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_queued_legal',2,'[]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE id='custom_queued_legal'; UPDATE assignments SET modules_json='[]' WHERE id='queued-account-grant'").execute(pool.raw()).await.unwrap();
        let mut access=prepared.access.clone(); access.permission="finance.read".into();access.module=None;access.destination="service/orders".into();
        let selected=prepared.selection.clone();
        let result=prepared.authorizer.execute_with_selected_transport_ingress_deferred("acme.example.test",prepared.proof.clone(),move||Ok(selected),(access.clone(),MetadataReadSources),
            SelectedTransportCallbacks::<_,SelectedContextRead,MetadataDenial,_>::new::<()>(
                |ingress:&SelectedWorkIngress<'_>| -> SelectedIngressFuture<SelectedContextRead,MetadataDenial> { let result=ingress.read_context().map_err(MetadataDenial::Error);Box::pin(async move {result}) },
                denied_metadata, |_,_,_|Box::pin(async { panic!("no Finance permission must not reach an authorized action") })
            )).await.unwrap();
        let metadata=match result { Err(MetadataDenial::Unavailable(value))=>value,Err(MetadataDenial::Error(error))=>panic!("unexpected identity error: {}",error.code),Ok(())=>panic!("unexpected authorized actor") };
        assert_eq!(metadata.user_id().as_str(),"fixture-a");assert_eq!(metadata.holding_id().as_str(),"h001");assert_eq!(metadata.selection().workspace().company_id.as_str(),"company");assert_eq!(metadata.selection().assignment().version,3);assert_eq!(metadata.selection().revision(),7);assert!(metadata.permissions().is_empty());assert!(metadata.modules().is_empty());
        let selected=prepared.selection.clone(); let clock=prepared.system.scenario.clock.clone();
        let expired=prepared.authorizer.execute_with_selected_transport_ingress_deferred("acme.example.test",prepared.proof.clone(),move||Ok(selected),(access.clone(),MetadataReadSources),
            SelectedTransportCallbacks::<_,SelectedContextRead,MetadataDenial,_>::new::<()>(
                move |ingress:&SelectedWorkIngress<'_>| -> SelectedIngressFuture<SelectedContextRead,MetadataDenial> {let result=ingress.read_context().map_err(MetadataDenial::Error);let clock=clock.clone();Box::pin(async move {clock.advance(3_600_001);result})},
                denied_metadata, |_,_,_|Box::pin(async {panic!("no Finance permission must not reach an authorized action")})
            )).await;
        assert!(matches!(expired,Err(PgTransactionError::Business(ref e)) if e.code=="WORK_AUTHORITY_EXPIRED"));
        prepared.system.scenario.clock.advance(-3_600_001);
        prepared.sign_out().await.unwrap();
        let selected=prepared.selection.clone();
        let revoked=prepared.authorizer.execute_with_selected_transport_ingress_deferred("acme.example.test",prepared.proof.clone(),move||Ok(selected),(access,MetadataReadSources),
            SelectedTransportCallbacks::<_,SelectedContextRead,MetadataDenial,_>::new::<()>(
                |ingress:&SelectedWorkIngress<'_>| -> SelectedIngressFuture<SelectedContextRead,MetadataDenial> {let result=ingress.read_context().map_err(MetadataDenial::Error);Box::pin(async move {result})},
                denied_metadata, |_,_,_|Box::pin(async {panic!("revoked child cannot enter")})
            )).await;
        assert!(matches!(revoked,Err(PgTransactionError::Business(ref e)) if e.code=="UNAUTHENTICATED"));
    }).await;
}

#[tokio::test]
async fn service_stock_authority_uses_selected_stock_permissions_and_current_unit() {
    use erpx_server_identity_access::public::selected_command_authority::capture_service_stock_authority;
    fixture::account::with_system(|system|async move {
        let prepared=fixture::prepare(system).await;let pool=prepared.system.holdings[0].clone();
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_queued_legal',2,'[\"legal.read\",\"module.legal.read\",\"inventory.read\",\"inventory.prepare\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0); UPDATE iam_roles SET published_revision=2 WHERE id='custom_queued_legal'; INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('stock-branch','branch','stock-branch','Stock Branch','company','company','stock-branch','active')").execute(pool.raw()).await.unwrap();
        for branch in [None,Some("stock-branch")] {
            let mut selection=prepared.selection.clone();selection.branch_slug=branch.map(str::to_owned);let clock=prepared.system.scenario.clock.clone();let other=pool.clone();
            prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),selection,prepared.access.clone(),move|c,context|{let clock=clock.clone();let other=other.clone();Box::pin(async move{
                let witness=capture_service_stock_authority(c,context,clock.as_ref()).await?;witness.require_current(c,clock.as_ref()).await?;
                let mut tx=other.raw().begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;assert!(witness.require_current(&mut tx,clock.as_ref()).await.is_err());tx.rollback().await?;Ok(())
            })}).await.unwrap();
        }
        sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_queued_legal',3,'[\"legal.read\",\"module.legal.read\",\"inventory.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=3 WHERE id='custom_queued_legal'").execute(pool.raw()).await.unwrap();
        let clock=prepared.system.scenario.clock.clone();let denied=prepared.authorizer.execute("acme.example.test",prepared.proof.clone(),prepared.selection.clone(),prepared.access.clone(),move|c,context|{let clock=clock.clone();Box::pin(async move{capture_service_stock_authority(c,context,clock.as_ref()).await?;Ok(())})}).await;assert!(denied.is_err());
    }).await;
}
