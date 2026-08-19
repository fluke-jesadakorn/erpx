//! Disposable end-to-end evidence for the SCM source/outbox/read boundary.
//!
//! This test intentionally uses two real PostgreSQL databases. It is ignored
//! in the normal suite and becomes acceptance evidence when both routed
//! database URLs are supplied.

use chrono::Utc;
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::procurement_inventory::{
    GoodsReceipt, GoodsReceiptLine, GoodsReceiptStatus, Product, ProductReference, PurchaseOrder,
    PurchaseOrderLine, PurchaseOrderStatus, PurchaseRequest, PurchaseRequestLine, ScmAggregateKind,
    StockMovement, StockMovementKind, StockMovementStatus, StockReservation,
    StockReservationStatus, StockTransfer, StockTransferLine, StockTransferStatus, VendorReference,
    WarehouseReference, deterministic_reference,
};
use erpx_infrastructure::{ScmAggregate, ScmReadScope, SqlxScmStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope) -> CommandContext {
    context_with_key(scope, "scm/e2e-product-1")
}

fn context_with_key(scope: AuthorityScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "scm-e2e-actor".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "scm.e2e.acceptance",
    )
    .expect("valid context")
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_source_pages_continue_from_the_returned_cursor() {
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

    let data_profile_key = format!("holding-scm-page-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let first = Product::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        "SKU-SCM-PAGE-1",
        "Cursor page product one",
        "each",
        Utc::now(),
        &context_with_key(scope.clone(), "scm/e2e-page-first"),
    )
    .expect("first product");
    let second = Product::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        "SKU-SCM-PAGE-2",
        "Cursor page product two",
        "each",
        Utc::now(),
        &context_with_key(scope.clone(), "scm/e2e-page-second"),
    )
    .expect("second product");
    let store =
        SqlxScmStore::new(write_pool, read_pool, data_profile_key.clone()).expect("SCM store");
    store
        .save(&ScmAggregate::Product(first), None)
        .await
        .expect("first source write");
    store
        .save(&ScmAggregate::Product(second), None)
        .await
        .expect("second source write");

    let first_page = store
        .read_source_event_page(None, 1)
        .await
        .expect("first source page");
    assert_eq!(first_page.events.len(), 1);
    assert!(first_page.has_more);
    let cursor = first_page
        .next_cursor
        .clone()
        .expect("first page continuation cursor");
    assert_eq!(cursor.data_profile_key, data_profile_key);
    assert_eq!(cursor.event_id, first_page.events[0].event_id);

    let second_page = store
        .read_source_event_page(Some(&cursor), 1)
        .await
        .expect("continued source page");
    assert_eq!(second_page.events.len(), 1);
    assert_ne!(
        first_page.events[0].aggregate_id,
        second_page.events[0].aggregate_id
    );
    assert!(!second_page.has_more);
    assert!(second_page.next_cursor.is_none());
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_cursor_does_not_skip_same_version_outbox_rows() {
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

    let data_profile_key = format!("holding-scm-cursor-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let product = Product::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        "SKU-SCM-CURSOR-TIE",
        "Cursor tie-break product",
        "each",
        Utc::now(),
        &context_with_key(scope, "scm/e2e-cursor-tie"),
    )
    .expect("product");
    let aggregate_id = product.id;
    let write_pool_for_duplicate = write_pool.clone();
    let store =
        SqlxScmStore::new(write_pool, read_pool, data_profile_key.clone()).expect("SCM store");
    store
        .save(&ScmAggregate::Product(product), None)
        .await
        .expect("source write");

    // A legacy/import row can share an aggregate version because the shared
    // outbox schema only makes event_id unique. The event_id tie-breaker must
    // make the continuation include it rather than silently skipping a row.
    let duplicate_event_id = Uuid::from_u128(u128::MAX);
    let mut tx = write_pool_for_duplicate
        .begin()
        .await
        .expect("duplicate source tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("route duplicate source tx");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         SELECT $1, aggregate_type, aggregate_id, data_profile_key,
                aggregate_version, event_type, payload, occurred_at
         FROM erpx_holding.outbox_events
         WHERE aggregate_type='product' AND aggregate_id=$2 AND data_profile_key=$3",
    )
    .bind(duplicate_event_id)
    .bind(aggregate_id)
    .bind(&data_profile_key)
    .execute(&mut *tx)
    .await
    .expect("duplicate source event");
    tx.commit().await.expect("commit duplicate source event");

    let first_page = store
        .read_source_event_page(None, 1)
        .await
        .expect("first source page");
    let first_event = &first_page.events[0];
    assert!(first_page.has_more);
    let cursor = first_page.next_cursor.expect("tie-break cursor");
    assert_eq!(cursor.event_id, first_event.event_id);

    let second_page = store
        .read_source_event_page(Some(&cursor), 1)
        .await
        .expect("continued source page");
    assert_eq!(second_page.events.len(), 1);
    assert_eq!(second_page.events[0].aggregate_id, first_event.aggregate_id);
    assert_eq!(
        second_page.events[0].aggregate_version,
        first_event.aggregate_version
    );
    assert_ne!(second_page.events[0].event_id, first_event.event_id);
    assert_eq!(second_page.events[0].event_id, duplicate_event_id);
    assert!(!second_page.has_more);
    assert!(second_page.next_cursor.is_none());
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_reconciliation_evidence_is_idempotent_and_rollback_safe() {
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

    let data_profile_key = format!("holding-scm-reconcile-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let product = Product::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        "SKU-SCM-RECONCILE",
        "Reconciliation evidence product",
        "each",
        Utc::now(),
        &context_with_key(scope, "scm/e2e-reconciliation-evidence"),
    )
    .expect("product");
    let store = SqlxScmStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("SCM store");
    let source_receipt = store
        .save(&ScmAggregate::Product(product), None)
        .await
        .expect("source write");
    let source_page = store
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert_eq!(source_page.events.len(), 1);
    let source_event = &source_page.events[0];
    store
        .project_event(
            source_event.event_id,
            &source_event.aggregate_type,
            source_event.aggregate_id,
            source_event.aggregate_version,
            &source_event.payload,
        )
        .await
        .expect("project source event");

    let (green_report, green_evidence, next_cursor, has_more) = store
        .reconcile_projection_page(None, 1)
        .await
        .expect("green bounded reconciliation page");
    assert!(green_report.is_consistent());
    assert!(green_evidence.healthy);
    assert!(!green_evidence.rollback_required);
    assert!(next_cursor.is_none());
    assert!(!has_more);
    let green_id = store
        .persist_reconciliation_evidence(&green_evidence)
        .await
        .expect("persist green evidence");
    assert_eq!(
        store
            .persist_reconciliation_evidence(&green_evidence)
            .await
            .expect("replay green evidence"),
        green_id
    );

    let mut tamper_tx = read_pool.begin().await.expect("tamper tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tamper_tx)
        .await
        .expect("route tamper tx");
    let mut tampered_payload = source_event.payload["aggregate"].clone();
    tampered_payload["name"] = serde_json::json!("tampered reconciliation product");
    sqlx::query(
        "UPDATE erpx_holding.scm_aggregates
         SET payload=$1
         WHERE aggregate_type=$2 AND aggregate_id=$3 AND data_profile_key=$4",
    )
    .bind(tampered_payload)
    .bind(&source_event.aggregate_type)
    .bind(source_event.aggregate_id)
    .bind(&data_profile_key)
    .execute(&mut *tamper_tx)
    .await
    .expect("tamper read model");
    tamper_tx.commit().await.expect("commit tamper");

    let (rollback_report, rollback_evidence, _, _) = store
        .reconcile_projection_page(None, 1)
        .await
        .expect("rollback-required reconciliation page");
    assert_eq!(rollback_report.invalid_projection_count, 1);
    assert!(!rollback_report.is_consistent());
    assert!(!rollback_evidence.healthy);
    assert!(rollback_evidence.rollback_required);
    assert_ne!(rollback_evidence.evidence_id, green_evidence.evidence_id);
    let rollback_id = store
        .persist_reconciliation_evidence(&rollback_evidence)
        .await
        .expect("persist rollback evidence");
    assert_eq!(
        store
            .persist_reconciliation_evidence(&rollback_evidence)
            .await
            .expect("replay rollback evidence"),
        rollback_id
    );

    let mut count_tx = write_pool.begin().await.expect("evidence count tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *count_tx)
        .await
        .expect("route evidence count tx");
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.outbox_events WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *count_tx)
    .await
    .expect("count source and evidence outbox");
    let idempotency_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.scm_command_idempotency WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *count_tx)
    .await
    .expect("count source and evidence idempotency");
    count_tx.commit().await.expect("commit evidence counts");
    assert_eq!(outbox_count, 3, "source plus two immutable evidence facts");
    assert_eq!(
        idempotency_count, 3,
        "source command plus two evidence idempotency rows"
    );
    assert_eq!(
        store
            .read_source_event_page(None, 10)
            .await
            .expect("source cursor excludes evidence facts")
            .events
            .len(),
        1
    );

    // Rollback only the disposable read model, then replay the authoritative
    // source. The original green evidence is replayed, not overwritten.
    store.clear_projection().await.expect("clear read model");
    store
        .replay_event(
            source_event.event_id,
            &source_event.aggregate_type,
            source_event.aggregate_id,
            source_event.aggregate_version,
            &source_event.payload,
        )
        .await
        .expect("replay source after rollback");
    let (recovered_report, recovered_evidence, _, _) = store
        .reconcile_projection_page(None, 1)
        .await
        .expect("recovered reconciliation page");
    assert!(recovered_report.is_consistent());
    assert_eq!(recovered_evidence.evidence_id, green_evidence.evidence_id);
    assert_eq!(
        store
            .persist_reconciliation_evidence(&recovered_evidence)
            .await
            .expect("replay recovered evidence"),
        green_id
    );
    assert_eq!(source_receipt.version, 1);
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_write_outbox_projection_scoped_read_and_reconciliation() {
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

    let data_profile_key = format!("holding-scm-{}", Uuid::new_v4());
    let write_pool_for_integrity = write_pool.clone();
    let read_pool_for_integrity = read_pool.clone();
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let product = Product::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        "SKU-SCM-E2E",
        "Anonymized SCM product",
        "each",
        Utc::now(),
        &context(scope.clone()),
    )
    .expect("valid product");
    let product_id = product.id;
    let store =
        SqlxScmStore::new(write_pool, read_pool, data_profile_key.clone()).expect("SCM store");
    let aggregate = ScmAggregate::Product(product);

    let first = store.save(&aggregate, None).await.expect("source write");
    assert_eq!(first.version, 1);
    assert!(!first.replayed);
    let replay = store
        .save(&aggregate, None)
        .await
        .expect("idempotent replay");
    assert!(replay.replayed);
    assert_eq!(replay.version, first.version);

    let page = store
        .read_source_event_page(None, 10)
        .await
        .expect("source outbox page");
    assert_eq!(page.events.len(), 1);
    let mut forged_payload = page.events[0].payload.clone();
    forged_payload["aggregate"]["name"] = serde_json::json!("forged before projection");
    assert!(
        store
            .project_event(
                page.events[0].event_id,
                &page.events[0].aggregate_type,
                page.events[0].aggregate_id,
                page.events[0].aggregate_version,
                &forged_payload,
            )
            .await
            .is_err(),
        "a valid but forged payload must be rejected before the read model is touched"
    );
    let application = store
        .project_event(
            page.events[0].event_id,
            &page.events[0].aggregate_type,
            page.events[0].aggregate_id,
            page.events[0].aggregate_version,
            &page.events[0].payload,
        )
        .await
        .expect("read projection");
    assert!(application.applied);
    assert!(
        store
            .project_event(
                Uuid::new_v4(),
                &page.events[0].aggregate_type,
                page.events[0].aggregate_id,
                page.events[0].aggregate_version,
                &page.events[0].payload,
            )
            .await
            .is_err()
    );
    let mut conflicting_payload = page.events[0].payload.clone();
    conflicting_payload["aggregate"]["name"] = serde_json::json!("conflicting product");
    assert!(
        store
            .project_event(
                page.events[0].event_id,
                &page.events[0].aggregate_type,
                page.events[0].aggregate_id,
                page.events[0].aggregate_version,
                &conflicting_payload,
            )
            .await
            .is_err()
    );
    assert!(
        !store
            .project_event(
                page.events[0].event_id,
                &page.events[0].aggregate_type,
                page.events[0].aggregate_id,
                page.events[0].aggregate_version,
                &page.events[0].payload,
            )
            .await
            .expect("idempotent projection")
            .applied
    );

    let row = store
        .get_for_scope("product", product_id, &scope)
        .await
        .expect("authorized read")
        .expect("projected product");
    assert_eq!(row.aggregate_id, product_id);
    assert_eq!(row.aggregate_version, 1);
    let read_page = store
        .list_for_scope(
            ScmReadScope {
                holding_id,
                company_id,
                branch_id: None,
            },
            25,
            None,
        )
        .await
        .expect("SCM typed read page");
    assert_eq!(read_page.rows.len(), 1);
    assert!(!read_page.has_more);
    assert_eq!(read_page.rows[0].row.aggregate_id, product_id);
    assert!(read_page.rows[0].reference.starts_with("PROD-"));
    assert!(
        store
            .list_for_scope(
                ScmReadScope {
                    holding_id,
                    company_id: Uuid::new_v4(),
                    branch_id: None,
                },
                25,
                None,
            )
            .await
            .expect("SCM empty scoped read")
            .rows
            .is_empty()
    );
    let wrong_scope = AuthorityScope::Company {
        holding_id,
        company_id: Uuid::new_v4(),
    };
    assert!(
        store
            .get_for_scope("product", product_id, &wrong_scope)
            .await
            .is_err()
    );

    let consistent = store
        .reconcile_projection()
        .await
        .expect("consistent source/read report");
    assert!(consistent.is_consistent());
    assert_eq!(consistent.source_event_count, 1);

    // Reconciliation must authenticate the complete source envelope, not
    // only the payload/version. A wrong event type with matching read state
    // would otherwise produce a false healthy result.
    let mut source_tamper_tx = write_pool_for_integrity
        .begin()
        .await
        .expect("source envelope tamper tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source_tamper_tx)
        .await
        .expect("route source envelope tamper tx");
    sqlx::query(
        "UPDATE erpx_holding.outbox_events
         SET event_type='scm.product.invalid'
         WHERE event_id=$1 AND data_profile_key=$2",
    )
    .bind(page.events[0].event_id)
    .bind(&data_profile_key)
    .execute(&mut *source_tamper_tx)
    .await
    .expect("tamper source event type");
    source_tamper_tx
        .commit()
        .await
        .expect("commit source envelope tamper");
    assert!(
        store.reconcile_projection().await.is_err(),
        "wrong source event type must fail closed during reconciliation"
    );

    let mut source_restore_tx = write_pool_for_integrity
        .begin()
        .await
        .expect("source envelope restore tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source_restore_tx)
        .await
        .expect("route source envelope restore tx");
    sqlx::query(
        "UPDATE erpx_holding.outbox_events
         SET event_type=$1
         WHERE event_id=$2 AND data_profile_key=$3",
    )
    .bind(&page.events[0].event_type)
    .bind(page.events[0].event_id)
    .bind(&data_profile_key)
    .execute(&mut *source_restore_tx)
    .await
    .expect("restore source event type");
    source_restore_tx
        .commit()
        .await
        .expect("commit source envelope restore");
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("restored source")
            .is_consistent()
    );

    // A same-version but valid payload change must not pass reconciliation.
    // Version/count-only checks cannot detect this drift, which would let a
    // damaged read model look healthy until a later source event arrives.
    let original_payload = page.events[0].payload["aggregate"].clone();
    let mut tamper_tx = read_pool_for_integrity.begin().await.expect("tamper tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tamper_tx)
        .await
        .expect("route payload tamper tx");
    let mut tampered_payload = original_payload.clone();
    tampered_payload["name"] = serde_json::json!("valid but tampered product");
    sqlx::query(
        "UPDATE erpx_holding.scm_aggregates
         SET payload=$1
         WHERE aggregate_type=$2 AND aggregate_id=$3 AND data_profile_key=$4",
    )
    .bind(&tampered_payload)
    .bind(&page.events[0].aggregate_type)
    .bind(page.events[0].aggregate_id)
    .bind(&data_profile_key)
    .execute(&mut *tamper_tx)
    .await
    .expect("tamper read payload");
    tamper_tx.commit().await.expect("commit payload tamper");

    let payload_drift = store
        .reconcile_projection()
        .await
        .expect("payload drift report");
    assert_eq!(payload_drift.invalid_projection_count, 1);
    assert!(!payload_drift.is_consistent());

    let mut restore_tx = read_pool_for_integrity.begin().await.expect("restore tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *restore_tx)
        .await
        .expect("route payload restore tx");
    sqlx::query(
        "UPDATE erpx_holding.scm_aggregates
         SET payload=$1
         WHERE aggregate_type=$2 AND aggregate_id=$3 AND data_profile_key=$4",
    )
    .bind(&original_payload)
    .bind(&page.events[0].aggregate_type)
    .bind(page.events[0].aggregate_id)
    .bind(&data_profile_key)
    .execute(&mut *restore_tx)
    .await
    .expect("restore read payload");
    restore_tx.commit().await.expect("commit payload restore");

    let mut tamper_tx = read_pool_for_integrity.begin().await.expect("tamper tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tamper_tx)
        .await
        .expect("route tamper tx");
    sqlx::query(
        "DELETE FROM erpx_holding.scm_projection_checkpoints
         WHERE aggregate_type=$1 AND aggregate_id=$2 AND data_profile_key=$3",
    )
    .bind(&page.events[0].aggregate_type)
    .bind(page.events[0].aggregate_id)
    .bind(&data_profile_key)
    .execute(&mut *tamper_tx)
    .await
    .expect("remove checkpoint for integrity test");
    tamper_tx.commit().await.expect("commit checkpoint removal");

    let checkpoint_missing = store
        .reconcile_projection()
        .await
        .expect("checkpoint mismatch report");
    assert_eq!(checkpoint_missing.missing_checkpoint_count, 1);
    assert!(!checkpoint_missing.is_consistent());
    assert!(
        store
            .get_for_scope("product", product_id, &scope)
            .await
            .is_err()
    );
    assert!(
        store
            .project_event(
                page.events[0].event_id,
                &page.events[0].aggregate_type,
                page.events[0].aggregate_id,
                page.events[0].aggregate_version,
                &page.events[0].payload,
            )
            .await
            .is_err()
    );

    store
        .clear_projection()
        .await
        .expect("clear damaged read model");
    store
        .replay_event(
            page.events[0].event_id,
            &page.events[0].aggregate_type,
            page.events[0].aggregate_id,
            page.events[0].aggregate_version,
            &page.events[0].payload,
        )
        .await
        .expect("rebuild damaged read model");
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("reconciled checkpoint report")
            .is_consistent()
    );

    let cleared = store.clear_projection().await.expect("clear read model");
    assert_eq!(cleared.rows_cleared, 1);
    let missing = store
        .reconcile_projection()
        .await
        .expect("missing projection report");
    assert_eq!(missing.missing_aggregate_count, 1);
    assert!(!missing.is_consistent());
    store
        .replay_event(
            page.events[0].event_id,
            &page.events[0].aggregate_type,
            page.events[0].aggregate_id,
            page.events[0].aggregate_version,
            &page.events[0].payload,
        )
        .await
        .expect("rollback-safe replay");
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("reconciled after replay")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_procurement_lifecycle_transition_projects_and_rebuilds() {
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

    let data_profile_key = format!("holding-scm-transition-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let vendor_id = Uuid::new_v4();
    let product_id = Uuid::new_v4();
    let request_scope = scope.clone();
    let request = PurchaseRequest::new(
        Uuid::new_v4(),
        request_scope.clone(),
        holding_id,
        company_id,
        None,
        VendorReference::new(
            vendor_id,
            request_scope.clone(),
            holding_id,
            company_id,
            None,
            "Transition vendor",
        )
        .expect("vendor"),
        "THB",
        vec![PurchaseRequestLine {
            line_id: Uuid::new_v4(),
            product: ProductReference {
                product_id,
                product_reference: deterministic_reference(ScmAggregateKind::Product, product_id),
                scope: request_scope,
                holding_id,
                company_id,
                branch_id: None,
            },
            quantity: 3,
            estimated_unit_cost_minor: 125,
        }],
        Utc::now(),
        &context_with_key(scope.clone(), "scm/transition/create"),
    )
    .expect("purchase request");
    let request_id = request.id;
    let store =
        SqlxScmStore::new(write_pool, read_pool, data_profile_key.clone()).expect("SCM store");
    let created = store
        .save(&ScmAggregate::PurchaseRequest(request), None)
        .await
        .expect("purchase request source write");
    assert_eq!(created.version, 1);
    let submitted = store
        .transition(
            "purchase_request",
            request_id,
            "submitted",
            created.version,
            &context_with_key(scope.clone(), "scm/transition/submit"),
        )
        .await
        .expect("purchase request submit transition");
    assert_eq!(submitted.version, 2);
    let approved = store
        .transition(
            "purchase_request",
            request_id,
            "approved",
            submitted.version,
            &context_with_key(scope.clone(), "scm/transition/approve"),
        )
        .await
        .expect("purchase request approve transition");
    assert_eq!(approved.version, 3);

    let page = store
        .read_source_event_page(None, 10)
        .await
        .expect("SCM lifecycle source page");
    assert_eq!(page.events.len(), 3);
    for event in &page.events {
        store
            .project_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("SCM lifecycle projection");
    }
    let projected = store
        .get("purchase_request", request_id)
        .await
        .expect("SCM lifecycle read")
        .expect("projected purchase request");
    assert_eq!(projected.aggregate_version, 3);
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("SCM lifecycle reconciliation")
            .is_consistent()
    );

    let cleared = store
        .clear_projection()
        .await
        .expect("clear SCM projection");
    assert_eq!(cleared.rows_cleared, 1);
    for event in &page.events {
        store
            .project_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("SCM lifecycle replay");
    }
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("SCM lifecycle reconciliation after rebuild")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_draft_stock_movement_cancellation_persists_and_rebuilds() {
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

    let data_profile_key = format!("holding-scm-movement-cancel-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let branch_id = Uuid::new_v4();
    let scope = AuthorityScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    let product_id = Uuid::new_v4();
    let warehouse_id = Uuid::new_v4();
    let product = ProductReference {
        product_id,
        product_reference: deterministic_reference(ScmAggregateKind::Product, product_id),
        scope: scope.clone(),
        holding_id,
        company_id,
        branch_id: Some(branch_id),
    };
    let warehouse = WarehouseReference {
        warehouse_id,
        warehouse_reference: deterministic_reference(ScmAggregateKind::Warehouse, warehouse_id),
        scope: scope.clone(),
        holding_id,
        company_id,
        branch_id,
    };
    let movement = StockMovement::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        Some(branch_id),
        product.clone(),
        warehouse.clone(),
        StockMovementKind::Receipt,
        4,
        125,
        Utc::now(),
        &context_with_key(scope.clone(), "scm/movement-cancel/create"),
    )
    .expect("draft stock movement");
    let movement_id = movement.id;
    let store = SqlxScmStore::new(write_pool, read_pool, data_profile_key).expect("SCM store");
    store
        .save(&ScmAggregate::StockMovement(movement), None)
        .await
        .expect("movement source write");

    let cancel_context = context_with_key(scope, "scm/movement-cancel/cancel");
    let cancelled = store
        .transition(
            "stock_movement",
            movement_id,
            "cancelled",
            1,
            &cancel_context,
        )
        .await
        .expect("authoritative movement cancellation");
    assert_eq!(cancelled.version, 2);
    assert!(!cancelled.replayed);
    let replay = store
        .transition(
            "stock_movement",
            movement_id,
            "cancelled",
            1,
            &cancel_context,
        )
        .await
        .expect("idempotent cancellation replay");
    assert!(replay.replayed);
    assert_eq!(replay.version, cancelled.version);

    let source_events = store
        .read_source_event_page(None, 10)
        .await
        .expect("movement source outbox page")
        .events;
    assert_eq!(
        source_events.len(),
        2,
        "create and cancellation source events"
    );
    for event in &source_events {
        store
            .project_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("project movement source event");
    }

    let projected = store
        .get("stock_movement", movement_id)
        .await
        .expect("projected movement read")
        .expect("projected movement");
    let ScmAggregate::StockMovement(projected) = projected.aggregate else {
        panic!("projected aggregate has the wrong type");
    };
    assert_eq!(projected.status, StockMovementStatus::Cancelled);
    assert_eq!(projected.id, movement_id);
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("movement reconciliation")
            .is_consistent()
    );

    store
        .clear_projection()
        .await
        .expect("clear movement projection");
    for event in &source_events {
        store
            .replay_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("replay movement source event");
    }
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("movement rebuild reconciliation")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_stock_movement_post_is_atomic_idempotent_and_rebuildable() {
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

    let data_profile_key = format!("holding-scm-movement-post-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let branch_id = Uuid::new_v4();
    let scope = AuthorityScope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    let product_id = Uuid::new_v4();
    let warehouse_id = Uuid::new_v4();
    let product = ProductReference {
        product_id,
        product_reference: deterministic_reference(ScmAggregateKind::Product, product_id),
        scope: scope.clone(),
        holding_id,
        company_id,
        branch_id: Some(branch_id),
    };
    let warehouse = WarehouseReference {
        warehouse_id,
        warehouse_reference: deterministic_reference(ScmAggregateKind::Warehouse, warehouse_id),
        scope: scope.clone(),
        holding_id,
        company_id,
        branch_id,
    };
    let receipt = StockMovement::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        Some(branch_id),
        product.clone(),
        warehouse.clone(),
        StockMovementKind::Receipt,
        4,
        125,
        Utc::now(),
        &context_with_key(scope.clone(), "scm/movement-post/receipt-create"),
    )
    .expect("receipt movement");
    let receipt_id = receipt.id;
    let store = SqlxScmStore::new(write_pool, read_pool, data_profile_key).expect("SCM store");
    store
        .save(&ScmAggregate::StockMovement(receipt), None)
        .await
        .expect("receipt source write");
    let receipt_post = store
        .transition(
            "stock_movement",
            receipt_id,
            "posted",
            1,
            &context_with_key(scope.clone(), "scm/movement-post/receipt-post"),
        )
        .await
        .expect("authoritative receipt posting");
    assert_eq!(receipt_post.version, 2);
    assert!(!receipt_post.replayed);
    let receipt_replay = store
        .transition(
            "stock_movement",
            receipt_id,
            "posted",
            1,
            &context_with_key(scope.clone(), "scm/movement-post/receipt-post"),
        )
        .await
        .expect("idempotent receipt replay");
    assert!(receipt_replay.replayed);
    assert_eq!(receipt_replay.version, receipt_post.version);

    let issue = StockMovement::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        Some(branch_id),
        product.clone(),
        warehouse.clone(),
        StockMovementKind::Issue,
        3,
        125,
        Utc::now(),
        &context_with_key(scope.clone(), "scm/movement-post/issue-create"),
    )
    .expect("issue movement");
    let issue_id = issue.id;
    store
        .save(&ScmAggregate::StockMovement(issue), None)
        .await
        .expect("issue source write");
    let issue_post = store
        .transition(
            "stock_movement",
            issue_id,
            "posted",
            1,
            &context_with_key(scope.clone(), "scm/movement-post/issue-post"),
        )
        .await
        .expect("authoritative issue posting");
    assert_eq!(issue_post.version, 2);

    let blocked_issue = StockMovement::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        Some(branch_id),
        product.clone(),
        warehouse.clone(),
        StockMovementKind::Issue,
        2,
        125,
        Utc::now(),
        &context_with_key(scope.clone(), "scm/movement-post/blocked-create"),
    )
    .expect("blocked issue movement");
    let blocked_id = blocked_issue.id;
    store
        .save(&ScmAggregate::StockMovement(blocked_issue), None)
        .await
        .expect("blocked issue source write");
    assert!(
        store
            .transition(
                "stock_movement",
                blocked_id,
                "posted",
                1,
                &context_with_key(scope.clone(), "scm/movement-post/blocked-post"),
            )
            .await
            .is_err(),
        "posting must fail closed when available stock is insufficient"
    );

    let destination_id = Uuid::new_v4();
    let destination = WarehouseReference {
        warehouse_id: destination_id,
        warehouse_reference: deterministic_reference(ScmAggregateKind::Warehouse, destination_id),
        scope: scope.clone(),
        holding_id,
        company_id,
        branch_id,
    };
    let transfer_id = Uuid::new_v4();
    let transfer = StockTransfer::new(
        transfer_id,
        scope.clone(),
        holding_id,
        company_id,
        Some(branch_id),
        warehouse.clone(),
        destination,
        vec![StockTransferLine {
            line_id: Uuid::new_v4(),
            product: product.clone(),
            quantity: 1,
        }],
        Utc::now(),
        &context_with_key(scope.clone(), "scm/movement-post/transfer-create"),
    )
    .expect("draft transfer");
    store
        .save(&ScmAggregate::StockTransfer(transfer), None)
        .await
        .expect("transfer source write");
    let transfer_cancel_context =
        context_with_key(scope.clone(), "scm/movement-post/transfer-cancel");
    let transfer_cancelled = store
        .transition(
            "stock_transfer",
            transfer_id,
            "cancelled",
            1,
            &transfer_cancel_context,
        )
        .await
        .expect("authoritative transfer cancellation");
    assert_eq!(transfer_cancelled.version, 2);
    let transfer_replay = store
        .transition(
            "stock_transfer",
            transfer_id,
            "cancelled",
            1,
            &transfer_cancel_context,
        )
        .await
        .expect("transfer cancellation replay");
    assert!(transfer_replay.replayed);

    let reservation_id = Uuid::new_v4();
    let reservation = StockReservation::new(
        reservation_id,
        scope.clone(),
        holding_id,
        company_id,
        Some(branch_id),
        product,
        warehouse,
        1,
        Utc::now(),
        &context_with_key(scope.clone(), "scm/movement-post/reservation-create"),
    )
    .expect("pending reservation");
    store
        .save(&ScmAggregate::StockReservation(reservation), None)
        .await
        .expect("reservation source write");
    let reservation_cancel_context =
        context_with_key(scope.clone(), "scm/movement-post/reservation-cancel");
    let reservation_cancelled = store
        .transition(
            "stock_reservation",
            reservation_id,
            "cancelled",
            1,
            &reservation_cancel_context,
        )
        .await
        .expect("authoritative reservation cancellation");
    assert_eq!(reservation_cancelled.version, 2);
    let reservation_replay = store
        .transition(
            "stock_reservation",
            reservation_id,
            "cancelled",
            1,
            &reservation_cancel_context,
        )
        .await
        .expect("reservation cancellation replay");
    assert!(reservation_replay.replayed);

    let source_events = store
        .read_source_event_page(None, 100)
        .await
        .expect("movement source events")
        .events;
    assert_eq!(
        source_events.len(),
        9,
        "two posted movements, blocked draft, and transfer/reservation cancellations"
    );
    for event in &source_events {
        store
            .project_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("project movement source event");
    }
    let projected = store
        .get("stock_movement", issue_id)
        .await
        .expect("projected issue read")
        .expect("projected issue");
    let ScmAggregate::StockMovement(projected) = projected.aggregate else {
        panic!("projected aggregate has the wrong type");
    };
    assert_eq!(projected.status, StockMovementStatus::Posted);
    let projected_transfer = store
        .get("stock_transfer", transfer_id)
        .await
        .expect("projected transfer read")
        .expect("projected transfer");
    let ScmAggregate::StockTransfer(projected_transfer) = projected_transfer.aggregate else {
        panic!("projected transfer has the wrong aggregate type");
    };
    assert_eq!(projected_transfer.status, StockTransferStatus::Cancelled);
    let projected_reservation = store
        .get("stock_reservation", reservation_id)
        .await
        .expect("projected reservation read")
        .expect("projected reservation");
    let ScmAggregate::StockReservation(projected_reservation) = projected_reservation.aggregate
    else {
        panic!("projected reservation has the wrong aggregate type");
    };
    assert_eq!(
        projected_reservation.status,
        StockReservationStatus::Cancelled
    );
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("movement reconciliation")
            .is_consistent()
    );

    let cleared = store.clear_projection().await.expect("clear read model");
    assert_eq!(cleared.rows_cleared, 5);
    for event in &source_events {
        store
            .replay_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("replay movement source event");
    }
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("movement rebuild reconciliation")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_goods_receipt_post_updates_order_and_receipt_atomically() {
    let write_url = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL")
        .expect("ERPX_TEST_HOLDING_READ_DATABASE_URL is required");
    let write_pool = PgPool::connect(&write_url).await.expect("write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("read pool");
    let write_probe = write_pool.clone();
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-scm-receipt-post-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let product_id = Uuid::new_v4();
    let product = ProductReference {
        product_id,
        product_reference: deterministic_reference(ScmAggregateKind::Product, product_id),
        scope: scope.clone(),
        holding_id,
        company_id,
        branch_id: None,
    };
    let vendor = VendorReference::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        "Goods receipt vendor",
    )
    .expect("vendor");
    let request_context = context_with_key(scope.clone(), "scm/receipt-post/request");
    let mut request = PurchaseRequest::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        vendor,
        "THB",
        vec![PurchaseRequestLine {
            line_id: Uuid::new_v4(),
            product: product.clone(),
            quantity: 10,
            estimated_unit_cost_minor: 100,
        }],
        Utc::now(),
        &request_context,
    )
    .expect("purchase request");
    request
        .submit(&request_context, Utc::now())
        .expect("request submit");
    request
        .approve(&request_context, Utc::now())
        .expect("request approve");

    let order_context = context_with_key(scope.clone(), "scm/receipt-post/order");
    let order = PurchaseOrder::new_from_request(
        Uuid::new_v4(),
        &request,
        vec![PurchaseOrderLine {
            line_id: request.lines[0].line_id,
            product: product.clone(),
            ordered_quantity: 10,
            unit_cost_minor: 100,
            received_quantity: 0,
        }],
        Utc::now(),
        &order_context,
    )
    .expect("purchase order");
    let order_id = order.id;
    let order_line_id = order.lines[0].line_id;
    let receipt_create_context = context_with_key(scope.clone(), "scm/receipt-post/receipt");
    let receipt = GoodsReceipt::new(
        Uuid::new_v4(),
        &PurchaseOrder {
            status: PurchaseOrderStatus::Issued,
            ..order.clone()
        },
        vec![GoodsReceiptLine {
            line_id: order_line_id,
            product,
            quantity: 5,
            unit_cost_minor: 100,
        }],
        Utc::now(),
        &receipt_create_context,
    )
    .expect("goods receipt");
    let receipt_id = receipt.id;

    let store =
        SqlxScmStore::new(write_pool, read_pool, data_profile_key.clone()).expect("SCM store");
    store
        .save(&ScmAggregate::PurchaseRequest(request), None)
        .await
        .expect("request source write");
    store
        .save(&ScmAggregate::PurchaseOrder(order), None)
        .await
        .expect("order source write");
    store
        .transition(
            "purchase_order",
            order_id,
            "submitted",
            1,
            &context_with_key(scope.clone(), "scm/receipt-post/order-submit"),
        )
        .await
        .expect("order submit");
    store
        .transition(
            "purchase_order",
            order_id,
            "approved",
            2,
            &context_with_key(scope.clone(), "scm/receipt-post/order-approve"),
        )
        .await
        .expect("order approve");
    store
        .transition(
            "purchase_order",
            order_id,
            "issued",
            3,
            &context_with_key(scope.clone(), "scm/receipt-post/order-issue"),
        )
        .await
        .expect("order issue");
    store
        .save(&ScmAggregate::GoodsReceipt(receipt), None)
        .await
        .expect("receipt source write");
    let post_context = context_with_key(scope.clone(), "scm/receipt-post/post");

    let posted = store
        .transition("goods_receipt", receipt_id, "posted", 1, &post_context)
        .await
        .expect("atomic goods receipt posting");
    assert_eq!(posted.aggregate_type, "goods_receipt");
    assert_eq!(posted.aggregate_id, receipt_id);
    assert_eq!(posted.version, 2);
    assert!(!posted.replayed);

    let source_events = store
        .read_source_event_page(None, 100)
        .await
        .expect("source events")
        .events;
    assert_eq!(
        source_events.len(),
        8,
        "pair posting emits two source events"
    );
    let receipt_event = source_events
        .iter()
        .find(|event| {
            event.aggregate_type == "goods_receipt"
                && event.aggregate_id == receipt_id
                && event.aggregate_version == 2
        })
        .cloned()
        .expect("posted receipt source event");

    let mut delete_tx = write_probe.begin().await.expect("delete transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *delete_tx)
        .await
        .expect("route delete transaction");
    sqlx::query("DELETE FROM erpx_holding.outbox_events WHERE event_id=$1 AND data_profile_key=$2")
        .bind(receipt_event.event_id)
        .bind(&data_profile_key)
        .execute(&mut *delete_tx)
        .await
        .expect("delete receipt outbox event");
    delete_tx.commit().await.expect("commit receipt deletion");
    assert!(
        store
            .transition("goods_receipt", receipt_id, "posted", 1, &post_context)
            .await
            .is_err(),
        "receipt replay must fail closed when its source event is missing"
    );

    let mut restore_tx = write_probe.begin().await.expect("restore transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *restore_tx)
        .await
        .expect("route restore transaction");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(receipt_event.event_id)
    .bind(&receipt_event.aggregate_type)
    .bind(receipt_event.aggregate_id)
    .bind(&data_profile_key)
    .bind(i64::try_from(receipt_event.aggregate_version).expect("receipt version"))
    .bind(&receipt_event.event_type)
    .bind(&receipt_event.payload)
    .bind(receipt_event.occurred_at)
    .execute(&mut *restore_tx)
    .await
    .expect("restore receipt outbox event");
    restore_tx.commit().await.expect("commit receipt restore");

    let replay = store
        .transition("goods_receipt", receipt_id, "posted", 1, &post_context)
        .await
        .expect("idempotent goods receipt replay");
    assert!(replay.replayed);
    assert_eq!(replay.version, posted.version);

    for event in &source_events {
        store
            .project_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("project source event");
    }

    let projected_order = store
        .get("purchase_order", order_id)
        .await
        .expect("projected order read")
        .expect("projected order");
    let ScmAggregate::PurchaseOrder(projected_order) = projected_order.aggregate else {
        panic!("projected order has the wrong aggregate type");
    };
    assert_eq!(
        projected_order.status,
        PurchaseOrderStatus::PartiallyReceived
    );
    assert_eq!(projected_order.lines[0].received_quantity, 5);

    let projected_receipt = store
        .get("goods_receipt", receipt_id)
        .await
        .expect("projected receipt read")
        .expect("projected receipt");
    let ScmAggregate::GoodsReceipt(projected_receipt) = projected_receipt.aggregate else {
        panic!("projected receipt has the wrong aggregate type");
    };
    assert_eq!(projected_receipt.status, GoodsReceiptStatus::Posted);
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("post reconciliation")
            .is_consistent()
    );

    let cleared = store.clear_projection().await.expect("clear read model");
    assert_eq!(cleared.rows_cleared, 3);
    for event in &source_events {
        store
            .replay_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("replay source event");
    }
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("post rebuild reconciliation")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_reconciliation_detects_authoritative_version_gaps() {
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

    let data_profile_key = format!("holding-scm-gap-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let product = Product::new(
        Uuid::new_v4(),
        scope,
        holding_id,
        company_id,
        None,
        "SKU-SCM-GAP",
        "Source gap product",
        "each",
        Utc::now(),
        &context_with_key(
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
            "scm/e2e-gap",
        ),
    )
    .expect("product");
    let product_id = product.id;
    let product_payload = serde_json::to_value(&product).expect("product payload");
    let store = SqlxScmStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("SCM store");
    store
        .save(&ScmAggregate::Product(product.clone()), None)
        .await
        .expect("authoritative version one");
    let first_event = store
        .read_source_event_page(None, 10)
        .await
        .expect("source event")
        .events
        .into_iter()
        .next()
        .expect("version one event");

    // Simulate an authoritative/import corruption with version 2 absent and
    // version 3 present. The read model is intentionally aligned to version
    // 3 so a latest-version/payload-only reconciliation would falsely pass.
    let gap_event_id = Uuid::new_v4();
    let gap_payload = serde_json::json!({
        "aggregate_type": "product",
        "aggregate": product_payload,
        "version": 3_u64,
    });
    let mut write_tx = write_pool.begin().await.expect("source gap tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *write_tx)
        .await
        .expect("route source gap tx");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1, 'product', $2, $3, 3, 'scm.product.v1', $4, now())",
    )
    .bind(gap_event_id)
    .bind(product_id)
    .bind(&data_profile_key)
    .bind(&gap_payload)
    .execute(&mut *write_tx)
    .await
    .expect("insert authoritative version three");
    write_tx.commit().await.expect("commit source gap");

    let mut read_tx = read_pool.begin().await.expect("aligned read tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *read_tx)
        .await
        .expect("route aligned read tx");
    sqlx::query(
        "INSERT INTO erpx_holding.scm_aggregates
            (aggregate_id, aggregate_type, data_profile_key, company_id, branch_id,
             reference, source_version, payload)
         VALUES ($1, 'product', $2, $3, NULL, $4, 3, $5)",
    )
    .bind(product_id)
    .bind(&data_profile_key)
    .bind(company_id)
    .bind(&product.reference)
    .bind(&product_payload)
    .execute(&mut *read_tx)
    .await
    .expect("align read aggregate to version three");
    sqlx::query(
        "INSERT INTO erpx_holding.scm_projection_checkpoints
            (aggregate_type, aggregate_id, data_profile_key, last_event_id,
             source_version)
         VALUES ('product', $1, $2, $3, 3)",
    )
    .bind(product_id)
    .bind(&data_profile_key)
    .bind(gap_event_id)
    .execute(&mut *read_tx)
    .await
    .expect("align read checkpoint to version three");
    read_tx.commit().await.expect("commit aligned read model");

    let report = store
        .reconcile_projection()
        .await
        .expect("source gap reconciliation report");
    assert_eq!(report.source_event_count, 2);
    assert_eq!(report.source_gap_count, 1);
    assert!(!report.is_consistent());
    assert_eq!(report.missing_aggregate_count, 0);
    assert_eq!(report.stale_aggregate_count, 0);
    assert_eq!(report.invalid_projection_count, 0);
    assert_ne!(first_event.aggregate_version, 3);
}

#[tokio::test]
#[ignore = "requires two disposable holding PostgreSQL databases"]
async fn live_scm_reconciliation_cycle_persists_page_evidence_and_replays_idempotently() {
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

    let data_profile_key = format!("holding-scm-cycle-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let product = Product::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        "SKU-SCM-CYCLE",
        "Reconciliation cycle product",
        "each",
        Utc::now(),
        &context_with_key(scope, "scm/e2e-reconciliation-cycle"),
    )
    .expect("product");
    let store = SqlxScmStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("SCM store");
    let receipt = store
        .save(&ScmAggregate::Product(product), None)
        .await
        .expect("source write");
    let source_event = store
        .read_source_event_page(None, 10)
        .await
        .expect("source event page")
        .events
        .into_iter()
        .next()
        .expect("source event");
    store
        .project_event(
            source_event.event_id,
            &source_event.aggregate_type,
            source_event.aggregate_id,
            source_event.aggregate_version,
            &source_event.payload,
        )
        .await
        .expect("project source event");

    let first = store
        .reconcile_projection_and_persist_evidence(1)
        .await
        .expect("persisted reconciliation cycle");
    assert!(first.is_consistent());
    assert_eq!(first.source_event_count, 1);

    let mut count_tx = write_pool.begin().await.expect("evidence count tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *count_tx)
        .await
        .expect("route evidence count tx");
    let first_outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.outbox_events WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *count_tx)
    .await
    .expect("first outbox count");
    let first_evidence_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.scm_command_idempotency
         WHERE data_profile_key=$1 AND aggregate_type='scm_reconciliation'",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *count_tx)
    .await
    .expect("first evidence count");
    count_tx.commit().await.expect("commit first counts");
    assert_eq!(first_outbox_count, 2, "source plus one page evidence fact");
    assert_eq!(first_evidence_count, 1);

    let replay = store
        .reconcile_projection_and_persist_evidence(1)
        .await
        .expect("replayed reconciliation cycle");
    assert_eq!(replay, first);

    let mut replay_count_tx = write_pool.begin().await.expect("replay count tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *replay_count_tx)
        .await
        .expect("route replay count tx");
    let replay_outbox_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.outbox_events WHERE data_profile_key=$1",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *replay_count_tx)
    .await
    .expect("replay outbox count");
    let replay_evidence_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM erpx_holding.scm_command_idempotency
         WHERE data_profile_key=$1 AND aggregate_type='scm_reconciliation'",
    )
    .bind(&data_profile_key)
    .fetch_one(&mut *replay_count_tx)
    .await
    .expect("replay evidence count");
    replay_count_tx
        .commit()
        .await
        .expect("commit replay counts");
    assert_eq!(replay_outbox_count, first_outbox_count);
    assert_eq!(replay_evidence_count, first_evidence_count);
    assert_eq!(receipt.version, 1);
}
