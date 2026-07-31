//! Quality read-row scope binding and projector rollback acceptance.
//!
//! This deliberately corrupts only disposable read-model metadata. The test
//! proves that a payload cannot be served or advanced when its relational
//! company/branch scope has drifted from the authoritative aggregate.

use chrono::{Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::quality::{QualityInspection, QualityInspectionKind};
use erpx_infrastructure::SqlxQualityStore;
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn context(scope: TenantScope, key: &str, expected: Option<u64>) -> CommandContext {
    let mut value = CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "quality-scope-integrity".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "quality.persistence.scope-integrity",
    )
    .expect("quality command context");
    value.expected_aggregate_version = expected;
    value
}

async fn set_holding_ref<'a>(tx: &mut sqlx::Transaction<'a, sqlx::Postgres>, holding_ref: &str) {
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut **tx)
        .await
        .expect("set read holding scope");
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases"]
async fn quality_read_and_projector_fail_closed_on_scope_drift() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url)
        .await
        .expect("quality write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("quality read pool");
    erpx_migration::migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");
    erpx_migration::migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let holding_ref = format!("holding-quality-scope-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = TenantScope::Company {
        holding_id,
        company_id,
    };
    let inspection_id = Uuid::new_v4();
    let fixture_suffix = Uuid::new_v4().simple().to_string();
    let create_context = context(
        scope.clone(),
        &format!("quality-scope-create-{fixture_suffix}"),
        Some(0),
    );
    let inspection = QualityInspection::new(
        inspection_id,
        scope.clone(),
        holding_id,
        company_id,
        None,
        QualityInspectionKind::Incoming,
        Uuid::new_v4(),
        Some("LOT-QUALITY-SCOPE".into()),
        10,
        Utc::now(),
        &create_context,
    )
    .expect("quality inspection");
    let store = SqlxQualityStore::new(write_pool.clone(), read_pool.clone(), holding_ref.clone())
        .expect("quality store");

    let created = store
        .save(&inspection, Some(0), &create_context)
        .await
        .expect("quality source write and outbox");
    assert_eq!(created.version, 1);
    let first_page = store
        .read_source_event_page(None, 10)
        .await
        .expect("authoritative quality outbox page");
    assert_eq!(first_page.events.len(), 1);
    store
        .project_event(
            first_page.events[0].event_id,
            first_page.events[0].aggregate_id,
            first_page.events[0].aggregate_version,
            &first_page.events[0].payload,
        )
        .await
        .expect("initial quality projection");

    let forged_company_id = Uuid::new_v4();
    let mut read_tx = read_pool.begin().await.expect("begin read corruption");
    set_holding_ref(&mut read_tx, &holding_ref).await;
    sqlx::query(
        "UPDATE erpx_tenant.quality_inspection_read_rows
         SET company_id=$3
         WHERE holding_ref=$1 AND inspection_id=$2",
    )
    .bind(&holding_ref)
    .bind(inspection_id)
    .bind(forged_company_id)
    .execute(&mut *read_tx)
    .await
    .expect("corrupt disposable quality scope");
    read_tx.commit().await.expect("commit read corruption");

    let read_error = store
        .get(inspection_id)
        .await
        .expect_err("scope-drifted quality payload must not be served");
    assert!(
        read_error
            .to_string()
            .contains("quality read row scope mismatch")
    );

    let start_context = context(
        scope.clone(),
        &format!("quality-scope-start-{fixture_suffix}"),
        Some(1),
    );
    let mut started = inspection.clone();
    started
        .start(Utc::now() + Duration::seconds(1), &start_context)
        .expect("start quality inspection");
    store
        .save(&started, Some(1), &start_context)
        .await
        .expect("quality source transition and outbox");
    let page = store
        .read_source_event_page(None, 10)
        .await
        .expect("quality transition outbox page");
    assert_eq!(page.events.len(), 2);

    let projection_error = store
        .project_event(
            page.events[1].event_id,
            page.events[1].aggregate_id,
            page.events[1].aggregate_version,
            &page.events[1].payload,
        )
        .await
        .expect_err("scope-drifted quality row must not advance");
    assert!(
        projection_error
            .to_string()
            .contains("quality projection row scope mismatch")
    );

    let mut checkpoint_tx = read_pool.begin().await.expect("begin checkpoint audit");
    set_holding_ref(&mut checkpoint_tx, &holding_ref).await;
    let checkpoint = sqlx::query(
        "SELECT source_version, last_event_id
         FROM erpx_tenant.quality_projection_checkpoints
         WHERE holding_ref=$1 AND inspection_id=$2",
    )
    .bind(&holding_ref)
    .bind(inspection_id)
    .fetch_one(&mut *checkpoint_tx)
    .await
    .expect("quality checkpoint after rejected projection");
    assert_eq!(checkpoint.try_get::<i64, _>("source_version").unwrap(), 1);
    assert_eq!(
        checkpoint.try_get::<Uuid, _>("last_event_id").unwrap(),
        first_page.events[0].event_id
    );
    checkpoint_tx
        .rollback()
        .await
        .expect("rollback checkpoint audit");

    let mut repair_tx = read_pool.begin().await.expect("begin read repair");
    set_holding_ref(&mut repair_tx, &holding_ref).await;
    sqlx::query(
        "UPDATE erpx_tenant.quality_inspection_read_rows
         SET company_id=$3
         WHERE holding_ref=$1 AND inspection_id=$2",
    )
    .bind(&holding_ref)
    .bind(inspection_id)
    .bind(company_id)
    .execute(&mut *repair_tx)
    .await
    .expect("repair disposable quality scope");
    repair_tx.commit().await.expect("commit read repair");

    assert!(
        store
            .project_event(
                page.events[1].event_id,
                page.events[1].aggregate_id,
                page.events[1].aggregate_version,
                &page.events[1].payload,
            )
            .await
            .expect("replay quality transition after repair")
            .applied
    );
    assert!(
        store
            .reconcile()
            .await
            .expect("quality reconciliation after repair")
            .is_consistent()
    );
}
