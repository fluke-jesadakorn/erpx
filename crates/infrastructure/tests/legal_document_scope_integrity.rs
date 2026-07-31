//! Live Legal projection/read regression for immutable document-version scope bindings.

use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::legal::{LegalDocument, LegalScope};
use erpx_infrastructure::{
    LegalReadScope, SqlxLegalPersistence, SqlxLegalProjectionStore, SqlxLegalReadStore,
};
use sqlx::PgPool;
use uuid::Uuid;

fn context(actor: &str, scope: TenantScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: actor.into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "legal.document.scope.integrity",
    )
    .expect("context")
}

#[tokio::test]
#[ignore = "requires disposable tenant write and read PostgreSQL databases"]
async fn document_version_scope_drift_fails_read_and_projector_closed_with_rollback() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migration");
    erpx_migration::migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migration");

    let holding_ref = format!("holding-legal-document-scope-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = LegalScope::new(holding_id, company_id, None).expect("Legal scope");
    let create_context = context("creator", scope.tenant_scope(), "legal-document-create");
    let mut document = LegalDocument::new(
        Uuid::new_v4(),
        scope.clone(),
        "Scoped document",
        &create_context,
    )
    .expect("document");
    document
        .add_version(
            "opaque/legal/document-v1",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &create_context,
        )
        .expect("first document version");

    let source = SqlxLegalPersistence::new(write_pool.clone(), &holding_ref).expect("source");
    source
        .save_document(&document, &create_context, None)
        .await
        .expect("persist first document event");
    let projection = SqlxLegalProjectionStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("projection");
    let first_page = projection
        .read_source_event_page(None, 10)
        .await
        .expect("first source page");
    assert_eq!(first_page.events.len(), 1);
    projection
        .project_event(&first_page.events[0])
        .await
        .expect("project first document event");

    let wrong_holding_id = Uuid::new_v4();
    let mut corrupt_tx = read_pool.begin().await.expect("corruption transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *corrupt_tx)
        .await
        .expect("corruption holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.legal_document_version_read_rows
         SET holding_id=$1
         WHERE holding_ref=$2 AND document_id=$3 AND version=1",
    )
    .bind(wrong_holding_id)
    .bind(&holding_ref)
    .bind(document.id)
    .execute(&mut *corrupt_tx)
    .await
    .expect("corrupt document-version holding binding");
    corrupt_tx.commit().await.expect("corruption commit");

    let read = SqlxLegalReadStore::new(read_pool.clone(), &holding_ref).expect("read store");
    let read_scope = LegalReadScope {
        holding_id,
        company_id,
        branch_id: None,
    };
    let read_error = read
        .list_documents(read_scope, 10, None)
        .await
        .expect_err("scope-drifted child row must not be readable");
    assert!(
        read_error
            .to_string()
            .contains("document version holding binding is inconsistent")
    );

    let update_context = context("creator", scope.tenant_scope(), "legal-document-update");
    document
        .add_version(
            "opaque/legal/document-v2",
            "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            &update_context,
        )
        .expect("second document version");
    source
        .save_document(&document, &update_context, Some(1))
        .await
        .expect("persist second document event");
    let events = projection
        .read_source_event_page(None, 10)
        .await
        .expect("document source page");
    let second_event = events
        .events
        .iter()
        .find(|event| event.aggregate_version == 2)
        .expect("second document event");
    let projector_error = projection
        .project_event(second_event)
        .await
        .expect_err("projector must reject the drifted immutable version");
    assert!(
        projector_error
            .to_string()
            .contains("document version is immutable")
    );

    let mut checkpoint_tx = read_pool
        .begin()
        .await
        .expect("checkpoint verification transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *checkpoint_tx)
        .await
        .expect("checkpoint verification holding scope");
    let checkpoint_version: i64 = sqlx::query_scalar(
        "SELECT source_version
         FROM erpx_tenant.legal_projection_checkpoints
         WHERE holding_ref=$1 AND projection_name='legal_documents'
           AND aggregate_type='legal_document' AND aggregate_id=$2",
    )
    .bind(&holding_ref)
    .bind(document.id)
    .fetch_one(&mut *checkpoint_tx)
    .await
    .expect("checkpoint remains at the last committed event");
    checkpoint_tx
        .commit()
        .await
        .expect("checkpoint verification commit");
    assert_eq!(checkpoint_version, 1);

    let mut repair_tx = read_pool.begin().await.expect("repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *repair_tx)
        .await
        .expect("repair holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.legal_document_version_read_rows
         SET holding_id=$1
         WHERE holding_ref=$2 AND document_id=$3 AND version=1",
    )
    .bind(holding_id)
    .bind(&holding_ref)
    .bind(document.id)
    .execute(&mut *repair_tx)
    .await
    .expect("repair document-version holding binding");
    repair_tx.commit().await.expect("repair commit");

    assert!(
        projection
            .project_event(second_event)
            .await
            .expect("replay after repair")
            .applied
    );
    let documents = read
        .list_documents(read_scope, 10, None)
        .await
        .expect("repaired document read");
    assert_eq!(documents.rows[0].versions.len(), 2);
    assert!(
        projection
            .reconcile_projection()
            .await
            .expect("reconciliation after repair")
            .is_consistent()
    );
}
