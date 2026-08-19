//! Disposable two-database acceptance for the Quality source/read boundary.

use chrono::{Duration, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::quality::{QualityInspection, QualityInspectionKind};
use erpx_infrastructure::{QualityReadScope, SqlxQualityStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope, key: &str, expected: Option<u64>) -> CommandContext {
    let mut value = CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "quality-e2e-actor".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "quality.persistence.acceptance",
    )
    .expect("quality command context");
    value.expected_aggregate_version = expected;
    value
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn quality_write_outbox_projection_read_reconcile_and_rebuild() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url)
        .await
        .expect("quality write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("quality read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migrations");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migrations");

    let data_profile_key = format!("holding-quality-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let inspection_id = Uuid::new_v4();
    let created_context = context(scope.clone(), "quality-create", Some(0));
    let inspection = QualityInspection::new(
        inspection_id,
        scope.clone(),
        holding_id,
        company_id,
        None,
        QualityInspectionKind::Incoming,
        Uuid::new_v4(),
        Some("LOT-QUALITY-E2E".into()),
        10,
        Utc::now(),
        &created_context,
    )
    .expect("quality inspection");
    let store = SqlxQualityStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("quality store");
    let created = store
        .save(&inspection, Some(0), &created_context)
        .await
        .expect("quality source create");
    assert_eq!(created.version, 1);
    assert!(!created.replayed);

    let replay = store
        .save(&inspection, Some(0), &created_context)
        .await
        .expect("quality create replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_id, created.event_id);

    let started_context = context(scope.clone(), "quality-start", Some(1));
    let mut started = inspection.clone();
    started
        .start(Utc::now() + Duration::seconds(1), &started_context)
        .expect("start inspection");
    let started_receipt = store
        .save(&started, Some(1), &started_context)
        .await
        .expect("quality source transition");
    assert_eq!(started_receipt.version, 2);

    let page = store
        .read_source_event_page(None, 10)
        .await
        .expect("quality outbox page");
    assert_eq!(page.events.len(), 2);
    assert!(!page.has_more);
    let mut forged = page.events[0].payload.clone();
    forged["aggregate"]["status"] = "released".into();
    assert!(
        store
            .project_event(
                page.events[0].event_id,
                page.events[0].aggregate_id,
                page.events[0].aggregate_version,
                &forged,
            )
            .await
            .is_err()
    );
    for event in &page.events {
        assert!(
            store
                .project_event(
                    event.event_id,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )
                .await
                .expect("quality projection")
                .applied
        );
    }
    assert!(
        !store
            .project_event(
                page.events[1].event_id,
                page.events[1].aggregate_id,
                page.events[1].aggregate_version,
                &page.events[1].payload,
            )
            .await
            .expect("quality duplicate projection")
            .applied
    );
    // A worker restart may resume from an older or reset durable cursor. The
    // authoritative projector must skip events already covered by the
    // contiguous read-row/checkpoint version instead of failing as out of order.
    assert!(
        !store
            .project_event(
                page.events[0].event_id,
                page.events[0].aggregate_id,
                page.events[0].aggregate_version,
                &page.events[0].payload,
            )
            .await
            .expect("quality restart replay")
            .applied
    );
    assert!(
        store
            .reconcile()
            .await
            .expect("quality reconciliation")
            .is_consistent()
    );
    let (read_value, read_version) = store
        .get(inspection_id)
        .await
        .expect("quality read")
        .expect("projected inspection");
    assert_eq!(read_version, 2);
    assert_eq!(read_value.status, started.status);
    let read_page = store
        .list_read_rows(
            QualityReadScope {
                holding_id,
                company_id,
                branch_id: None,
            },
            25,
            None,
        )
        .await
        .expect("quality typed read page");
    assert_eq!(read_page.rows.len(), 1);
    assert!(!read_page.has_more);
    assert_eq!(read_page.rows[0].inspection.id, inspection_id);
    assert_eq!(read_page.rows[0].source_version, 2);
    assert!(
        store
            .list_read_rows(
                QualityReadScope {
                    holding_id,
                    company_id: Uuid::new_v4(),
                    branch_id: None,
                },
                25,
                None,
            )
            .await
            .expect("quality empty scoped read")
            .rows
            .is_empty()
    );

    let source_event_count = store
        .source_event_high_watermark()
        .await
        .expect("quality source watermark");
    let (preparation, events_replayed) = store
        .rebuild_projection_with_source_fence(Some(source_event_count), 1)
        .await
        .expect("quality fenced rebuild");
    assert_eq!(preparation.rows_cleared, 1);
    assert_eq!(events_replayed, 2);
    assert!(
        store
            .reconcile()
            .await
            .expect("quality rebuilt reconciliation")
            .is_consistent()
    );

    // A malformed authoritative event must roll the replacement read
    // transaction back rather than leaving a partially cleared projection.
    let original_source_payload = page.events[0].payload.clone();
    sqlx::query(
        "UPDATE erpx_holding.outbox_events
         SET payload=$2
         WHERE data_profile_key=$1 AND event_id=$3",
    )
    .bind(&data_profile_key)
    .bind(serde_json::json!({"forged": true}))
    .bind(page.events[0].event_id)
    .execute(&write_pool)
    .await
    .expect("tamper quality source event for rollback check");
    assert!(
        store
            .rebuild_projection_with_source_fence(Some(source_event_count), 1)
            .await
            .is_err()
    );
    let (preserved, preserved_version) = store
        .get(inspection_id)
        .await
        .expect("quality rollback read")
        .expect("quality projection must survive failed rebuild");
    assert_eq!(preserved_version, 2);
    assert_eq!(preserved.status, started.status);
    sqlx::query(
        "UPDATE erpx_holding.outbox_events
         SET payload=$2
         WHERE data_profile_key=$1 AND event_id=$3",
    )
    .bind(&data_profile_key)
    .bind(original_source_payload)
    .bind(page.events[0].event_id)
    .execute(&write_pool)
    .await
    .expect("restore quality source event after rollback check");
    assert!(
        store
            .reconcile()
            .await
            .expect("quality restored after rollback check")
            .is_consistent()
    );

    let mut tampered_payload = page.events[1].payload["aggregate"].clone();
    tampered_payload["reference"] = serde_json::Value::String("QI-FORGED".into());
    let forged_checkpoint_event_id = Uuid::new_v4();
    sqlx::query(
        "UPDATE erpx_holding.quality_inspection_read_rows
         SET payload=$3
         WHERE data_profile_key=$1 AND inspection_id=$2",
    )
    .bind(&data_profile_key)
    .bind(inspection_id)
    .bind(&tampered_payload)
    .execute(&read_pool)
    .await
    .expect("tamper quality read payload for reconciliation");
    sqlx::query(
        "UPDATE erpx_holding.quality_projection_checkpoints
         SET last_event_id=$3
         WHERE data_profile_key=$1 AND inspection_id=$2",
    )
    .bind(&data_profile_key)
    .bind(inspection_id)
    .bind(forged_checkpoint_event_id)
    .execute(&read_pool)
    .await
    .expect("tamper quality checkpoint for reconciliation");
    let tampered = store
        .reconcile()
        .await
        .expect("quality tamper reconciliation");
    assert_eq!(tampered.source_aggregate_count, 1);
    assert_eq!(tampered.projected_aggregate_count, 1);
    assert_eq!(tampered.invalid_projection_count, 2);
    assert!(!tampered.is_consistent());
    sqlx::query(
        "UPDATE erpx_holding.quality_inspection_read_rows
         SET payload=$3
         WHERE data_profile_key=$1 AND inspection_id=$2",
    )
    .bind(&data_profile_key)
    .bind(inspection_id)
    .bind(&page.events[1].payload["aggregate"])
    .execute(&read_pool)
    .await
    .expect("restore quality read payload after reconciliation");
    sqlx::query(
        "UPDATE erpx_holding.quality_projection_checkpoints
         SET last_event_id=$3
         WHERE data_profile_key=$1 AND inspection_id=$2",
    )
    .bind(&data_profile_key)
    .bind(inspection_id)
    .bind(page.events[1].event_id)
    .execute(&read_pool)
    .await
    .expect("restore quality checkpoint after reconciliation");
    assert!(
        store
            .reconcile()
            .await
            .expect("quality restored reconciliation")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn quality_scope_is_immutable_across_source_versions() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url)
        .await
        .expect("quality write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("quality read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-quality-scope-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let create_context = context(scope.clone(), "quality-scope-create", Some(0));
    let inspection = QualityInspection::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        QualityInspectionKind::Incoming,
        Uuid::new_v4(),
        Some("LOT-QUALITY-SCOPE-MOVE".into()),
        10,
        Utc::now(),
        &create_context,
    )
    .expect("quality inspection");
    let store =
        SqlxQualityStore::new(write_pool, read_pool, data_profile_key).expect("quality store");
    store
        .save(&inspection, Some(0), &create_context)
        .await
        .expect("quality source create");

    let mut forged = inspection.clone();
    let start_context = context(scope, "quality-scope-start", Some(1));
    forged
        .start(Utc::now() + chrono::Duration::seconds(1), &start_context)
        .expect("next source version");
    let forged_holding_id = Uuid::new_v4();
    let forged_company_id = Uuid::new_v4();
    forged.scope = AuthorityScope::Company {
        holding_id: forged_holding_id,
        company_id: forged_company_id,
    };
    forged.holding_id = forged_holding_id;
    forged.company_id = forged_company_id;
    let forged_context = context(forged.scope.clone(), "quality-scope-move", Some(1));

    let error = store
        .save(&forged, Some(1), &forged_context)
        .await
        .expect_err("quality scope moves must fail closed");
    assert!(error.to_string().contains("scope is immutable"));
}
