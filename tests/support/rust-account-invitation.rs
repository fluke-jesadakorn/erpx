#[allow(
    dead_code,
    reason = "shared Account fixture includes auth/recovery helpers used by other suites"
)]
#[path = "rust-account-credentials.rs"]
pub mod credentials;
use erpx_server_identity_access::public::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::HoldingId;
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicI64},
};
pub async fn setup(
    control: &DatabasePool,
    holding: &DatabasePool,
) -> (credentials::Scenario, OpaqueSecret) {
    let clock = Arc::new(credentials::MutableClock(AtomicI64::new(
        credentials::account::NOW,
    )));
    let authority = Arc::new(PostgresInvitationCredentialAuthority::new(
        BTreeMap::from([(HoldingId::new("h001").unwrap(), holding.clone())]),
        clock.clone(),
    ));
    let scenario = credentials::setup_with_clock(control, authority, clock).await;
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(holding, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','acme','Synthetic Company',NULL,'company',NULL,'active'),('branch','branch','north','North','company','company','branch','active'),('department','department','payroll','Payroll','branch','company','branch','active');
      INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('company','native_inviter','Inviter',false,1,'active');
      INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) SELECT 'company','native_inviter',1,'[\"iam.manage\"]'::jsonb,v.permissions_json||v.grantable_json,'[\"holding\",\"branch\",\"department\"]'::jsonb,'fixture',0 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.id='finance_preparer' AND r.company_id='*';
      INSERT INTO hr_people(id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES('person','company','Synthetic Invitee','TH','fixture',0,0);
      INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES('engagement','person','company','branch','department','employee','salaried','active','2026-01-01','Synthetic Staff','THB','fixture',0,0)").execute(holding.raw()).await.unwrap();
    let now = credentials::account::NOW;
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('parent','fixture-a','native_inviter','[]','company',$1,$2,1,'holding')").bind(0_i64).bind(now+7_200_000).execute(holding.raw()).await.unwrap();
    let revision: i64 = sqlx::query_scalar("SELECT published_revision::bigint FROM iam_roles WHERE company_id='*' AND id='finance_preparer'").fetch_one(holding.raw()).await.unwrap();
    let payload = serde_json::json!({"invitationId":"invitation","userId":"usr_invite_invitation","roleId":"finance_preparer","expectedRoleRevision":revision,"scopeKind":"department","scopeNodeId":"department","validUntil":now+3_600_000,"parentId":"parent","authorizingAssignmentId":"parent","reason":"Reviewed synthetic staff access"});
    sqlx::query("INSERT INTO iam_reviews(id,company_id,kind,payload,created_by,created_person,created_at,status,version,reviewed_by,reviewed_at,reason) VALUES('review','company','invitation.authorize',$1,'fixture-a','fixture-a',$2,'approved',2,'fixture-b',$2,'Synthetic independent review')").bind(payload).bind(now).execute(holding.raw()).await.unwrap();
    let token = SystemAuthenticationSecrets.issue().unwrap();
    let digest = SystemAuthenticationSecrets.digest(SecretPurpose::Invitation, &token);
    sqlx::query("INSERT INTO account_invitations(id,token_hash,holding_id,company_id,person_id,email,name,role,scope_kind,branch_id,department_id,created_by,assignment_id,expires_at,created_at) VALUES('invitation',$1,'h001','company','person','invited@example.test','Synthetic Invitee',$2,'department','branch','department','fixture-a','parent',$3,$4)")
        .bind(digest.as_str()).bind(serde_json::json!({"role":"finance_preparer","expires":now+3_600_000}).to_string()).bind(now+3_600_000).bind(now).execute(control.raw()).await.unwrap();
    (scenario, token)
}
pub async fn signup(
    scenario: &credentials::Scenario,
    token: &OpaqueSecret,
) -> Result<(), erpx_server_shared::public::BusinessError> {
    scenario
        .credentials
        .sign_up(
            "Browser Supplied Name".into(),
            "invited@example.test",
            credentials::PASSWORD.into(),
            Some(token.expose().into()),
            None,
        )
        .await
}
pub async fn effects(control: &DatabasePool) -> i64 {
    sqlx::query_scalar("SELECT (SELECT count(*) FROM \"user\" WHERE id='usr_invite_invitation')+(SELECT count(*) FROM account_invitation_claims)+(SELECT count(*) FROM native_account_mail_outbox)+(SELECT count(*) FROM native_account_credential_events)").fetch_one(control.raw()).await.unwrap()
}
