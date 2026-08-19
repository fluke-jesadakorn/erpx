use chrono::{Duration, Utc};
use erpx_domain::cqrs::OutboxEvent;
use erpx_infrastructure::{
    ControlPlaneDatabaseConfig, ControlPlanePoolSettings, ControlPlanePools, EventMetadata,
    SqlxControlPlane, WorkerError,
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
            erpx_control.idempotency_keys,
            erpx_control.holding_registry,
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
    let active_scope = json!({"holding_id": "holding-context-1", "data_profile_key": "holding-1"});
    let actor = json!({"principal_ref": "worker-context", "token": "never-persist"});
    let causation_id = Uuid::new_v4();
    let correlation_id = Uuid::new_v4();
    let worker_id = Uuid::new_v4();
    let mut transaction = store.begin_write().await.expect("begin write");
    transaction
        .append_outbox_event(
            &event,
            &EventMetadata {
                active_scope: Some(active_scope.clone()),
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
    assert_eq!(context["active_scope"], active_scope);
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
