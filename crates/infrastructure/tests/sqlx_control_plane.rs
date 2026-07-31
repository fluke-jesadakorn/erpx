use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use erpx_application::{
    AuditEvent, AuthoritativeRoleLifecycleService, AuthoritativeSupportService, CommandMutation,
    CommandUnitOfWork, CustomerReadStore, RoleAssignmentStore, SupportStore, SystemClock,
};
use erpx_domain::{
    access::{RiskTier, RoleAssignment, Scope},
    commercial::{
        BillingEvent, BillingLifecycle, BillingState, CatalogPrice, CatalogPriceKind, ChargeKind,
        CommercialCatalogRevisionDraft, EntitlementAllocationBook, EntitlementCapacity, Money,
        PricingUnit,
    },
    contracts::{ActorAttribution, ActorKind, AuditContext, CommandContext, TenantScope},
    cqrs::OutboxEvent,
    customer::{
        Customer360, CustomerAccount, CustomerHealth, CustomerStatus, GlobalReadActor,
        GlobalReadAuditRecord, GlobalReadScope, HealthState,
    },
};
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePoolSettings, ControlPlanePools, EventMetadata,
    IdempotencyState, ObjectAccessBindingRequest, OutboxEnvelope, OutboxHandler, OutboxWorker,
    PersistenceError, ProjectionCheckpointDecision, ProjectionHandler, SodDecision,
    SqlxCommercialStore, SqlxControlPlane, SqlxTenantStoreRegistry, StripeInboxEvent,
    StripeInboxInsert, TenantStoreRegistration, WorkerError,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

fn test_config() -> ControlPlaneDatabaseConfig {
    ControlPlaneDatabaseConfig {
        write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required for this ignored test"),
        read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required for this ignored test"),
    }
}

async fn reset_disposable_control_fixture(pools: &ControlPlanePools) {
    sqlx::query(
        "TRUNCATE TABLE erpx_control.outbox_events,
            erpx_control.aggregate_snapshots,
            erpx_control.idempotency_keys,
            erpx_control.tenant_registry,
            erpx_control.audit_events,
            erpx_control.object_access_bindings,
            erpx_control.commercial_catalog_revisions,
            erpx_control.grandfathered_price_migrations,
            erpx_control.billing_lifecycles,
            erpx_control.entitlement_allocation_books,
            erpx_control.stripe_event_inbox,
            erpx_control.projection_checkpoints,
            erpx_control.projection_event_receipts CASCADE",
    )
    .execute(&pools.write)
    .await
    .expect("clear disposable control source fixtures");
    sqlx::query("DELETE FROM erpx_control.customers")
        .execute(&pools.read)
        .await
        .expect("clear disposable control read customer fixtures");
}

fn event(aggregate_id: Uuid, version: u64, event_type: &str) -> OutboxEvent {
    OutboxEvent {
        id: Uuid::new_v4(),
        aggregate_type: "test_aggregate".into(),
        aggregate_id,
        aggregate_version: version,
        event_type: event_type.into(),
        payload: json!({ "version": version }),
        occurred_at: Utc::now(),
    }
}

fn customer(id: Uuid) -> Customer360 {
    Customer360 {
        account: CustomerAccount {
            id,
            account_code: "TEST-001".into(),
            trading_name: "Test Customer".into(),
            legal_name: Some("Test Customer Co., Ltd.".into()),
            industry: Some("test".into()),
            country: "TH".into(),
            status: CustomerStatus::Active,
            billing_state: BillingState::Active,
            renewal_at: None,
            lifetime_value_minor: 0,
        },
        health: CustomerHealth {
            customer_account_id: id,
            state: HealthState::Healthy,
            projection_lag_seconds: 0,
            worker_failures: 0,
            backup_status: "verified".into(),
            last_activity_at: None,
        },
        holding_count: 1,
        company_count: 1,
        branch_count: 1,
        active_user_count: 1,
        expiring_role_count: 0,
        open_ticket_count: 0,
        module_count: 1,
    }
}

#[derive(Debug, Clone)]
struct RecordingHandler {
    events: Arc<tokio::sync::Mutex<Vec<(Uuid, u64)>>>,
}

#[async_trait]
impl OutboxHandler for RecordingHandler {
    async fn handle(&self, event: &OutboxEnvelope) -> Result<(), WorkerError> {
        self.events
            .lock()
            .await
            .push((event.aggregate_id, event.aggregate_version));
        Ok(())
    }
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn control_plane_preserves_atomicity_ordering_and_read_projection() {
    let config = test_config();
    assert_ne!(config.write_url, config.read_url);
    let pools = ControlPlanePools::connect_with_settings(
        &config,
        ControlPlanePoolSettings {
            write_max_connections: 3,
            read_max_connections: 3,
            ..ControlPlanePoolSettings::default()
        },
    )
    .await
    .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("read migration");
    reset_disposable_control_fixture(&pools).await;
    let store = Arc::new(SqlxControlPlane::new(pools));
    let drain_handler = RecordingHandler {
        events: Arc::new(tokio::sync::Mutex::new(Vec::new())),
    };
    let drain_worker = OutboxWorker::new(store.clone(), Uuid::new_v4()).with_batch_size(100);
    loop {
        let report = drain_worker
            .run_once(&drain_handler)
            .await
            .expect("drain pre-existing outbox events");
        if report.published == 0 {
            break;
        }
    }
    let heartbeat_id = Uuid::new_v4();
    let heartbeat_at = Utc::now();
    store
        .record_worker_heartbeat(heartbeat_id, heartbeat_at)
        .await
        .expect("worker heartbeat write");
    assert_eq!(
        store
            .load_worker_heartbeat(heartbeat_id)
            .await
            .expect("worker heartbeat read"),
        Some(heartbeat_at)
    );
    let bound_customer = Uuid::new_v4();
    let bound_holding = Uuid::new_v4();
    let bound_company = Uuid::new_v4();
    let sibling_company = Uuid::new_v4();
    let bound_branch = Uuid::new_v4();
    store
        .upsert_customer_scope_binding(
            bound_customer,
            bound_holding,
            Some(bound_company),
            Some(bound_branch),
        )
        .await
        .expect("customer scope binding");
    assert!(
        store
            .customer_scope_contains(
                bound_customer,
                &Scope::Branch {
                    holding_id: bound_holding,
                    company_id: bound_company,
                    branch_id: bound_branch,
                },
            )
            .await
            .expect("bound branch lookup")
    );
    assert!(
        !store
            .customer_scope_contains(
                bound_customer,
                &Scope::Company {
                    holding_id: bound_holding,
                    company_id: sibling_company,
                },
            )
            .await
            .expect("sibling company lookup")
    );
    let tenant_registry = SqlxTenantStoreRegistry::new(store.clone());
    tenant_registry
        .register(TenantStoreRegistration {
            tenant_id: "tenant-opaque-1".into(),
            holding_ref: "holding-opaque-1".into(),
            write_database_key: "write-key-1".into(),
            read_database_key: "read-key-1".into(),
            storage_namespace: "storage-namespace-1".into(),
            credential_key_ref: "key-ref-1".into(),
            backup_chain_ref: "backup-chain-1".into(),
            state: "active".into(),
            registry_version: 1,
        })
        .await
        .expect("tenant registry write");
    let tenant = tenant_registry
        .get("tenant-opaque-1")
        .await
        .expect("tenant registry read")
        .expect("tenant registration");
    assert_eq!(tenant.read_database_key, "read-key-1");
    let read_registry_table: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('erpx_control.tenant_registry')::text")
            .fetch_one(&store.pools().read)
            .await
            .expect("read registry isolation query");
    assert_eq!(read_registry_table, None);

    let command_aggregate_id = Uuid::new_v4();
    let mut command_event = event(command_aggregate_id, 1, "support_ticket.created");
    command_event.aggregate_type = "support_ticket".into();
    let command = CommandMutation {
        command_id: Uuid::new_v4(),
        idempotency_key: "command-key".into(),
        request_hash: "command-hash".into(),
        aggregate_type: "support_ticket".into(),
        aggregate_id: command_aggregate_id,
        expected_aggregate_version: None,
        aggregate_state: json!({ "status": "new" }),
        event: command_event,
        event_version: 1,
        tenant_scope: Some(json!({ "scope": "holding" })),
        actor: Some(json!({ "principal_ref": "test-agent" })),
        causation_id: None,
        audit: AuditEvent {
            actor_ref: "test-agent".into(),
            customer_account_id: None,
            action: "support_ticket_created".into(),
            purpose: "integration_test".into(),
            correlation_id: Uuid::new_v4(),
        },
        response: json!({ "accepted": true }),
    };
    let first_receipt = store
        .execute_command(command.clone())
        .await
        .expect("atomic command");
    assert!(!first_receipt.replayed);
    let replay_receipt = store
        .execute_command(command)
        .await
        .expect("idempotent command replay");
    assert!(replay_receipt.replayed);

    let support =
        AuthoritativeSupportService::new(store.clone(), Arc::new(SystemClock), store.clone());
    let support_customer_id = Uuid::new_v4();
    let support_correlation = Uuid::new_v4();
    let support_context = CommandContext {
        contract_version: erpx_domain::contracts::CONTRACT_VERSION,
        command_id: Uuid::new_v4(),
        idempotency_key: "support-ticket-command".into(),
        correlation_id: support_correlation,
        causation_id: None,
        actor: ActorAttribution {
            principal_ref: "integration-agent".into(),
            kind: ActorKind::PlatformUser,
            session_id: Some(Uuid::new_v4()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        tenant_scope: TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        },
        audit: AuditContext {
            purpose: "integration_test".into(),
            customer_visible: true,
            module: Some("support".into()),
            correlation_id: support_correlation,
        },
        expected_aggregate_version: None,
        subscription_id: None,
    };
    let support_ticket = support
        .open_ticket(
            &support_context,
            support_customer_id,
            "high",
            "command-backed support ticket",
        )
        .await
        .expect("command-backed support ticket");
    let reloaded_support_ticket = store
        .get_ticket(support_ticket.id)
        .await
        .expect("support ticket read")
        .expect("command-backed support ticket projection fallback");
    assert_eq!(
        reloaded_support_ticket.description,
        support_ticket.description
    );

    let legacy_assignment = RoleAssignment {
        id: Uuid::new_v4(),
        principal_ref: "role-user".into(),
        role_key: "holding_owner".into(),
        permissions: vec!["support.read".into()],
        functional_domain: "support".into(),
        authority_level: 2,
        approval_limit_minor: 0,
        scope: Scope::Holding {
            holding_id: Uuid::new_v4(),
        },
        valid_from: Utc::now(),
        valid_until: Utc::now() + chrono::Duration::days(10),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "integration fixture".into(),
        ticket_reference: None,
        grantor: "fixture".into(),
        approver: None,
        renewal_reviewer: None,
        parent_assignment_id: None,
        assignment_version: 1,
    };
    store
        .create(legacy_assignment.clone())
        .await
        .expect("legacy role fixture");
    assert!(
        store
            .principal_has_role(
                &legacy_assignment.principal_ref,
                &legacy_assignment.scope,
                "support.read",
                Utc::now(),
            )
            .await
            .expect("role authorization lookup")
    );
    assert!(
        store
            .principal_has_role_key(
                &legacy_assignment.principal_ref,
                &legacy_assignment.scope,
                &["holding_owner", "customer_security_head"],
                Utc::now(),
            )
            .await
            .expect("customer approver role lookup")
    );
    assert!(
        !store
            .principal_has_role(
                &legacy_assignment.principal_ref,
                &legacy_assignment.scope,
                "finance.write",
                Utc::now(),
            )
            .await
            .expect("role authorization denial")
    );
    store
        .record_sod_decision(&SodDecision {
            principal_ref: legacy_assignment.principal_ref.clone(),
            module_key: "support".into(),
            scope: legacy_assignment.scope.clone(),
            passed: true,
            reviewer_ref: "security-reviewer".into(),
            reason: "independent support access review".into(),
            valid_from: legacy_assignment.valid_from,
            valid_until: legacy_assignment.valid_until,
            decision_version: 1,
        })
        .await
        .expect("SoD decision persistence");
    let effective = store
        .effective_role_assignment(
            &legacy_assignment.principal_ref,
            &legacy_assignment.scope,
            "support.read",
            Utc::now(),
        )
        .await
        .expect("effective assignment lookup")
        .expect("effective assignment");
    assert_eq!(effective.0.id, legacy_assignment.id);
    assert!(
        store
            .sod_decision_active(
                &legacy_assignment.principal_ref,
                "support",
                &legacy_assignment.scope,
                Utc::now(),
            )
            .await
            .expect("SoD lookup")
    );
    let role_correlation = Uuid::new_v4();
    let role_context = CommandContext {
        command_id: Uuid::new_v4(),
        idempotency_key: "role-renew-command".into(),
        correlation_id: role_correlation,
        audit: AuditContext {
            correlation_id: role_correlation,
            ..support_context.audit.clone()
        },
        ..support_context.clone()
    };
    let roles =
        AuthoritativeRoleLifecycleService::new(store.clone(), Arc::new(SystemClock), store.clone());
    let renewed = roles
        .renew(
            &role_context,
            legacy_assignment.id,
            Utc::now() + chrono::Duration::days(20),
            "role-user",
            None,
        )
        .await
        .expect("command-backed role renewal");
    let reloaded_role = store
        .get(renewed.id)
        .await
        .expect("renewed role read")
        .expect("command-backed role snapshot fallback");
    assert_eq!(reloaded_role.assignment_version, 2);

    let aggregate_id = Uuid::new_v4();
    let first = event(aggregate_id, 1, "customer.created");
    let second = event(aggregate_id, 2, "customer.updated");

    let mut transaction = store.begin_write().await.expect("transaction");
    assert_eq!(
        transaction
            .upsert_aggregate_snapshot(
                "test_aggregate",
                aggregate_id,
                json!({ "version": 1 }),
                None
            )
            .await
            .expect("snapshot"),
        1
    );
    transaction
        .append_outbox_event(&first, &EventMetadata::for_event(&first))
        .await
        .expect("outbox append");
    transaction.commit().await.expect("atomic commit");

    let state_version: i64 = sqlx::query_scalar(
        "SELECT aggregate_version FROM erpx_control.aggregate_snapshots WHERE aggregate_id = $1",
    )
    .bind(aggregate_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("snapshot query");
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.outbox_events WHERE aggregate_id = $1",
    )
    .bind(aggregate_id)
    .fetch_one(&store.pools().write)
    .await
    .expect("outbox query");
    assert_eq!(state_version, 1);
    assert_eq!(event_count, 1);

    let mut conflict = store.begin_write().await.expect("conflict transaction");
    let result = conflict
        .upsert_aggregate_snapshot(
            "test_aggregate",
            aggregate_id,
            json!({ "version": 0 }),
            Some(0),
        )
        .await;
    assert!(matches!(
        result,
        Err(PersistenceError::OptimisticConcurrency {
            expected: Some(0),
            actual: Some(1),
            ..
        })
    ));
    conflict.rollback().await.expect("conflict rollback");

    let reservation = store
        .reserve_idempotency("test-key", "request-hash", None)
        .await
        .expect("idempotency reservation");
    assert_eq!(reservation.state, IdempotencyState::Reserved);
    let in_progress = store
        .reserve_idempotency("test-key", "request-hash", None)
        .await
        .expect("idempotency replay");
    assert_eq!(
        in_progress.state,
        IdempotencyState::InProgress { command_id: None }
    );
    store
        .complete_idempotency("test-key", json!({ "accepted": true }))
        .await
        .expect("idempotency completion");
    let completed = store
        .reserve_idempotency("test-key", "request-hash", None)
        .await
        .expect("completed replay");
    assert_eq!(
        completed.state,
        IdempotencyState::Completed {
            response: json!({ "accepted": true })
        }
    );

    let mut second_transaction = store.begin_write().await.expect("second transaction");
    second_transaction
        .upsert_aggregate_snapshot(
            "test_aggregate",
            aggregate_id,
            json!({ "version": 2 }),
            Some(1),
        )
        .await
        .expect("second snapshot");
    second_transaction
        .append_outbox_event(&second, &EventMetadata::for_event(&second))
        .await
        .expect("second outbox append");
    second_transaction.commit().await.expect("second commit");

    let recorded = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let worker = OutboxWorker::new(store.clone(), Uuid::new_v4()).with_batch_size(10);
    let handler = RecordingHandler {
        events: recorded.clone(),
    };
    let first_report = worker.run_once(&handler).await.expect("first worker run");
    assert_eq!(first_report.published, 4);
    let second_report = worker.run_once(&handler).await.expect("second worker run");
    assert_eq!(second_report.published, 1);
    let aggregate_versions: Vec<_> = recorded
        .lock()
        .await
        .iter()
        .filter(|(id, _)| *id == aggregate_id)
        .map(|(_, version)| *version)
        .collect();
    assert_eq!(aggregate_versions, vec![1, 2]);

    let projected_events = store.events_for_rebuild().await.expect("projected events");
    let first_event = projected_events
        .iter()
        .find(|event| event.aggregate_id == aggregate_id && event.aggregate_version == 1)
        .expect("first customer event");
    assert!(
        store
            .advance_projection_checkpoint("customer_360", first_event, Utc::now())
            .await
            .expect("advance checkpoint")
    );
    assert!(
        !store
            .advance_projection_checkpoint("customer_360", first_event, Utc::now())
            .await
            .expect("replayed checkpoint")
    );
    let mut conflicting_event = first_event.clone();
    conflicting_event.id = Uuid::new_v4();
    conflicting_event.event_type = "customer.forged".into();
    assert!(matches!(
        store
            .projection_checkpoint_decision("customer_360", &conflicting_event)
            .await
            .expect("conflicting checkpoint decision"),
        ProjectionCheckpointDecision::ConflictingEvent {
            expected_event_id: Some(expected_event_id)
        } if expected_event_id == first_event.id
    ));

    #[derive(Debug)]
    struct RejectingProjectionHandler;

    #[async_trait]
    impl ProjectionHandler for RejectingProjectionHandler {
        fn handles_event(&self, _projection_name: &str, _event: &OutboxEnvelope) -> bool {
            true
        }

        async fn clear(&self, _projection_name: &str) -> Result<(), WorkerError> {
            Ok(())
        }

        async fn project(
            &self,
            _projection_name: &str,
            _event: &OutboxEnvelope,
        ) -> Result<(), WorkerError> {
            Err(WorkerError::Permanent(
                "conflicting event was projected".into(),
            ))
        }
    }

    let projection_worker_id = Uuid::new_v4();
    assert!(
        store
            .try_claim_projection_event(
                "customer_360",
                first_event,
                projection_worker_id,
                Utc::now(),
                chrono::Duration::minutes(5),
            )
            .await
            .expect("projection claim")
    );
    assert!(
        !store
            .try_claim_projection_event(
                "customer_360",
                first_event,
                projection_worker_id,
                Utc::now(),
                chrono::Duration::minutes(5),
            )
            .await
            .expect("duplicate projection claim")
    );
    store
        .complete_projection_event(
            "customer_360",
            first_event.id,
            projection_worker_id,
            Utc::now(),
        )
        .await
        .expect("complete projection claim");
    assert!(
        !store
            .try_claim_projection_event(
                "customer_360",
                first_event,
                projection_worker_id,
                Utc::now(),
                chrono::Duration::minutes(5),
            )
            .await
            .expect("completed projection claim")
    );
    let projection_error = worker
        .project_once(
            "customer_360",
            &conflicting_event,
            &RejectingProjectionHandler,
        )
        .await
        .expect_err("conflicting event must fail closed");
    assert!(
        projection_error
            .to_string()
            .contains("conflicts with the current checkpoint event identity")
    );

    let third = event(aggregate_id, 3, "customer.repriced");
    let mut third_transaction = store.begin_write().await.expect("third transaction");
    third_transaction
        .upsert_aggregate_snapshot(
            "test_aggregate",
            aggregate_id,
            json!({ "version": 3 }),
            Some(2),
        )
        .await
        .expect("third snapshot");
    third_transaction
        .append_outbox_event(&third, &EventMetadata::for_event(&third))
        .await
        .expect("third outbox append");
    third_transaction.commit().await.expect("third commit");
    let third_claim = store
        .claim_outbox(
            worker.worker_id(),
            10,
            Utc::now(),
            chrono::Duration::minutes(5),
        )
        .await
        .expect("third claim");
    let third_event = third_claim
        .iter()
        .find(|event| event.id == third.id)
        .expect("third event");
    store
        .dead_letter_outbox(
            third_event.id,
            worker.worker_id(),
            "test failure",
            Utc::now(),
        )
        .await
        .expect("dead-letter event");
    store
        .replay_dead_letter(third_event.id, Utc::now())
        .await
        .expect("replay dead-letter event");
    let replayed_claim = store
        .claim_outbox(
            worker.worker_id(),
            10,
            Utc::now(),
            chrono::Duration::minutes(5),
        )
        .await
        .expect("replayed event claim");
    assert!(replayed_claim.iter().any(|event| event.id == third.id));
    for event in &replayed_claim {
        store
            .mark_outbox_published(event.id, worker.worker_id(), Utc::now())
            .await
            .expect("publish replayed event");
    }

    let customer_id = bound_customer;
    store
        .upsert_customer_read_model(&customer(customer_id))
        .await
        .expect("read projection");
    let projected = store
        .get_customer(customer_id)
        .await
        .expect("read customer")
        .expect("projected customer");
    assert_eq!(projected.account.account_code, "TEST-001");

    let global_read = GlobalReadAuditRecord::new(
        GlobalReadActor {
            principal_ref: "platform-auditor".into(),
            session_ref: Some("session-auditor".into()),
        },
        customer_id,
        "platform.customer_360",
        GlobalReadScope {
            label: "platform".into(),
            holding_id: None,
            company_id: None,
            branch_id: None,
        },
        "integration_test",
        Utc::now(),
        Uuid::new_v4(),
    )
    .expect("global read audit");
    store
        .record_global_read(&global_read)
        .await
        .expect("global read audit persistence");
    assert_eq!(
        store
            .list_global_reads(customer_id, 10)
            .await
            .unwrap()
            .len(),
        1
    );

    let object_scope = Scope::Company {
        holding_id: bound_holding,
        company_id: bound_company,
    };
    assert!(
        store
            .authorize_object_binding(&ObjectAccessBindingRequest {
                tenant_id: "tenant-opaque-1",
                storage_namespace: "storage-namespace-1",
                object_key: "customer-object-1",
                customer_id,
                scope: &object_scope,
                actor_ref: "uploader",
                for_write: true,
            })
            .await
            .expect("create object binding")
    );
    assert!(
        store
            .authorize_object_binding(&ObjectAccessBindingRequest {
                tenant_id: "tenant-opaque-1",
                storage_namespace: "storage-namespace-1",
                object_key: "customer-object-1",
                customer_id,
                scope: &object_scope,
                actor_ref: "reader",
                for_write: false,
            })
            .await
            .expect("read object binding")
    );
    assert!(
        !store
            .authorize_object_binding(&ObjectAccessBindingRequest {
                tenant_id: "tenant-opaque-1",
                storage_namespace: "storage-namespace-1",
                object_key: "customer-object-1",
                customer_id,
                scope: &Scope::Company {
                    holding_id: bound_holding,
                    company_id: sibling_company,
                },
                actor_ref: "sibling-reader",
                for_write: false,
            })
            .await
            .expect("reject sibling object scope")
    );
    assert!(
        !store
            .authorize_object_binding(&ObjectAccessBindingRequest {
                tenant_id: "tenant-opaque-2",
                storage_namespace: "storage-namespace-1",
                object_key: "customer-object-1",
                customer_id,
                scope: &object_scope,
                actor_ref: "other-tenant-reader",
                for_write: false,
            })
            .await
            .expect("reject cross-tenant object scope")
    );

    let commercial = SqlxCommercialStore::new(&store);
    let catalog = CommercialCatalogRevisionDraft {
        id: Uuid::new_v4(),
        revision_number: 1,
        prices: vec![CatalogPrice {
            item_key: "base".into(),
            display_name: "Base".into(),
            kind: CatalogPriceKind::BaseFee,
            pricing_unit: PricingUnit::Flat,
            charge: ChargeKind::Recurring(erpx_domain::commercial::BillingInterval::Monthly),
            amount: Money {
                amount_minor: 100,
                currency: "THB".into(),
            },
            trial: None,
            coupon: None,
            setup_fee: None,
            included_usage: None,
            overage: None,
        }],
    };
    let published = commercial
        .publish_catalog_revision(catalog.clone(), Utc::now(), "billing-admin")
        .await
        .expect("catalog persistence");
    assert_eq!(
        commercial
            .get_catalog_revision(published.revision_number())
            .await
            .unwrap()
            .unwrap()
            .id(),
        published.id()
    );
    assert!(
        commercial
            .publish_catalog_revision(catalog, Utc::now(), "billing-admin")
            .await
            .is_err()
    );

    let subscription_id = Uuid::new_v4();
    let mut lifecycle = BillingLifecycle::new(BillingState::Trialing, Utc::now());
    lifecycle
        .transition(BillingEvent::TrialEnded, Utc::now())
        .expect("billing transition");
    assert_eq!(
        commercial
            .upsert_billing_lifecycle(customer_id, subscription_id, &lifecycle, None)
            .await
            .unwrap(),
        1
    );
    let book = EntitlementAllocationBook::new(
        "finance",
        EntitlementCapacity {
            seats: 10,
            companies: 2,
            branches: 4,
        },
        None,
    );
    assert_eq!(
        commercial
            .upsert_entitlement_book(customer_id, &book, None)
            .await
            .unwrap(),
        1
    );
    let stripe = StripeInboxEvent {
        event_id: "evt_integration_1".into(),
        event_type: "invoice.paid".into(),
        stripe_created_at: Some(Utc::now()),
        payload: json!({"id":"evt_integration_1","type":"invoice.paid"}),
        raw_body: br#"{"id":"evt_integration_1","type":"invoice.paid"}"#.to_vec(),
    };
    assert_eq!(
        commercial.insert_stripe_event(&stripe).await.unwrap(),
        StripeInboxInsert::Inserted
    );
    assert_eq!(
        commercial.insert_stripe_event(&stripe).await.unwrap(),
        StripeInboxInsert::Duplicate
    );
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn worker_failure_context_is_persisted_with_scope_and_secret_redaction() {
    let config = test_config();
    let pools = ControlPlanePools::connect_with_settings(
        &config,
        ControlPlanePoolSettings {
            write_max_connections: 2,
            read_max_connections: 1,
            ..ControlPlanePoolSettings::default()
        },
    )
    .await
    .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("read migration");
    reset_disposable_control_fixture(&pools).await;
    let store = SqlxControlPlane::new(pools);
    let event = event(Uuid::new_v4(), 1, "worker.context.fixture");
    let tenant_scope = json!({"tenant_id": "tenant-context-1", "holding_ref": "holding-1"});
    let actor = json!({"principal_ref": "worker-context", "token": "never-persist"});
    let causation_id = Uuid::new_v4();
    let correlation_id = Uuid::new_v4();
    let worker_id = Uuid::new_v4();
    let mut transaction = store.begin_write().await.expect("begin write");
    transaction
        .append_outbox_event(
            &event,
            &EventMetadata {
                tenant_scope: Some(tenant_scope.clone()),
                actor: Some(actor),
                correlation_id: Some(correlation_id),
                causation_id: Some(causation_id),
                idempotency_key: Some("worker-context-fixture".into()),
                event_version: 1,
            },
        )
        .await
        .expect("append fixture event");
    transaction.commit().await.expect("commit fixture event");

    let claimed = store
        .claim_outbox(
            worker_id,
            100,
            Utc::now() + Duration::seconds(1),
            Duration::minutes(5),
        )
        .await
        .expect("claim fixture event");
    let claimed_event = claimed
        .into_iter()
        .find(|candidate| candidate.id == event.id)
        .expect("fixture event claim");
    let failure = WorkerError::Retryable("provider password=never-persist".into())
        .context(&claimed_event, worker_id);
    failure
        .validate_for(&claimed_event, worker_id)
        .expect("failure context binding");
    store
        .retry_outbox_with_context(
            claimed_event.id,
            worker_id,
            Utc::now() + chrono::Duration::seconds(5),
            &failure.message,
            Some(&failure),
            Utc::now(),
        )
        .await
        .expect("persist retry context");

    let row = sqlx::query(
        "SELECT last_error, last_error_context
         FROM erpx_control.outbox_events WHERE id = $1",
    )
    .bind(event.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("read retry context");
    let last_error: String = row.try_get("last_error").expect("last error");
    let context: serde_json::Value = row
        .try_get("last_error_context")
        .expect("last error context");
    assert_eq!(last_error, "worker handler failed");
    assert_eq!(context["event_id"], event.id.to_string());
    assert_eq!(context["correlation_id"], correlation_id.to_string());
    assert_eq!(context["causation_id"], causation_id.to_string());
    assert_eq!(context["tenant_scope"], tenant_scope);
    assert!(context["actor"].is_null());
    assert!(!context.to_string().contains("never-persist"));
    assert!(!context.to_string().contains("password"));
}

#[tokio::test]
#[ignore = "requires two disposable PostgreSQL databases"]
async fn audited_dead_letter_replay_is_atomic_idempotent_and_redacted() {
    let config = test_config();
    let pools = ControlPlanePools::connect_with_settings(
        &config,
        ControlPlanePoolSettings {
            write_max_connections: 2,
            read_max_connections: 1,
            ..ControlPlanePoolSettings::default()
        },
    )
    .await
    .expect("control pools");
    migrate_control_plane(&pools.write)
        .await
        .expect("write migration");
    migrate_control_read(&pools.read)
        .await
        .expect("read migration");
    reset_disposable_control_fixture(&pools).await;
    let store = SqlxControlPlane::new(pools);
    let mut event = event(Uuid::new_v4(), 1, "worker.replay.fixture");
    event.payload = json!({"password": "never-persist", "accepted": true});
    let mut transaction = store.begin_write().await.expect("begin write");
    transaction
        .append_outbox_event(
            &event,
            &EventMetadata {
                actor: Some(json!({"principal_ref": "operator"})),
                correlation_id: Some(Uuid::new_v4()),
                idempotency_key: Some(format!("dead-letter-replay-fixture-source:{}", event.id)),
                event_version: 1,
                ..EventMetadata::default()
            },
        )
        .await
        .expect("append replay fixture");
    transaction.commit().await.expect("commit replay fixture");

    let worker_id = Uuid::new_v4();
    let claimed = store
        .claim_outbox_for_aggregate_types(
            worker_id,
            1_000,
            Utc::now(),
            chrono::Duration::minutes(5),
            &["test_aggregate"],
        )
        .await
        .expect("claim replay fixture");
    assert!(claimed.iter().any(|candidate| candidate.id == event.id));
    store
        .dead_letter_outbox(event.id, worker_id, "fixture failure", Utc::now())
        .await
        .expect("dead-letter replay fixture");

    let correlation_id = Uuid::new_v4();
    let command_id = Uuid::new_v4();
    let available_at = Utc::now();
    let first = store
        .replay_dead_letter_with_audit(erpx_infrastructure::DeadLetterReplayCommand {
            event_id: event.id,
            available_at,
            actor_ref: "operator-1",
            purpose: "incident remediation",
            correlation_id,
            idempotency_key: "dead-letter-replay-command-1",
            command_id,
        })
        .await
        .expect("audited replay");
    assert_eq!(first.event_id, event.id);
    assert_eq!(first.correlation_id, correlation_id);
    assert!(!first.replayed);

    let duplicate = store
        .replay_dead_letter_with_audit(erpx_infrastructure::DeadLetterReplayCommand {
            event_id: event.id,
            available_at,
            actor_ref: "operator-1",
            purpose: "incident remediation",
            correlation_id,
            idempotency_key: "dead-letter-replay-command-1",
            command_id,
        })
        .await
        .expect("idempotent audited replay");
    assert!(duplicate.replayed);

    let row = sqlx::query(
        "SELECT dead_lettered_at, last_error, last_error_context
         FROM erpx_control.outbox_events WHERE id = $1",
    )
    .bind(event.id)
    .fetch_one(&store.pools().write)
    .await
    .expect("read replayed outbox event");
    assert!(
        row.try_get::<Option<chrono::DateTime<Utc>>, _>("dead_lettered_at")
            .expect("dead-letter timestamp")
            .is_none()
    );
    assert!(
        row.try_get::<Option<String>, _>("last_error")
            .expect("last error")
            .is_none()
    );
    assert!(
        row.try_get::<Option<serde_json::Value>, _>("last_error_context")
            .expect("last error context")
            .is_none()
    );

    let audit_rows = sqlx::query(
        "SELECT actor_ref, action, purpose, correlation_id, payload
         FROM erpx_control.audit_events
         WHERE action = 'platform.outbox.dead_letter.replay'",
    )
    .fetch_all(&store.pools().write)
    .await
    .expect("read replay audit");
    assert_eq!(audit_rows.len(), 1);
    let audit_payload: serde_json::Value = audit_rows[0].try_get("payload").expect("audit payload");
    assert_eq!(
        audit_rows[0].try_get::<String, _>("actor_ref").unwrap(),
        "operator-1"
    );
    assert_eq!(
        audit_rows[0].try_get::<String, _>("purpose").unwrap(),
        "incident remediation"
    );
    assert_eq!(
        audit_rows[0].try_get::<Uuid, _>("correlation_id").unwrap(),
        correlation_id
    );
    assert_eq!(audit_payload["event_id"], event.id.to_string());
    assert_eq!(audit_payload["command_id"], command_id.to_string());
    assert!(audit_payload.get("payload").is_none());
    assert!(!audit_payload.to_string().contains("never-persist"));
}
