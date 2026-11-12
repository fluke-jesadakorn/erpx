//! Native Branding profile, storefront and media commands over a leased
//! PostgreSQL database: command receipt, replay, audit/outbox, published
//! identity control and media pointer/revocation evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_brand::public::branding::*;
use erpx_server_brand::public::identity::{
    ActiveBrandIdentity, BrandIdentity, IdentitySection, VisualSection,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, FailureKind, UserId};
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

fn owner_selected(branch: Option<&str>) -> BrandActor {
    let mut command = command_actor("company", "owner", "assign-owner", "holding_owner");
    command.branch_id = branch
        .map(|id| CommandField::Value(id.into()))
        .unwrap_or(CommandField::Null);
    BrandActor::new(
        "h001",
        command,
        vec!["brand".into()],
        vec![
            "company.owner".into(),
            "organization.manage".into(),
            "records.read".into(),
        ],
    )
}

fn presentation_context() -> BrandPresentationContext {
    BrandPresentationContext::new(
        "Acme Holding",
        "acme",
        "/api/v1/holdings/acme/branches/branch-1",
        false,
    )
    .unwrap()
}

async fn seed_presentation(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO storefront_app_brands(company_id,short_name,accent_color,logo_attachment_id,logo_publication_id,updated_by,updated_at) \
         VALUES('company','Legacy storefront','#123456','attachment-logo','publication-logo','owner',1);\
         INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,primary_attachment_id,inverse_attachment_id,primary_publication_id,inverse_publication_id,document_mode,updated_by,updated_at) VALUES\
         ('holding','h001',NULL,NULL,NULL,NULL,NULL,'holding','owner',2),\
         ('branch','branch-1','company','attachment-logo',NULL,'publication-logo',NULL,'both','owner',3),\
         ('holding','h-other',NULL,NULL,NULL,'publication-logo',NULL,'holding','owner',4),\
         ('branch','branch-other','other-company',NULL,NULL,'publication-logo',NULL,'branch','owner',5);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_published_identity(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO brand_identity_drafts(id,company_id,based_revision,identity_json,status,version,created_by,updated_by,created_at,updated_at) VALUES\
         ('read-draft','company',0,'{\"identity\":{\"shortName\":\"Published storefront\",\"tagline\":\"\"},\"visual\":{\"accentColor\":\"#224488\",\"logoPublicationId\":null}}','published',2,'maker','reviewer',1,1);\
         INSERT INTO brand_identity_versions(id,company_id,revision,identity_json,draft_id,draft_version,created_by,published_by,published_at) VALUES\
         ('read-version','company',1,'{\"identity\":{\"shortName\":\"Published storefront\",\"tagline\":\"\"},\"visual\":{\"accentColor\":\"#224488\",\"logoPublicationId\":null}}','read-draft',1,'maker','reviewer',1);\
         INSERT INTO brand_identity_active(company_id,identity_version_id,version,updated_by,updated_at) VALUES\
         ('company','read-version',1,'reviewer',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn workspace_brand(
    pool: &DatabasePool,
    actor: &BrandActor,
    context: &BrandPresentationContext,
) -> Result<WorkspaceBrand, Error> {
    let actor = actor.clone();
    let context = context.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let context = context.clone();
            Box::pin(async move {
                let mut ports = PgBranding::new(c, &actor, NOW);
                resolve_workspace_brand(&mut ports, &context).await
            })
        })
        .await
}

async fn document_brand(
    pool: &DatabasePool,
    actor: &BrandActor,
    context: &BrandPresentationContext,
    branch_id: Option<&str>,
    requested_mode: Option<&str>,
) -> Result<DocumentBrand, Error> {
    let actor = actor.clone();
    let context = context.clone();
    let branch_id = branch_id.map(str::to_owned);
    let requested_mode = requested_mode.map(str::to_owned);
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let context = context.clone();
            let branch_id = branch_id.clone();
            let requested_mode = requested_mode.clone();
            Box::pin(async move {
                let mut ports = PgBranding::new(c, &actor, NOW);
                resolve_document_brand(
                    &mut ports,
                    &context,
                    branch_id.as_deref(),
                    requested_mode.as_deref(),
                )
                .await
            })
        })
        .await
}

async fn profile_list(
    pool: &DatabasePool,
    actor: &BrandActor,
    context: &BrandPresentationContext,
) -> Result<BrandingProfiles, Error> {
    let actor = actor.clone();
    let context = context.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let context = context.clone();
            Box::pin(async move {
                let mut ports = PgBranding::new(c, &actor, NOW);
                list_brand_profiles(&mut ports, &context).await
            })
        })
        .await
}

async fn storefront_brand(
    pool: &DatabasePool,
    actor: &BrandActor,
    context: &BrandPresentationContext,
) -> Result<StorefrontAppBrand, Error> {
    let actor = actor.clone();
    let context = context.clone();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let context = context.clone();
            Box::pin(async move {
                let mut ports = PgBranding::new(c, &actor, NOW);
                read_storefront_app_brand(&mut ports, &context).await
            })
        })
        .await
}

async fn brand_evidence(
    pool: &DatabasePool,
    actor: &BrandActor,
    branch_id: &str,
) -> Result<Option<BrandProfileEvidence>, Error> {
    let actor = actor.clone();
    let branch_id = branch_id.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run::<_, BusinessError, _>(TransactionOptions::default(), move |c| {
            let actor = actor.clone();
            let branch_id = branch_id.clone();
            Box::pin(async move {
                let mut ports = PgBranding::new(c, &actor, NOW);
                read_brand_profile_evidence(&mut ports, &branch_id).await
            })
        })
        .await
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(error) => panic!("unexpected persistence failure: {error}"),
    }
}

#[tokio::test]
async fn presentation_reads_render_brand_workspace_documents_profiles_and_evidence() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        seed_presentation(&pool).await;
        let context = presentation_context();
        let owner = owner();

        let holding = workspace_brand(&pool, &owner, &context).await.unwrap();
        assert_eq!(holding.scope_kind, "holding");
        assert_eq!(holding.scope_id, "h001");
        assert_eq!(holding.label, "Acme Holding");
        assert_eq!(holding.document_mode, "holding");
        assert_eq!(holding.primary_url, None);

        let branch_actor = owner_selected(Some("branch-1"));
        let branch = workspace_brand(&pool, &branch_actor, &context).await.unwrap();
        assert_eq!(branch.scope_kind, "branch");
        assert_eq!(branch.scope_id, "branch-1");
        assert_eq!(branch.label, "Branch One");
        assert_eq!(branch.document_mode, "both");
        assert_eq!(
            branch.primary_url.as_deref(),
            Some("/api/public/holdings/acme/media-publications/publication-logo/content")
        );

        let unknown = owner_selected(Some("branch-other"));
        let fallback = workspace_brand(&pool, &unknown, &context).await.unwrap();
        assert_eq!(fallback.scope_kind, "holding");
        assert_eq!(fallback.scope_id, "h001");
        assert_eq!(fallback.primary_url, None);

        let document = document_brand(&pool, &owner, &context, Some("branch-1"), None)
            .await
            .unwrap();
        assert_eq!(document.mode, "both");
        assert_eq!(document.company_name, "Acme");
        assert_eq!(document.branch_name.as_deref(), Some("Branch One"));
        assert_eq!(document.holding.scope_kind, "holding");
        assert_eq!(
            document.branch.as_ref().unwrap().primary_url.as_deref(),
            Some("/api/public/holdings/acme/media-publications/publication-logo/content")
        );
        let holding_only = document_brand(&pool, &owner, &context, None, Some("branch"))
            .await
            .unwrap();
        assert_eq!(holding_only.mode, "holding");
        assert_eq!(holding_only.branch, None);
        assert_eq!(holding_only.branch_name, None);
        let requested = document_brand(&pool, &owner, &context, Some("branch-1"), Some("branch"))
            .await
            .unwrap();
        assert_eq!(requested.mode, "branch");
        let not_found = business(
            document_brand(&pool, &owner, &context, Some("branch-other"), None)
                .await
                .unwrap_err(),
        );
        assert_eq!(not_found.kind, FailureKind::NotFound);
        assert_eq!(not_found.code, "BRANCH_NOT_FOUND");

        let profiles = profile_list(&pool, &owner, &context).await.unwrap();
        assert_eq!(profiles.holding.scope_id, "h001");
        assert_eq!(profiles.holding.primary_url, None);
        assert_eq!(profiles.branches.len(), 1);
        assert_eq!(profiles.branches[0].scope_id, "branch-1");
        assert_eq!(profiles.branches[0].document_mode, "both");
        assert_eq!(
            profiles.storefront.short_name, "Legacy storefront"
        );
        assert_eq!(profiles.storefront.accent_color, "#123456");
        assert_eq!(
            profiles.storefront.logo_url.as_deref(),
            Some("/api/public/holdings/acme/media-publications/publication-logo/content")
        );
        assert_eq!(profiles.storefront.identity_revision, None);

        let storefront = storefront_brand(&pool, &owner, &context).await.unwrap();
        assert_eq!(storefront, profiles.storefront);

        let evidence = brand_evidence(&pool, &owner, "branch-1")
            .await
            .unwrap()
            .expect("branch evidence");
        assert_eq!(evidence.id, "branch-1");
        assert_eq!(evidence.branch_name, "Branch One");
        assert_eq!(evidence.document_mode, "both");
        assert_eq!(evidence.updated_at, 3);
        assert_eq!(evidence.primary_attachment_id.as_deref(), Some("attachment-logo"));
        assert_eq!(evidence.inverse_attachment_id, None);
        let rendered = serde_json::to_value(&evidence).unwrap();
        assert_eq!(rendered["branch_name"], "Branch One");
        assert_eq!(rendered["primary_attachment_id"], "attachment-logo");
        assert!(brand_evidence(&pool, &owner, "branch-other").await.unwrap().is_none());

        let unprivileged = BrandActor::new(
            "h001",
            command_actor("company", "owner", "assign-owner", "holding_owner"),
            vec![],
            vec!["records.read".into()],
        );
        assert_eq!(
            code(workspace_brand(&pool, &unprivileged, &context).await.unwrap_err()),
            "MODULE_FORBIDDEN"
        );
    })
    .await;
}

#[tokio::test]
async fn presentation_storefront_uses_the_published_identity_overlay() {
    fixture::with_database(2, |pool| async move {
        migrate(&pool).await;
        seed(&pool).await;
        seed_presentation(&pool).await;
        seed_published_identity(&pool).await;
        let context = presentation_context();
        let owner = owner();

        let storefront = storefront_brand(&pool, &owner, &context).await.unwrap();
        assert_eq!(storefront.short_name, "Published storefront");
        assert_eq!(storefront.accent_color, "#224488");
        assert_eq!(storefront.logo_url, None);
        assert_eq!(storefront.identity_revision, Some(1));

        let profiles = profile_list(&pool, &owner, &context).await.unwrap();
        assert_eq!(profiles.storefront, storefront);
        assert_eq!(
            profiles.branches[0].primary_url.as_deref(),
            Some("/api/public/holdings/acme/media-publications/publication-logo/content")
        );
    })
    .await;
}

fn presentation_profile(
    primary_publication_id: Option<&str>,
    primary_attachment_id: Option<&str>,
    inverse_attachment_id: Option<&str>,
) -> BrandingProfile {
    BrandingProfile {
        scope_kind: "branch".into(),
        scope_id: "branch-1".into(),
        company_id: Some("company".into()),
        primary_attachment_id: primary_attachment_id.map(str::to_owned),
        inverse_attachment_id: inverse_attachment_id.map(str::to_owned),
        primary_publication_id: primary_publication_id.map(str::to_owned),
        inverse_publication_id: None,
        document_mode: "both".into(),
        updated_at: 7,
    }
}

fn presentation_storefront(short_name: Option<&str>, accent_color: Option<&str>) -> BrandingStorefront {
    BrandingStorefront {
        company_id: "company".into(),
        company_slug: "acme".into(),
        company_name: "Acme Company".into(),
        short_name: short_name.map(str::to_owned),
        accent_color: accent_color.map(str::to_owned),
        logo_attachment_id: Some("attach-logo".into()),
        logo_publication_id: Some("pub-row-logo".into()),
        updated_at: 9,
    }
}

fn published_identity(
    short_name: &str,
    accent_color: &str,
    logo_publication_id: Option<&str>,
    revision: i64,
) -> ActiveBrandIdentity {
    ActiveBrandIdentity {
        id: "version-1".into(),
        company_id: "company".into(),
        revision,
        identity_json: "{}".into(),
        draft_id: "draft-1".into(),
        draft_version: 1,
        created_by: "maker".into(),
        published_by: "reviewer".into(),
        published_at: 3,
        active_version: 1,
        identity: BrandIdentity {
            identity: IdentitySection {
                short_name: short_name.into(),
                tagline: "Approved identity".into(),
            },
            visual: VisualSection {
                accent_color: accent_color.into(),
                logo_publication_id: logo_publication_id.map(str::to_owned),
            },
        },
    }
}

#[test]
fn presentation_workspace_brand_prefers_publications_and_keeps_the_legacy_fallback() {
    let urls = presentation_context().media().clone();
    let row = presentation_profile(Some("pub-primary"), Some("attach-primary"), Some("attach-inverse"));
    let brand = brand_presentation(
        BrandScopeKind::Branch,
        "branch-1",
        "Branch One",
        Some(&row),
        &urls,
        false,
    );
    assert_eq!(
        brand.primary_url.as_deref(),
        Some("/api/public/holdings/acme/media-publications/pub-primary/content")
    );
    assert_eq!(
        brand.inverse_url.as_deref(),
        Some("/api/v1/holdings/acme/branches/branch-1/attachments/attach-inverse/content")
    );
    assert_eq!(brand.document_mode, "both");
    assert_eq!(brand.initials, "BO");
    assert_eq!(
        serde_json::to_value(&brand).unwrap(),
        json!({
            "scopeKind": "branch",
            "scopeId": "branch-1",
            "label": "Branch One",
            "initials": "BO",
            "primaryUrl": "/api/public/holdings/acme/media-publications/pub-primary/content",
            "inverseUrl": "/api/v1/holdings/acme/branches/branch-1/attachments/attach-inverse/content",
            "documentMode": "both",
        })
    );
    let legacy = brand_presentation(
        BrandScopeKind::Branch,
        "branch-1",
        "Branch One",
        Some(&row),
        &urls,
        true,
    );
    assert_eq!(legacy.primary_url, brand.primary_url);
    assert_eq!(legacy.inverse_url, None);
    let holding = brand_presentation(
        BrandScopeKind::Holding,
        "h001",
        "Acme Holding",
        None,
        &urls,
        false,
    );
    assert_eq!(holding.document_mode, "holding");
    assert_eq!(holding.primary_url, None);
    assert_eq!(holding.initials, "AH");
}

#[test]
fn presentation_initials_match_the_retained_words_rule() {
    let urls = presentation_context().media().clone();
    let initials = |label: &str| {
        brand_presentation(BrandScopeKind::Holding, "h001", label, None, &urls, false).initials
    };
    assert_eq!(initials("Acme"), "A");
    assert_eq!(initials("Acme Holdings"), "AH");
    assert_eq!(initials("Acme Global Holdings"), "AH");
    assert_eq!(initials("  "), "—");
    assert_eq!(initials("ørsted holdings"), "ØH");
}

#[test]
fn presentation_storefront_overlay_rules() {
    let context = presentation_context();
    let row = presentation_storefront(Some("Legacy storefront"), Some("#123456"));
    let active = published_identity("Published storefront", "#224488", Some("pub-published"), 4);
    let brand =
        storefront_presentation("company", &context, &row, Some(&active), context.media());
    assert_eq!(brand.short_name, "Published storefront");
    assert_eq!(brand.accent_color, "#224488");
    assert_eq!(
        brand.logo_url.as_deref(),
        Some("/api/public/holdings/acme/media-publications/pub-published/content")
    );
    assert_eq!(brand.identity_revision, Some(4));
    let value = serde_json::to_value(&brand).unwrap();
    assert_eq!(value["identityRevision"], 4);
    assert_eq!(value["holdingSlug"], "acme");
    let no_logo = published_identity("Published storefront", "#224488", None, 4);
    let brand = storefront_presentation("company", &context, &row, Some(&no_logo), context.media());
    assert_eq!(brand.logo_url, None);
}

#[test]
fn presentation_storefront_falls_back_to_the_retained_rows_and_defaults() {
    let retained = presentation_context();
    let brand = storefront_presentation(
        "company",
        &retained,
        &presentation_storefront(Some("  Legacy storefront  "), Some("#123456")),
        None,
        retained.media(),
    );
    assert_eq!(brand.short_name, "Legacy storefront");
    assert_eq!(brand.accent_color, "#123456");
    assert_eq!(brand.identity_revision, None);
    assert!(serde_json::to_value(&brand).unwrap().get("identityRevision").is_none());

    let derived = storefront_presentation(
        "company",
        &retained,
        &presentation_storefront(Some("   "), None),
        None,
        retained.media(),
    );
    assert_eq!(derived.short_name, "Acme Company");
    assert_eq!(derived.accent_color, "#3659c9");
    assert_eq!(
        derived.logo_url.as_deref(),
        Some("/api/public/holdings/acme/media-publications/pub-row-logo/content")
    );

    let legacy = BrandPresentationContext::new(
        "Acme Holding",
        "acme",
        "/api/v1/holdings/acme/branches/branch-1",
        true,
    )
    .unwrap();
    let attachment_only = BrandingStorefront {
        logo_publication_id: None,
        ..presentation_storefront(None, None)
    };
    let hidden =
        storefront_presentation("company", &legacy, &attachment_only, None, legacy.media());
    assert_eq!(hidden.logo_url, None);
    let readable =
        storefront_presentation("company", &retained, &attachment_only, None, retained.media());
    assert_eq!(
        readable.logo_url.as_deref(),
        Some("/api/v1/holdings/acme/branches/branch-1/attachments/attach-logo/content")
    );

    let long_name = BrandingStorefront {
        company_name: "Acme Company With A Very Long Legal Display Name Indeed".into(),
        ..presentation_storefront(None, None)
    };
    let wide = storefront_presentation("company", &retained, &long_name, None, retained.media());
    assert_eq!(wide.short_name, "Acme Company With A Very Long Legal Disp");
}

#[test]
fn presentation_media_urls_escape_identifiers_and_the_context_fails_closed() {
    let urls = presentation_context().media().clone();
    assert_eq!(
        urls.publication(Some("publication/one?x")).as_deref(),
        Some("/api/public/holdings/acme/media-publications/publication%2Fone%3Fx/content")
    );
    assert_eq!(urls.publication(Some("")), None);
    assert_eq!(urls.attachment(None), None);
    for (name, slug, base) in [
        ("", "acme", "/api/v1/holdings/acme"),
        ("Acme", "Acme", "/api/v1/holdings/acme"),
        ("Acme", "", "/api/v1/holdings/acme"),
        ("Acme", "acme", "api/v1/holdings/acme"),
        ("Acme", "acme", "/api/v1/holdings/acme?x=1"),
        ("Acme", "acme", "/"),
    ] {
        assert!(
            BrandPresentationContext::new(name, slug, base, false).is_err(),
            "{name:?}/{slug:?}/{base:?}"
        );
    }
    let trimmed = BrandPresentationContext::new(
        "  Acme Holding  ",
        " acme ",
        "/api/v1/holdings/acme/branches/branch-1/",
        true,
    )
    .unwrap();
    assert_eq!(trimmed.holding_name(), "Acme Holding");
    assert_eq!(trimmed.holding_slug(), "acme");
    assert!(trimmed.legacy_media_readable());
    assert_eq!(
        trimmed.media().attachment(Some("attach-1")).as_deref(),
        Some("/api/v1/holdings/acme/branches/branch-1/attachments/attach-1/content")
    );
}
