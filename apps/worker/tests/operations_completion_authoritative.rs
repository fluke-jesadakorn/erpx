use std::collections::BTreeSet;

use chrono::{Duration, Utc};
use erpx_domain::{
    access::{RiskTier, RoleAssignment, Scope},
    contracts::{ActorAttribution, ActorKind, TenantScope},
    operations::completion::{
        OperationsAuthorizationProof, OperationsDetectionPolicy, OperationsManagerChainProof,
        OperationsPermission,
    },
    operations::{
        ExceptionCase, ExceptionStatus, OperationsEvent, OperationsSourceDomain,
        OperationsSourceEvent, ProjectionEventMetadata,
    },
};
use erpx_infrastructure::{MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE, SqlxOperationsStore};
use erpx_migration::{migrate_tenant_read, migrate_tenant_write};
use erpx_worker::{
    OperationsCompletionPass, OperationsCompletionWorker, OperationsCompletionWorkerError,
    OperationsDetectionSchedule, OperationsDetectionScheduleRule, OperationsEscalationSchedule,
    OperationsEscalationScheduleRule, OperationsOutboxSlaNotificationPublisher,
    OperationsReassignmentSchedule, OperationsReassignmentScheduleRule,
    OperationsSlaNotificationHandler,
};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

type TestError = Box<dyn std::error::Error + Send + Sync>;
type TestResult<T = ()> = Result<T, TestError>;

fn valid_pass(scope: Scope, authorization_scope: Scope) -> OperationsCompletionPass {
    let at = Utc::now();
    OperationsCompletionPass {
        tenant_id: "tenant-operations-authoritative".into(),
        holding_ref: "holding-operations-authoritative".into(),
        scope,
        authorization: OperationsAuthorizationProof {
            principal_ref: "operations-reconciliation-worker".into(),
            permission: OperationsPermission::Reconcile,
            scope: authorization_scope,
            valid_from: at - Duration::minutes(1),
            valid_until: at + Duration::minutes(10),
            mfa_verified: true,
            break_glass: None,
        },
        at,
        worker_id: Uuid::new_v4(),
        outbox_repair_limit: 100,
        projection_page_size: 100,
        max_projection_pages: 10,
        detections: Vec::new(),
        detection_schedule: None,
        reassignment_schedule: None,
        escalation_schedule: None,
        reassignments: Vec::new(),
        escalations: Vec::new(),
        legacy_operations_baseline: None,
    }
}

#[test]
fn completion_pass_rejects_branch_scope_and_cross_scope_authorization() {
    let holding_id = Uuid::from_u128(0x7701);
    let company_id = Uuid::from_u128(0x7702);
    let branch_id = Uuid::from_u128(0x7703);
    let branch = Scope::Branch {
        holding_id,
        company_id,
        branch_id,
    };
    let holding = Scope::Holding { holding_id };

    assert!(matches!(
        valid_pass(branch.clone(), branch).validate(),
        Err(OperationsCompletionWorkerError::Configuration(message))
            if message.contains("holding scope")
    ));
    assert!(matches!(
        valid_pass(holding.clone(), Scope::Holding {
            holding_id: Uuid::from_u128(0x7704),
        })
        .validate(),
        Err(OperationsCompletionWorkerError::Configuration(message))
            if message.contains("authorization")
    ));
}

#[tokio::test]
#[ignore = "requires a real migrated tenant write/read PostgreSQL pair"]
async fn source_reconciliation_is_authoritative_and_scope_isolated() -> TestResult {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")?;
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")?;
    let write_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&write_url)
        .await?;
    let read_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&read_url)
        .await?;
    migrate_tenant_write(&write_pool).await?;
    migrate_tenant_read(&read_pool).await?;

    let holding_ref = format!(
        "holding-operations-authoritative-{}",
        Uuid::new_v4().simple()
    );
    let holding_id = Uuid::from_u128(0x7710);
    let actual_scope = Scope::Branch {
        holding_id,
        company_id: Uuid::from_u128(0x7711),
        branch_id: Uuid::from_u128(0x7712),
    };
    let source_event_id = Uuid::new_v4();
    let source_event = OperationsSourceEvent {
        source_domain: OperationsSourceDomain::Sales,
        source_type: "sales.order".into(),
        source_id: Uuid::new_v4(),
        source_event_id,
        source_version: 1,
        event_type: "order.created".into(),
        event_version: 1,
        holding_ref: holding_ref.clone(),
        scope: actual_scope.clone(),
        actor: ActorAttribution {
            principal_ref: "sales-source".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        correlation_id: Uuid::new_v4(),
        causation_id: None,
        occurred_at: Utc::now(),
        payload: serde_json::json!({"order_ref": "order-authoritative"}),
    };
    let mut feed_tx = write_pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *feed_tx)
        .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
    )
    .bind(source_event_id)
    .bind(&source_event.source_type)
    .bind(source_event.source_id)
    .bind(&holding_ref)
    .bind(i64::try_from(source_event.source_version)?)
    .bind("sales.order.created")
    .bind(&source_event.payload)
    .bind(source_event.occurred_at)
    .bind(i32::from(erpx_domain::contracts::CONTRACT_VERSION))
    .bind(i32::from(source_event.event_version))
    .bind(serde_json::to_value(TenantScope::Branch {
        holding_id,
        company_id: Uuid::from_u128(0x7711),
        branch_id: Uuid::from_u128(0x7712),
    })?)
    .bind(serde_json::to_value(&source_event.actor)?)
    .bind(source_event.correlation_id)
    .bind(source_event.causation_id)
    .bind("operations-authoritative-source-feed")
    .execute(&mut *feed_tx)
    .await?;
    feed_tx.commit().await?;
    let exception = ExceptionCase::from_source_event(
        &source_event,
        Uuid::new_v4(),
        "source-reconciliation",
        "high",
    )?;
    let store = SqlxOperationsStore::from_pools(&write_pool, &read_pool, &holding_ref)?;
    store
        .ingest_source_event_and_project(&source_event, &exception, None)
        .await?;

    let holding_scope = Scope::Holding { holding_id };
    let clean =
        OperationsCompletionWorker::reconcile_source_events(&store, &holding_scope, 100, 10)
            .await?;
    assert_eq!(clean.source_references_checked, 1);
    assert_eq!(clean.source_event_links_checked, 1);
    assert_eq!(clean.source_event_link_gaps, 0);
    assert_eq!(clean.source_scope_violations, 0);
    assert_eq!(clean.source_outbox_events_checked, 1);
    assert_eq!(clean.source_outbox_scope_violations, 0);
    assert!(clean.source_event_reconciliation_healthy);
    assert!(clean.healthy);

    let wrong_holding = OperationsCompletionWorker::reconcile_source_events(
        &store,
        &Scope::Holding {
            holding_id: Uuid::from_u128(0x7713),
        },
        100,
        10,
    )
    .await?;
    assert!(wrong_holding.source_scope_violations >= 1);
    assert_eq!(wrong_holding.source_outbox_events_checked, 1);
    assert_eq!(wrong_holding.source_outbox_scope_violations, 1);
    assert!(!wrong_holding.healthy);

    let mut tamper_tx = write_pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *tamper_tx)
        .await?;
    sqlx::query(
        "UPDATE erpx_tenant.operations_source_references
         SET operations_event_id = NULL
         WHERE holding_ref = $1 AND source_event_id = $2",
    )
    .bind(&holding_ref)
    .bind(source_event_id)
    .execute(&mut *tamper_tx)
    .await?;
    tamper_tx.commit().await?;

    let tampered =
        OperationsCompletionWorker::reconcile_source_events(&store, &holding_scope, 100, 10)
            .await?;
    assert_eq!(tampered.source_references_checked, 1);
    assert_eq!(tampered.source_event_links_checked, 0);
    assert_eq!(tampered.source_event_link_gaps, 1);
    assert!(!tampered.source_event_reconciliation_healthy);
    assert!(!tampered.healthy);

    write_pool.close().await;
    read_pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires a real migrated tenant write/read PostgreSQL pair"]
async fn detection_scheduler_reads_source_outbox_and_derives_stable_authority_bound_requests()
-> TestResult {
    let write_url = std::env::var("ERPX_TEST_TENANT_WRITE_DATABASE_URL")?;
    let read_url = std::env::var("ERPX_TEST_TENANT_READ_DATABASE_URL")?;
    let write_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&write_url)
        .await?;
    let read_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&read_url)
        .await?;
    migrate_tenant_write(&write_pool).await?;
    migrate_tenant_read(&read_pool).await?;

    let holding_ref = format!("holding-operations-scheduler-{}", Uuid::new_v4().simple());
    let holding_id = Uuid::from_u128(0x7730);
    // Keep the scheduler event in the past so the SQLx claim lease is fenced
    // against the real database clock rather than a synthetic future time.
    let detection_at = Utc::now() - Duration::minutes(31);
    let source_event = OperationsSourceEvent {
        source_domain: OperationsSourceDomain::Sales,
        source_type: "sales.order".into(),
        source_id: Uuid::new_v4(),
        source_event_id: Uuid::new_v4(),
        source_version: 1,
        event_type: "order.delivery_breached".into(),
        event_version: 1,
        holding_ref: holding_ref.clone(),
        scope: Scope::Holding { holding_id },
        actor: ActorAttribution {
            principal_ref: "sales-source".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        correlation_id: Uuid::new_v4(),
        causation_id: None,
        occurred_at: detection_at - Duration::minutes(1),
        payload: serde_json::json!({"order_ref": "scheduler-authoritative"}),
    };
    let feed_event_id = Uuid::new_v4();
    let mut feed_tx = write_pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *feed_tx)
        .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
    )
    .bind(feed_event_id)
    .bind(&source_event.source_type)
    .bind(source_event.source_id)
    .bind(&holding_ref)
    .bind(i64::try_from(source_event.source_version)?)
    .bind("sales.order.delivery_breached")
    .bind(&source_event.payload)
    .bind(source_event.occurred_at)
    .bind(i32::from(erpx_domain::contracts::CONTRACT_VERSION))
    .bind(i32::from(source_event.event_version))
    .bind(serde_json::to_value(TenantScope::Holding { holding_id })?)
    .bind(serde_json::to_value(&source_event.actor)?)
    .bind(source_event.correlation_id)
    .bind(source_event.causation_id)
    .bind("sales-order-delivery-breached")
    .execute(&mut *feed_tx)
    .await?;
    feed_tx.commit().await?;

    let scope = Scope::Holding { holding_id };
    let manager_assignment_id = Uuid::new_v4();
    let manager = RoleAssignment {
        id: manager_assignment_id,
        principal_ref: "operations-manager".into(),
        role_key: "operations_manager".into(),
        permissions: vec![OperationsPermission::ExceptionManage.as_str().into()],
        functional_domain: "operations".into(),
        authority_level: 2,
        approval_limit_minor: 0,
        scope: scope.clone(),
        valid_from: detection_at - Duration::minutes(5),
        valid_until: detection_at + Duration::hours(1),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "scheduler acceptance".into(),
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
        scope: scope.clone(),
        valid_from: detection_at - Duration::minutes(5),
        valid_until: detection_at + Duration::hours(1),
        timezone: "Asia/Bangkok".into(),
        risk_tier: RiskTier::Standard,
        grant_reason: "scheduler acceptance".into(),
        ticket_reference: None,
        grantor: "operations-manager".into(),
        approver: None,
        renewal_reviewer: None,
        parent_assignment_id: Some(manager_assignment_id),
        assignment_version: 1,
    };
    let schedule = OperationsDetectionSchedule {
        rules: vec![OperationsDetectionScheduleRule {
            source_domain: OperationsSourceDomain::Sales,
            source_types: BTreeSet::from(["sales.order".into()]),
            policy: OperationsDetectionPolicy {
                policy_ref: "sales-delivery-breach".into(),
                source_domains: BTreeSet::from([OperationsSourceDomain::Sales]),
                category: "delivery_breach".into(),
                severity: "high".into(),
                due_after_minutes: 30,
                escalation_after_minutes: 60,
            },
        }],
        authorization: OperationsAuthorizationProof {
            principal_ref: "operations-scheduler".into(),
            permission: OperationsPermission::ExceptionManage,
            scope: scope.clone(),
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
        assignee: erpx_domain::operations::ExceptionPrincipalReference {
            principal_ref: "operations-assignee".into(),
            valid_from: detection_at - Duration::minutes(5),
            valid_until: detection_at + Duration::hours(1),
        },
        manager: erpx_domain::operations::ExceptionPrincipalReference {
            principal_ref: "operations-manager".into(),
            valid_from: detection_at - Duration::minutes(5),
            valid_until: detection_at + Duration::hours(1),
        },
        manager_chain: OperationsManagerChainProof {
            subject_principal_ref: "operations-assignee".into(),
            manager_principal_ref: "operations-manager".into(),
            scope: scope.clone(),
            evaluated_at: detection_at,
            assignments: vec![assignee, manager],
            authority_source_ref: "control-plane-role-evaluator".into(),
        },
        at: detection_at,
    };
    let store = SqlxOperationsStore::from_pools(&write_pool, &read_pool, &holding_ref)?;
    let first =
        OperationsCompletionWorker::schedule_detections(&store, &scope, &schedule, 100, 10).await?;
    let second =
        OperationsCompletionWorker::schedule_detections(&store, &scope, &schedule, 100, 10).await?;
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(first[0].exception_id, second[0].exception_id);
    assert_eq!(first[0].idempotency_key, second[0].idempotency_key);
    let applied = store.detect_exception_at(&first[0], detection_at).await?;
    assert!(applied.ingestion.ingestion.applied);
    assert!(applied.assignment.applied);
    let exception = store
        .get_aggregate::<ExceptionCase>(first[0].exception_id)
        .await?
        .expect("scheduled exception persisted");
    assert_eq!(exception.category, "delivery_breach");
    assert_eq!(
        exception.assigned_to.as_deref(),
        Some("operations-assignee")
    );

    let first_cursor_pass =
        OperationsCompletionWorker::apply_scheduled_detections(&store, &scope, &schedule, 100, 10)
            .await?;
    assert_eq!(first_cursor_pass, (1, 0));
    assert_eq!(
        store
            .source_detection_cursor()
            .await?
            .expect("durable source detection cursor")
            .source_event_id,
        feed_event_id
    );
    assert_eq!(
        OperationsCompletionWorker::apply_scheduled_detections(&store, &scope, &schedule, 100, 10)
            .await?,
        (0, 0)
    );

    // Bring the assignment event through the real read projector before the
    // scheduler scans the active queue. This keeps the test on the same
    // write-event/read-model boundary used by the registered worker.
    for record in store.read_event_page(None, 100).await?.events {
        if record.envelope.aggregate_id != first[0].exception_id {
            continue;
        }
        let aggregate: ExceptionCase = serde_json::from_value(record.envelope.payload.clone())?;
        let event_id = record.envelope.event_id;
        let aggregate_id = record.envelope.aggregate_id;
        let aggregate_type = record.envelope.aggregate_type.clone();
        let aggregate_version = record.envelope.aggregate_version;
        let event = OperationsEvent {
            envelope: record.envelope,
            projection: ProjectionEventMetadata {
                projection_name: erpx_infrastructure::OPERATIONS_READ_PROJECTION.into(),
                event_id,
                aggregate_type,
                aggregate_id,
                aggregate_version,
                projected_at: record.applied_at,
            },
        };
        store.project_event(&event, &aggregate).await?;
    }
    let mut reassignment_chain = schedule.manager_chain.clone();
    reassignment_chain.subject_principal_ref = "operations-replacement".into();
    reassignment_chain.assignments[0].principal_ref = "operations-replacement".into();
    let reassignment_schedule = OperationsReassignmentSchedule {
        rules: vec![OperationsReassignmentScheduleRule {
            expired_principal_ref: "operations-assignee".into(),
            replacement: erpx_domain::operations::ExceptionPrincipalReference {
                principal_ref: "operations-replacement".into(),
                valid_from: detection_at - Duration::minutes(5),
                valid_until: detection_at + Duration::hours(1),
            },
            manager: schedule.manager.clone(),
            manager_chain: reassignment_chain,
        }],
        authorization: schedule.authorization.clone(),
        actor_attribution: schedule.actor_attribution.clone(),
        at: detection_at,
    };
    let reassigned_first = OperationsCompletionWorker::schedule_reassignments(
        &store,
        &scope,
        &reassignment_schedule,
        100,
        10,
    )
    .await?;
    let reassigned_second = OperationsCompletionWorker::schedule_reassignments(
        &store,
        &scope,
        &reassignment_schedule,
        100,
        10,
    )
    .await?;
    assert_eq!(reassigned_first.len(), 1);
    assert_eq!(reassigned_second.len(), 1);
    assert_eq!(
        reassigned_first[0].idempotency_key,
        reassigned_second[0].idempotency_key
    );
    assert_eq!(
        reassigned_first[0].correlation_id,
        reassigned_second[0].correlation_id
    );
    let reassigned = store
        .reassign_expired_principal(&reassigned_first[0])
        .await?;
    assert!(reassigned.lifecycle.applied);
    let replacement = store
        .get_aggregate::<ExceptionCase>(first[0].exception_id)
        .await?
        .expect("reassigned exception persisted");
    assert_eq!(
        replacement.assigned_to.as_deref(),
        Some("operations-replacement")
    );

    // Project the reassignment event so the next scheduler reads the current
    // assigned principal and persisted SLA due time from the read model.
    for record in store.read_event_page(None, 100).await?.events {
        if record.envelope.aggregate_id != first[0].exception_id {
            continue;
        }
        let aggregate: ExceptionCase = serde_json::from_value(record.envelope.payload.clone())?;
        let event_id = record.envelope.event_id;
        let aggregate_id = record.envelope.aggregate_id;
        let aggregate_type = record.envelope.aggregate_type.clone();
        let aggregate_version = record.envelope.aggregate_version;
        let event = OperationsEvent {
            envelope: record.envelope,
            projection: ProjectionEventMetadata {
                projection_name: erpx_infrastructure::OPERATIONS_READ_PROJECTION.into(),
                event_id,
                aggregate_type,
                aggregate_id,
                aggregate_version,
                projected_at: record.applied_at,
            },
        };
        store.project_event(&event, &aggregate).await?;
    }
    let escalation_at = detection_at + Duration::minutes(31);
    let mut escalation_chain = schedule.manager_chain.clone();
    escalation_chain.subject_principal_ref = "operations-replacement".into();
    escalation_chain.assignments[0].principal_ref = "operations-replacement".into();
    escalation_chain.evaluated_at = escalation_at;
    let escalation_schedule = OperationsEscalationSchedule {
        rules: vec![OperationsEscalationScheduleRule {
            assigned_principal_ref: "operations-replacement".into(),
            manager: schedule.manager.clone(),
            manager_chain: escalation_chain,
        }],
        authorization: OperationsAuthorizationProof {
            principal_ref: "operations-sla-scheduler".into(),
            permission: OperationsPermission::ExceptionManage,
            scope: scope.clone(),
            valid_from: detection_at - Duration::minutes(1),
            valid_until: detection_at + Duration::hours(1),
            mfa_verified: true,
            break_glass: None,
        },
        actor_attribution: ActorAttribution {
            principal_ref: "operations-sla-scheduler".into(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        },
        at: escalation_at,
    };
    let escalations_first = OperationsCompletionWorker::schedule_escalations(
        &store,
        &scope,
        &escalation_schedule,
        100,
        10,
    )
    .await?;
    let escalations_second = OperationsCompletionWorker::schedule_escalations(
        &store,
        &scope,
        &escalation_schedule,
        100,
        10,
    )
    .await?;
    assert_eq!(escalations_first.len(), 1);
    assert_eq!(escalations_second.len(), 1);
    assert_eq!(
        escalations_first[0].idempotency_key,
        escalations_second[0].idempotency_key
    );
    assert_eq!(
        escalations_first[0].correlation_id,
        escalations_second[0].correlation_id
    );
    let escalated = store.escalate_due_exception(&escalations_first[0]).await?;
    assert!(escalated.lifecycle.applied);
    let escalated_exception = store
        .get_aggregate::<ExceptionCase>(first[0].exception_id)
        .await?
        .expect("escalated exception persisted");
    assert_eq!(escalated_exception.status, ExceptionStatus::Escalated);

    // Project the escalation event before checking the final read-side
    // reconciliation. The delivery handoff below is intentionally outbox-only;
    // it must not be mistaken for a missing Operations aggregate event.
    for record in store.read_event_page(None, 100).await?.events {
        if record.envelope.aggregate_id != first[0].exception_id {
            continue;
        }
        let aggregate: ExceptionCase = serde_json::from_value(record.envelope.payload.clone())?;
        let event_id = record.envelope.event_id;
        let aggregate_id = record.envelope.aggregate_id;
        let aggregate_type = record.envelope.aggregate_type.clone();
        let aggregate_version = record.envelope.aggregate_version;
        let event = OperationsEvent {
            envelope: record.envelope,
            projection: ProjectionEventMetadata {
                projection_name: erpx_infrastructure::OPERATIONS_READ_PROJECTION.into(),
                event_id,
                aggregate_type,
                aggregate_id,
                aggregate_version,
                projected_at: record.applied_at,
            },
        };
        store.project_event(&event, &aggregate).await?;
    }

    let publisher = OperationsOutboxSlaNotificationPublisher::new(
        store.clone(),
        "operations-notification-worker",
    )?;
    let notification_handler =
        OperationsSlaNotificationHandler::new(store.clone(), publisher, 10, Duration::seconds(30))?;
    let notification_report = notification_handler
        .run_once_at(Uuid::new_v4(), escalation_at)
        .await?;
    assert_eq!(notification_report.claimed, 1);
    assert_eq!(notification_report.processed, 1);
    assert_eq!(notification_report.retried, 0);
    let notification_state: String = sqlx::query_scalar(
        "SELECT status FROM erpx_tenant.operations_sla_notifications
         WHERE exception_id = $1",
    )
    .bind(first[0].exception_id)
    .fetch_one(&write_pool)
    .await?;
    assert_eq!(notification_state, "delivered");
    let delivery_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_tenant.outbox_events
         WHERE event_type = 'operations.sla_notification_delivered.v1'
           AND aggregate_id = $1",
    )
    .bind(first[0].exception_id)
    .fetch_one(&write_pool)
    .await?;
    assert_eq!(delivery_events, 1);
    let empty_notification_report = notification_handler
        .run_once_at(Uuid::new_v4(), escalation_at)
        .await?;
    assert_eq!(empty_notification_report.claimed, 0);

    let reconciliation_authorization = OperationsAuthorizationProof {
        principal_ref: "operations-reconciliation-worker".into(),
        permission: OperationsPermission::Reconcile,
        scope: scope.clone(),
        valid_from: detection_at - Duration::minutes(1),
        valid_until: escalation_at + Duration::minutes(10),
        mfa_verified: true,
        break_glass: None,
    };

    // Simulate a crash after the durable inbox was acknowledged but before a
    // downstream handoff envelope was retained. Reconciliation must surface
    // this gap, and the holding-scoped append-only repair must restore the
    // deterministic event without changing notification state.
    let mut corruption_tx = write_pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(&holding_ref)
        .execute(&mut *corruption_tx)
        .await?;
    let deleted_handoff = sqlx::query(
        "DELETE FROM erpx_tenant.outbox_events
         WHERE event_type = 'operations.sla_notification_delivered.v1'
           AND aggregate_id = $1",
    )
    .bind(first[0].exception_id)
    .execute(&mut *corruption_tx)
    .await?;
    corruption_tx.commit().await?;
    assert_eq!(deleted_handoff.rows_affected(), 1);

    let handoff_gap = store
        .reconciliation_report(
            &reconciliation_authorization,
            &scope,
            escalation_at + Duration::seconds(1),
        )
        .await?;
    assert!(!handoff_gap.healthy);
    assert_eq!(handoff_gap.outbox_mismatch_events, 1);

    let repaired = store
        .repair_missing_sla_notification_handoff_events(
            &reconciliation_authorization,
            &scope,
            escalation_at + Duration::seconds(1),
            "operations-reconciliation-worker",
            10,
        )
        .await?;
    assert_eq!(repaired, 1);
    assert_eq!(
        store
            .repair_missing_sla_notification_handoff_events(
                &reconciliation_authorization,
                &scope,
                escalation_at + Duration::seconds(1),
                "operations-reconciliation-worker",
                10,
            )
            .await?,
        0
    );

    let reconciliation = store
        .reconciliation_report(
            &reconciliation_authorization,
            &scope,
            escalation_at + Duration::seconds(1),
        )
        .await?;
    assert!(
        reconciliation.healthy,
        "unexpected reconciliation report: {reconciliation:?}"
    );
    assert_eq!(reconciliation.missing_outbox_events, 0);
    assert_eq!(reconciliation.outbox_mismatch_events, 0);

    write_pool.close().await;
    read_pool.close().await;
    Ok(())
}

#[test]
fn completion_page_budget_fails_closed_without_database_access() {
    let mut pass = valid_pass(
        Scope::Holding {
            holding_id: Uuid::from_u128(0x7720),
        },
        Scope::Holding {
            holding_id: Uuid::from_u128(0x7720),
        },
    );
    pass.max_projection_pages = 1_001;
    assert!(matches!(
        pass.validate(),
        Err(OperationsCompletionWorkerError::Configuration(message))
            if message.contains("bounds")
    ));
}

#[tokio::test]
async fn completion_outbox_and_source_page_bounds_fail_closed_before_database_access() {
    let holding_id = Uuid::from_u128(0x7721);
    let holding = Scope::Holding { holding_id };

    let mut oversized_outbox = valid_pass(holding.clone(), holding.clone());
    oversized_outbox.outbox_repair_limit = MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE + 1;
    assert!(matches!(
        oversized_outbox.validate(),
        Err(OperationsCompletionWorkerError::Configuration(message))
            if message.contains("SQLx page bound")
    ));

    let write_pool = sqlx::PgPool::connect_lazy("postgres://unused").expect("lazy write pool");
    let read_pool = sqlx::PgPool::connect_lazy("postgres://unused-read").expect("lazy read pool");
    let store = SqlxOperationsStore::from_pools(
        &write_pool,
        &read_pool,
        "holding-operations-authoritative",
    )
    .expect("store construction");
    let result = OperationsCompletionWorker::reconcile_source_events(
        &store,
        &holding,
        MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE + 1,
        1,
    )
    .await;
    assert!(matches!(
        result,
        Err(OperationsCompletionWorkerError::Configuration(message))
            if message.contains("source reconciliation page size")
    ));
}
