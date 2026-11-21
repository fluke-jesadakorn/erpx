//! Native Platform IAM role command over the Control PostgreSQL track.
//!
//! One command path (Platform role save) proves the Platform Company/scope,
//! catalogue-gated delegation, receipt/audit/outbox effects and replay. It does
//! not prove the retained Account/Platform HTTP resolution or a browser.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_identity_access::public::iam_roles::{
    AccessReviewCampaignInput, IamRoleActor, PgIamRolesReviews, RoleSaveInput, save_role,
    start_access_review,
};
use erpx_server_identity_access::public::permission_catalog::permission_catalog;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, HoldingId, RecordId, UserId,
};
use serde_json::Value;

const NOW: i64 = 500;

fn id(value: &str) -> RecordId {
    RecordId::new(value).unwrap()
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn platform_actor() -> IamRoleActor {
    let permissions = vec![
        "platform.iam.read".to_owned(),
        "platform.iam.manage".to_owned(),
        "platform.iam.roles.manage".to_owned(),
        "platform.iam.directory.manage".to_owned(),
        "platform.iam.review".to_owned(),
        "platform.iam.export".to_owned(),
        "platform.iam.recovery".to_owned(),
        "platform.iam.self".to_owned(),
    ];
    IamRoleActor {
        user_id: UserId::new("platform-operator").unwrap(),
        session_id: id("platform-session"),
        holding_id: HoldingId::new("h003").unwrap(),
        company_id: CompanyId::new("platform").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: AssignmentId::new("platform-assignment").unwrap(),
        scope_kind: "platform".into(),
        scope_node_id: None,
        role: Some("platform_admin".into()),
        role_revision: Some(1),
        assignment_version: Some(1),
        identity_version: None,
        authority_revision: None,
        permissions: permissions.clone(),
        grantable_permissions: permissions,
        person_user_ids: vec![UserId::new("platform-operator").unwrap()],
        person_id: None,
        platform: true,
        emergency_until: None,
        expires_at: 1_000_000,
    }
}

fn input() -> RoleSaveInput {
    RoleSaveInput {
        id: None,
        name: "Platform finance reader".into(),
        expected_version: 0,
        permissions: vec!["platform.iam.read".into()],
        grantable_permissions: vec!["platform.iam.read".into()],
        scopes: vec!["platform".into()],
        reason: "  Independent Platform role revision  ".into(),
    }
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn setup(pool: &erpx_server_runtime::public::postgres::DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::Control)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::Control, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO \"user\"(id,name,email,\"emailVerified\",\"createdAt\",\"updatedAt\") \
         VALUES('platform-operator','Platform Operator','platform-operator@example.test',0,0,0) \
         ON CONFLICT(id) DO NOTHING;\
         INSERT INTO iam_members(company_id,user_id,principal_id,person_id,status,login_policy,version) \
         VALUES('platform','platform-operator','local:platform-operator',NULL,'active','local',1) \
         ON CONFLICT(company_id,user_id) DO UPDATE SET status='active';\
         INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until,revoked_at,version) \
         VALUES('platform-assignment','platform-operator','platform_admin','[]',0,1000000,NULL,1) \
         ON CONFLICT(id) DO UPDATE SET revoked_at=NULL,version=EXCLUDED.version;",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn save(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    input: &RoleSaveInput,
    command_key: &str,
) -> Result<Value, PgTransactionError<BusinessError>> {
    let mut tx = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await
        .unwrap();
    let result = {
        let mut repository = PgIamRolesReviews::new_platform(&mut tx);
        save_role(
            &mut repository,
            &platform_actor(),
            permission_catalog().unwrap(),
            &key(command_key),
            input,
            NOW,
        )
        .await
    };
    match &result {
        Ok(_) => tx.commit().await.unwrap(),
        Err(_) => tx.rollback().await.unwrap(),
    }
    result
}

async fn effects(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    key: &str,
) -> (i64, i64) {
    let receipt = sqlx::query_scalar("SELECT count(*)::bigint FROM iam_receipts WHERE id=$1")
        .bind(key)
        .fetch_one(pool.raw())
        .await
        .unwrap();
    let audit = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM iam_audit \
         WHERE action='iam.role.save' AND actor_id='platform-operator'",
    )
    .fetch_one(pool.raw())
    .await
    .unwrap();
    (receipt, audit)
}

#[tokio::test]
async fn platform_role_save_persists_platform_company_revision_review_and_receipt() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let value = save(&pool, &input(), "platform-role-key-1").await.unwrap();
        assert_eq!(value["status"], "pending");
        assert_eq!(value["revision"], 1);
        let role_id = value["roleId"].as_str().unwrap().to_owned();
        assert!(role_id.starts_with("platform_custom_"), "{role_id}");
        let stored: (String, bool, i64) = sqlx::query_as(
            "SELECT name,built_in,published_revision::bigint FROM iam_roles \
             WHERE company_id='platform' AND id=$1",
        )
        .bind(&role_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(stored, ("Platform finance reader".into(), false, 0));
        let revision: (String, String, String) = sqlx::query_as(
            "SELECT permissions_json::text,grantable_json::text,scopes_json::text \
             FROM iam_role_revisions WHERE company_id='platform' AND role_id=$1 AND revision=1",
        )
        .bind(&role_id)
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(revision.0, "[\"platform.iam.read\"]");
        assert_eq!(revision.1, "[\"platform.iam.read\"]");
        assert_eq!(revision.2, "[\"platform\"]");
        let review: (String, String, i64) = sqlx::query_as(
            "SELECT status,kind,version::bigint FROM iam_reviews \
             WHERE id=$1 AND company_id='platform'",
        )
        .bind(value["id"].as_str().unwrap())
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(review, ("pending".into(), "role.publish".into(), 1));
        assert_eq!(effects(&pool, "platform-role-key-1").await, (1, 1));

        let replay = save(&pool, &input(), "platform-role-key-1").await.unwrap();
        assert_eq!(replay, value);
        assert_eq!(effects(&pool, "platform-role-key-1").await, (1, 1));

        let mut different = input();
        different.name = "Different Platform role".into();
        assert_eq!(
            business(
                save(&pool, &different, "platform-role-key-1")
                    .await
                    .unwrap_err()
            )
            .code,
            "IDEMPOTENCY_CONFLICT"
        );

        let mut tx = pool
            .raw()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        let review_error = {
            let mut repository = PgIamRolesReviews::new_platform(&mut tx);
            start_access_review(
                &mut repository,
                &platform_actor(),
                &key("platform-campaign-key-1"),
                &AccessReviewCampaignInput {
                    name: "Platform quarterly certification".into(),
                    due_at: 86400500,
                    reason: "Quarterly review".into(),
                },
                NOW,
            )
            .await
            .unwrap_err()
        };
        tx.rollback().await.unwrap();
        assert_eq!(business(review_error).code, "HOLDING_REVIEW_REQUIRED");
    })
    .await;
}
