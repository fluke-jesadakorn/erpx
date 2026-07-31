use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, Utc};
use erpx_domain::{
    access::{RiskTier, RoleAssignment, Scope},
    contracts::{ActorAttribution, ActorKind, TenantScope},
    operations::completion::{
        OperationsAuthorizationProof, OperationsDetectionPolicy, OperationsDetectionRequest,
        OperationsExpiredPrincipalReassignment, OperationsManagerChainProof, OperationsPermission,
    },
    operations::{
        AggregateMetadata, ExceptionCase, ExceptionLifecycleAction, ExceptionLifecycleCommand,
        ExceptionPrincipalReference, ExceptionSlaMetadata, ExceptionStatus, Holding,
        MAX_EXCEPTION_IDEMPOTENCY_KEY_BYTES, OperationalPlan, OperationsAggregate, OperationsEvent,
        OperationsSourceDomain, OperationsSourceEvent, OrganizationStatus, ProjectionEventMetadata,
        SourceTransactionRef, WorkQueuePolicy, event_for,
    },
};
use erpx_infrastructure::{
    FolioOperationsBaseline, OPERATIONS_DETECTION_CURSOR_PROJECTION, OPERATIONS_READ_PROJECTION,
    OperationsEventRecord, OperationsFolioReconciliation, OperationsReconciliationReport,
    OperationsSourceOutboxCursor, PersistenceError, SqlxOperationsStore,
};
use erpx_migration::{FolioSourceSnapshot, migrate_tenant_read, migrate_tenant_write};
use serde_json::json;
use sqlx::{Row, postgres::PgPoolOptions};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

fn holding_with_ref(
    version: u64,
    holding_id: Uuid,
    at: chrono::DateTime<Utc>,
    holding_ref: &str,
) -> Holding {
    Holding {
        meta: AggregateMetadata {
            id: holding_id,
            holding_ref: holding_ref.into(),
            scope: Scope::Holding { holding_id },
            version,
            created_at: at,
            updated_at: at,
        },
        code: "OPS".into(),
        name: "Operations Test Holding".into(),
        status: OrganizationStatus::Active,
    }
}

fn holding(version: u64, holding_id: Uuid, at: chrono::DateTime<Utc>) -> Holding {
    holding_with_ref(version, holding_id, at, "holding-operations-test")
}

fn actor() -> ActorAttribution {
    ActorAttribution {
        principal_ref: "operations-test-worker".into(),
        kind: ActorKind::SystemWorker,
        session_id: None,
        represented_principal_ref: None,
        support_capability_id: None,
    }
}

fn event_for_holding(aggregate: &Holding, at: chrono::DateTime<Utc>) -> OperationsEvent {
    event_for(
        aggregate,
        "organization.holding.created",
        1,
        actor(),
        Some(format!(
            "operations-test-{}-{}",
            aggregate.meta.id, aggregate.meta.version
        )),
        Uuid::new_v4(),
        None,
        at,
    )
    .expect("valid Operations event")
}

fn exception_source_with_ids(source_id: Uuid, source_event_id: Uuid) -> SourceTransactionRef {
    SourceTransactionRef::new("sales", "sales_order", source_id, source_event_id)
        .expect("valid exception source")
}

fn exception_source() -> SourceTransactionRef {
    exception_source_with_ids(Uuid::from_u128(0x7301), Uuid::from_u128(0x7302))
}

fn operations_plan_with_fixture(
    at: chrono::DateTime<Utc>,
    id: Uuid,
    holding_ref: &str,
    holding_id: Uuid,
    source: SourceTransactionRef,
) -> OperationalPlan {
    OperationalPlan::new(
        AggregateMetadata {
            id,
            holding_ref: holding_ref.into(),
            scope: Scope::Holding { holding_id },
            version: 1,
            created_at: at,
            updated_at: at,
        },
        "contract test plan",
        at,
        at + Duration::hours(1),
        Vec::new(),
        vec![source],
    )
    .expect("valid Operations plan")
}

fn operations_plan(at: chrono::DateTime<Utc>, id: Uuid) -> OperationalPlan {
    operations_plan_with_fixture(
        at,
        id,
        "holding-operations-contract-test",
        Uuid::from_u128(0x7400),
        exception_source(),
    )
}

fn exception_case_with_fixture(
    at: chrono::DateTime<Utc>,
    id: Uuid,
    holding_ref: &str,
    holding_id: Uuid,
    source: SourceTransactionRef,
) -> ExceptionCase {
    ExceptionCase {
        meta: AggregateMetadata {
            id,
            holding_ref: holding_ref.into(),
            scope: Scope::Holding { holding_id },
            version: 0,
            created_at: at,
            updated_at: at,
        },
        category: "late_delivery".into(),
        severity: "high".into(),
        source,
        assigned_to: None,
        due_at: None,
        manager_ref: None,
        sla: None,
        status: ExceptionStatus::Detected,
    }
}

fn fixture_ref(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

async fn hold_projection_maintenance_lock<'a>(
    pool: &'a sqlx::PgPool,
    holding_ref: &str,
) -> sqlx::Transaction<'a, sqlx::Postgres> {
    let mut tx = pool.begin().await.expect("lock-holder transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await
        .expect("scope lock-holder transaction");
    sqlx::query(
        "SELECT pg_advisory_xact_lock(
                    hashtextextended($1 || ':' || $2, 0)
                )",
    )
    .bind(holding_ref)
    .bind(OPERATIONS_READ_PROJECTION)
    .execute(&mut *tx)
    .await
    .expect("hold projection maintenance lock");
    tx
}

fn principal(
    principal_ref: &str,
    valid_from: chrono::DateTime<Utc>,
    valid_until: chrono::DateTime<Utc>,
) -> ExceptionPrincipalReference {
    ExceptionPrincipalReference {
        principal_ref: principal_ref.into(),
        valid_from,
        valid_until,
    }
}

fn lifecycle_command(
    exception_id: Uuid,
    expected_version: u64,
    at: chrono::DateTime<Utc>,
    idempotency_key: &str,
    action: ExceptionLifecycleAction,
) -> ExceptionLifecycleCommand {
    ExceptionLifecycleCommand {
        exception_id,
        expected_version,
        at,
        actor: principal(
            "operations-operator",
            at - Duration::minutes(1),
            at + Duration::hours(2),
        ),
        actor_attribution: ActorAttribution {
            principal_ref: "operations-operator".into(),
            kind: ActorKind::TenantUser,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        idempotency_key: idempotency_key.into(),
        correlation_id: Uuid::new_v4(),
        causation_id: None,
        action,
    }
}

#[tokio::test]
async fn repository_constructor_rejects_unsafe_holding_reference() {
    let result = SqlxOperationsStore::new(
        sqlx::PgPool::connect_lazy("postgres://unused").expect("lazy pool"),
        sqlx::PgPool::connect_lazy("postgres://unused-read").expect("lazy read pool"),
        "holding/credential-looking-ref",
    );
    assert!(matches!(result, Err(PersistenceError::InvalidData(_))));
}

#[tokio::test]
async fn policy_publish_rejects_unknown_and_duplicate_domains_before_database_access() {
    let pool = sqlx::PgPool::connect_lazy("postgres://unused").expect("lazy tenant pool");
    let store = SqlxOperationsStore::from_pools(&pool, &pool, "holding-operations-policy-test")
        .expect("tenant Operations store");
    let scope = Scope::Holding {
        holding_id: Uuid::from_u128(0x7101),
    };

    for domains in [
        vec!["unknown".to_string()],
        vec!["sales".to_string(), "sales".to_string()],
        Vec::new(),
    ] {
        let result = store
            .publish_work_queue_policy(
                Uuid::from_u128(0x7102),
                scope.clone(),
                "queue policy",
                domains,
                60,
                0,
                actor(),
                "operations-policy-invalid-input".into(),
                Uuid::from_u128(0x7103),
                None,
                Utc::now(),
            )
            .await;
        assert!(matches!(result, Err(PersistenceError::InvalidData(_))));
    }
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn operations_work_queue_policy_publish_is_evented_projected_and_idempotent() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let holding_ref = fixture_ref("holding-operations-policy");
    let holding_id = Uuid::new_v4();
    let policy_id = Uuid::new_v4();
    let store = SqlxOperationsStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("tenant Operations store");
    let at = Utc::now();
    let first = store
        .publish_work_queue_policy(
            policy_id,
            Scope::Holding { holding_id },
            "queue policy",
            vec!["sales".into(), "procurement".into()],
            45,
            0,
            actor(),
            "operations-policy-create".into(),
            Uuid::new_v4(),
            None,
            at,
        )
        .await
        .expect("publish policy");
    assert!(first.applied);
    assert_eq!(first.status, "active");
    assert_eq!(first.aggregate_version, 1);
    let read = store
        .get_read_aggregate::<WorkQueuePolicy>(policy_id)
        .await
        .expect("read projected policy")
        .expect("policy read row");
    assert_eq!(read.value.meta.version, 1);
    assert_eq!(read.value.name, "queue policy");
    assert_eq!(
        read.value.allowed_domains,
        vec!["procurement".to_string(), "sales".to_string()]
    );

    let replay = store
        .publish_work_queue_policy(
            policy_id,
            Scope::Holding { holding_id },
            "queue policy",
            vec!["sales".into(), "procurement".into()],
            45,
            0,
            actor(),
            "operations-policy-create".into(),
            Uuid::new_v4(),
            None,
            at,
        )
        .await
        .expect("replay policy publish");
    assert!(!replay.applied);
    assert_eq!(replay.event_id, first.event_id);
    assert_eq!(replay.aggregate_version, 1);

    let updated = store
        .publish_work_queue_policy(
            policy_id,
            Scope::Holding { holding_id },
            "queue policy v2",
            vec!["maintenance".into()],
            90,
            1,
            actor(),
            "operations-policy-update".into(),
            Uuid::new_v4(),
            Some(first.event_id),
            at + Duration::minutes(1),
        )
        .await
        .expect("update policy publish");
    assert!(updated.applied);
    assert_eq!(updated.aggregate_version, 2);
    assert_ne!(updated.event_id, first.event_id);
    let updated_read = store
        .get_read_aggregate::<WorkQueuePolicy>(policy_id)
        .await
        .expect("read updated policy")
        .expect("updated policy read row");
    assert_eq!(updated_read.value.meta.version, 2);
    assert_eq!(updated_read.value.name, "queue policy v2");
    assert_eq!(
        updated_read.value.allowed_domains,
        vec!["maintenance".to_string()]
    );
    assert_eq!(
        store
            .read_json_page("operations.work_queue_policy", None, 10)
            .await
            .expect("list policy read model")
            .rows
            .len(),
        1
    );
    assert_eq!(
        store
            .read_summary()
            .await
            .expect("policy summary")
            .event_count,
        2
    );
}

#[tokio::test]
#[ignore = "requires two disposable tenant PostgreSQL databases; URLs are never printed"]
async fn operations_persistence_write_read_boundary_is_versioned_and_idempotent() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let holding_ref = fixture_ref("holding-operations-test");
    let store = SqlxOperationsStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("tenant Operations store");
    // Keep the two aggregate IDs ordered so the page-size-two assertion still
    // exercises both versions of the first aggregate, while the namespace is
    // unique for every test invocation.
    let fixture_namespace = Uuid::new_v4().as_u128() & !0xffff;
    let holding_id = Uuid::from_u128(fixture_namespace | 1);
    let first_at = Utc::now() - Duration::minutes(2);
    let first = holding_with_ref(1, holding_id, first_at, &holding_ref);
    let first_event = event_for_holding(&first, first_at);
    let applied = store
        .apply_event(&first_event, &first, None, &[])
        .await
        .expect("apply first Operations event");
    assert!(applied.applied);
    assert_eq!(applied.aggregate_version, 1);

    let replay = store
        .apply_event(&first_event, &first, Some(0), &[])
        .await
        .expect("replay first Operations event");
    assert!(!replay.applied);

    let projected = store
        .project_event(&first_event, &first)
        .await
        .expect("project first Operations event");
    assert!(projected.applied);
    let projected_replay = store
        .project_event(&first_event, &first)
        .await
        .expect("replay read projection");
    assert!(!projected_replay.applied);
    let read = store
        .get_read_aggregate::<Holding>(holding_id)
        .await
        .expect("read Operations row")
        .expect("Operations read row");
    assert_eq!(read.value.meta.version, 1);
    assert_eq!(read.projection_name, OPERATIONS_READ_PROJECTION);
    let summary = store.read_summary().await.expect("read Operations summary");
    assert_eq!(summary.read_row_count, 1);
    assert_eq!(summary.exception_count, 0);
    assert_eq!(summary.projection_checkpoint_count, 1);
    assert_eq!(summary.event_count, 1);
    assert_eq!(summary.source_reference_count, 0);
    assert!(summary.source_domain_counts.is_empty());
    assert_eq!(summary.latest_event_at, Some(first_at));

    let mut second = first.clone();
    second
        .transition_to(
            OrganizationStatus::Suspended,
            first_at + Duration::minutes(1),
        )
        .expect("valid status transition");
    second.meta.version = 2;
    let second_event = event_for_holding(&second, second.meta.updated_at);
    let stale = store
        .apply_event(&second_event, &second, Some(0), &[])
        .await;
    assert!(matches!(
        stale,
        Err(PersistenceError::OptimisticConcurrency { .. })
    ));
    store
        .apply_event(&second_event, &second, Some(1), &[])
        .await
        .expect("apply second Operations event");
    store
        .project_event(&second_event, &second)
        .await
        .expect("project second Operations event");

    let other_holding_id = Uuid::from_u128(fixture_namespace | 2);
    let other_holding = holding_with_ref(
        1,
        other_holding_id,
        first_at + Duration::minutes(2),
        &holding_ref,
    );
    let other_event = event_for_holding(&other_holding, other_holding.meta.created_at);
    store
        .apply_event(&other_event, &other_holding, None, &[])
        .await
        .expect("apply other aggregate Operations event");
    store
        .project_event(&other_event, &other_holding)
        .await
        .expect("project other aggregate Operations event");

    let reconciliation_at = Utc::now();
    let authorization = OperationsAuthorizationProof {
        principal_ref: "operations-reconciliation-test".into(),
        permission: OperationsPermission::Reconcile,
        scope: Scope::Holding { holding_id },
        valid_from: reconciliation_at - Duration::minutes(1),
        valid_until: reconciliation_at + Duration::minutes(1),
        mfa_verified: true,
        break_glass: None,
    };
    let healthy = store
        .reconciliation_report(
            &authorization,
            &Scope::Holding { holding_id },
            reconciliation_at,
        )
        .await
        .expect("healthy Operations reconciliation");
    assert!(healthy.healthy);
    assert_eq!(healthy.missing_outbox_events, 0);
    assert_eq!(healthy.outbox_mismatch_events, 0);

    // An Operations event with a matching outbox envelope but no authoritative
    // snapshot must not be hidden by count-based reconciliation. The event
    // stream and snapshot are both write-side authority; a missing snapshot is
    // projection drift even when the read model still looks complete.
    let orphan_operations_event_id = Uuid::new_v4();
    let orphan_operations_aggregate_id = Uuid::new_v4();
    let orphan_operations_correlation_id = Uuid::new_v4();
    let orphan_operations_at = reconciliation_at + Duration::seconds(1);
    let orphan_operations_payload = serde_json::json!({ "orphan": true });
    let orphan_operations_scope = serde_json::to_value(TenantScope::Holding { holding_id })
        .expect("serialize orphan Operations scope");
    let orphan_operations_actor = serde_json::to_value(actor()).expect("serialize orphan actor");
    let mut orphan_operations_tx = write_pool
        .begin()
        .await
        .expect("orphan Operations event transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *orphan_operations_tx)
        .await
        .expect("scope orphan Operations event transaction");
    sqlx::query(
        "INSERT INTO erpx_tenant.operations_events
            (event_id, contract_version, event_type, event_version, occurred_at,
             aggregate_type, aggregate_id, aggregate_version, holding_ref,
             tenant_scope, actor, correlation_id, causation_id, idempotency_key,
             payload)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NULL, $13, $14)",
    )
    .bind(orphan_operations_event_id)
    .bind(1_i32)
    .bind("organization.holding.created")
    .bind(1_i32)
    .bind(orphan_operations_at)
    .bind(Holding::AGGREGATE_TYPE)
    .bind(orphan_operations_aggregate_id)
    .bind(1_i64)
    .bind(&holding_ref)
    .bind(&orphan_operations_scope)
    .bind(&orphan_operations_actor)
    .bind(orphan_operations_correlation_id)
    .bind("orphan-operations-event")
    .bind(&orphan_operations_payload)
    .execute(&mut *orphan_operations_tx)
    .await
    .expect("insert orphan Operations event");
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                 $9, $10, $11, $12, $13, NULL, $14)",
    )
    .bind(orphan_operations_event_id)
    .bind(Holding::AGGREGATE_TYPE)
    .bind(orphan_operations_aggregate_id)
    .bind(&holding_ref)
    .bind(1_i64)
    .bind("organization.holding.created")
    .bind(&orphan_operations_payload)
    .bind(orphan_operations_at)
    .bind(1_i32)
    .bind(1_i32)
    .bind(&orphan_operations_scope)
    .bind(&orphan_operations_actor)
    .bind(orphan_operations_correlation_id)
    .bind("orphan-operations-event")
    .execute(&mut *orphan_operations_tx)
    .await
    .expect("insert orphan Operations outbox envelope");
    orphan_operations_tx
        .commit()
        .await
        .expect("commit orphan Operations event");
    let missing_snapshot = store
        .reconciliation_report(
            &authorization,
            &Scope::Holding { holding_id },
            orphan_operations_at + Duration::seconds(1),
        )
        .await
        .expect("reconciliation detects missing Operations snapshot");
    assert!(missing_snapshot.stale_aggregate_count >= 1);
    assert!(!missing_snapshot.healthy);
    let mut remove_orphan_operations_tx = write_pool
        .begin()
        .await
        .expect("remove orphan Operations event transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *remove_orphan_operations_tx)
        .await
        .expect("scope remove orphan Operations event transaction");
    sqlx::query("DELETE FROM erpx_tenant.outbox_events WHERE event_id = $1")
        .bind(orphan_operations_event_id)
        .execute(&mut *remove_orphan_operations_tx)
        .await
        .expect("remove orphan Operations outbox envelope");
    sqlx::query("DELETE FROM erpx_tenant.operations_events WHERE event_id = $1")
        .bind(orphan_operations_event_id)
        .execute(&mut *remove_orphan_operations_tx)
        .await
        .expect("remove orphan Operations event");
    remove_orphan_operations_tx
        .commit()
        .await
        .expect("commit orphan Operations event removal");

    // An outbox row without an authoritative Operations event must not be
    // certified as healthy. The tenant outbox is transport state, not a
    // second source of truth, even when the row satisfies its base schema.
    let orphan_event_id = Uuid::new_v4();
    let mut orphan_outbox_tx = write_pool.begin().await.expect("orphan outbox transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *orphan_outbox_tx)
        .await
        .expect("scope orphan outbox transaction");
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(orphan_event_id)
    .bind(Holding::AGGREGATE_TYPE)
    .bind(Uuid::new_v4())
    .bind(&holding_ref)
    .bind(1_i64)
    .bind("operations.orphaned")
    .bind(serde_json::json!({"orphan": true}))
    .bind(reconciliation_at)
    .execute(&mut *orphan_outbox_tx)
    .await
    .expect("insert orphan outbox row");
    orphan_outbox_tx
        .commit()
        .await
        .expect("commit orphan outbox row");
    let orphaned = store
        .reconciliation_report(
            &authorization,
            &Scope::Holding { holding_id },
            reconciliation_at,
        )
        .await
        .expect("reconciliation detects orphan outbox row");
    assert_eq!(orphaned.missing_outbox_events, 0);
    assert_eq!(orphaned.outbox_mismatch_events, 1);
    assert!(!orphaned.healthy);
    let mut remove_orphan_tx = write_pool.begin().await.expect("remove orphan transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *remove_orphan_tx)
        .await
        .expect("scope remove orphan transaction");
    sqlx::query("DELETE FROM erpx_tenant.outbox_events WHERE event_id = $1")
        .bind(orphan_event_id)
        .execute(&mut *remove_orphan_tx)
        .await
        .expect("remove orphan outbox row");
    remove_orphan_tx
        .commit()
        .await
        .expect("commit orphan removal");

    // A matching event_id is not enough: immutable envelope tampering that the
    // legacy replay check did not inspect must be rejected and reconciled as
    // unhealthy. Correlation is immutable event identity metadata; it is not
    // delivery state and cannot be repaired in place.
    let mut tamper_tx = write_pool.begin().await.expect("tamper transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper_tx)
        .await
        .expect("scope tamper transaction");
    sqlx::query(
        "UPDATE erpx_tenant.outbox_events
         SET correlation_id = '00000000-0000-0000-0000-000000000001'
         WHERE event_id = $1",
    )
    .bind(first_event.envelope.event_id)
    .execute(&mut *tamper_tx)
    .await
    .expect("tamper Operations outbox correlation");
    tamper_tx.commit().await.expect("commit outbox tamper");

    let replay_after_tamper = store.apply_event(&first_event, &first, Some(0), &[]).await;
    assert!(matches!(
        replay_after_tamper,
        Err(PersistenceError::InvalidData(message))
            if message.contains("outbox event ID replay did not match")
    ));
    let corrupted = store
        .reconciliation_report(
            &authorization,
            &Scope::Holding { holding_id },
            reconciliation_at,
        )
        .await
        .expect("reconciliation detects outbox tamper");
    assert_eq!(corrupted.missing_outbox_events, 0);
    assert_eq!(corrupted.outbox_mismatch_events, 1);
    assert!(!corrupted.healthy);

    // Equal row/checkpoint counts are not sufficient: replace a legitimate
    // read row with a same-shaped row under a different aggregate key. The
    // authoritative write snapshot, read row, and checkpoint key sets must
    // agree or reconciliation must fail closed.
    let mut repair_outbox_tx = write_pool.begin().await.expect("repair transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *repair_outbox_tx)
        .await
        .expect("scope repair transaction");
    sqlx::query(
        "UPDATE erpx_tenant.outbox_events
         SET correlation_id = $1
         WHERE event_id = $2",
    )
    .bind(first_event.envelope.correlation_id)
    .bind(first_event.envelope.event_id)
    .execute(&mut *repair_outbox_tx)
    .await
    .expect("restore test outbox envelope");
    repair_outbox_tx
        .commit()
        .await
        .expect("commit restored test outbox envelope");

    let mut rogue_projection_tx = read_pool
        .begin()
        .await
        .expect("rogue projection transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *rogue_projection_tx)
        .await
        .expect("scope rogue projection transaction");
    let original_row = sqlx::query(
        "SELECT state, lifecycle_status, updated_at
         FROM erpx_tenant.operations_read_rows
         WHERE projection_name = $1
           AND aggregate_type = $2
           AND aggregate_id = $3
           AND holding_ref = $4",
    )
    .bind(OPERATIONS_READ_PROJECTION)
    .bind(Holding::AGGREGATE_TYPE)
    .bind(holding_id)
    .bind(&holding_ref)
    .fetch_one(&mut *rogue_projection_tx)
    .await
    .expect("read original projection row");
    sqlx::query(
        "DELETE FROM erpx_tenant.operations_read_rows
         WHERE projection_name = $1
           AND aggregate_type = $2
           AND aggregate_id = $3
           AND holding_ref = $4",
    )
    .bind(OPERATIONS_READ_PROJECTION)
    .bind(Holding::AGGREGATE_TYPE)
    .bind(holding_id)
    .bind(&holding_ref)
    .execute(&mut *rogue_projection_tx)
    .await
    .expect("delete original projection row");
    let rogue_aggregate_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO erpx_tenant.operations_read_rows
            (projection_name, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, lifecycle_status, state, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(OPERATIONS_READ_PROJECTION)
    .bind(Holding::AGGREGATE_TYPE)
    .bind(rogue_aggregate_id)
    .bind(&holding_ref)
    .bind(1_i64)
    .bind(
        original_row
            .try_get::<String, _>("lifecycle_status")
            .unwrap(),
    )
    .bind(
        original_row
            .try_get::<serde_json::Value, _>("state")
            .unwrap(),
    )
    .bind(
        original_row
            .try_get::<chrono::DateTime<Utc>, _>("updated_at")
            .unwrap(),
    )
    .execute(&mut *rogue_projection_tx)
    .await
    .expect("insert rogue projection row");
    rogue_projection_tx
        .commit()
        .await
        .expect("commit rogue projection row");

    let key_set_corruption = store
        .reconciliation_report(
            &authorization,
            &Scope::Holding { holding_id },
            reconciliation_at,
        )
        .await
        .expect("reconciliation detects projection key drift");
    assert_eq!(key_set_corruption.outbox_mismatch_events, 0);
    assert_eq!(key_set_corruption.stale_aggregate_count, 2);
    assert!(!key_set_corruption.healthy);

    let first_page = store
        .read_event_page(None, 2)
        .await
        .expect("read first ordered event page");
    assert_eq!(first_page.events.len(), 2);
    assert!(first_page.has_more);
    assert_eq!(
        first_page
            .events
            .iter()
            .map(|event| (
                event.envelope.aggregate_id,
                event.envelope.aggregate_version
            ))
            .collect::<Vec<_>>(),
        vec![(holding_id, 1), (holding_id, 2)]
    );
    let second_page = store
        .read_event_page(first_page.next_cursor, 10_000)
        .await
        .expect("read second ordered event page");
    assert_eq!(second_page.events.len(), 1);
    assert!(!second_page.has_more);
    assert_eq!(
        second_page.events[0].envelope.aggregate_id,
        other_holding_id
    );
    assert_eq!(second_page.events[0].envelope.aggregate_version, 1);
    assert!(matches!(
        store.read_event_page(None, 0).await,
        Err(PersistenceError::InvalidData(_))
    ));

    let alternate_projection = format!("operations_rebuild_fixture-{}", Uuid::new_v4());
    let mut alternate_first_event = first_event.clone();
    alternate_first_event.projection.projection_name = alternate_projection.clone();
    store
        .project_event(&alternate_first_event, &first)
        .await
        .expect("project first alternate Operations projection event");
    let mut alternate_event = second_event.clone();
    alternate_event.projection.projection_name = alternate_projection.clone();
    store
        .project_event(&alternate_event, &second)
        .await
        .expect("project alternate Operations projection");
    let checkpoint_before_rebuild = store
        .get_projection_checkpoint(
            OPERATIONS_READ_PROJECTION,
            Holding::AGGREGATE_TYPE,
            holding_id,
        )
        .await
        .expect("read projection checkpoint before rebuild")
        .expect("projection checkpoint before rebuild");
    assert_eq!(checkpoint_before_rebuild.last_aggregate_version, 2);
    let preparation = store
        .prepare_projection_rebuild(OPERATIONS_READ_PROJECTION)
        .await
        .expect("prepare Operations projection rebuild");
    assert_eq!(preparation.projection_name, OPERATIONS_READ_PROJECTION);
    assert_eq!(preparation.holding_ref, holding_ref);
    assert_eq!(preparation.read_rows_cleared, 2);
    assert_eq!(preparation.checkpoints_cleared, 2);
    assert!(
        store
            .get_read_aggregate::<Holding>(holding_id)
            .await
            .expect("read cleared Operations row")
            .is_none()
    );
    assert!(
        store
            .get_read_aggregate_for_projection::<Holding>(&alternate_projection, holding_id)
            .await
            .expect("read alternate Operations projection")
            .is_some()
    );
    assert!(
        store
            .get_projection_checkpoint(
                OPERATIONS_READ_PROJECTION,
                Holding::AGGREGATE_TYPE,
                holding_id,
            )
            .await
            .expect("read cleared projection checkpoint")
            .is_none()
    );
}

#[tokio::test]
async fn aggregate_provenance_requires_explicit_references_and_authoritative_events() {
    let pool = sqlx::PgPool::connect_lazy("postgres://unused").expect("lazy tenant pool");
    let store = SqlxOperationsStore::from_pools(&pool, &pool, "holding-operations-contract-test")
        .expect("tenant Operations store");
    let plan = operations_plan(Utc::now(), Uuid::from_u128(0x7401));
    let result = store
        .save_aggregate_with_source_refs(&plan, None, &[])
        .await;
    assert!(matches!(
        result,
        Err(PersistenceError::InvalidData(message))
            if message.contains("exactly match the aggregate provenance")
    ));

    let result = store
        .save_aggregate_with_source_refs(&plan, None, plan.source_references())
        .await;
    assert!(matches!(
        result,
        Err(PersistenceError::InvalidData(message))
            if message.contains("snapshot-only Operations persistence is disabled")
    ));
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL database"]
async fn source_referenced_operations_aggregate_round_trips_and_replays() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");

    let holding_ref = fixture_ref("holding-operations-contract-test");
    let holding_id = Uuid::new_v4();
    let source = exception_source_with_ids(Uuid::new_v4(), Uuid::new_v4());
    let store = SqlxOperationsStore::from_pools(&write_pool, &write_pool, &holding_ref)
        .expect("tenant Operations store");
    let at = Utc::now() - Duration::minutes(1);
    let plan =
        operations_plan_with_fixture(at, Uuid::new_v4(), &holding_ref, holding_id, source.clone());
    let event = event_for(
        &plan,
        "operations.operational_plan.created",
        1,
        actor(),
        Some(format!("operations-contract-replay-{}", Uuid::new_v4())),
        Uuid::new_v4(),
        None,
        at,
    )
    .expect("valid Operations plan event");
    let applied = store
        .apply_event(&event, &plan, None, plan.source_references())
        .await
        .expect("apply source-referenced Operations plan");
    assert!(applied.applied);
    assert_eq!(applied.aggregate_version, 1);

    let replay = store
        .apply_event(&event, &plan, Some(0), plan.source_references())
        .await
        .expect("replay source-referenced Operations plan");
    assert!(!replay.applied);
    let restored = store
        .get_aggregate::<OperationalPlan>(plan.meta.id)
        .await
        .expect("read restored Operations plan")
        .expect("restored Operations plan");
    assert_eq!(restored.meta.version, 1);
    assert_eq!(restored.source_references(), plan.source_references());
    assert_eq!(
        store
            .get_source_references(OperationalPlan::AGGREGATE_TYPE, plan.meta.id)
            .await
            .expect("read immutable source references"),
        plan.source_references()
    );
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL read database"]
async fn source_detection_cursor_is_durable_monotonic_and_rebuild_safe() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant source migrations");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_read(&pool)
        .await
        .expect("tenant read migrations");

    let holding_ref = fixture_ref("holding-operations-detection-cursor-test");
    let holding_id = Uuid::new_v4();
    let tenant_scope = serde_json::to_value(TenantScope::Holding { holding_id })
        .expect("serialize source tenant scope");
    let actor_value = serde_json::to_value(actor()).expect("serialize source actor");
    let store = SqlxOperationsStore::from_pools(&write_pool, &pool, &holding_ref)
        .expect("tenant Operations store");
    let first =
        OperationsSourceOutboxCursor::new(Utc::now() - Duration::minutes(2), Uuid::new_v4())
            .expect("first source cursor");
    let second =
        OperationsSourceOutboxCursor::new(first.occurred_at + Duration::minutes(1), Uuid::new_v4())
            .expect("second source cursor");
    for (cursor, source_id) in [(first, Uuid::new_v4()), (second, Uuid::new_v4())] {
        sqlx::query(
            "INSERT INTO erpx_tenant.outbox_events
                (event_id, aggregate_type, aggregate_id, holding_ref,
                 aggregate_version, event_type, payload, occurred_at,
                 contract_version, event_version, tenant_scope, actor,
                 correlation_id, causation_id, idempotency_key)
             VALUES ($1, 'sales_order', $2, $3, 1, 'sales.order.created.v1',
                     $4, $5, 1, 1, $6, $7, $8, NULL, $9)",
        )
        .bind(cursor.source_event_id)
        .bind(source_id)
        .bind(&holding_ref)
        .bind(json!({"fixture": "operations-source-cursor"}))
        .bind(cursor.occurred_at)
        .bind(&tenant_scope)
        .bind(&actor_value)
        .bind(Uuid::new_v4())
        .bind(format!(
            "operations-source-cursor:{}",
            cursor.source_event_id
        ))
        .execute(&write_pool)
        .await
        .expect("seed authoritative source event");
    }

    assert_eq!(
        store.source_detection_cursor().await.expect("empty cursor"),
        None
    );
    store
        .save_source_detection_cursor(first)
        .await
        .expect("save first source cursor");
    assert_eq!(
        store
            .source_detection_cursor()
            .await
            .expect("read first source cursor"),
        Some(first)
    );
    store
        .save_source_detection_cursor(second)
        .await
        .expect("save second source cursor");
    assert_eq!(
        store
            .source_detection_cursor()
            .await
            .expect("read second source cursor"),
        Some(second)
    );
    assert!(matches!(
        store.save_source_detection_cursor(first).await,
        Err(PersistenceError::InvalidData(message))
            if message.contains("cannot move backwards")
    ));
    assert_eq!(
        store
            .source_detection_cursor()
            .await
            .expect("cursor remains monotonic"),
        Some(second)
    );

    let preparation = store
        .prepare_projection_rebuild(OPERATIONS_READ_PROJECTION)
        .await
        .expect("prepare Operations read rebuild");
    assert_eq!(preparation.checkpoints_cleared, 1);
    assert_eq!(
        store
            .source_detection_cursor()
            .await
            .expect("cursor cleared"),
        None
    );
    assert!(
        !store
            .clear_source_detection_cursor()
            .await
            .expect("clear absent cursor")
    );
    assert_eq!(
        OPERATIONS_DETECTION_CURSOR_PROJECTION,
        "operations_detection_cursor"
    );
    write_pool.close().await;
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn source_outbox_page_excludes_operations_owned_events() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    migrate_tenant_write(&pool)
        .await
        .expect("tenant write migrations");
    let holding_ref = fixture_ref("holding-operations-source-feed");
    let holding_id = Uuid::new_v4();
    let scope = serde_json::to_value(TenantScope::Holding { holding_id })
        .expect("serialize source feed scope");
    let actor_value = serde_json::to_value(actor()).expect("serialize source feed actor");
    let source_event_id = Uuid::new_v4();
    let source_correlation_id = Uuid::new_v4();
    let source_at = Utc::now() - Duration::minutes(1);
    let organization_event_id = Uuid::new_v4();
    let organization_at = source_at - Duration::seconds(1);
    let mut tx = pool.begin().await.expect("source feed transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("scope source feed transaction");
    for (event_id, aggregate_type, event_type, occurred_at, correlation_id, payload) in [
        (
            organization_event_id,
            Holding::AGGREGATE_TYPE,
            "organization.holding.created",
            organization_at,
            Uuid::new_v4(),
            serde_json::json!({"operations_owned": true}),
        ),
        (
            source_event_id,
            "sales.order",
            "sales.order.delivery_breached",
            source_at,
            source_correlation_id,
            serde_json::json!({"source_owned": true}),
        ),
    ] {
        sqlx::query(
            "INSERT INTO erpx_tenant.outbox_events
                (event_id, aggregate_type, aggregate_id, holding_ref,
                 aggregate_version, event_type, payload, occurred_at,
                 contract_version, event_version, tenant_scope, actor,
                 correlation_id, causation_id, idempotency_key)
             VALUES ($1, $2, $3, $4, 1, $5, $6, $7, 1, 1, $8, $9, $10, NULL, $11)",
        )
        .bind(event_id)
        .bind(aggregate_type)
        .bind(Uuid::new_v4())
        .bind(&holding_ref)
        .bind(event_type)
        .bind(payload)
        .bind(occurred_at)
        .bind(&scope)
        .bind(&actor_value)
        .bind(correlation_id)
        .bind(format!("source-feed-{event_id}"))
        .execute(&mut *tx)
        .await
        .expect("insert source feed envelope");
    }
    tx.commit().await.expect("commit source feed envelopes");

    let store = SqlxOperationsStore::from_pools(&pool, &pool, &holding_ref)
        .expect("tenant Operations store");
    let page = store
        .read_source_outbox_event_page(None, 10)
        .await
        .expect("read source-owned outbox page");
    let high_watermark = store
        .source_outbox_high_watermark()
        .await
        .expect("read source-owned outbox high-watermark")
        .expect("source-owned outbox high-watermark");
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].source_event.source_event_id, source_event_id);
    assert_eq!(high_watermark.source_event_id, source_event_id);
    assert_eq!(high_watermark.occurred_at, source_at);
    assert_eq!(
        page.events[0].source_event.source_domain,
        OperationsSourceDomain::Sales
    );
    assert!(!page.has_more);
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL write database"]
async fn reconciliation_rejects_a_delivery_handoff_for_a_pending_notification() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_write(&pool)
        .await
        .expect("tenant write migrations");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let holding_ref = fixture_ref("holding-operations-pending-handoff");
    let holding_id = Uuid::new_v4();
    let notification_key = format!("operations-sla:pending:{}", Uuid::new_v4());
    let exception_id = Uuid::new_v4();
    let at = Utc::now();
    let payload = serde_json::json!({"severity": "high"});
    let tenant_scope =
        serde_json::to_value(TenantScope::Platform).expect("serialize notification tenant scope");
    let actor_value = serde_json::to_value(actor()).expect("serialize notification actor");
    let event_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("operations-sla-notification-delivered:{notification_key}").as_bytes(),
    );
    let correlation_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, notification_key.as_bytes());
    let handoff_payload = serde_json::json!({
        "notification_key": notification_key,
        "exception_id": exception_id,
        "exception_version": 1,
        "recipient_ref": "principal:manager",
        "kind": "sla_escalated",
    });
    let mut tx = pool.begin().await.expect("pending handoff transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tx)
        .await
        .expect("scope pending handoff transaction");
    sqlx::query(
        "INSERT INTO erpx_tenant.operations_sla_notifications
            (notification_key, holding_ref, exception_id, exception_version,
             recipient_ref, kind, not_before, expires_at, status, payload)
         VALUES ($1, $2, $3, 1, $4, 'sla_escalated', $5, $6, 'pending', $7)",
    )
    .bind(&notification_key)
    .bind(&holding_ref)
    .bind(exception_id)
    .bind("principal:manager")
    .bind(at)
    .bind(at + Duration::hours(1))
    .bind(&payload)
    .execute(&mut *tx)
    .await
    .expect("insert pending notification");
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1, 'operations_notification', $2, $3, 1,
                 'operations.sla_notification_delivered.v1', $4, $5,
                 1, 1, $6, $7, $8, NULL, $9)",
    )
    .bind(event_id)
    .bind(exception_id)
    .bind(&holding_ref)
    .bind(&handoff_payload)
    .bind(at)
    .bind(&tenant_scope)
    .bind(&actor_value)
    .bind(correlation_id)
    .bind(&notification_key)
    .execute(&mut *tx)
    .await
    .expect("insert forged pending handoff");
    tx.commit().await.expect("commit pending handoff");

    let store = SqlxOperationsStore::from_pools(&pool, &read_pool, &holding_ref)
        .expect("tenant Operations store");
    let authorization = OperationsAuthorizationProof {
        principal_ref: "operations-reconciliation-test".into(),
        permission: OperationsPermission::Reconcile,
        scope: Scope::Holding { holding_id },
        valid_from: at - Duration::minutes(1),
        valid_until: at + Duration::minutes(1),
        mfa_verified: true,
        break_glass: None,
    };
    let report = store
        .reconciliation_report(&authorization, &Scope::Holding { holding_id }, at)
        .await
        .expect("reconcile pending handoff");
    assert!(!report.healthy);
    assert_eq!(report.outbox_mismatch_events, 1);
    read_pool.close().await;
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL read database"]
async fn projection_rebuild_and_projector_share_a_rollback_boundary_lock() {
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_read(&pool)
        .await
        .expect("tenant read migrations");

    let holding_ref = fixture_ref("holding-operations-rebuild-lock-test");
    let store = SqlxOperationsStore::from_pools(&pool, &pool, &holding_ref)
        .expect("tenant Operations store");

    let holder = hold_projection_maintenance_lock(&pool, &holding_ref).await;
    let rebuild_store = store.clone();
    let mut rebuild = tokio::spawn(async move {
        rebuild_store
            .prepare_projection_rebuild(OPERATIONS_READ_PROJECTION)
            .await
    });
    tokio::select! {
        result = &mut rebuild => panic!("rebuild bypassed the projection lock: {result:?}"),
        _ = sleep(std::time::Duration::from_millis(100)) => {}
    }
    holder.commit().await.expect("release rebuild lock");
    let preparation = timeout(std::time::Duration::from_secs(2), rebuild)
        .await
        .expect("rebuild released after projection lock")
        .expect("rebuild task")
        .expect("prepare projection rebuild");
    assert_eq!(preparation.projection_name, OPERATIONS_READ_PROJECTION);

    let at = Utc::now();
    let aggregate = holding_with_ref(1, Uuid::new_v4(), at, &holding_ref);
    let event = event_for_holding(&aggregate, at);
    let holder = hold_projection_maintenance_lock(&pool, &holding_ref).await;
    let projector_store = store.clone();
    let aggregate_for_projector = aggregate.clone();
    let event_for_projector = event.clone();
    let mut projector = tokio::spawn(async move {
        projector_store
            .project_event(&event_for_projector, &aggregate_for_projector)
            .await
    });
    tokio::select! {
        result = &mut projector => panic!("projector bypassed the projection lock: {result:?}"),
        _ = sleep(std::time::Duration::from_millis(100)) => {}
    }
    holder.commit().await.expect("release projector lock");
    let projected = timeout(std::time::Duration::from_secs(2), projector)
        .await
        .expect("projector released after projection lock")
        .expect("projector task")
        .expect("project Operations event");
    assert!(projected.applied);
    assert_eq!(projected.aggregate_version, 1);
    assert!(
        store
            .get_read_aggregate::<Holding>(aggregate.meta.id)
            .await
            .expect("read projected aggregate")
            .is_some()
    );

    pool.close().await;
}

#[test]
fn event_record_rejects_a_holding_mismatch() {
    let now = Utc::now();
    let aggregate = holding(1, Uuid::from_u128(0x7003), now);
    let event = event_for_holding(&aggregate, now);
    let record = OperationsEventRecord {
        envelope: event.envelope,
        holding_ref: "different-holding".into(),
        applied_at: now,
    };
    assert!(matches!(
        record.validate_for_holding("holding-operations-test"),
        Err(PersistenceError::InvalidData(_))
    ));
}

#[tokio::test]
#[ignore = "requires one disposable tenant PostgreSQL database"]
async fn exception_lifecycle_is_atomic_versioned_idempotent_and_tenant_bound() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let write_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");

    let holding_ref = fixture_ref("holding-operations-lifecycle-test");
    let other_holding_ref = format!("{holding_ref}-other");
    let holding_id = Uuid::new_v4();
    let source = exception_source_with_ids(Uuid::new_v4(), Uuid::new_v4());
    let store = SqlxOperationsStore::from_pools(&write_pool, &write_pool, &holding_ref)
        .expect("tenant Operations store");
    let other_store = SqlxOperationsStore::from_pools(&write_pool, &write_pool, &other_holding_ref)
        .expect("other tenant Operations store");
    let created_at = Utc::now() - Duration::minutes(10);
    let exception_id = Uuid::new_v4();
    let exception = exception_case_with_fixture(
        created_at,
        exception_id,
        &holding_ref,
        holding_id,
        source.clone(),
    );
    let mut exception = exception;
    exception.meta.version = 1;
    let detected_event = event_for(
        &exception,
        "operations.exception_case.detected",
        1,
        actor(),
        Some(format!("operations-exception-detected-{}", exception_id)),
        Uuid::new_v4(),
        None,
        created_at,
    )
    .expect("valid detected exception event");
    assert_eq!(
        store
            .apply_event(&detected_event, &exception, None, &[source])
            .await
            .expect("persist detected exception event")
            .aggregate_version,
        1
    );

    let assign_at = created_at + Duration::minutes(1);
    let assignee = principal(
        "operations-assignee",
        created_at,
        created_at + Duration::hours(1),
    );
    let manager = principal(
        "operations-manager",
        created_at,
        created_at + Duration::hours(1),
    );
    let assign_command = lifecycle_command(
        exception_id,
        1,
        assign_at,
        &format!("exception-lifecycle-assign-{}", Uuid::new_v4()),
        ExceptionLifecycleAction::Assign {
            assignee: assignee.clone(),
            manager: manager.clone(),
            sla: ExceptionSlaMetadata {
                policy_ref: "high-severity".into(),
                due_at: assign_at + Duration::hours(1),
                escalation_after_minutes: 60,
            },
        },
    );
    let assigned = store
        .apply_exception_lifecycle_command(&assign_command)
        .await
        .expect("assign exception atomically");
    assert!(assigned.applied);
    assert_eq!(assigned.from, ExceptionStatus::Detected);
    assert_eq!(assigned.to, ExceptionStatus::Assigned);
    assert_eq!(assigned.aggregate_version, 2);

    let retry = store
        .apply_exception_lifecycle_command(&assign_command)
        .await
        .expect("same lifecycle command is an idempotent replay");
    assert!(!retry.applied);
    assert_eq!(retry.event_id, assigned.event_id);
    assert_eq!(retry.aggregate_version, assigned.aggregate_version);

    let stale = lifecycle_command(
        exception_id,
        1,
        assign_at + Duration::minutes(1),
        &format!("exception-lifecycle-stale-{}", Uuid::new_v4()),
        ExceptionLifecycleAction::Acknowledge,
    );
    assert!(matches!(
        store.apply_exception_lifecycle_command(&stale).await,
        Err(PersistenceError::OptimisticConcurrency { .. })
    ));

    let expired_assignee = principal("expired-assignee", created_at, assign_at);
    let expired_reassignment = lifecycle_command(
        exception_id,
        2,
        assign_at + Duration::minutes(1),
        &format!("exception-lifecycle-expired-reassign-{}", Uuid::new_v4()),
        ExceptionLifecycleAction::Reassign {
            assignee: expired_assignee,
            manager: manager.clone(),
        },
    );
    assert!(matches!(
        store
            .apply_exception_lifecycle_command(&expired_reassignment)
            .await,
        Err(PersistenceError::InvalidData(_))
    ));

    let mut oversized = assign_command.clone();
    oversized.idempotency_key = "x".repeat(MAX_EXCEPTION_IDEMPOTENCY_KEY_BYTES + 1);
    assert!(matches!(
        store.apply_exception_lifecycle_command(&oversized).await,
        Err(PersistenceError::InvalidData(_))
    ));

    let acknowledge = lifecycle_command(
        exception_id,
        2,
        assign_at + Duration::minutes(2),
        &format!("exception-lifecycle-ack-{}", Uuid::new_v4()),
        ExceptionLifecycleAction::Acknowledge,
    );
    store
        .apply_exception_lifecycle_command(&acknowledge)
        .await
        .expect("acknowledge assigned exception");
    let escalate = lifecycle_command(
        exception_id,
        3,
        assign_at + Duration::minutes(3),
        &format!("exception-lifecycle-escalate-{}", Uuid::new_v4()),
        ExceptionLifecycleAction::Escalate {
            manager: manager.clone(),
        },
    );
    store
        .apply_exception_lifecycle_command(&escalate)
        .await
        .expect("escalate acknowledged exception");
    let resolve = lifecycle_command(
        exception_id,
        4,
        assign_at + Duration::minutes(4),
        &format!("exception-lifecycle-resolve-{}", Uuid::new_v4()),
        ExceptionLifecycleAction::Resolve,
    );
    let resolved = store
        .apply_exception_lifecycle_command(&resolve)
        .await
        .expect("resolve escalated exception");
    assert_eq!(resolved.to, ExceptionStatus::Resolved);
    assert_eq!(resolved.aggregate_version, 5);

    let close = lifecycle_command(
        exception_id,
        5,
        assign_at + Duration::minutes(5),
        &format!("exception-lifecycle-close-{}", Uuid::new_v4()),
        ExceptionLifecycleAction::Close,
    );
    let closed = store
        .apply_exception_lifecycle_command(&close)
        .await
        .expect("close resolved exception");
    assert_eq!(closed.from, ExceptionStatus::Resolved);
    assert_eq!(closed.to, ExceptionStatus::Closed);
    assert_eq!(closed.aggregate_version, 6);

    let cross_scope = other_store
        .apply_exception_lifecycle_command(&resolve)
        .await;
    assert!(matches!(
        cross_scope,
        Err(PersistenceError::NotFound {
            entity: "exception case",
            ..
        }) | Err(PersistenceError::InvalidData(_))
    ));
    let mut count_tx = write_pool.begin().await.expect("event count transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *count_tx)
        .await
        .expect("scope event count transaction");
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_tenant.operations_events
         WHERE aggregate_type = $1 AND aggregate_id = $2",
    )
    .bind(ExceptionCase::AGGREGATE_TYPE)
    .bind(exception_id)
    .fetch_one(&mut *count_tx)
    .await
    .expect("count exception lifecycle events");
    count_tx
        .commit()
        .await
        .expect("commit event count transaction");
    assert_eq!(event_count, 6);
}

#[tokio::test]
#[ignore = "requires disposable tenant write and read PostgreSQL databases"]
async fn scheduled_detection_writes_source_event_outbox_and_read_model() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    let write_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let holding_ref = fixture_ref("holding-operations-detection-test");
    let holding_id = Uuid::new_v4();
    let detection_at = Utc::now();
    let source_event_id = Uuid::new_v4();
    let source_event = OperationsSourceEvent {
        source_domain: OperationsSourceDomain::Sales,
        source_type: "sales.order".into(),
        source_id: Uuid::new_v4(),
        source_event_id,
        source_version: 1,
        event_type: "order.delivery_breached".into(),
        event_version: 1,
        holding_ref: holding_ref.clone(),
        scope: Scope::Holding { holding_id },
        actor: actor(),
        correlation_id: Uuid::new_v4(),
        causation_id: None,
        occurred_at: detection_at - Duration::minutes(1),
        payload: serde_json::json!({"order_ref": "sales-order-detection"}),
    };
    let assignment_scope = Scope::Holding { holding_id };
    let manager_assignment_id = Uuid::new_v4();
    let manager = RoleAssignment {
        id: manager_assignment_id,
        principal_ref: "operations-manager".into(),
        role_key: "operations_manager".into(),
        permissions: vec![OperationsPermission::ExceptionManage.as_str().into()],
        functional_domain: "operations".into(),
        authority_level: 2,
        approval_limit_minor: 0,
        scope: assignment_scope.clone(),
        valid_from: detection_at - Duration::minutes(5),
        valid_until: detection_at + Duration::hours(1),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "Operations detection test".into(),
        ticket_reference: None,
        grantor: "operations-root".into(),
        approver: None,
        renewal_reviewer: None,
        parent_assignment_id: None,
        assignment_version: 1,
    };
    let assignee = RoleAssignment {
        id: Uuid::new_v4(),
        principal_ref: "operations-assignee".into(),
        role_key: "operations_assignee".into(),
        permissions: vec![OperationsPermission::ExceptionManage.as_str().into()],
        functional_domain: "operations".into(),
        authority_level: 1,
        approval_limit_minor: 0,
        scope: assignment_scope.clone(),
        valid_from: detection_at - Duration::minutes(5),
        valid_until: detection_at + Duration::minutes(1),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "Operations detection test".into(),
        ticket_reference: None,
        grantor: "operations-manager".into(),
        approver: None,
        renewal_reviewer: None,
        parent_assignment_id: Some(manager_assignment_id),
        assignment_version: 1,
    };
    let request = OperationsDetectionRequest {
        source_event,
        exception_id: Uuid::new_v4(),
        policy: OperationsDetectionPolicy {
            policy_ref: "sales-delivery-breach".into(),
            source_domains: BTreeSet::from([OperationsSourceDomain::Sales]),
            category: "delivery_breach".into(),
            severity: "high".into(),
            due_after_minutes: 30,
            escalation_after_minutes: 60,
        },
        authorization: OperationsAuthorizationProof {
            principal_ref: "operations-scheduler".into(),
            permission: OperationsPermission::ExceptionManage,
            scope: assignment_scope.clone(),
            valid_from: detection_at - Duration::minutes(5),
            valid_until: detection_at + Duration::minutes(5),
            mfa_verified: true,
            break_glass: None,
        },
        actor_attribution: ActorAttribution {
            principal_ref: "operations-scheduler".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        assignee: principal(
            "operations-assignee",
            detection_at - Duration::minutes(5),
            detection_at + Duration::minutes(1),
        ),
        manager: principal(
            "operations-manager",
            detection_at - Duration::minutes(5),
            detection_at + Duration::hours(1),
        ),
        manager_chain: OperationsManagerChainProof {
            subject_principal_ref: "operations-assignee".into(),
            manager_principal_ref: "operations-manager".into(),
            scope: assignment_scope.clone(),
            evaluated_at: detection_at,
            assignments: vec![assignee, manager.clone()],
            authority_source_ref: "control-plane-role-evaluator".into(),
        },
        idempotency_key: format!("operations-detection:{source_event_id}"),
        correlation_id: Uuid::new_v4(),
    };
    let source_event = &request.source_event;
    let mut source_tx = write_pool
        .begin()
        .await
        .expect("source envelope transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *source_tx)
        .await
        .expect("scope source envelope transaction");
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(source_event.source_event_id)
    .bind(&source_event.source_type)
    .bind(source_event.source_id)
    .bind(&holding_ref)
    .bind(i64::try_from(source_event.source_version).expect("source version fits SQL bigint"))
    .bind(format!("sales.{}", source_event.event_type))
    .bind(&source_event.payload)
    .bind(source_event.occurred_at)
    .bind(i32::from(erpx_domain::contracts::CONTRACT_VERSION))
    .bind(i32::from(source_event.event_version))
    .bind(
        serde_json::to_value(TenantScope::Holding { holding_id })
            .expect("serialize source tenant scope"),
    )
    .bind(serde_json::to_value(&source_event.actor).expect("serialize source actor"))
    .bind(source_event.correlation_id)
    .bind(source_event.causation_id)
    .bind(format!(
        "operations-detection-source-{}",
        source_event.source_event_id
    ))
    .execute(&mut *source_tx)
    .await
    .expect("insert authoritative source envelope");
    source_tx.commit().await.expect("commit source envelope");
    let store = SqlxOperationsStore::from_pools(&write_pool, &read_pool, &holding_ref)
        .expect("tenant Operations store");
    let applied = store
        .detect_exception_at(&request, detection_at)
        .await
        .expect("scheduled Operations detection");
    assert!(applied.ingestion.ingestion.applied);
    assert!(applied.assignment.applied);

    let replay = store
        .detect_exception_at(&request, detection_at)
        .await
        .expect("idempotent scheduled detection replay");
    assert!(!replay.ingestion.ingestion.applied);
    assert!(!replay.assignment.applied);

    let exception = store
        .get_aggregate::<ExceptionCase>(request.exception_id)
        .await
        .expect("read detected exception")
        .expect("detected exception exists");
    assert_eq!(exception.meta.version, 2);
    assert_eq!(exception.status, ExceptionStatus::Assigned);
    assert_eq!(
        exception.assigned_to.as_deref(),
        Some("operations-assignee")
    );

    let event_page = store
        .read_event_page(None, 10)
        .await
        .expect("read detection Operations events");
    assert_eq!(event_page.events.len(), 2);
    let assignment_record = event_page
        .events
        .iter()
        .find(|event| event.envelope.aggregate_version == 2)
        .expect("assignment event");
    let assignment_event = OperationsEvent {
        envelope: assignment_record.envelope.clone(),
        projection: ProjectionEventMetadata {
            projection_name: OPERATIONS_READ_PROJECTION.into(),
            event_id: assignment_record.envelope.event_id,
            aggregate_type: assignment_record.envelope.aggregate_type.clone(),
            aggregate_id: assignment_record.envelope.aggregate_id,
            aggregate_version: assignment_record.envelope.aggregate_version,
            projected_at: assignment_record.envelope.occurred_at,
        },
    };
    store
        .project_event(&assignment_event, &exception)
        .await
        .expect("project assigned exception");
    let page = store
        .read_active_exception_page(OPERATIONS_READ_PROJECTION, None, 10)
        .await
        .expect("read Operations exception queue");
    assert_eq!(page.cases.len(), 1);
    assert_eq!(page.cases[0].value.status, ExceptionStatus::Assigned);

    // The source-owned detection is now followed by a bounded worker
    // reassignment after the assignee's authoritative validity window ends.
    // The manager chain is re-evaluated for the replacement; it is not copied
    // from the stale read row.
    let reassignment_at = detection_at + Duration::minutes(2);
    let replacement_assignment = RoleAssignment {
        id: Uuid::new_v4(),
        principal_ref: "operations-replacement".into(),
        role_key: "operations_assignee".into(),
        permissions: vec![OperationsPermission::ExceptionManage.as_str().into()],
        functional_domain: "operations".into(),
        authority_level: 1,
        approval_limit_minor: 0,
        scope: assignment_scope.clone(),
        valid_from: detection_at,
        // The replacement remains contained by the manager assignment's
        // authoritative validity window.
        valid_until: detection_at + Duration::hours(1),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "Operations expired-principal reassignment test".into(),
        ticket_reference: None,
        grantor: "operations-manager".into(),
        approver: None,
        renewal_reviewer: None,
        parent_assignment_id: Some(manager_assignment_id),
        assignment_version: 1,
    };
    let reassignment = OperationsExpiredPrincipalReassignment {
        exception_id: request.exception_id,
        expected_version: 2,
        expired_principal_ref: "operations-assignee".into(),
        replacement: principal(
            "operations-replacement",
            detection_at,
            detection_at + Duration::hours(1),
        ),
        manager: principal(
            "operations-manager",
            detection_at - Duration::minutes(5),
            detection_at + Duration::hours(1),
        ),
        manager_chain: OperationsManagerChainProof {
            subject_principal_ref: "operations-replacement".into(),
            manager_principal_ref: "operations-manager".into(),
            scope: assignment_scope.clone(),
            evaluated_at: reassignment_at,
            assignments: vec![replacement_assignment, manager.clone()],
            authority_source_ref: "control-plane-role-evaluator".into(),
        },
        authorization: OperationsAuthorizationProof {
            principal_ref: "operations-reassignment-worker".into(),
            permission: OperationsPermission::ExceptionManage,
            scope: assignment_scope.clone(),
            valid_from: detection_at,
            valid_until: reassignment_at + Duration::minutes(5),
            mfa_verified: true,
            break_glass: None,
        },
        actor_attribution: ActorAttribution {
            principal_ref: "operations-reassignment-worker".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        at: reassignment_at,
        idempotency_key: format!("operations-reassignment:{}", request.exception_id),
        correlation_id: Uuid::new_v4(),
    };
    let reassigned = store
        .reassign_expired_principal(&reassignment)
        .await
        .expect("reassign expired principal");
    assert!(reassigned.lifecycle.applied);
    assert_eq!(reassigned.lifecycle.aggregate_version, 3);
    assert_eq!(
        reassigned.replacement_principal_ref,
        "operations-replacement"
    );
    let reassignment_replay = store
        .reassign_expired_principal(&reassignment)
        .await
        .expect("replay expired-principal reassignment");
    assert!(!reassignment_replay.lifecycle.applied);
    assert_eq!(
        reassignment_replay.lifecycle.event_id,
        reassigned.lifecycle.event_id
    );

    let reassigned_exception = store
        .get_aggregate::<ExceptionCase>(request.exception_id)
        .await
        .expect("read reassigned exception")
        .expect("reassigned exception exists");
    let reassignment_event_record = store
        .read_event_page(None, 10)
        .await
        .expect("read reassignment Operations events")
        .events
        .into_iter()
        .find(|event| event.envelope.aggregate_version == 3)
        .expect("reassignment event");
    let reassignment_event = OperationsEvent {
        envelope: reassignment_event_record.envelope.clone(),
        projection: ProjectionEventMetadata {
            projection_name: OPERATIONS_READ_PROJECTION.into(),
            event_id: reassignment_event_record.envelope.event_id,
            aggregate_type: reassignment_event_record.envelope.aggregate_type.clone(),
            aggregate_id: reassignment_event_record.envelope.aggregate_id,
            aggregate_version: reassignment_event_record.envelope.aggregate_version,
            projected_at: reassignment_event_record.envelope.occurred_at,
        },
    };
    store
        .project_event(&reassignment_event, &reassigned_exception)
        .await
        .expect("project reassigned exception");
    let reassigned_page = store
        .read_active_exception_page(OPERATIONS_READ_PROJECTION, None, 10)
        .await
        .expect("read reassigned Operations queue");
    assert_eq!(reassigned_page.cases.len(), 1);
    assert_eq!(
        reassigned_page.cases[0].value.assigned_to.as_deref(),
        Some("operations-replacement")
    );

    let reconciliation_at = reassignment_at + Duration::minutes(1);
    let reconciliation = store
        .reconciliation_report(
            &OperationsAuthorizationProof {
                principal_ref: "operations-reconciler".into(),
                permission: OperationsPermission::Reconcile,
                scope: assignment_scope.clone(),
                valid_from: detection_at - Duration::minutes(1),
                valid_until: reconciliation_at + Duration::minutes(5),
                mfa_verified: true,
                break_glass: None,
            },
            &assignment_scope,
            reconciliation_at,
        )
        .await
        .expect("reconcile scheduled detection");
    assert!(reconciliation.healthy);
    assert_eq!(reconciliation.events_observed, 3);
    assert_eq!(reconciliation.exception_count, 1);
    assert_eq!(reconciliation.missing_outbox_events, 0);
    let folio_baseline = FolioOperationsBaseline::new("folio-contract-hash-detection", 1)
        .expect("Folio Operations baseline");
    let folio_reconciliation = reconciliation
        .reconcile_folio_baseline(&folio_baseline)
        .expect("reconcile Folio Operations exception total");
    assert!(folio_reconciliation.healthy);
    assert_eq!(folio_reconciliation.exception_count_delta, 0);

    write_pool.close().await;
    read_pool.close().await;
}

#[tokio::test]
#[ignore = "requires one disposable tenant write and read PostgreSQL database"]
async fn sla_notification_claims_are_holding_scoped_and_persist_worker_identity() {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_WRITE_DATABASE_URL is required for this ignored test");
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")
        .expect("ERPX_TEST_TENANT_READ_DATABASE_URL is required for this ignored test");
    let write_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&write_url)
        .await
        .expect("tenant write pool");
    let read_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&read_url)
        .await
        .expect("tenant read pool");
    migrate_tenant_write(&write_pool)
        .await
        .expect("tenant write migrations");
    migrate_tenant_read(&read_pool)
        .await
        .expect("tenant read migrations");

    let at = Utc::now();
    let holding_a = fixture_ref("holding-operations-claim-a");
    let holding_b = fixture_ref("holding-operations-claim-b");
    let notification_a = format!("operations-sla:claim-a:{}", Uuid::new_v4());
    let notification_b = format!("operations-sla:claim-b:{}", Uuid::new_v4());
    let exception_a = Uuid::new_v4();
    let exception_b = Uuid::new_v4();

    for (holding_ref, notification_key, exception_id) in [
        (&holding_a, &notification_a, exception_a),
        (&holding_b, &notification_b, exception_b),
    ] {
        let mut tx = write_pool.begin().await.expect("claim fixture transaction");
        sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
            .bind(holding_ref)
            .execute(&mut *tx)
            .await
            .expect("scope claim fixture transaction");
        sqlx::query(
            "INSERT INTO erpx_tenant.operations_sla_notifications
                (notification_key, holding_ref, exception_id, exception_version,
                 recipient_ref, kind, not_before, expires_at, status, payload)
             VALUES ($1, $2, $3, 1, $4, 'sla_escalated', $5, $6, 'pending', $7)",
        )
        .bind(notification_key)
        .bind(holding_ref)
        .bind(exception_id)
        .bind("principal:operations-manager")
        .bind(at - Duration::minutes(1))
        .bind(at + Duration::hours(1))
        .bind(json!({ "holding_ref": holding_ref }))
        .execute(&mut *tx)
        .await
        .expect("persist claim fixture notification");
        tx.commit().await.expect("commit claim fixture");
    }

    let store_a = SqlxOperationsStore::from_pools(&write_pool, &read_pool, &holding_a)
        .expect("holding A Operations store");
    let worker_a = Uuid::new_v4();
    let claims_a = store_a
        .claim_sla_notifications(worker_a, at, Duration::seconds(30), 10)
        .await
        .expect("claim holding A notification");
    assert_eq!(claims_a.len(), 1);
    assert_eq!(claims_a[0].notification.holding_ref, holding_a);
    assert_eq!(claims_a[0].claimed_by, worker_a);
    assert!(!claims_a[0].claim_token.is_nil());

    let mut verify_tx = write_pool.begin().await.expect("verify claim transaction");
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_a)
        .execute(&mut *verify_tx)
        .await
        .expect("scope claim verification");
    let persisted = sqlx::query(
        "SELECT claimed_by, claim_token
         FROM erpx_tenant.operations_sla_notifications
         WHERE notification_key = $1 AND holding_ref = $2",
    )
    .bind(&notification_a)
    .bind(&holding_a)
    .fetch_one(&mut *verify_tx)
    .await
    .expect("read persisted claim identity");
    assert_eq!(
        persisted.try_get::<Uuid, _>("claimed_by").unwrap(),
        worker_a
    );
    assert_eq!(
        persisted.try_get::<Uuid, _>("claim_token").unwrap(),
        claims_a[0].claim_token
    );
    verify_tx.commit().await.expect("commit claim verification");

    let store_b = SqlxOperationsStore::from_pools(&write_pool, &read_pool, &holding_b)
        .expect("holding B Operations store");
    let claims_b = store_b
        .claim_sla_notifications(Uuid::new_v4(), at, Duration::seconds(30), 10)
        .await
        .expect("claim holding B notification");
    assert_eq!(claims_b.len(), 1);
    assert_eq!(claims_b[0].notification.holding_ref, holding_b);

    write_pool.close().await;
    read_pool.close().await;
}

#[test]
fn folio_operations_baseline_is_bounded_and_never_masks_drift() {
    let mut counts = BTreeMap::new();
    counts.insert("operations_exceptions".into(), 2);
    let snapshot = FolioSourceSnapshot {
        extractor_version: "g13-mig-test".into(),
        generated_at: Utc::now(),
        source_contract_hash: "folio-contract-hash-v1".into(),
        missing_tables: Vec::new(),
        counts,
        statuses: BTreeMap::new(),
        totals_minor: BTreeMap::new(),
        object_bytes: BTreeMap::new(),
        id_mappings: BTreeMap::new(),
    };
    let baseline = FolioOperationsBaseline::from_folio_snapshot(&snapshot)
        .expect("Folio snapshot exposes the Operations exception metric");
    let report = OperationsReconciliationReport {
        holding_ref: "holding-operations-test".into(),
        events_observed: 2,
        aggregate_count: 1,
        read_row_count: 1,
        exception_count: 2,
        checkpoint_count: 1,
        outbox_event_count: 2,
        missing_outbox_events: 0,
        outbox_mismatch_events: 0,
        stale_aggregate_count: 0,
        last_event_at: None,
        last_projected_at: None,
        projection_lag_seconds: None,
        healthy: true,
    };
    let matched = report
        .reconcile_folio_baseline(&baseline)
        .expect("matching Folio baseline");
    assert_eq!(
        matched,
        OperationsFolioReconciliation {
            source_snapshot_ref: "folio-contract-hash-v1".into(),
            folio_operations_exception_count: 2,
            erpx_operations_exception_count: 2,
            exception_count_delta: 0,
            operations_reconciliation_healthy: true,
            exception_count_match: true,
            healthy: true,
        }
    );

    let drift =
        FolioOperationsBaseline::new("folio-contract-hash-v1", 3).expect("valid drift baseline");
    let evidence = report
        .reconcile_folio_baseline(&drift)
        .expect("drift is evidence, not an unhandled error");
    assert_eq!(evidence.exception_count_delta, -1);
    assert!(!evidence.exception_count_match);
    assert!(!evidence.healthy);

    let mut missing_metric = snapshot;
    missing_metric.counts.clear();
    assert!(matches!(
        FolioOperationsBaseline::from_folio_snapshot(&missing_metric),
        Err(PersistenceError::InvalidData(message))
            if message.contains("operations_exceptions")
    ));
    assert!(matches!(
        FolioOperationsBaseline::new("folio/source", 1),
        Err(PersistenceError::InvalidData(_))
    ));
}
