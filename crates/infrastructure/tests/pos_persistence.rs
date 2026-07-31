//! Disposable end-to-end evidence for the normalized POS write boundary.

use chrono::Utc;
use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext, TenantScope};
use erpx_domain::pos::{Payment, PaymentMethod, Sale, SaleLine, SaleStatus};
use erpx_infrastructure::{SqlxPosPersistence, SqlxPosProjectionStore};
use sqlx::PgPool;
use uuid::Uuid;

fn command_context(holding_id: Uuid, company_id: Uuid) -> CommandContext {
    command_context_with_key(holding_id, company_id, "pos-create-1")
}

fn command_context_with_key(holding_id: Uuid, company_id: Uuid, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "pos-operator".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "pos.sale.create",
    )
    .expect("command context")
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_pos_write_is_normalized_scoped_and_idempotent() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant write migration");

    let holding_ref = format!("holding-pos-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let line = SaleLine::new(
        Uuid::new_v4(),
        "SKU-001",
        "Anonymized POS item",
        2,
        1_250,
        250,
    )
    .expect("sale line");
    let sale = Sale::new(
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "THB",
        vec![line],
        Utc::now(),
        &command_context(holding_id, company_id),
    )
    .expect("sale");
    let store = SqlxPosPersistence::new(pool.clone(), &holding_ref).expect("POS store");
    let first = store.save_sale(&sale, None).await.expect("first sale");
    assert_eq!(first.version, 1);
    assert!(!first.replayed);
    let replay = store.save_sale(&sale, None).await.expect("sale replay");
    assert_eq!(
        replay,
        erpx_infrastructure::PosWriteReceipt {
            replayed: true,
            ..first.clone()
        }
    );

    let mut next_sale = sale.clone();
    let mut next_context =
        command_context_with_key(holding_id, company_id, "pos-open-after-replay");
    next_context.expected_aggregate_version = Some(first.version);
    next_sale
        .transition(SaleStatus::Open, &next_context, Utc::now())
        .expect("next POS sale version");
    let second = store
        .save_sale(&next_sale, Some(first.version))
        .await
        .expect("second POS sale version");
    assert_eq!(second.version, 2);
    let delayed_replay = store
        .save_sale(&sale, None)
        .await
        .expect("historical idempotency replay");
    assert_eq!(
        delayed_replay,
        erpx_infrastructure::PosWriteReceipt {
            replayed: true,
            ..first
        }
    );

    let mut tx = pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT count(*) FROM erpx_tenant.pos_sales WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.pos_sale_lines WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.pos_sale_payments WHERE holding_ref=$1),
            (SELECT count(*) FROM erpx_tenant.outbox_events WHERE holding_ref=$1 AND aggregate_type='pos_sale')",
    )
    .bind(&holding_ref)
    .fetch_one(&mut *tx)
    .await
    .expect("normalized counts");
    assert_eq!(counts, (1, 1, 0, 2));
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_pos_replay_rejects_orphaned_outbox_event() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant write migration");

    let holding_ref = format!("holding-pos-replay-integrity-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let sale = Sale::new(
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "THB",
        vec![
            SaleLine::new(
                Uuid::new_v4(),
                "SKU-REPLAY",
                "Anonymized replay-integrity item",
                1,
                500,
                50,
            )
            .expect("sale line"),
        ],
        Utc::now(),
        &command_context(holding_id, company_id),
    )
    .expect("sale");
    let store = SqlxPosPersistence::new(pool.clone(), &holding_ref).expect("POS store");
    let first = store.save_sale(&sale, None).await.expect("first sale");

    let mut tx = pool.begin().await.expect("integrity transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.outbox_events
         SET occurred_at=occurred_at + INTERVAL '1 second'
         WHERE event_id=$1",
    )
    .bind(first.event_id)
    .execute(&mut *tx)
    .await
    .expect("tamper outbox timestamp");
    tx.commit().await.expect("commit timestamp tamper");
    assert!(
        store.save_sale(&sale, None).await.is_err(),
        "idempotent replay must bind the outbox occurrence timestamp"
    );

    let mut restore = pool.begin().await.expect("timestamp repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *restore)
        .await
        .expect("holding scope for timestamp repair");
    sqlx::query("UPDATE erpx_tenant.outbox_events SET occurred_at=$1 WHERE event_id=$2")
        .bind(
            sale.command_history
                .last()
                .expect("sale event")
                .event
                .occurred_at,
        )
        .bind(first.event_id)
        .execute(&mut *restore)
        .await
        .expect("restore outbox timestamp");
    restore.commit().await.expect("commit timestamp repair");

    let mut tx = pool.begin().await.expect("integrity transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    sqlx::query("DELETE FROM erpx_tenant.outbox_events WHERE event_id=$1")
        .bind(first.event_id)
        .execute(&mut *tx)
        .await
        .expect("remove disposable outbox event");
    tx.commit().await.expect("commit disposable corruption");

    let error = store
        .save_sale(&sale, None)
        .await
        .expect_err("orphaned replay must fail closed");
    assert!(error.to_string().contains("not backed"));
    assert!(!error.to_string().contains(&sale.reference));

    let mut inspection = pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *inspection)
        .await
        .expect("holding scope");
    let outbox_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_id=$2",
    )
    .bind(&holding_ref)
    .bind(sale.id)
    .fetch_one(&mut *inspection)
    .await
    .expect("outbox count");
    assert_eq!(outbox_count, 0);
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_pos_version_advance_rejects_missing_prior_outbox_and_rolls_back() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant write migration");

    let holding_ref = format!("holding-pos-history-gap-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let sale = Sale::new(
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "THB",
        vec![
            SaleLine::new(
                Uuid::new_v4(),
                "SKU-HISTORY-GAP",
                "Anonymized history-gap item",
                1,
                700,
                70,
            )
            .expect("sale line"),
        ],
        Utc::now(),
        &command_context(holding_id, company_id),
    )
    .expect("sale");
    let store = SqlxPosPersistence::new(pool.clone(), &holding_ref).expect("POS store");
    let first = store.save_sale(&sale, None).await.expect("first sale");

    let mut tx = pool.begin().await.expect("history-gap transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("holding scope");
    sqlx::query("DELETE FROM erpx_tenant.outbox_events WHERE event_id=$1")
        .bind(first.event_id)
        .execute(&mut *tx)
        .await
        .expect("remove disposable prior event");
    tx.commit().await.expect("commit history-gap setup");

    let mut next_sale = sale.clone();
    let mut next_context = command_context_with_key(holding_id, company_id, "pos-open-1");
    next_context.expected_aggregate_version = Some(1);
    next_sale
        .transition(SaleStatus::Open, &next_context, Utc::now())
        .expect("next domain version");
    let error = store
        .save_sale(&next_sale, Some(1))
        .await
        .expect_err("version advance must fail closed on an outbox gap");
    assert!(
        error
            .to_string()
            .contains("outbox history is not contiguous")
    );
    assert!(!error.to_string().contains(&next_sale.reference));

    let mut inspection = pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *inspection)
        .await
        .expect("holding scope");
    let source: (i64, String, i64) = sqlx::query_as(
        "SELECT version, status,
                (SELECT count(*) FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_id=$2)
         FROM erpx_tenant.pos_sales
         WHERE holding_ref=$1 AND sale_id=$2",
    )
    .bind(&holding_ref)
    .bind(sale.id)
    .fetch_one(&mut *inspection)
    .await
    .expect("source rollback state");
    assert_eq!(source, (1, "draft".into(), 0));
    let idempotency_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_tenant.pos_command_idempotency
         WHERE holding_ref=$1 AND aggregate_id=$2",
    )
    .bind(&holding_ref)
    .bind(sale.id)
    .fetch_one(&mut *inspection)
    .await
    .expect("idempotency rollback state");
    assert_eq!(idempotency_count, 1);
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_pos_payment_facts_are_append_only_and_immutable() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant write migration");

    let holding_ref = format!("holding-pos-payment-source-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let sale = Sale::new(
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "THB",
        vec![
            SaleLine::new(
                Uuid::new_v4(),
                "SKU-PAYMENT-SOURCE",
                "Anonymized payment-source item",
                1,
                1_000,
                0,
            )
            .expect("sale line"),
        ],
        Utc::now(),
        &command_context(holding_id, company_id),
    )
    .expect("sale");
    let store = SqlxPosPersistence::new(pool.clone(), &holding_ref).expect("POS store");
    let created = store.save_sale(&sale, None).await.expect("create sale");

    let mut sale = sale;
    let mut open_context = command_context_with_key(holding_id, company_id, "pos-source-open");
    open_context.expected_aggregate_version = Some(created.version);
    sale.transition(SaleStatus::Open, &open_context, Utc::now())
        .expect("open sale");
    let opened = store
        .save_sale(&sale, Some(created.version))
        .await
        .expect("open sale");

    let mut payment_context =
        command_context_with_key(holding_id, company_id, "pos-source-payment");
    payment_context.expected_aggregate_version = Some(opened.version);
    sale.record_payment(
        Payment::new(Uuid::new_v4(), 990, PaymentMethod::Card, Utc::now()).expect("payment"),
        &payment_context,
        Utc::now(),
    )
    .expect("record payment");
    let paid = store
        .save_sale(&sale, Some(opened.version))
        .await
        .expect("record payment");
    assert_eq!(paid.version, 3);

    let mut unauthorized_addition = sale.clone();
    let unauthorized_payment =
        Payment::new(Uuid::new_v4(), 1, PaymentMethod::Cash, Utc::now()).expect("payment");
    let mut unauthorized_context =
        command_context_with_key(holding_id, company_id, "pos-source-unauthorized-payment");
    unauthorized_context.expected_aggregate_version = Some(paid.version);
    unauthorized_addition
        .transition(SaleStatus::Held, &unauthorized_context, Utc::now())
        .expect("hold unauthorized candidate");
    unauthorized_addition.payments.push(unauthorized_payment);
    unauthorized_addition.paid_minor += 1;
    let unauthorized_error = store
        .save_sale(&unauthorized_addition, Some(paid.version))
        .await
        .expect_err("payment addition must require a payment event");
    assert!(
        unauthorized_error
            .to_string()
            .contains("require a payment_recorded event")
    );

    let mut changed = sale.clone();
    let mut changed_context =
        command_context_with_key(holding_id, company_id, "pos-source-payment-change");
    changed_context.expected_aggregate_version = Some(paid.version);
    changed
        .transition(SaleStatus::Held, &changed_context, Utc::now())
        .expect("hold changed candidate");
    changed.payments[0].amount_minor = 989;
    changed.paid_minor = 989;
    let changed_error = store
        .save_sale(&changed, Some(paid.version))
        .await
        .expect_err("payment rewrite must fail closed");
    assert!(
        changed_error
            .to_string()
            .contains("payment facts cannot be removed or changed")
    );

    let mut removed = sale.clone();
    let mut removed_context =
        command_context_with_key(holding_id, company_id, "pos-source-payment-removal");
    removed_context.expected_aggregate_version = Some(paid.version);
    removed
        .transition(SaleStatus::Held, &removed_context, Utc::now())
        .expect("hold removed candidate");
    removed.payments.clear();
    removed.paid_minor = 0;
    let removed_error = store
        .save_sale(&removed, Some(paid.version))
        .await
        .expect_err("payment removal must fail closed");
    assert!(
        removed_error
            .to_string()
            .contains("payment facts cannot be removed or changed")
    );

    let mut inspection = pool.begin().await.expect("inspection transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *inspection)
        .await
        .expect("holding scope");
    let source: (i64, String, i64, i64) = sqlx::query_as(
        "SELECT version, status,
                (SELECT count(*) FROM erpx_tenant.pos_sale_payments
                 WHERE holding_ref=$1 AND sale_id=$2),
                (SELECT count(*) FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_id=$2)
         FROM erpx_tenant.pos_sales
         WHERE holding_ref=$1 AND sale_id=$2",
    )
    .bind(&holding_ref)
    .bind(sale.id)
    .fetch_one(&mut *inspection)
    .await
    .expect("source rollback state");
    assert_eq!(source, (3, "open".into(), 1, 3));
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases"]
async fn live_pos_projection_rejects_forged_source_payload_before_read_write() {
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

    let holding_ref = format!("holding-pos-projection-integrity-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let sale = Sale::new(
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "THB",
        vec![
            SaleLine::new(
                Uuid::new_v4(),
                "SKU-PROJECTION-INTEGRITY",
                "Anonymized projection integrity item",
                1,
                900,
                90,
            )
            .expect("sale line"),
        ],
        Utc::now(),
        &command_context(holding_id, company_id),
    )
    .expect("sale");
    SqlxPosPersistence::new(write_pool.clone(), &holding_ref)
        .expect("source store")
        .save_sale(&sale, None)
        .await
        .expect("source write");

    let projection = SqlxPosProjectionStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("projection store");
    let page = projection
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert_eq!(page.events.len(), 1);
    let mut forged = page.events[0].clone();
    forged.payload["aggregate"]["reference"] = serde_json::json!("POS-FORGED");
    assert!(
        projection.project_event(&forged).await.is_err(),
        "a forged POS payload must be rejected before read rows or receipts"
    );
    assert!(
        projection
            .project_event(&page.events[0])
            .await
            .expect("project")
            .applied,
        "the exact authoritative event remains projectable"
    );

    let healthy = projection
        .reconcile_projection()
        .await
        .expect("healthy POS reconciliation");
    assert!(healthy.is_consistent());

    let mut source_tamper = write_pool.begin().await.expect("source tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *source_tamper)
        .await
        .expect("source holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sales
         SET reference=$1
         WHERE holding_ref=$2 AND sale_id=$3",
    )
    .bind("POS-SOURCE-TAMPERED")
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *source_tamper)
    .await
    .expect("tamper authoritative POS source");
    source_tamper.commit().await.expect("commit source tamper");
    assert!(
        projection.reconcile_projection().await.is_err(),
        "POS reconciliation must fail closed on normalized source drift"
    );
    assert!(
        projection.project_event(&page.events[0]).await.is_err(),
        "POS projector must not acknowledge an event against a drifted source snapshot"
    );

    let mut source_repair = write_pool.begin().await.expect("source repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *source_repair)
        .await
        .expect("source repair holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sales
         SET reference=$1
         WHERE holding_ref=$2 AND sale_id=$3",
    )
    .bind(&sale.reference)
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *source_repair)
    .await
    .expect("repair authoritative POS source");
    source_repair.commit().await.expect("commit source repair");
    assert!(
        projection
            .reconcile_projection()
            .await
            .unwrap()
            .is_consistent()
    );

    let forged_checkpoint_event_id = Uuid::new_v4();
    let mut tamper = read_pool
        .begin()
        .await
        .expect("checkpoint tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper)
        .await
        .expect("read holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_projection_checkpoints
         SET last_event_id=$1
         WHERE holding_ref=$2 AND projection_name=$3 AND aggregate_id=$4",
    )
    .bind(forged_checkpoint_event_id)
    .bind(&holding_ref)
    .bind(erpx_infrastructure::POS_PROJECTION_NAME)
    .bind(sale.id)
    .execute(&mut *tamper)
    .await
    .expect("tamper checkpoint identity");
    tamper.commit().await.expect("commit checkpoint tamper");

    let drifted = projection
        .reconcile_projection()
        .await
        .expect("drifted POS reconciliation");
    assert_eq!(drifted.stale_aggregate_count, 1);
    assert!(!drifted.is_consistent());

    let mut repair = read_pool
        .begin()
        .await
        .expect("checkpoint repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *repair)
        .await
        .expect("read holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_projection_checkpoints
         SET last_event_id=$1
         WHERE holding_ref=$2 AND projection_name=$3 AND aggregate_id=$4",
    )
    .bind(page.events[0].event_id)
    .bind(&holding_ref)
    .bind(erpx_infrastructure::POS_PROJECTION_NAME)
    .bind(sale.id)
    .execute(&mut *repair)
    .await
    .expect("repair checkpoint identity");
    repair.commit().await.expect("commit checkpoint repair");

    let repaired = projection
        .reconcile_projection()
        .await
        .expect("repaired POS reconciliation");
    assert!(repaired.is_consistent());

    let forged_read_event_id = Uuid::new_v4();
    let mut read_tamper = read_pool
        .begin()
        .await
        .expect("read-row tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *read_tamper)
        .await
        .expect("read-row holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sale_read_rows
         SET source_event_id=$1
         WHERE holding_ref=$2 AND sale_id=$3",
    )
    .bind(forged_read_event_id)
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *read_tamper)
    .await
    .expect("tamper sale read identity");
    read_tamper.commit().await.expect("commit read-row tamper");

    let read_drift = projection
        .reconcile_projection()
        .await
        .expect("read-row drift reconciliation");
    assert_eq!(read_drift.stale_aggregate_count, 1);
    assert!(!read_drift.is_consistent());

    let mut read_repair = read_pool
        .begin()
        .await
        .expect("read-row repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *read_repair)
        .await
        .expect("read-row repair holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sale_read_rows
         SET source_event_id=$1
         WHERE holding_ref=$2 AND sale_id=$3",
    )
    .bind(page.events[0].event_id)
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *read_repair)
    .await
    .expect("repair sale read identity");
    read_repair.commit().await.expect("commit read-row repair");

    let forged_holding_id = Uuid::new_v4();
    let mut scope_tamper = read_pool
        .begin()
        .await
        .expect("read scope tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *scope_tamper)
        .await
        .expect("read scope tamper holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sale_read_rows
         SET holding_id=$1
         WHERE holding_ref=$2 AND sale_id=$3",
    )
    .bind(forged_holding_id)
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *scope_tamper)
    .await
    .expect("tamper sale read scope");
    scope_tamper
        .commit()
        .await
        .expect("commit read scope tamper");
    let scope_drift = projection
        .reconcile_projection()
        .await
        .expect_err("read scope drift must fail closed");
    assert!(scope_drift.to_string().contains("scope"));

    let mut scope_repair = read_pool
        .begin()
        .await
        .expect("read scope repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *scope_repair)
        .await
        .expect("read scope repair holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sale_read_rows
         SET holding_id=$1
         WHERE holding_ref=$2 AND sale_id=$3",
    )
    .bind(holding_id)
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *scope_repair)
    .await
    .expect("repair sale read scope");
    scope_repair
        .commit()
        .await
        .expect("commit read scope repair");

    assert!(
        projection
            .reconcile_projection()
            .await
            .expect("repaired read-row reconciliation")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases"]
async fn live_pos_payment_reconciliation_rejects_read_drift() {
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

    let holding_ref = format!("holding-pos-payment-reconcile-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let created_at = Utc::now();
    let sale_line = SaleLine::new(
        Uuid::new_v4(),
        "SKU-PAYMENT-RECONCILE",
        "Anonymized payment reconciliation item",
        1,
        900,
        90,
    )
    .expect("sale line");
    let mut sale = Sale::new(
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "THB",
        vec![sale_line],
        created_at,
        &command_context(holding_id, company_id),
    )
    .expect("sale");
    let source = SqlxPosPersistence::new(write_pool.clone(), &holding_ref).expect("source store");
    let created = source.save_sale(&sale, None).await.expect("create sale");

    let mut open_context = command_context_with_key(holding_id, company_id, "pos-payment-open");
    open_context.expected_aggregate_version = Some(created.version);
    sale.transition(SaleStatus::Open, &open_context, Utc::now())
        .expect("open sale");
    let opened = source
        .save_sale(&sale, Some(created.version))
        .await
        .expect("open sale source event");

    let mut payment_context =
        command_context_with_key(holding_id, company_id, "pos-payment-record");
    payment_context.expected_aggregate_version = Some(opened.version);
    sale.record_payment(
        Payment::new(Uuid::new_v4(), 990, PaymentMethod::Card, Utc::now()).expect("payment"),
        &payment_context,
        Utc::now(),
    )
    .expect("record payment");
    let paid = source
        .save_sale(&sale, Some(opened.version))
        .await
        .expect("payment source event");
    assert_eq!(paid.version, 3);

    let projection = SqlxPosProjectionStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("projection store");
    let page = projection
        .read_source_event_page(None, 10)
        .await
        .expect("source events");
    assert_eq!(page.events.len(), 3);
    for event in &page.events {
        assert!(
            projection
                .project_event(event)
                .await
                .expect("project source event")
                .applied
        );
    }
    assert!(
        projection
            .reconcile_projection()
            .await
            .expect("healthy payment reconciliation")
            .is_consistent()
    );

    let mut tamper = read_pool.begin().await.expect("payment tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper)
        .await
        .expect("read holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sale_payment_read_rows
         SET amount_minor=989
         WHERE holding_ref=$1 AND sale_id=$2",
    )
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *tamper)
    .await
    .expect("tamper payment amount");
    tamper.commit().await.expect("commit payment tamper");
    let drift = projection
        .reconcile_projection()
        .await
        .expect_err("payment amount drift must fail closed");
    assert!(drift.to_string().contains("paid total"));

    let mut repair = read_pool.begin().await.expect("payment repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *repair)
        .await
        .expect("read repair holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sale_payment_read_rows
         SET amount_minor=990
         WHERE holding_ref=$1 AND sale_id=$2",
    )
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *repair)
    .await
    .expect("repair payment amount");
    repair.commit().await.expect("commit payment repair");

    assert!(
        projection
            .reconcile_projection()
            .await
            .expect("repaired payment reconciliation")
            .is_consistent()
    );

    let forged_event_id = Uuid::new_v4();
    let mut identity_tamper = read_pool
        .begin()
        .await
        .expect("payment identity tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *identity_tamper)
        .await
        .expect("read identity holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sale_payment_read_rows
         SET source_event_id=$1
         WHERE holding_ref=$2 AND sale_id=$3",
    )
    .bind(forged_event_id)
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *identity_tamper)
    .await
    .expect("tamper payment source identity");
    identity_tamper
        .commit()
        .await
        .expect("commit payment identity tamper");
    let identity_drift = projection
        .reconcile_projection()
        .await
        .expect_err("payment source identity drift must fail closed");
    assert!(identity_drift.to_string().contains("source identity"));

    let mut identity_repair = read_pool
        .begin()
        .await
        .expect("payment identity repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *identity_repair)
        .await
        .expect("read identity repair holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.pos_sale_payment_read_rows
         SET source_event_id=$1
         WHERE holding_ref=$2 AND sale_id=$3",
    )
    .bind(page.events[2].event_id)
    .bind(&holding_ref)
    .bind(sale.id)
    .execute(&mut *identity_repair)
    .await
    .expect("repair payment source identity");
    identity_repair
        .commit()
        .await
        .expect("commit payment identity repair");
    assert!(
        projection
            .reconcile_projection()
            .await
            .expect("repaired payment identity reconciliation")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases"]
async fn live_pos_projection_rebuild_rolls_back_after_late_source_failure() {
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

    let holding_ref = format!("holding-pos-rebuild-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let mut sale = Sale::new(
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "THB",
        vec![
            SaleLine::new(
                Uuid::new_v4(),
                "SKU-POS-REBUILD",
                "Anonymized POS rebuild item",
                1,
                1_000,
                100,
            )
            .expect("sale line"),
        ],
        Utc::now(),
        &command_context(holding_id, company_id),
    )
    .expect("sale");
    let source = SqlxPosPersistence::new(write_pool.clone(), &holding_ref).expect("source store");
    let created = source.save_sale(&sale, None).await.expect("source write");
    let mut open_context = command_context_with_key(holding_id, company_id, "pos-rebuild-open");
    open_context.expected_aggregate_version = Some(created.version);
    sale.transition(SaleStatus::Open, &open_context, Utc::now())
        .expect("open sale");
    source
        .save_sale(&sale, Some(created.version))
        .await
        .expect("second source write");

    let projection = SqlxPosProjectionStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("projection store");
    let page = projection
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    assert_eq!(page.events.len(), 2);
    for event in &page.events {
        projection
            .project_event(event)
            .await
            .expect("initial projection");
    }

    let mut tamper = write_pool.begin().await.expect("source tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper)
        .await
        .expect("source holding scope");
    sqlx::query(
        "UPDATE erpx_tenant.outbox_events
         SET payload=$1
         WHERE holding_ref=$2 AND event_id=$3",
    )
    .bind(serde_json::json!({
        "aggregate_type": "pos_sale",
        "version": 2,
        "aggregate": {}
    }))
    .bind(&holding_ref)
    .bind(page.events[1].event_id)
    .execute(&mut *tamper)
    .await
    .expect("tamper source payload");
    tamper.commit().await.expect("commit source tamper");

    assert!(
        projection
            .rebuild_projection_with_source_fence(None, 10)
            .await
            .is_err(),
        "malformed source must abort the atomic rebuild"
    );
    let mut inspection = read_pool
        .begin()
        .await
        .expect("read inspection transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *inspection)
        .await
        .expect("read holding scope");
    let rows: (i64, i64) = sqlx::query_as(
        "SELECT count(*), COALESCE(max(source_version), 0)
         FROM erpx_tenant.pos_sale_read_rows
         WHERE holding_ref=$1 AND sale_id=$2",
    )
    .bind(&holding_ref)
    .bind(sale.id)
    .fetch_one(&mut *inspection)
    .await
    .expect("preserved read row count");
    assert_eq!(
        rows,
        (1, 2),
        "failed rebuild must preserve the prior latest read model"
    );

    drop(inspection);
    let mut repair = write_pool.begin().await.expect("source repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *repair)
        .await
        .expect("source repair scope");
    sqlx::query(
        "UPDATE erpx_tenant.outbox_events
         SET payload=$1
         WHERE holding_ref=$2 AND event_id=$3",
    )
    .bind(&page.events[1].payload)
    .bind(&holding_ref)
    .bind(page.events[1].event_id)
    .execute(&mut *repair)
    .await
    .expect("repair source payload");
    repair.commit().await.expect("commit source repair");

    projection
        .rebuild_projection_with_source_fence(None, 10)
        .await
        .expect("repaired source rebuild");
    assert!(
        projection
            .reconcile_projection()
            .await
            .expect("repaired rebuild reconciliation")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_pos_source_watermark_rejects_duplicate_aggregate_version() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant write migration");

    let holding_ref = format!("holding-pos-duplicate-version-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let sale = Sale::new(
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "THB",
        vec![
            SaleLine::new(
                Uuid::new_v4(),
                "SKU-POS-DUPLICATE",
                "Anonymized POS duplicate-version item",
                1,
                1_000,
                100,
            )
            .expect("sale line"),
        ],
        Utc::now(),
        &command_context(holding_id, company_id),
    )
    .expect("sale");
    SqlxPosPersistence::new(pool.clone(), &holding_ref)
        .expect("source store")
        .save_sale(&sale, None)
        .await
        .expect("source write");

    let projection =
        SqlxPosProjectionStore::from_pools(&pool, &pool, &holding_ref).expect("projection store");
    let source_page = projection
        .read_source_event_page(None, 10)
        .await
        .expect("source page");
    let source_event = &source_page.events[0];

    let mut tamper = pool.begin().await.expect("duplicate source transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper)
        .await
        .expect("duplicate source scope");
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,'pos_sale',$2,$3,$4,$5,$6,$7)",
    )
    .bind(Uuid::new_v4())
    .bind(source_event.aggregate_id)
    .bind(&holding_ref)
    .bind(i64::try_from(source_event.aggregate_version).expect("source version"))
    .bind(&source_event.event_type)
    .bind(&source_event.payload)
    .bind(source_event.occurred_at)
    .execute(&mut *tamper)
    .await
    .expect("duplicate source event");
    tamper.commit().await.expect("commit duplicate source");

    assert!(
        projection.source_event_high_watermark().await.is_err(),
        "duplicate aggregate versions must stop the watermark before any apply"
    );
    assert!(
        projection.reconcile_projection().await.is_err(),
        "reconciliation must fail closed on duplicate aggregate versions"
    );
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn live_pos_scope_is_immutable_across_source_versions() {
    let url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required");
    let pool = PgPool::connect(&url).await.expect("write pool");
    erpx_migration::migrate_tenant_write(&pool)
        .await
        .expect("tenant write migration");

    let holding_ref = format!("holding-pos-scope-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let sale = Sale::new(
        Uuid::new_v4(),
        TenantScope::Company {
            holding_id,
            company_id,
        },
        "THB",
        vec![
            SaleLine::new(
                Uuid::new_v4(),
                "SKU-SCOPE",
                "Anonymized POS scope item",
                1,
                500,
                50,
            )
            .expect("sale line"),
        ],
        Utc::now(),
        &command_context(holding_id, company_id),
    )
    .expect("sale");
    let store = SqlxPosPersistence::new(pool, &holding_ref).expect("POS store");
    store.save_sale(&sale, None).await.expect("source create");

    let forged_holding_id = Uuid::new_v4();
    let forged_company_id = Uuid::new_v4();
    let forged_scope = TenantScope::Company {
        holding_id: forged_holding_id,
        company_id: forged_company_id,
    };
    let mut forged = sale.clone();
    let mut context = command_context_with_key(holding_id, company_id, "pos-scope-move");
    context.expected_aggregate_version = Some(1);
    forged
        .transition(SaleStatus::Open, &context, Utc::now())
        .expect("next source version");
    forged.scope = forged_scope.clone();
    forged.holding_id = forged_holding_id;
    forged.company_id = forged_company_id;
    for command in &mut forged.command_history {
        command.event.tenant_scope = forged_scope.clone();
    }

    let error = store
        .save_sale(&forged, Some(1))
        .await
        .expect_err("source scope moves must fail closed");
    assert!(error.to_string().contains("scope is immutable"));
}
