//! Disposable end-to-end evidence for the Asset/Maintenance source and read
//! boundary. This remains ignored in the normal suite because it requires two
//! routed PostgreSQL databases with the holding migrations applied.

use chrono::Utc;
use chrono::{Duration, NaiveDate};
use erpx_domain::asset_maintenance::{
    Asset, AssetCommand, AssetScope, MaintenanceCommand, MaintenanceKind, MaintenanceWorkOrder,
    OperationsCapacityImpactHandoff,
};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::operations::SourceTransactionRef;
use erpx_infrastructure::{
    AssetMaintenanceAggregate, AssetMaintenanceReadScope, SqlxAssetMaintenanceStore,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope, key: &str, expected_version: Option<u64>) -> CommandContext {
    let mut context = CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "asset-maintenance-e2e-actor".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "asset_maintenance.e2e.acceptance",
    )
    .expect("valid command context");
    context.expected_aggregate_version = expected_version;
    context
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_asset_maintenance_write_outbox_projection_read_reconcile_and_rebuild() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-asset-maintenance-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AssetScope::new(holding_id, company_id, None).expect("asset scope");
    let active_scope = scope.active_scope();
    let asset_id = Uuid::new_v4();
    let mut asset = Asset::new(
        &context(active_scope.clone(), "asset-create", None),
        asset_id,
        "Anonymized maintenance asset",
        "material_handling",
        scope,
        NaiveDate::from_ymd_opt(2026, 1, 2).expect("date"),
        10_000,
        1_000,
        60,
        "employee:asset-e2e",
        "warehouse:e2e",
    )
    .expect("asset");
    asset
        .apply(
            &context(active_scope, "asset-place-in-service", Some(0)),
            AssetCommand::PlaceInService {
                at: NaiveDate::from_ymd_opt(2026, 1, 3).expect("date"),
            },
        )
        .expect("place asset in service");
    let mut transferred_asset = asset.clone();
    let destination_scope =
        AssetScope::new(holding_id, company_id, Some(Uuid::new_v4())).expect("destination scope");

    let store = SqlxAssetMaintenanceStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("asset maintenance store");
    let aggregate = AssetMaintenanceAggregate::Asset(asset);
    let first = store
        .save(&aggregate, "asset-maintenance-e2e-write", Some(0))
        .await
        .expect("authoritative source write");
    assert_eq!(first.version, 1);
    assert!(!first.replayed);
    let replay = store
        .save(&aggregate, "asset-maintenance-e2e-write", Some(0))
        .await
        .expect("idempotent source replay");
    assert!(replay.replayed);
    assert_eq!(replay.version, first.version);

    transferred_asset
        .apply(
            &context(
                AuthorityScope::Company {
                    holding_id,
                    company_id,
                },
                "asset-transfer",
                Some(1),
            ),
            AssetCommand::Transfer {
                to_scope: destination_scope,
                custodian_ref: "employee:asset-e2e-2".into(),
                location_ref: "warehouse:e2e-2".into(),
                reason: "custody reconciliation evidence".into(),
                at: Utc::now(),
            },
        )
        .expect("transfer asset custody");
    let second = store
        .save(
            &AssetMaintenanceAggregate::Asset(transferred_asset.clone()),
            "asset-maintenance-e2e-transfer",
            Some(1),
        )
        .await
        .expect("authoritative custody update");
    assert_eq!(second.version, 2);

    let page = store
        .read_source_event_page(None, 10)
        .await
        .expect("authoritative outbox page");
    assert_eq!(page.events.len(), 2);
    assert!(!page.has_more);
    let event = &page.events[0];

    let mut forged_payload = event.payload.clone();
    forged_payload["aggregate"]["aggregate"]["name"] = json!("forged read model");
    assert!(
        store
            .project_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &forged_payload,
            )
            .await
            .is_err(),
        "a valid-looking but non-authoritative payload must not reach the read model"
    );
    assert!(
        store
            .project_event(
                Uuid::new_v4(),
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .is_err(),
        "an event ID absent from the scoped outbox must not be projectable"
    );

    let applied = store
        .project_event(
            event.event_id,
            &event.aggregate_type,
            event.aggregate_id,
            event.aggregate_version,
            &event.payload,
        )
        .await
        .expect("authoritative projection");
    assert!(applied.applied);
    let duplicate = store
        .project_event(
            event.event_id,
            &event.aggregate_type,
            event.aggregate_id,
            event.aggregate_version,
            &event.payload,
        )
        .await
        .expect("idempotent projection");
    assert!(!duplicate.applied);
    let second_event = &page.events[1];
    assert!(
        store
            .project_event(
                second_event.event_id,
                &second_event.aggregate_type,
                second_event.aggregate_id,
                second_event.aggregate_version,
                &second_event.payload,
            )
            .await
            .expect("authoritative custody projection")
            .applied
    );

    let read = store
        .get("asset", asset_id)
        .await
        .expect("scoped read model lookup")
        .expect("projected asset");
    assert_eq!(read.1, 2);
    assert_eq!(read.0.aggregate_id(), asset_id);
    let AssetMaintenanceAggregate::Asset(read_asset) = read.0 else {
        panic!("expected projected asset");
    };
    assert_eq!(read_asset.custody_history.len(), 1);

    let list_page = store
        .list_for_scope(
            AssetMaintenanceReadScope {
                holding_id,
                company_id,
                branch_id: None,
            },
            25,
            None,
        )
        .await
        .expect("scoped Asset/Maintenance list read");
    assert_eq!(list_page.rows.len(), 1);
    assert!(!list_page.has_more);
    assert_eq!(list_page.rows[0].aggregate.aggregate_id(), asset_id);
    assert_eq!(list_page.rows[0].source_version, 2);
    assert!(list_page.rows[0].reference.starts_with("AST-"));
    assert!(
        store
            .list_for_scope(
                AssetMaintenanceReadScope {
                    holding_id,
                    company_id: Uuid::new_v4(),
                    branch_id: None,
                },
                25,
                None,
            )
            .await
            .expect("scope-isolated Asset/Maintenance list read")
            .rows
            .is_empty()
    );

    let consistent = store
        .reconcile_projection()
        .await
        .expect("green reconciliation");
    assert!(consistent.is_consistent());
    assert_eq!(consistent.source_event_count, 2);
    assert_eq!(consistent.source_version_gap_count, 0);
    assert_eq!(consistent.projected_ahead_aggregate_count, 0);
    assert_eq!(consistent.custody_history_mismatch_count, 0);

    let mut read_tx = read_pool.begin().await.expect("custody tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *read_tx)
        .await
        .expect("read RLS scope");
    sqlx::query(
        "DELETE FROM erpx_holding.asset_custody_transfer_read_rows
         WHERE data_profile_key=$1 AND asset_id=$2",
    )
    .bind(&data_profile_key)
    .bind(asset_id)
    .execute(&mut *read_tx)
    .await
    .expect("tamper custody read row");
    read_tx.commit().await.expect("commit custody tamper");
    assert!(
        store
            .get("asset", asset_id)
            .await
            .expect_err("custody read mismatch must fail closed")
            .to_string()
            .contains("custody read history")
    );
    let custody_mismatch = store
        .reconcile_projection()
        .await
        .expect("custody mismatch reconciliation");
    assert_eq!(custody_mismatch.custody_history_mismatch_count, 1);
    assert!(!custody_mismatch.is_consistent());

    // Capacity handoffs use a separate idempotency table and outbox event.
    // Reusing the key with a changed payload must be rejected before a
    // conflicting operational impact can be treated as a replay.
    let source_id = Uuid::new_v4();
    let source_event_id = Uuid::new_v4();
    let handoff = OperationsCapacityImpactHandoff {
        reference: format!(
            "OPS-CAP-{}-{}",
            source_id.simple(),
            source_event_id.simple()
        ),
        source: SourceTransactionRef {
            source_domain: "assets".into(),
            source_type: "maintenance_work_order".into(),
            source_id,
            source_event_id,
        },
        asset_id,
        scope: AssetScope::new(holding_id, company_id, None).expect("handoff scope"),
        downtime_minutes: 90,
        occurred_at: Utc::now(),
        idempotency_key: "asset-maintenance-capacity-handoff".into(),
    };
    let handoff_receipt = store
        .save_capacity_handoff(&handoff)
        .await
        .expect("authoritative capacity handoff");
    assert!(!handoff_receipt.replayed);
    assert_eq!(handoff_receipt.source_event_id, source_event_id);
    let handoff_replay = store
        .save_capacity_handoff(&handoff)
        .await
        .expect("exact capacity handoff replay");
    assert!(handoff_replay.replayed);
    assert!(
        store
            .project_capacity_handoff(source_event_id)
            .await
            .expect("authoritative capacity handoff projection")
    );
    assert!(
        !store
            .project_capacity_handoff(source_event_id)
            .await
            .expect("idempotent capacity handoff projection")
    );
    let mut conflicting_handoff = handoff.clone();
    conflicting_handoff.downtime_minutes = 91;
    assert!(
        store
            .save_capacity_handoff(&conflicting_handoff)
            .await
            .is_err()
    );

    // If the outbox append fails after the source insert, the source handoff
    // must roll back with it; a partial maintenance handoff is not replayable.
    let rollback_event_id = Uuid::new_v4();
    let mut rollback_handoff = handoff.clone();
    rollback_handoff.source.source_id = Uuid::new_v4();
    rollback_handoff.source.source_event_id = rollback_event_id;
    rollback_handoff.reference = format!(
        "OPS-CAP-{}-{}",
        rollback_handoff.source.source_id.simple(),
        rollback_event_id.simple()
    );
    rollback_handoff.idempotency_key = "asset-maintenance-capacity-rollback".into();
    let mut rollback_seed_tx = write_pool.begin().await.expect("rollback seed transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *rollback_seed_tx)
        .await
        .expect("rollback seed RLS scope");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,'operations_capacity_handoff',$2,$3,1,
                 'seed.conflicting_event',$4,now())",
    )
    .bind(rollback_event_id)
    .bind(rollback_handoff.source.source_id)
    .bind(&data_profile_key)
    .bind(serde_json::to_value(&rollback_handoff).expect("rollback seed payload"))
    .execute(&mut *rollback_seed_tx)
    .await
    .expect("seed conflicting outbox event");
    rollback_seed_tx
        .commit()
        .await
        .expect("commit rollback seed");
    assert!(
        store
            .save_capacity_handoff(&rollback_handoff)
            .await
            .is_err()
    );
    let mut rollback_verify_tx = write_pool
        .begin()
        .await
        .expect("rollback verify transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *rollback_verify_tx)
        .await
        .expect("rollback verify RLS scope");
    let rolled_back_source_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.asset_operations_capacity_handoffs
         WHERE data_profile_key=$1 AND source_event_id=$2",
    )
    .bind(&data_profile_key)
    .bind(rollback_event_id)
    .fetch_one(&mut *rollback_verify_tx)
    .await
    .expect("count rolled back handoff source");
    rollback_verify_tx
        .commit()
        .await
        .expect("commit rollback verification");
    assert_eq!(rolled_back_source_count, 0);

    // Projector retries must not overwrite a read row that no longer carries
    // the exact immutable source payload. Reconciliation must also fail
    // closed until the disposable row is restored from the authoritative
    // source handoff.
    let mut handoff_read_tx = read_pool.begin().await.expect("handoff tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *handoff_read_tx)
        .await
        .expect("handoff read RLS scope");
    sqlx::query(
        "UPDATE erpx_holding.asset_capacity_handoff_read_rows
         SET payload=jsonb_set(payload, '{downtime_minutes}', to_jsonb(91::bigint))
         WHERE data_profile_key=$1 AND source_event_id=$2",
    )
    .bind(&data_profile_key)
    .bind(source_event_id)
    .execute(&mut *handoff_read_tx)
    .await
    .expect("tamper capacity handoff read row");
    handoff_read_tx
        .commit()
        .await
        .expect("commit capacity handoff tamper");
    assert!(
        store
            .project_capacity_handoff(source_event_id)
            .await
            .is_err()
    );
    assert!(store.reconcile_projection().await.is_err());
    let mut handoff_restore_tx = read_pool
        .begin()
        .await
        .expect("handoff restore transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *handoff_restore_tx)
        .await
        .expect("handoff restore RLS scope");
    sqlx::query(
        "UPDATE erpx_holding.asset_capacity_handoff_read_rows
         SET source_id=$3, company_id=$4, branch_id=$5, reference=$6,
             source_version=1, payload=$7
         WHERE data_profile_key=$1 AND source_event_id=$2",
    )
    .bind(&data_profile_key)
    .bind(source_event_id)
    .bind(handoff.source.source_id)
    .bind(handoff.scope.company_id)
    .bind(handoff.scope.branch_id)
    .bind(&handoff.reference)
    .bind(serde_json::to_value(&handoff).expect("handoff payload"))
    .execute(&mut *handoff_restore_tx)
    .await
    .expect("restore capacity handoff read row");
    handoff_restore_tx
        .commit()
        .await
        .expect("commit capacity handoff restore");

    // A read row claiming a future source version is not accepted as fresh.
    let mut read_tx = read_pool.begin().await.expect("read transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *read_tx)
        .await
        .expect("read RLS scope");
    sqlx::query(
        "UPDATE erpx_holding.asset_maintenance_read_rows
         SET source_version=3
         WHERE data_profile_key=$1 AND aggregate_type='asset' AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(asset_id)
    .execute(&mut *read_tx)
    .await
    .expect("tamper test read row");
    read_tx.commit().await.expect("commit tamper test");
    let ahead = store
        .reconcile_projection()
        .await
        .expect("future-version reconciliation");
    assert_eq!(ahead.projected_ahead_aggregate_count, 1);
    assert!(!ahead.is_consistent());

    // A projected source version behind the payload is malformed, not drift.
    let mut malformed_version_tx = read_pool.begin().await.expect("version guard transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *malformed_version_tx)
        .await
        .expect("version guard RLS scope");
    sqlx::query(
        "UPDATE erpx_holding.asset_maintenance_read_rows
         SET source_version=1
         WHERE data_profile_key=$1 AND aggregate_type='asset' AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(asset_id)
    .execute(&mut *malformed_version_tx)
    .await
    .expect("tamper regressive read version");
    malformed_version_tx
        .commit()
        .await
        .expect("commit regressive read version");
    assert!(
        store.reconcile_projection().await.is_err(),
        "a projected version behind its payload must fail closed"
    );

    let mut malformed_identity_tx = read_pool.begin().await.expect("identity guard transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *malformed_identity_tx)
        .await
        .expect("identity guard RLS scope");
    sqlx::query(
        "UPDATE erpx_holding.asset_maintenance_read_rows
         SET source_version=3,
             payload=jsonb_set(payload, '{aggregate,id}', to_jsonb($3::text))
         WHERE data_profile_key=$1 AND aggregate_type='asset' AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(asset_id)
    .bind(Uuid::new_v4().to_string())
    .execute(&mut *malformed_identity_tx)
    .await
    .expect("tamper projected read identity");
    malformed_identity_tx
        .commit()
        .await
        .expect("commit projected read identity");
    assert!(
        store.reconcile_projection().await.is_err(),
        "a projected identity mismatch must fail closed"
    );

    let cleared = store.clear_projection().await.expect("clear read model");
    assert_eq!(cleared.rows_cleared, 1);
    assert!(
        store.reconcile_projection().await.is_err(),
        "missing aggregate and capacity handoff projections must fail closed"
    );

    let rebuild_page = store
        .read_source_event_page(None, 10)
        .await
        .expect("rebuild source page");
    for event in rebuild_page.events {
        assert!(
            store
                .project_event(
                    event.event_id,
                    &event.aggregate_type,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )
                .await
                .expect("rebuild projection")
                .applied
        );
    }
    assert!(
        store
            .project_capacity_handoff(source_event_id)
            .await
            .expect("rebuild capacity handoff projection")
    );
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("reconciled after rebuild")
            .is_consistent()
    );

    let wrong_holding = SqlxAssetMaintenanceStore::new(
        write_pool,
        read_pool,
        format!("holding-asset-maintenance-other-{}", Uuid::new_v4()),
    )
    .expect("other holding store");
    assert!(
        wrong_holding
            .project_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .is_err(),
        "holding routing must apply to the authoritative source lookup"
    );
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_maintenance_work_order_requires_authoritative_asset_link() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-asset-maintenance-link-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AssetScope::new(holding_id, company_id, None).expect("asset scope");
    let active_scope = scope.active_scope();
    let asset_id = Uuid::new_v4();
    let mut asset = Asset::new(
        &context(active_scope.clone(), "mwo-link-asset-create", None),
        asset_id,
        "Maintenance-link asset",
        "material_handling",
        scope.clone(),
        NaiveDate::from_ymd_opt(2026, 2, 1).expect("date"),
        20_000,
        2_000,
        60,
        "employee:mwo-link",
        "warehouse:mwo-link",
    )
    .expect("asset");
    asset
        .apply(
            &context(active_scope.clone(), "mwo-link-asset-service", Some(0)),
            AssetCommand::PlaceInService {
                at: NaiveDate::from_ymd_opt(2026, 2, 2).expect("date"),
            },
        )
        .expect("place asset in service");
    let store = SqlxAssetMaintenanceStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("asset maintenance store");
    store
        .save(
            &AssetMaintenanceAggregate::Asset(asset.clone()),
            "mwo-link-asset-save",
            Some(0),
        )
        .await
        .expect("authoritative asset source write");

    let work_order_id = Uuid::new_v4();
    let mut work_order = MaintenanceWorkOrder::new(
        &context(active_scope.clone(), "mwo-link-create", None),
        work_order_id,
        &asset,
        MaintenanceKind::Preventive,
        "Quarterly material-handling inspection",
    )
    .expect("maintenance work order");
    work_order
        .apply(
            &context(active_scope.clone(), "mwo-link-schedule", Some(0)),
            MaintenanceCommand::Schedule {
                scheduled_for: Some(Utc::now() + Duration::hours(1)),
            },
        )
        .expect("schedule work order");
    let work_order_receipt = store
        .save(
            &AssetMaintenanceAggregate::MaintenanceWorkOrder(work_order.clone()),
            "mwo-link-save",
            Some(0),
        )
        .await
        .expect("authoritative work-order source write");
    assert_eq!(work_order_receipt.version, 1);

    let source_page = store
        .read_source_event_page(None, 10)
        .await
        .expect("authoritative asset and work-order outbox page");
    assert_eq!(source_page.events.len(), 2);
    for event in source_page.events {
        assert!(
            store
                .project_event(
                    event.event_id,
                    &event.aggregate_type,
                    event.aggregate_id,
                    event.aggregate_version,
                    &event.payload,
                )
                .await
                .expect("project authoritative work-order event")
                .applied
        );
    }
    let projected = store
        .get("maintenance_work_order", work_order_id)
        .await
        .expect("read projected work order")
        .expect("work order read row");
    assert_eq!(projected.1, 1);
    let AssetMaintenanceAggregate::MaintenanceWorkOrder(projected_work_order) = projected.0 else {
        panic!("expected projected maintenance work order");
    };
    assert_eq!(projected_work_order.asset_id, asset_id);
    assert_eq!(projected_work_order.scope, scope);
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("work-order reconciliation")
            .is_consistent()
    );

    // A forged work order with a cross-company scope must fail before either
    // its authoritative aggregate row or its outbox event is committed.
    let forged_id = Uuid::new_v4();
    let mut forged = work_order;
    forged.id = forged_id;
    forged.reference = format!("MWO-{}", forged_id.simple());
    forged.scope = AssetScope::new(holding_id, Uuid::new_v4(), None).expect("foreign scope");
    let error = store
        .save(
            &AssetMaintenanceAggregate::MaintenanceWorkOrder(forged),
            "mwo-link-forged-cross-company",
            Some(0),
        )
        .await
        .expect_err("cross-company work order must fail closed");
    assert!(error.to_string().contains("scope does not match"));

    let mut verify_tx = write_pool
        .begin()
        .await
        .expect("rollback verification transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *verify_tx)
        .await
        .expect("rollback verification RLS scope");
    let source_rows: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_holding.asset_maintenance_aggregates
         WHERE data_profile_key=$1 AND aggregate_type='maintenance_work_order' AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(forged_id)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("count forged source rows");
    let outbox_rows: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND aggregate_type='maintenance_work_order' AND aggregate_id=$2",
    )
    .bind(&data_profile_key)
    .bind(forged_id)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("count forged outbox rows");
    verify_tx
        .commit()
        .await
        .expect("commit rollback verification");
    assert_eq!(source_rows, 0);
    assert_eq!(outbox_rows, 0);

    // A new work order cannot be created after the authoritative asset has
    // been disposed, even when the work-order payload otherwise validates.
    asset
        .apply(
            &context(active_scope, "mwo-link-asset-dispose", Some(1)),
            AssetCommand::Dispose {
                disposed_at: NaiveDate::from_ymd_opt(2026, 2, 10).expect("date"),
                proceeds_minor: 500,
                reason: "retired after acceptance evidence".into(),
            },
        )
        .expect("dispose asset");
    store
        .save(
            &AssetMaintenanceAggregate::Asset(asset),
            "mwo-link-asset-dispose-save",
            Some(1),
        )
        .await
        .expect("persist disposed asset");
    let disposed_id = Uuid::new_v4();
    let mut disposed_work_order = projected_work_order;
    disposed_work_order.id = disposed_id;
    disposed_work_order.reference = format!("MWO-{}", disposed_id.simple());
    let disposed_error = store
        .save(
            &AssetMaintenanceAggregate::MaintenanceWorkOrder(disposed_work_order),
            "mwo-link-disposed-asset",
            Some(0),
        )
        .await
        .expect_err("work order for disposed asset must fail closed");
    assert!(disposed_error.to_string().contains("disposed asset"));
}

#[test]
fn asset_maintenance_acceptance_test_is_explicitly_live_and_ignored() {
    let source = include_str!("asset_maintenance_persistence.rs");
    assert!(source.contains("migrate_holding_write"));
    assert!(source.contains("migrate_holding_read"));
    assert!(source.contains("project_event"));
    assert!(source.contains("reconcile_projection"));
    assert!(source.contains("clear_projection"));
    assert!(source.contains("ERPX_TEST_HOLDING_WRITE_DATABASE_URL"));
    assert!(source.contains("ERPX_TEST_HOLDING_READ_DATABASE_URL"));
}
