//! Disposable end-to-end evidence for the Sales source/outbox/read boundary.
//!
//! The acceptance test uses two real holding PostgreSQL databases when the
//! routed URLs are configured.  Without them it records the certification
//! blocker and exits without claiming live evidence.

use chrono::Utc;
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_domain::sales::{
    CollectionState, CustomerMasterReference, HandoffStatus, InvoiceHandoff, Lead, Opportunity,
    Quote, QuoteStatus, SalesAggregateKind, SalesLine, SalesOrder, SalesOrderStatus,
    deterministic_idempotency_key, deterministic_reference,
};
use erpx_infrastructure::{SalesAggregate, SalesReadScope, SqlxSalesStore};
use sqlx::PgPool;
use uuid::Uuid;

fn context(scope: AuthorityScope, key: &str) -> CommandContext {
    CommandContext::try_new(
        key,
        ActorAttribution {
            principal_ref: "sales-e2e-actor".into(),
            kind: ActorKind::HoldingUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope,
        "sales.e2e.acceptance",
    )
    .expect("valid Sales command context")
}

#[tokio::test]
#[ignore = "requires disposable holding write and read PostgreSQL databases"]
async fn live_sales_write_outbox_projection_scoped_read_reconciliation_and_rebuild() {
    let Ok(write_url) = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL") else {
        eprintln!(
            "G11 Sales live SQLx blocker queued: ERPX_TEST_HOLDING_WRITE_DATABASE_URL is unavailable"
        );
        return;
    };
    let Ok(read_url) = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL") else {
        eprintln!(
            "G11 Sales live SQLx blocker queued: ERPX_TEST_HOLDING_READ_DATABASE_URL is unavailable"
        );
        return;
    };

    let write_pool = PgPool::connect(&write_url).await.expect("Sales write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("Sales read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-sales-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let lead = Lead::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        None,
        "Anonymized Sales lead",
        "acceptance",
        Utc::now(),
        &context(scope.clone(), "sales/lead-create"),
    )
    .expect("valid Sales lead");
    let lead_id = lead.id;
    let store = SqlxSalesStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("Sales store");
    let aggregate = SalesAggregate::Lead(lead);

    let first = store
        .save(&aggregate, None)
        .await
        .expect("Sales source write");
    assert_eq!(first.version, 1);
    assert!(!first.replayed);
    let replay = store
        .save(&aggregate, None)
        .await
        .expect("idempotent Sales source replay");
    assert!(replay.replayed);
    assert_eq!(replay.version, first.version);

    let second = store
        .transition(
            "lead",
            lead_id,
            "qualified",
            first.version,
            &context(scope.clone(), "sales/lead-qualify"),
        )
        .await
        .expect("Sales version-two lifecycle transition");
    assert_eq!(second.version, 2);
    assert!(!second.replayed);

    // The holding outbox is shared by bounded contexts.  A non-Sales row must
    // not make the Sales source reader fail or enter the Sales projector.
    let mut tx = write_pool.begin().await.expect("cross-context test tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("set holding scope");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1, 'pos_sale', $2, $3, 1, 'pos.sale.v1', '{}'::jsonb, now())",
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .bind(&data_profile_key)
    .execute(&mut *tx)
    .await
    .expect("shared outbox test row");
    tx.commit().await.expect("cross-context test commit");

    let page = store
        .read_source_event_page(None, 10)
        .await
        .expect("Sales source page");
    assert_eq!(page.events.len(), 2);
    assert!(
        page.events
            .iter()
            .all(|event| event.aggregate_id == lead_id)
    );
    for event in &page.events {
        assert_eq!(event.payload["aggregate_type"], event.aggregate_type);
        assert_eq!(
            event.payload["aggregate"]["aggregate_type"],
            event.aggregate_type
        );
        assert_eq!(
            event.payload["aggregate"]["aggregate"]["id"],
            event.aggregate_id.to_string()
        );
    }

    // The worker cursor shape is intentionally holding-agnostic for
    // compatibility, so the persistence adapter must bind it to the routed
    // source row before using it as a continuation position.
    let first_page = store
        .read_source_event_page(None, 1)
        .await
        .expect("Sales first source page");
    let cursor = first_page
        .next_cursor
        .as_ref()
        .expect("Sales continuation cursor")
        .clone();
    let other_holding_store = SqlxSalesStore::new(
        write_pool.clone(),
        read_pool.clone(),
        format!("holding-sales-other-{}", Uuid::new_v4()),
    )
    .expect("other Sales store");
    assert!(
        other_holding_store
            .read_source_event_page(Some(&cursor), 1)
            .await
            .is_err()
    );

    // A valid-looking same-version payload, or a random event ID, must not
    // be able to write the read model without matching the committed source
    // outbox row exactly.
    let mut forged_payload = page.events[0].payload.clone();
    forged_payload["aggregate"]["aggregate"]["reference"] =
        serde_json::Value::String("FORGED-REFERENCE".into());
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
            .is_err()
    );
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

    for event in &page.events {
        let application = store
            .project_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("Sales read projection");
        assert!(application.applied);
    }
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
            .expect("idempotent Sales projection replay")
            .applied
    );

    let row = store
        .get_for_scope("lead", lead_id, &scope)
        .await
        .expect("authorized Sales read")
        .expect("projected Sales lead");
    assert_eq!(row.1, 2);
    assert!(
        store
            .get_for_scope(
                "lead",
                lead_id,
                &AuthorityScope::Company {
                    holding_id,
                    company_id: Uuid::new_v4(),
                },
            )
            .await
            .is_err()
    );
    let list = store
        .list_for_scope(
            SalesReadScope {
                holding_id,
                company_id,
                branch_id: None,
            },
            10,
            None,
        )
        .await
        .expect("authorized Sales list read");
    assert_eq!(list.rows.len(), 1);
    assert!(!list.has_more);
    assert_eq!(list.rows[0].aggregate_id, lead_id);
    assert_eq!(list.rows[0].source_version, 2);
    assert!(
        store
            .list_for_scope(
                SalesReadScope {
                    holding_id,
                    company_id: Uuid::new_v4(),
                    branch_id: None,
                },
                10,
                None,
            )
            .await
            .expect("isolated Sales list read")
            .rows
            .is_empty()
    );

    let consistent = store
        .reconcile_projection()
        .await
        .expect("green Sales reconciliation");
    assert!(consistent.is_consistent());
    assert_eq!(consistent.source_event_count, 2);

    let latest_source_state_payload = page
        .events
        .last()
        .expect("latest Sales source event")
        .payload
        .get("aggregate")
        .and_then(|aggregate| aggregate.get("aggregate"))
        .cloned()
        .expect("latest Sales source state payload");
    let mut source_tamper = write_pool.begin().await.expect("source tamper transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source_tamper)
        .await
        .expect("source holding scope");
    sqlx::query(
        "UPDATE erpx_holding.sales_aggregates
         SET payload=$1
         WHERE data_profile_key=$2 AND aggregate_type=$3 AND aggregate_id=$4",
    )
    .bind(serde_json::json!({"aggregate_type": "lead", "id": lead_id}))
    .bind(&data_profile_key)
    .bind("lead")
    .bind(lead_id)
    .execute(&mut *source_tamper)
    .await
    .expect("tamper authoritative Sales source");
    source_tamper.commit().await.expect("commit source tamper");
    assert!(
        store.reconcile_projection().await.is_err(),
        "Sales reconciliation must fail closed on source snapshot drift"
    );
    assert!(
        store
            .project_event(
                page.events[1].event_id,
                &page.events[1].aggregate_type,
                page.events[1].aggregate_id,
                page.events[1].aggregate_version,
                &page.events[1].payload,
            )
            .await
            .is_err(),
        "Sales projector must not acknowledge an event against a drifted source snapshot"
    );

    let mut source_repair = write_pool.begin().await.expect("source repair transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *source_repair)
        .await
        .expect("source repair holding scope");
    sqlx::query(
        "UPDATE erpx_holding.sales_aggregates
         SET payload=$1
         WHERE data_profile_key=$2 AND aggregate_type=$3 AND aggregate_id=$4",
    )
    .bind(latest_source_state_payload)
    .bind(&data_profile_key)
    .bind("lead")
    .bind(lead_id)
    .execute(&mut *source_repair)
    .await
    .expect("repair authoritative Sales source");
    source_repair.commit().await.expect("commit source repair");
    assert!(store.reconcile_projection().await.unwrap().is_consistent());

    let cleared = store
        .clear_projection()
        .await
        .expect("clear Sales read model");
    assert_eq!(cleared.rows_cleared, 1);
    let missing = store
        .reconcile_projection()
        .await
        .expect("Sales missing projection report");
    assert_eq!(missing.missing_aggregate_count, 1);
    assert!(!missing.is_consistent());
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
            .expect("rollback-safe Sales replay");
    }
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("Sales reconciliation after rebuild")
            .is_consistent()
    );
}

#[tokio::test]
#[ignore = "requires disposable holding write and read PostgreSQL databases"]
async fn live_sales_projection_checkpoint_integrity_fails_closed_and_rebuilds() {
    let Ok(write_url) = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL") else {
        eprintln!("G11 Sales checkpoint blocker queued: write database is unavailable");
        return;
    };
    let Ok(read_url) = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL") else {
        eprintln!("G11 Sales checkpoint blocker queued: read database is unavailable");
        return;
    };

    let write_pool = PgPool::connect(&write_url).await.expect("Sales write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("Sales read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-sales-checkpoint-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let lead = Lead::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        None,
        "Checkpoint integrity lead",
        "checkpoint",
        Utc::now(),
        &context(scope.clone(), "sales/checkpoint-integrity"),
    )
    .expect("valid Sales lead");
    let lead_id = lead.id;
    let store = SqlxSalesStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("Sales store");
    let aggregate = SalesAggregate::Lead(lead);
    store.save(&aggregate, None).await.expect("source write");
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
        .expect("initial projection");

    // A same-version event that is present in the authoritative outbox but
    // does not match the installed checkpoint must not be accepted as a
    // harmless replay.
    let duplicate_event_id = Uuid::new_v4();
    let mut duplicate_tx = write_pool.begin().await.expect("duplicate event tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *duplicate_tx)
        .await
        .expect("route duplicate event tx");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
    )
    .bind(duplicate_event_id)
    .bind(&source_event.aggregate_type)
    .bind(source_event.aggregate_id)
    .bind(&data_profile_key)
    .bind(source_event.aggregate_version as i64)
    .bind(&source_event.event_type)
    .bind(&source_event.payload)
    .execute(&mut *duplicate_tx)
    .await
    .expect("duplicate authoritative event");
    duplicate_tx.commit().await.expect("commit duplicate event");
    assert!(
        store
            .project_event(
                duplicate_event_id,
                &source_event.aggregate_type,
                source_event.aggregate_id,
                source_event.aggregate_version,
                &source_event.payload,
            )
            .await
            .is_err()
    );

    let mut duplicate_cleanup_tx = write_pool.begin().await.expect("duplicate cleanup tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *duplicate_cleanup_tx)
        .await
        .expect("route duplicate cleanup tx");
    sqlx::query("DELETE FROM erpx_holding.outbox_events WHERE event_id=$1")
        .bind(duplicate_event_id)
        .execute(&mut *duplicate_cleanup_tx)
        .await
        .expect("remove rejected duplicate source event");
    duplicate_cleanup_tx
        .commit()
        .await
        .expect("commit duplicate cleanup");

    let mut payload_damage_tx = read_pool.begin().await.expect("payload damage tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *payload_damage_tx)
        .await
        .expect("route payload damage tx");
    sqlx::query(
        "UPDATE erpx_holding.sales_aggregates
         SET payload=jsonb_set(payload, '{aggregate,reference}', to_jsonb('FORGED-REFERENCE'::text), false)
         WHERE aggregate_type=$1 AND aggregate_id=$2 AND data_profile_key=$3",
    )
    .bind(&source_event.aggregate_type)
    .bind(source_event.aggregate_id)
    .bind(&data_profile_key)
    .execute(&mut *payload_damage_tx)
    .await
    .expect("tamper projected payload");
    payload_damage_tx
        .commit()
        .await
        .expect("commit payload damage");
    let damaged = store
        .reconcile_projection()
        .await
        .expect("Sales payload-drift reconciliation");
    assert_eq!(damaged.invalid_projection_count, 1);
    assert!(!damaged.is_consistent());
    assert!(
        store
            .project_event(
                source_event.event_id,
                &source_event.aggregate_type,
                source_event.aggregate_id,
                source_event.aggregate_version,
                &source_event.payload,
            )
            .await
            .is_err()
    );

    // Missing checkpoints are not a readable state and are not repaired by a
    // projector replay. The only recovery path is a controlled replacement
    // projection rebuild from the authoritative source.
    let mut damage_tx = read_pool.begin().await.expect("checkpoint damage tx");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *damage_tx)
        .await
        .expect("route checkpoint damage tx");
    sqlx::query(
        "DELETE FROM erpx_holding.sales_projection_checkpoints
         WHERE aggregate_type=$1 AND aggregate_id=$2 AND data_profile_key=$3",
    )
    .bind(&source_event.aggregate_type)
    .bind(source_event.aggregate_id)
    .bind(&data_profile_key)
    .execute(&mut *damage_tx)
    .await
    .expect("delete checkpoint");
    damage_tx.commit().await.expect("commit checkpoint damage");
    assert!(store.get("lead", lead_id).await.is_err());
    assert!(
        store
            .project_event(
                source_event.event_id,
                &source_event.aggregate_type,
                source_event.aggregate_id,
                source_event.aggregate_version,
                &source_event.payload,
            )
            .await
            .is_err()
    );

    let preparation = store.clear_projection().await.expect("clear projection");
    assert_eq!(preparation.rows_cleared, 1);
    assert_eq!(preparation.checkpoints_cleared, 0);
    store
        .project_event(
            source_event.event_id,
            &source_event.aggregate_type,
            source_event.aggregate_id,
            source_event.aggregate_version,
            &source_event.payload,
        )
        .await
        .expect("rebuild projection");
    let rebuilt = store
        .get_for_scope("lead", lead_id, &scope)
        .await
        .expect("rebuilt authorized read")
        .expect("rebuilt lead");
    assert_eq!(rebuilt.1, 1);
}

#[tokio::test]
#[ignore = "requires disposable holding write and read PostgreSQL databases"]
async fn live_sales_source_fence_rebuild_is_atomic_and_preserves_read_state_on_failure() {
    let Ok(write_url) = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL") else {
        eprintln!("G9 Sales source-fence blocker queued: write database is unavailable");
        return;
    };
    let Ok(read_url) = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL") else {
        eprintln!("G9 Sales source-fence blocker queued: read database is unavailable");
        return;
    };

    let write_pool = PgPool::connect(&write_url).await.expect("Sales write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("Sales read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-sales-fence-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let lead = Lead::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        None,
        "Fenced Sales lead",
        "fence",
        Utc::now(),
        &context(scope.clone(), "sales/fence-create"),
    )
    .expect("valid Sales lead");
    let lead_id = lead.id;
    let store = SqlxSalesStore::new(
        write_pool.clone(),
        read_pool.clone(),
        data_profile_key.clone(),
    )
    .expect("Sales store");

    let first = store
        .save(&SalesAggregate::Lead(lead), None)
        .await
        .expect("source version one");
    let first_event = store
        .read_source_event_page(None, 10)
        .await
        .expect("version-one source page")
        .events
        .into_iter()
        .next()
        .expect("version-one source event");
    store
        .project_event(
            first_event.event_id,
            &first_event.aggregate_type,
            first_event.aggregate_id,
            first_event.aggregate_version,
            &first_event.payload,
        )
        .await
        .expect("initial Sales projection");
    assert_eq!(store.source_event_high_watermark().await.unwrap(), 1);

    let second = store
        .transition(
            "lead",
            lead_id,
            "qualified",
            first.version,
            &context(scope.clone(), "sales/fence-qualify"),
        )
        .await
        .expect("source version two");
    assert_eq!(second.version, 2);
    assert_eq!(store.source_event_high_watermark().await.unwrap(), 2);

    // A stale source fence must fail before the disposable read model is
    // cleared, so an operator cannot accidentally replace a fresh projection
    // from a moving source stream.
    let fence_error = store
        .rebuild_projection_with_source_fence(Some(1), 1)
        .await
        .expect_err("stale source fence must fail closed");
    assert!(fence_error.to_string().contains("source fence"));
    assert_eq!(store.get("lead", lead_id).await.unwrap().unwrap().1, 1);

    let (preparation, rebuilt_source_count) = store
        .rebuild_projection_with_source_fence(Some(2), 1)
        .await
        .expect("fenced Sales rebuild");
    assert_eq!(preparation.rows_cleared, 1);
    assert_eq!(rebuilt_source_count, 2);
    assert_eq!(store.get("lead", lead_id).await.unwrap().unwrap().1, 2);
    assert!(store.reconcile_projection().await.unwrap().is_consistent());

    // A duplicate version is an invalid authoritative stream. The rebuild
    // must roll back its read transaction, retaining the last good version.
    let version_two_event = store
        .read_source_event_page(None, 10)
        .await
        .expect("source page after version two")
        .events
        .into_iter()
        .find(|event| event.aggregate_version == 2)
        .expect("version-two source event");
    let duplicate_event_id = Uuid::new_v4();
    let mut tx = write_pool
        .begin()
        .await
        .expect("duplicate source transaction");
    sqlx::query("SELECT set_config('erpx.data_profile_key', $1, true)")
        .bind(&data_profile_key)
        .execute(&mut *tx)
        .await
        .expect("route duplicate source transaction");
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
    )
    .bind(duplicate_event_id)
    .bind(&version_two_event.aggregate_type)
    .bind(version_two_event.aggregate_id)
    .bind(&data_profile_key)
    .bind(version_two_event.aggregate_version as i64)
    .bind(&version_two_event.event_type)
    .bind(&version_two_event.payload)
    .execute(&mut *tx)
    .await
    .expect("duplicate authoritative source event");
    tx.commit().await.expect("commit duplicate source event");
    assert_eq!(store.source_event_high_watermark().await.unwrap(), 3);

    let rollback_error = store
        .rebuild_projection_with_source_fence(Some(3), 1)
        .await
        .expect_err("duplicate source version must roll back rebuild");
    assert!(
        rollback_error
            .to_string()
            .contains("duplicate aggregate version")
    );
    let retained = store
        .get("lead", lead_id)
        .await
        .expect("retained read state")
        .expect("retained Sales lead");
    assert_eq!(retained.1, 2);
}

#[tokio::test]
#[ignore = "requires disposable holding write and read PostgreSQL databases"]
async fn live_sales_collection_payment_is_idempotent_projected_and_rebuildable() {
    let Ok(write_url) = std::env::var("ERPX_TEST_HOLDING_WRITE_DATABASE_URL") else {
        eprintln!(
            "G9 Sales payment blocker queued: ERPX_TEST_HOLDING_WRITE_DATABASE_URL is unavailable"
        );
        return;
    };
    let Ok(read_url) = std::env::var("ERPX_TEST_HOLDING_READ_DATABASE_URL") else {
        eprintln!(
            "G9 Sales payment blocker queued: ERPX_TEST_HOLDING_READ_DATABASE_URL is unavailable"
        );
        return;
    };

    let write_pool = PgPool::connect(&write_url).await.expect("Sales write pool");
    let read_pool = PgPool::connect(&read_url).await.expect("Sales read pool");
    erpx_migration::migrate_holding_write(&write_pool)
        .await
        .expect("holding write migration");
    erpx_migration::migrate_holding_read(&read_pool)
        .await
        .expect("holding read migration");

    let data_profile_key = format!("holding-sales-payment-{}", Uuid::new_v4());
    let holding_id = Uuid::new_v4();
    let company_id = Uuid::new_v4();
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let customer =
        CustomerMasterReference::new(Uuid::new_v4(), scope.clone(), holding_id, company_id, None)
            .expect("valid customer reference");
    let opportunity_context = context(scope.clone(), "sales/opportunity-handoff");
    let opportunity = Opportunity::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        None,
        customer.clone(),
        "Payment acceptance opportunity",
        "USD",
        1_000,
        Utc::now(),
        &opportunity_context,
    )
    .expect("valid opportunity");
    let quote_context = context(scope.clone(), "sales/quote-handoff");
    let mut quote = Quote::new(
        Uuid::new_v4(),
        scope.clone(),
        holding_id,
        company_id,
        None,
        Some(opportunity.id),
        customer,
        "USD",
        vec![SalesLine {
            line_id: Uuid::new_v4(),
            item_reference: "SKU-payment-test".into(),
            description: "Payment acceptance line".into(),
            quantity: 1,
            unit_price_minor: 1_000,
            tax_minor: 0,
        }],
        Utc::now() + chrono::Duration::hours(1),
        Utc::now(),
        &quote_context,
    )
    .expect("valid quote");
    quote
        .transition(
            QuoteStatus::Sent,
            &context(scope.clone(), "sales/quote-sent"),
            Utc::now(),
        )
        .expect("send quote");
    quote
        .transition(
            QuoteStatus::Accepted,
            &context(scope.clone(), "sales/quote-accepted"),
            Utc::now(),
        )
        .expect("accept quote");
    let order_context = context(scope.clone(), "sales/order-handoff");
    let mut order = SalesOrder::new_from_quote(Uuid::new_v4(), &quote, &order_context, Utc::now())
        .expect("valid sales order");
    order
        .transition(
            SalesOrderStatus::Confirmed,
            &context(scope.clone(), "sales/order-confirmed"),
            Utc::now(),
        )
        .expect("confirm sales order");
    let order_id = order.id;
    let invoice_context = context(scope.clone(), "sales/invoice-handoff");
    let invoice_id = Uuid::new_v4();
    let invoice = InvoiceHandoff {
        id: invoice_id,
        reference: deterministic_reference(SalesAggregateKind::InvoiceHandoff, invoice_id),
        idempotency_key: deterministic_idempotency_key(
            SalesAggregateKind::InvoiceHandoff,
            &invoice_context,
        )
        .expect("invoice idempotency key"),
        command_id: invoice_context.command_id,
        scope: scope.clone(),
        holding_id,
        company_id,
        branch_id: None,
        sales_order_id: order_id,
        sales_order_reference: order.reference.clone(),
        delivery_handoff_id: None,
        currency: "USD".into(),
        amount_due_minor: 1_000,
        status: HandoffStatus::HandedOff,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    invoice.validate().expect("valid invoice handoff");
    let collection_context = context(scope.clone(), "sales/collection-create");
    let collection = CollectionState::new_from_invoice(
        Uuid::new_v4(),
        &invoice,
        &collection_context,
        Utc::now(),
    )
    .expect("collection state");
    let collection_id = collection.id;
    let store = SqlxSalesStore::new(write_pool.clone(), read_pool.clone(), data_profile_key)
        .expect("Sales store");
    store
        .save(&SalesAggregate::Opportunity(opportunity), None)
        .await
        .expect("opportunity source write");
    store
        .save(&SalesAggregate::Quote(quote), None)
        .await
        .expect("quote source write");
    store
        .save(&SalesAggregate::SalesOrder(order), None)
        .await
        .expect("sales order source write");
    store
        .save(&SalesAggregate::InvoiceHandoff(invoice), None)
        .await
        .expect("invoice source write");
    let created = store
        .save(&SalesAggregate::CollectionState(collection), None)
        .await
        .expect("collection source write");
    assert_eq!(created.version, 1);

    let mut payment_context = context(scope.clone(), "sales/collection-payment");
    payment_context.expected_aggregate_version = Some(created.version);
    let paid = store
        .record_payment(collection_id, 400, created.version, &payment_context)
        .await
        .expect("partial payment source write");
    assert_eq!(paid.version, 2);
    assert!(!paid.replayed);

    let mut cursor = None;
    let mut projected_events = 0_u64;
    loop {
        let page = store
            .read_source_event_page(cursor.as_ref(), 1)
            .await
            .expect("payment source page");
        let event = page.events.first().expect("source event");
        store
            .project_event(
                event.event_id,
                &event.aggregate_type,
                event.aggregate_id,
                event.aggregate_version,
                &event.payload,
            )
            .await
            .expect("project Sales source event");
        projected_events += 1;
        if !page.has_more {
            break;
        }
        cursor = page.next_cursor;
    }
    assert_eq!(projected_events, 6);

    let replay = store
        .record_payment(collection_id, 400, created.version, &payment_context)
        .await
        .expect("replay payment");
    assert!(replay.replayed);
    assert_eq!(replay.version, paid.version);
    assert!(
        store
            .record_payment(collection_id, 401, created.version, &payment_context)
            .await
            .is_err()
    );
    let projected = store
        .get_for_scope("collection_state", collection_id, &scope)
        .await
        .expect("read projected collection")
        .expect("projected collection");
    let SalesAggregate::CollectionState(projected) = projected.0 else {
        panic!("expected projected collection state");
    };
    assert_eq!(projected.amount_collected_minor, 400);
    let reconciliation = store.reconcile_projection().await.expect("reconcile");
    assert!(reconciliation.is_consistent(), "{reconciliation:?}");

    let (_, rebuilt_events) = store
        .rebuild_projection_with_source_fence(Some(6), 1)
        .await
        .expect("rebuild payment projection");
    assert_eq!(rebuilt_events, 6);
    let rebuilt = store
        .get_for_scope("collection_state", collection_id, &scope)
        .await
        .expect("read rebuilt collection")
        .expect("rebuilt collection");
    let SalesAggregate::CollectionState(rebuilt) = rebuilt.0 else {
        panic!("expected rebuilt collection state");
    };
    assert_eq!(rebuilt.amount_collected_minor, 400);
    assert!(
        store
            .reconcile_projection()
            .await
            .expect("reconcile rebuild")
            .is_consistent()
    );
}
