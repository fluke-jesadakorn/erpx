//! Native Branding profile, storefront and media commands over a leased
//! PostgreSQL database: command receipt, replay, audit/outbox, published
//! identity control and media pointer/revocation evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_brand::public::branding::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, UserId};
use erpx_server_workflow::public::commands::{CommandActor, CommandField, CommandJson, WorkAuthority};
use serde_json::{Value, json};
use sqlx::AssertSqlSafe;

const NOW: i64 = 1_000;
type Error = PgTransactionError<BusinessError>;

fn command_actor(company: &str, user: &str, assignment: &str, role: &str) -> CommandActor {
    CommandActor {
        user_id: UserId::new(user).unwrap(),
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: role.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: CommandField::Omitted,
        department_id: CommandField::Omitted,
        authority: CommandField::Value(WorkAuthority {
            session_id: "session".into(),
            revision: 1,
            assignment_version: 1,
        }),
        iam: CommandField::Value(
            CommandJson::parse(
                r#"{"roleRevision":1,"identityVersion":1,"permissions":[],"grantablePermissions":[],"principalId":"principal","personId":null,"personUserIds":[],"freshUntil":null}"#,
            )
            .unwrap(),
        ),
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: None,
    }
}

fn owner() -> BrandActor {
    BrandActor::new(
        "h001",
        command_actor("company", "owner", "assign-owner", "holding_owner"),
        vec!["brand".into()],
        vec![
            "company.owner".into(),
            "organization.manage".into(),
            "records.read".into(),
        ],
    )
}

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite).await.unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files).await.unwrap();
}

async fn seed(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status) VALUES\
         ('company','company','acme','Acme',NULL,'company','active'),\
         ('branch-1','branch','branch-1','Branch One','company','company','active');\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES\
         ('assign-owner','owner','holding_owner','[\"brand\"]','company',0,9007199254740000,'holding');\
         INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES\
         ('upload-logo','branch_brand','branch-1','primary','logo.webp','image/webp',10,'staging-logo','ready','owner',9007199254740000,1,1);\
         INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id) VALUES\
         ('attachment-logo','upload-logo','branch_brand','branch-1','primary','brand/logo.webp',1,'logo.webp','image/webp',10,'checksum','owner',1,'company');\
         INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,purpose_key,expected_target_version,policy_version,classification,audience_json,created_by,created_at,updated_at) VALUES\
         ('set-logo','company','branch_brand','branch-1','publication','primary',1,1,'public','[]','owner',1,1);\
         INSERT INTO attachment_links(id,set_id,state,created_by,created_at,expires_at,attachment_id) VALUES\
         ('link-logo','set-logo','draft','owner',1,9007199254740000,'attachment-logo');\
         INSERT INTO attachment_publications(id,company_id,source_link_id,target_type,target_id,derivative_object_key,derivative_media_type,derivative_checksum_sha256,derivative_bytes,alt_text,revision,state,created_by,created_at,approved_by,approved_at) VALUES\
         ('publication-logo','company','link-logo','branch_brand','branch-1','brand/logo-approved.webp','image/webp','checksum',10,'Approved logo',1,'approved','owner',1,'owner',2);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn profile(
    pool: &DatabasePool,
    actor: &BrandActor,
    raw: Value,
    key: &str,
) -> Result<Value, Error> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.to_owned();
            Box::pin(async move {
                let mut ports = PgBranding::new(c, &actor, NOW);
                update_brand_profile(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn storefront(
    pool: &DatabasePool,
    actor: &BrandActor,
    raw: Value,
    key: &str,
) -> Result<Value, Error> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.to_owned();
            Box::pin(async move {
                let mut ports = PgBranding::new(c, &actor, NOW);
                update_storefront(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn media(
    pool: &DatabasePool,
    actor: &BrandActor,
    raw: Value,
    key: &str,
) -> Result<Value, Error> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let raw = raw.clone();
            let key = key.to_owned();
            Box::pin(async move {
                let mut ports = PgBranding::new(c, &actor, NOW);
                attach_brand_media(&mut ports, &raw, &key).await
            })
        })
        .await
}

async fn workspace(pool: &DatabasePool, actor: &BrandActor) -> Result<BrandingWorkspace, Error> {
    let actor = actor.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            Box::pin(async move {
                let mut ports = PgBranding::new(c, &actor, NOW);
                read_branding_workspace(&mut ports).await
            })
        })
        .await
}

fn code(error: Error) -> &'static str {
    match error {
        PgTransactionError::Business(error) => error.code,
        PgTransactionError::Persistence(error) => panic!("unexpected persistence failure: {error}"),
    }
}

#[tokio::test]
async fn profile_storefront_and_media_commands_retain_evidence() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        let owner = owner();
        let profile_result = profile(
            &pool,
            &owner,
            json!({"scopeKind":"branch","scopeId":"branch-1","documentMode":"both"}),
            "branding-profile-1",
        )
        .await
        .unwrap();
        assert_eq!(profile_result["documentMode"], "both");
        assert_eq!(
            profile(
                &pool,
                &owner,
                json!({"scopeKind":"branch","scopeId":"branch-1","documentMode":"both"}),
                "branding-profile-1",
            )
            .await
            .unwrap(),
            profile_result
        );
        assert_eq!(
            code(
                profile(
                    &pool,
                    &owner,
                    json!({"scopeKind":"branch","scopeId":"branch-1","documentMode":"branch"}),
                    "branding-profile-1",
                )
                .await
                .unwrap_err()
            ),
            "IDEMPOTENCY_CONFLICT"
        );
        let row: (String, i64) = sqlx::query_as(
            "SELECT document_mode,updated_at FROM organization_brand_profiles WHERE scope_kind='branch' AND scope_id='branch-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, ("both".into(), NOW));
        for table in ["command_receipts", "audit_events", "outbox_events"] {
            let sql = format!(
                "SELECT count(*) FROM {table} WHERE command_id='branding-profile-1'"
            );
            let count: i64 = sqlx::query_scalar(AssertSqlSafe(sql))
                .fetch_one(pool.raw())
                .await
                .unwrap();
            assert_eq!(count, 1, "{table}");
        }

        let storefront_result = storefront(
            &pool,
            &owner,
            json!({"shortName":"Reviewed storefront","accentColor":"#AABBCC"}),
            "branding-storefront-1",
        )
        .await
        .unwrap();
        assert_eq!(storefront_result["shortName"], "Reviewed storefront");
        assert_eq!(storefront_result["accentColor"], "#aabbcc");
        let row: (String, String) = sqlx::query_as(
            "SELECT short_name,accent_color FROM storefront_app_brands WHERE company_id='company'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, ("Reviewed storefront".into(), "#aabbcc".into()));
        assert_eq!(
            storefront(
                &pool,
                &owner,
                json!({"shortName":"Reviewed storefront","accentColor":"#AABBCC"}),
                "branding-storefront-1",
            )
            .await
            .unwrap(),
            storefront_result
        );

        let media_result = media(
            &pool,
            &owner,
            json!({"scopeKind":"branch","scopeId":"branch-1","variant":"primary","attachmentId":"attachment-logo","publicationId":"publication-logo"}),
            "branding-media-1",
        )
        .await
        .unwrap();
        assert_eq!(media_result["attachmentId"], "attachment-logo");
        assert_eq!(media_result["publicationId"], "publication-logo");
        let pointer: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT primary_attachment_id,primary_publication_id FROM organization_brand_profiles WHERE scope_kind='branch' AND scope_id='branch-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            pointer,
            (Some("attachment-logo".into()), Some("publication-logo".into()))
        );
        assert_eq!(
            media(
                &pool,
                &owner,
                json!({"scopeKind":"branch","scopeId":"branch-1","variant":"primary","attachmentId":"attachment-logo","publicationId":"publication-logo"}),
                "branding-media-1",
            )
            .await
            .unwrap(),
            media_result
        );

        let removed = profile(
            &pool,
            &owner,
            json!({"scopeKind":"branch","scopeId":"branch-1","remove":"primary"}),
            "branding-media-remove-1",
        )
        .await
        .unwrap();
        assert_eq!(removed["removedVariant"], "primary");
        let pointer: (Option<String>, Option<String>, String) = sqlx::query_as(
            "SELECT p.primary_attachment_id,p.primary_publication_id,a.state \
             FROM organization_brand_profiles p JOIN attachment_publications a ON a.id='publication-logo' \
             WHERE p.scope_kind='branch' AND p.scope_id='branch-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(pointer, (None, None, "revoked".into()));

        let read = workspace(&pool, &owner).await.unwrap();
        assert_eq!(read.company_id, "company");
        assert_eq!(read.branches.len(), 1);
        assert_eq!(read.branches[0].0.id, "branch-1");
        assert_eq!(read.storefront.short_name.as_deref(), Some("Reviewed storefront"));
    })
    .await;
}

#[tokio::test]
async fn published_identity_blocks_storefront_writes_without_effects() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO brand_identity_drafts(id,company_id,based_revision,identity_json,status,version,created_by,updated_by,created_at,updated_at) VALUES\
             ('draft-1','company',0,'{\"identity\":{\"shortName\":\"Published\",\"tagline\":\"\"},\"visual\":{\"accentColor\":\"#224488\",\"logoPublicationId\":null}}','published',2,'maker','reviewer',1,1);\
             INSERT INTO brand_identity_versions(id,company_id,revision,identity_json,draft_id,draft_version,created_by,published_by,published_at) VALUES\
             ('version-1','company',1,'{\"identity\":{\"shortName\":\"Published\",\"tagline\":\"\"},\"visual\":{\"accentColor\":\"#224488\",\"logoPublicationId\":null}}','draft-1',1,'maker','reviewer',1);\
             INSERT INTO brand_identity_active(company_id,identity_version_id,version,updated_by,updated_at) VALUES\
             ('company','version-1',1,'reviewer',1);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let owner = owner();
        assert_eq!(
            code(
                storefront(
                    &pool,
                    &owner,
                    json!({"shortName":"Blocked"}),
                    "branding-storefront-blocked",
                )
                .await
                .unwrap_err()
            ),
            "BRAND_PUBLISHED_IDENTITY_CONTROLLED"
        );
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM storefront_app_brands WHERE company_id='company'),\
             (SELECT count(*) FROM command_receipts WHERE command_id='branding-storefront-blocked'),\
             (SELECT count(*) FROM outbox_events WHERE command_id='branding-storefront-blocked')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (0, 0, 0));
    })
    .await;
}
