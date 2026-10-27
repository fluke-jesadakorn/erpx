#[allow(
    dead_code,
    reason = "shared real Account/SSO fixture exposes helpers for multiple suites"
)]
#[path = "../../support/rust-account-queued-authority.rs"]
mod fixture;
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
    sqlx::raw_sql("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('person','fixture-a','company','Synthetic Person','TH','fixture',0,0);INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES('metadata-connection','company','Synthetic Directory','entra_id','{}','active','fixture',0); INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,connection_id,identity_id,version) VALUES('company','fixture-a','directory:fixture-a','person','active','directory','metadata-connection','metadata-identity',7),('company','alias-a','local:alias-a','person','active','local',NULL,NULL,1),('company','fixture-b','local:fixture-b','person','active','local',NULL,NULL,1)").execute(pool.raw()).await.unwrap();
    sqlx::query("INSERT INTO iam_identities(id,company_id,connection_id,external_id,username,name,enabled,observed_at,user_id,person_id) VALUES('metadata-identity','company','metadata-connection','synthetic-external','synthetic','Synthetic Directory User',true,$1,'fixture-a','person')").bind(fixture::account::fixture::account::NOW-1000).execute(pool.raw()).await.unwrap();
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
        let prepared=fixture::prepare(system).await;directory(&prepared).await;
        sqlx::query("UPDATE iam_identities SET observed_at=$1 WHERE id='metadata-identity'").bind(9_007_199_254_740_992_i64).execute(prepared.system.holdings[0].raw()).await.unwrap();
        assert!(matches!(projection(&prepared,false).await,Err(PgTransactionError::Business(ref error)) if error.code=="COMMAND_IAM_METADATA_UNAVAILABLE"));
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
