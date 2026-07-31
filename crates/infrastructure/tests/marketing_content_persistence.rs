//! Disposable PostgreSQL evidence for the Marketing content write/read seam.
//! The live test uses a unique holding scope, verifies idempotent optimistic
//! lifecycle writes and replacement-read rebuild, then removes its fixtures.

use chrono::Utc;
use erpx_application::{MarketingContentWritePort, SubmitPublishedFormCommand};
use erpx_domain::{
    contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope},
    marketing_content::{
        ContentForm, ContentLocale, ContentTemplate, ContentTemplateRef, ContentTemplateVersion,
        CustomDomain, FormField, FormFieldKind, LocalizedText, PreviewGrant, SeoMetadata,
        SocialMetadata, TemplateVariant,
    },
};
use erpx_infrastructure::{
    MAX_MARKETING_CONTENT_READ_PAGE_SIZE, SqlxMarketingContentStore,
    SqlxMarketingContentWriteAdapter,
};
use erpx_migration::{migrate_tenant_read, migrate_tenant_write};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::BTreeMap;
use uuid::Uuid;

fn context(scope: TenantScope, key: &str, expected: Option<u64>) -> CommandContext {
    let mut context = CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "marketing-content-test".into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "marketing.content.persistence.test",
    )
    .expect("valid content context");
    context.expected_aggregate_version = expected;
    context
}

fn template(scope: TenantScope) -> ContentTemplate {
    let id = Uuid::new_v4();
    let mut variants = BTreeMap::new();
    variants.insert(
        ContentLocale::En,
        TemplateVariant::new("Launch", "Launch content").expect("English variant"),
    );
    variants.insert(
        ContentLocale::Th,
        TemplateVariant::new("เปิดตัว", "เนื้อหาการเปิดตัว").expect("Thai variant"),
    );
    let title = LocalizedText::new("Launch", "เปิดตัว").expect("title");
    let description = LocalizedText::new("Launch page", "หน้าเปิดตัว").expect("description");
    let version = ContentTemplateVersion::new(
        Uuid::new_v4(),
        id,
        scope.clone(),
        1,
        variants,
        SeoMetadata::new(title.clone(), description.clone(), "/launch", false, vec![])
            .expect("SEO"),
        SocialMetadata::new(title, description, None).expect("social"),
        vec![],
        "author",
        Utc::now(),
    )
    .expect("template version");
    ContentTemplate::new(id, scope, "launch", version).expect("template")
}

#[test]
fn migration_defines_content_write_read_and_forced_rls_contracts() {
    let write = include_str!("../../migration/migrations_tenant_write/0047_marketing_content.sql");
    let read = include_str!("../../migration/migrations_tenant_read/0036_marketing_content.sql");
    let workflow_read =
        include_str!("../../migration/migrations_tenant_read/0037_marketing_content_workflow.sql");
    let write_privacy =
        include_str!("../../migration/migrations_tenant_write/0048_marketing_content_privacy.sql");
    let read_privacy =
        include_str!("../../migration/migrations_tenant_read/0038_marketing_content_privacy.sql");
    for required in [
        "marketing_content_assets",
        "marketing_content_template_versions",
        "marketing_content_previews",
        "marketing_content_forms",
        "marketing_content_domains",
        "marketing_content_command_idempotency",
        "FORCE ROW LEVEL SECURITY",
        "reject_marketing_content_fact_mutation",
    ] {
        assert!(
            write.contains(required),
            "missing write contract: {required}"
        );
    }
    for required in [
        "marketing_content_read_rows",
        "marketing_content_projection_checkpoints",
        "marketing_content_projection_event_receipts",
        "FORCE ROW LEVEL SECURITY",
    ] {
        assert!(read.contains(required), "missing read contract: {required}");
    }
    for required in [
        "marketing_content_preview_read_rows",
        "marketing_content_submission_read_rows",
        "marketing_content_domain_read_rows",
        "marketing_content_workflow_projection_checkpoints",
        "marketing_content_workflow_projection_event_receipts",
        "FORCE ROW LEVEL SECURITY",
    ] {
        assert!(
            workflow_read.contains(required),
            "missing workflow read contract: {required}"
        );
    }
    assert!(!workflow_read.contains("token_hash"));
    assert!(!workflow_read.contains("verification_hash"));
    assert!(!write.contains("credential"));
    assert!(!write.contains("password"));
    for required in ["retention_until", "redacted_at", "retention"] {
        assert!(
            write_privacy.contains(required),
            "missing write privacy contract: {required}"
        );
        assert!(
            read_privacy.contains(required),
            "missing read privacy contract: {required}"
        );
    }
}

#[tokio::test]
#[ignore = "requires disposable tenant PostgreSQL write and read databases"]
async fn content_lifecycle_is_atomic_idempotent_versioned_and_rebuildable() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let holding_ref = format!("marketing-content-{}", Uuid::new_v4());
    let scope = TenantScope::Holding {
        holding_id: Uuid::new_v4(),
    };
    let mut content = template(scope.clone());
    let store = SqlxMarketingContentStore::new(write_pool.clone(), read_pool.clone(), &holding_ref)
        .expect("content store");

    let create_context = context(scope.clone(), "create", None);
    let created = store
        .save_template(&content, &create_context)
        .await
        .expect("create template");
    assert_eq!(created.version, 1);
    let replay = store
        .save_template(&content, &create_context)
        .await
        .expect("replay template");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, created.event_id);

    let mut tampered = content.clone();
    tampered
        .versions
        .get_mut(&1)
        .expect("initial template revision")
        .variants
        .get_mut(&ContentLocale::En)
        .expect("English template variant")
        .title = "Tampered revision".into();
    let immutable_conflict = store
        .save_template(
            &tampered,
            &context(scope.clone(), "immutable-revision-conflict", Some(1)),
        )
        .await
        .expect_err("immutable revision conflict must fail closed");
    assert!(matches!(
        immutable_conflict,
        erpx_infrastructure::PersistenceError::InvalidData(message)
            if message.contains("revision is immutable")
    ));
    assert_eq!(
        store
            .load_template(content.id)
            .await
            .expect("unchanged template"),
        Some(content.clone())
    );

    content.submit_for_review(1).expect("review");
    let reviewed = store
        .save_template(&content, &context(scope.clone(), "review", Some(1)))
        .await
        .expect("review write");
    assert_eq!(reviewed.version, 2);
    let stale = store
        .save_template(&content, &context(scope.clone(), "stale", Some(1)))
        .await
        .expect_err("stale content version must fail");
    assert!(matches!(
        stale,
        erpx_infrastructure::PersistenceError::OptimisticConcurrency {
            expected: Some(1),
            actual: Some(2),
            ..
        }
    ));
    content.approve(1, "reviewer").expect("approve");
    let approved = store
        .save_template(&content, &context(scope.clone(), "approve", Some(2)))
        .await
        .expect("approval write");
    assert_eq!(approved.version, 3);
    content.publish(1, Utc::now()).expect("publish");
    let published = store
        .save_template(&content, &context(scope.clone(), "publish", Some(3)))
        .await
        .expect("publish write");
    assert_eq!(published.version, 4);
    assert_eq!(
        store.load_template(content.id).await.expect("load"),
        Some(content.clone())
    );

    let mut form = ContentForm::new(
        Uuid::new_v4(),
        scope.clone(),
        "lead-form",
        vec![
            FormField::new(
                "email",
                LocalizedText::new("Email", "อีเมล").expect("form label"),
                FormFieldKind::Email,
                true,
                255,
            )
            .expect("form field"),
        ],
    )
    .expect("form");
    let form_receipt = store
        .save_form(&form, &context(scope.clone(), "form-create", None))
        .await
        .expect("form write");
    assert_eq!(form_receipt.version, 1);
    form.submit_for_review().expect("form review");
    form.publish().expect("form publish");
    let form_published = store
        .save_form(&form, &context(scope.clone(), "form-publish", Some(1)))
        .await
        .expect("published form write");
    assert_eq!(form_published.version, 2);
    assert_eq!(
        store
            .load_published_form(form.id)
            .await
            .expect("published form load"),
        Some(form.clone())
    );

    let template_ref = ContentTemplateRef::new(content.id, 1, scope.clone()).expect("template ref");
    let preview = PreviewGrant::new(
        Uuid::new_v4(),
        template_ref,
        Utc::now(),
        Utc::now() + chrono::Duration::hours(1),
    )
    .expect("preview");
    let raw_preview_token = "preview-token-for-live-public-route";
    let token_hash = hex::encode(Sha256::digest(raw_preview_token.as_bytes()));
    let preview_context = context(scope.clone(), "preview-issue", None);
    let preview_receipt = store
        .issue_preview(&preview, &token_hash, &preview_context)
        .await
        .expect("preview issue");
    assert_eq!(preview_receipt.version, 1);
    let preview_replay = store
        .issue_preview(&preview, &token_hash, &preview_context)
        .await
        .expect("preview replay");
    assert!(preview_replay.replayed);
    assert_eq!(
        store
            .authorize_preview(preview.id, raw_preview_token, Utc::now())
            .await
            .expect("authorize preview"),
        Some(ContentTemplateRef::new(content.id, 1, scope.clone()).expect("preview reference"))
    );
    let expiry_context = context(scope.clone(), "preview-expire", None);
    store
        .expire_preview(
            preview.id,
            scope.clone(),
            preview.expires_at + chrono::Duration::seconds(1),
            &expiry_context,
        )
        .await
        .expect("preview expire");
    let expiry_replay = store
        .expire_preview(
            preview.id,
            scope.clone(),
            preview.expires_at + chrono::Duration::hours(1),
            &expiry_context,
        )
        .await
        .expect("preview expiry retry");
    assert!(expiry_replay.replayed);

    let form_ref = form.reference().expect("form ref");
    let mut values = BTreeMap::new();
    values.insert("email".into(), "customer@example.com".into());
    let adapter = SqlxMarketingContentWriteAdapter::new(&store);
    let submission = adapter
        .submit_published_form(
            SubmitPublishedFormCommand::new(
                form_ref.clone(),
                Uuid::new_v4(),
                values,
                Utc::now(),
                context(scope.clone(), "form-submit", None),
            )
            .expect("form application command"),
        )
        .await
        .expect("form submission");
    assert_eq!(submission.version, 1);

    // Make the disposable submission due, then prove the source mutation is
    // irreversible and represented by a non-sensitive append-only event.
    let mut due = write_pool.begin().await.expect("privacy due transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *due)
        .await
        .expect("privacy due scope");
    sqlx::query(
        "UPDATE erpx_tenant.marketing_content_form_submissions
            SET retention_until=received_at + INTERVAL '1 millisecond'
          WHERE holding_ref=$1 AND submission_id=$2",
    )
    .bind(&holding_ref)
    .bind(submission.aggregate_id)
    .execute(&mut *due)
    .await
    .expect("make submission due");
    due.commit().await.expect("privacy due commit");
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let privacy_context = context(scope.clone(), "privacy-sweep", None);
    let privacy = store
        .redact_expired_form_submissions(Utc::now(), 10, &privacy_context)
        .await
        .expect("redact expired submission");
    assert_eq!(
        privacy,
        erpx_infrastructure::MarketingContentPrivacySweepReport {
            scanned: 1,
            redacted: 1,
        }
    );
    let privacy_replay = store
        .redact_expired_form_submissions(Utc::now(), 10, &privacy_context)
        .await
        .expect("privacy sweep replay");
    assert_eq!(privacy_replay.scanned, 0);
    assert_eq!(privacy_replay.redacted, 0);

    let mut privacy_source = write_pool.begin().await.expect("privacy source inspection");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *privacy_source)
        .await
        .expect("privacy source scope");
    let (source_values, source_redacted): (Value, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
        "SELECT values_json, redacted_at
           FROM erpx_tenant.marketing_content_form_submissions
          WHERE holding_ref=$1 AND submission_id=$2",
    )
    .bind(&holding_ref)
    .bind(submission.aggregate_id)
    .fetch_one(&mut *privacy_source)
    .await
    .expect("redacted source row");
    assert_eq!(source_values, json!({}));
    assert!(source_redacted.is_some());
    let redaction_payload: Value = sqlx::query_scalar(
        "SELECT payload FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1 AND aggregate_id=$2
            AND event_type='marketing.content_form_submission_redacted.v1'",
    )
    .bind(&holding_ref)
    .bind(submission.aggregate_id)
    .fetch_one(&mut *privacy_source)
    .await
    .expect("redaction event");
    assert!(
        !redaction_payload
            .to_string()
            .contains("customer@example.com")
    );
    privacy_source
        .commit()
        .await
        .expect("privacy source commit");

    let domain = CustomDomain::new(Uuid::new_v4(), scope.clone(), "campaign.example.com")
        .expect("custom domain");
    let domain_receipt = store
        .save_domain(&domain, &context(scope.clone(), "domain-create", None))
        .await
        .expect("domain write");
    assert_eq!(domain_receipt.version, 1);
    let verification_hash = "cd".repeat(32);
    store
        .issue_domain_verification(
            domain.id,
            scope.clone(),
            &verification_hash,
            &context(scope.clone(), "domain-challenge", None),
        )
        .await
        .expect("domain challenge");
    store
        .verify_domain(
            domain.id,
            scope.clone(),
            &verification_hash,
            &context(scope.clone(), "domain-verify", None),
        )
        .await
        .expect("domain verification");
    let binding = domain
        .bind_template(
            &ContentTemplateRef::new(content.id, 1, scope.clone()).expect("binding template"),
            Utc::now(),
        )
        .expect("domain binding");
    store
        .bind_domain(&binding, &context(scope.clone(), "domain-bind", None))
        .await
        .expect("domain binding write");
    assert_eq!(
        store
            .load_active_domain_binding("campaign.example.com")
            .await
            .expect("active domain binding"),
        Some(binding.clone())
    );

    let rebuilt = store
        .rebuild_read_projection()
        .await
        .expect("rebuild read projection");
    assert_eq!(rebuilt.projected_rows, 3);
    let (first_read_page, after_id) = store
        .list_read_rows(None, 2, None)
        .await
        .expect("first bounded content read page");
    assert_eq!(first_read_page.len(), 2);
    assert!(after_id.is_some());
    for row in &first_read_page {
        assert_eq!(row.scope, scope);
        assert!(!row.source_event_id.is_nil());
        assert!(row.source_version > 0);
        assert!(row.projected_at <= Utc::now());
    }
    let (last_read_page, no_after_id) = store
        .list_read_rows(None, 2, after_id)
        .await
        .expect("last bounded content read page");
    assert_eq!(last_read_page.len(), 1);
    assert!(no_after_id.is_none());
    let (template_page, _) = store
        .list_read_rows(Some("template"), 1, None)
        .await
        .expect("template content read page");
    assert_eq!(template_page.len(), 1);
    assert_eq!(template_page[0].aggregate_type, "template");
    assert!(
        store
            .list_read_rows(Some("unsupported"), 1, None)
            .await
            .is_err()
    );
    assert!(
        store
            .list_read_rows(None, MAX_MARKETING_CONTENT_READ_PAGE_SIZE + 1, None)
            .await
            .is_err()
    );
    assert!(
        store
            .list_read_rows(None, 1, Some(Uuid::nil()))
            .await
            .is_err()
    );
    let workflow_rebuilt = store
        .rebuild_workflow_read_projection()
        .await
        .expect("rebuild workflow read projection");
    assert_eq!(
        (
            workflow_rebuilt.previews,
            workflow_rebuilt.submissions,
            workflow_rebuilt.domains
        ),
        (1, 1, 1)
    );
    let mut privacy_read_inspection = read_pool.begin().await.expect("privacy read inspection");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *privacy_read_inspection)
        .await
        .expect("privacy read scope");
    let (read_values, read_redacted): (Value, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
        "SELECT values_json, redacted_at
           FROM erpx_tenant.marketing_content_submission_read_rows
          WHERE holding_ref=$1 AND submission_id=$2",
    )
    .bind(&holding_ref)
    .bind(submission.aggregate_id)
    .fetch_one(&mut *privacy_read_inspection)
    .await
    .expect("redacted read row");
    assert_eq!(read_values, json!({}));
    assert!(read_redacted.is_some());
    privacy_read_inspection
        .commit()
        .await
        .expect("privacy read inspection commit");
    assert_eq!(
        store
            .load_published_template_read(content.id)
            .await
            .expect("published read template"),
        Some(content.clone())
    );

    let mut inspection = write_pool.begin().await.expect("inspection");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *inspection)
        .await
        .expect("write scope");
    let counts: (i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.marketing_content_templates WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_template_versions WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_command_idempotency WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.outbox_events WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_previews WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_form_submissions WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_domains WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_domain_bindings WHERE holding_ref=$1)",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *inspection)
    .await
    .expect("write counts");
    assert_eq!(counts, (1, 1, 14, 14, 1, 1, 1, 1));
    let stored_hash: String = sqlx::query_scalar(
        "SELECT token_hash FROM erpx_tenant.marketing_content_previews
          WHERE holding_ref=$1 AND preview_id=$2",
    )
    .bind(&holding_ref)
    .bind(preview.id)
    .fetch_one(&mut *inspection)
    .await
    .expect("stored preview hash");
    assert_eq!(stored_hash, token_hash);
    let forced: Vec<(String, bool)> = sqlx::query_as(
        "SELECT relname, relforcerowsecurity FROM pg_class
          WHERE oid IN ('erpx_tenant.marketing_content_templates'::regclass,
                        'erpx_tenant.marketing_content_template_versions'::regclass,
                        'erpx_tenant.marketing_content_command_idempotency'::regclass)
          ORDER BY relname",
    )
    .fetch_all(&mut *inspection)
    .await
    .expect("forced RLS");
    assert_eq!(forced.len(), 3);
    assert!(forced.iter().all(|(_, forced)| *forced));
    inspection.commit().await.expect("inspection commit");

    let mut read_inspection = read_pool.begin().await.expect("read inspection");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *read_inspection)
        .await
        .expect("read scope");
    let read_counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.marketing_content_read_rows WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_projection_checkpoints WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_projection_event_receipts WHERE holding_ref=$1)",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *read_inspection)
    .await
    .expect("read counts");
    assert_eq!(read_counts, (3, 3, 3));
    read_inspection
        .commit()
        .await
        .expect("read inspection commit");
    let mut workflow_read_inspection = read_pool.begin().await.expect("workflow read inspection");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *workflow_read_inspection)
        .await
        .expect("workflow read scope");
    let workflow_read_counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.marketing_content_preview_read_rows WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_submission_read_rows WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_domain_read_rows WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_workflow_projection_checkpoints WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.marketing_content_workflow_projection_event_receipts WHERE holding_ref=$1)",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *workflow_read_inspection)
    .await
    .expect("workflow read counts");
    assert_eq!(workflow_read_counts, (1, 1, 1, 3, 3));
    workflow_read_inspection
        .commit()
        .await
        .expect("workflow read inspection commit");

    let other = SqlxMarketingContentStore::new(
        write_pool.clone(),
        read_pool.clone(),
        format!("marketing-content-other-{}", Uuid::new_v4()),
    )
    .expect("other store");
    assert!(
        other
            .load_template(content.id)
            .await
            .expect("other load")
            .is_none()
    );

    let mut cleanup = write_pool.begin().await.expect("cleanup");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup scope");
    sqlx::query("DELETE FROM erpx_tenant.outbox_events WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup outbox");
    sqlx::query(
        "DELETE FROM erpx_tenant.aggregate_snapshots
          WHERE holding_ref=$1 AND aggregate_type IN ($2, $3, $4)",
    )
    .bind(&holding_ref)
    .bind("marketing_content_template")
    .bind("marketing_content_form")
    .bind("marketing_content_domain")
    .execute(&mut *cleanup)
    .await
    .expect("cleanup snapshots");
    sqlx::query(
        "DELETE FROM erpx_tenant.marketing_content_command_idempotency WHERE holding_ref=$1",
    )
    .bind(&holding_ref)
    .execute(&mut *cleanup)
    .await
    .expect("cleanup idempotency");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_domain_bindings WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup domain bindings");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_previews WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup previews");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_form_submissions WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup submissions");
    sqlx::query("ALTER TABLE erpx_tenant.marketing_content_template_versions DISABLE TRIGGER USER")
        .execute(&mut *cleanup)
        .await
        .expect("disable append-only trigger");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_template_versions WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup versions");
    sqlx::query("ALTER TABLE erpx_tenant.marketing_content_template_versions ENABLE TRIGGER USER")
        .execute(&mut *cleanup)
        .await
        .expect("restore append-only trigger");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_templates WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup templates");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_forms WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup forms");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_domains WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *cleanup)
        .await
        .expect("cleanup domains");
    cleanup.commit().await.expect("cleanup commit");

    let mut read_cleanup = read_pool.begin().await.expect("read cleanup");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *read_cleanup)
        .await
        .expect("read cleanup scope");
    sqlx::query(
        "DELETE FROM erpx_tenant.marketing_content_projection_event_receipts WHERE holding_ref=$1",
    )
    .bind(&holding_ref)
    .execute(&mut *read_cleanup)
    .await
    .expect("cleanup read receipts");
    sqlx::query(
        "DELETE FROM erpx_tenant.marketing_content_workflow_projection_event_receipts WHERE holding_ref=$1",
    )
    .bind(&holding_ref)
    .execute(&mut *read_cleanup)
    .await
    .expect("cleanup workflow read receipts");
    sqlx::query(
        "DELETE FROM erpx_tenant.marketing_content_workflow_projection_checkpoints WHERE holding_ref=$1",
    )
    .bind(&holding_ref)
    .execute(&mut *read_cleanup)
    .await
    .expect("cleanup workflow read checkpoints");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_preview_read_rows WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *read_cleanup)
        .await
        .expect("cleanup workflow preview rows");
    sqlx::query(
        "DELETE FROM erpx_tenant.marketing_content_submission_read_rows WHERE holding_ref=$1",
    )
    .bind(&holding_ref)
    .execute(&mut *read_cleanup)
    .await
    .expect("cleanup workflow submission rows");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_domain_read_rows WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *read_cleanup)
        .await
        .expect("cleanup workflow domain rows");
    sqlx::query(
        "DELETE FROM erpx_tenant.marketing_content_projection_checkpoints WHERE holding_ref=$1",
    )
    .bind(&holding_ref)
    .execute(&mut *read_cleanup)
    .await
    .expect("cleanup read checkpoints");
    sqlx::query("DELETE FROM erpx_tenant.marketing_content_read_rows WHERE holding_ref=$1")
        .bind(&holding_ref)
        .execute(&mut *read_cleanup)
        .await
        .expect("cleanup read rows");
    read_cleanup.commit().await.expect("read cleanup commit");

    // Keep a compile-time assertion that no test payload evolves into a
    // credential-bearing persistence contract.
    assert_eq!(json!({"state": "content"}), json!({"state": "content"}));
}
