use erpx_server_identity_access::public::effective_authority::{
    HoldingWorkAuthorizer, WorkAuthorizationRequest,
};
use erpx_server_identity_access::public::work_context::{
    ExpectedWorkSelection, HoldingWorkspaceRequest,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, HoldingOrigin, SecretDigest,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{AssignmentId, Clock, CompanyId, HoldingId, TimestampMillis};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
pub const NOW: i64 = 1800000000000;
pub struct MutableClock(pub AtomicI64);
impl Clock for MutableClock {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(self.0.load(Ordering::SeqCst))
    }
}
pub fn proof() -> DeviceProof {
    DeviceProof {
        active: Some(SecretDigest::new("a".repeat(64)).unwrap()),
        ..Default::default()
    }
}
pub fn input() -> HoldingWorkspaceRequest {
    HoldingWorkspaceRequest {
        company_id: CompanyId::new("company").unwrap(),
        branch_slug: None,
        department_slug: None,
        expected: Some(ExpectedWorkSelection {
            revision: 7,
            assignment_id: AssignmentId::new("grant").unwrap(),
            assignment_version: 3,
        }),
        mutation: true,
    }
}
pub fn access() -> WorkAuthorizationRequest {
    WorkAuthorizationRequest {
        permission: "finance.write".into(),
        module: Some("finance".into()),
        destination: "finance/journals".into(),
        api: true,
    }
}
pub async fn setup(
    control: &DatabasePool,
    holding: &DatabasePool,
) -> (Arc<HoldingWorkAuthorizer>, Arc<MutableClock>) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    for (pool, track) in [
        (control, MigrationTrack::Control),
        (holding, MigrationTrack::HoldingWrite),
    ] {
        let migrations = load_migrations(&root, track).await.unwrap();
        migrate_database(pool, track, &migrations).await.unwrap();
    }
    sqlx::query("INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") VALUES('user','Synthetic','user@example.test',1,$1,$1)").bind(NOW).execute(control.raw()).await.unwrap();
    for (id, digest) in [("parent", "b".repeat(64)), ("session", "a".repeat(64))] {
        sqlx::query("INSERT INTO session(id,\"userId\",token,\"expiresAt\",\"createdAt\",\"updatedAt\") VALUES($1,'user',$2,to_timestamp(($3::bigint+3600000)::double precision/1000)::text,to_timestamp($3::double precision/1000)::text,to_timestamp($3::double precision/1000)::text)").bind(id).bind(digest).bind(NOW).execute(control.raw()).await.unwrap();
    }
    sqlx::query("INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES('parent','account','development',NULL,$1,$2),('session','holding:development:h001:acme.example.test','development','parent',$1,$2)").bind(NOW).bind(NOW+3600000).execute(control.raw()).await.unwrap();
    sqlx::query("INSERT INTO account_device_browsers(id,active_session_id,revision,updated_at) VALUES('browser','parent',1,$1)").bind(NOW).execute(control.raw()).await.unwrap();
    sqlx::query("INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES('parent','browser','saved-parent')").execute(control.raw()).await.unwrap();
    sqlx::raw_sql("CREATE TABLE native_owner_effects(id text PRIMARY KEY); INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES('company','company','acme','Acme',NULL,'company',NULL,'active'),('branch','branch','north','North','company','company','branch','active'); INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','custom_native_authority','Native authority',false,1,'active'); INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','custom_native_authority',1,'[\"finance.read\",\"finance.write\",\"module.finance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0)").execute(holding.raw()).await.unwrap();
    sqlx::query("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('grant','user','custom_native_authority','[\"finance\"]','company',$1,$2,3,'holding')").bind(NOW-1000).bind(NOW+3600000).execute(holding.raw()).await.unwrap();
    sqlx::query("INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES('session','user','company','grant',3,7,$1,$2)").bind(NOW+3600000).bind(NOW).execute(holding.raw()).await.unwrap();
    let clock = Arc::new(MutableClock(AtomicI64::new(NOW)));
    let policy = AccountPolicy {
        account_origin: "https://account.example.test".into(),
        account_host: "account.example.test".into(),
        environment: "development".into(),
        require_email_verification: true,
        holdings: vec![HoldingOrigin {
            origin: "https://acme.example.test".into(),
            host: "acme.example.test".into(),
            audience: "holding:development:h001:acme.example.test".into(),
        }],
    };
    (
        Arc::new(
            HoldingWorkAuthorizer::new(
                control.clone(),
                holding.clone(),
                HoldingId::new("h001").unwrap(),
                policy,
                clock.clone(),
            )
            .unwrap(),
        ),
        clock,
    )
}
