//! Disposable PostgreSQL acceptance for the durable G4 support workflow.
//!
//! The test is intentionally ignored unless a real control write/read pair is
//! supplied. It never substitutes an in-memory fake for the SQL transaction,
//! outbox, projector, or rollback boundary.

use chrono::{Duration, Utc};
use erpx_application::{PersistApprovedSupportGrantCommand, RevokeSupportGrantCommand};
use erpx_domain::{
    access::Scope,
    support::{
        Incident, IncidentStatus, SupportCapability, SupportCapabilityBinding,
        SupportDeviceSessionBinding, SupportMode, SupportSession, SupportTicket, TicketStatus,
    },
};
use erpx_infrastructure::{
    DiagnosticsLimits, DiagnosticsSnapshotInput, SupportDiagnosticsAuditRequest,
    SupportIdempotencyKey, SupportPersistence, SupportWorkflowPersistenceError,
    SupportWorkflowStore,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use serde_json::Value;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

fn grant_fixture(
    ticket_id: Uuid,
    customer_account_id: Uuid,
    scope: &Scope,
    at: chrono::DateTime<Utc>,
) -> (PersistApprovedSupportGrantCommand, String) {
    let mut session = SupportSession::request_with_module(
        ticket_id,
        customer_account_id,
        "support-agent-acceptance",
        SupportMode::TenantMutation,
        vec!["ticket.read".into()],
        scope.canonical_key(),
        "support",
        "reconcile durable support grant",
        at - Duration::minutes(2),
        Duration::minutes(30),
        None,
    )
    .expect("support session");
    session
        .approve("customer-approver-acceptance", at - Duration::minutes(1))
        .expect("support approval");
    let device = SupportDeviceSessionBinding::new(
        session.id,
        "support-agent-acceptance",
        "device-acceptance",
        Some(at - Duration::minutes(1)),
        at - Duration::minutes(1),
    )
    .expect("device binding");
    let binding = SupportCapabilityBinding::new(
        customer_account_id,
        "support-agent-acceptance",
        "device-acceptance",
        session.id,
        ticket_id,
        SupportMode::TenantMutation,
        vec!["ticket.read".into()],
        scope.canonical_key(),
        "support",
    )
    .expect("capability binding");
    let (capability, secret) =
        SupportCapability::issue(binding, at - Duration::seconds(30), Duration::minutes(10))
            .expect("capability");
    let secret_hex = secret.as_hex();
    (
        PersistApprovedSupportGrantCommand::new(
            session,
            device,
            capability,
            scope.clone(),
            scope.clone(),
            true,
            "approval-acceptance-1",
            at,
        )
        .expect("approved grant command"),
        secret_hex,
    )
}

async fn pool_from_env(name: &str) -> PgPool {
    let url = std::env::var(name).unwrap_or_else(|_| {
        panic!("{name} is required for this ignored real-PostgreSQL acceptance")
    });
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .unwrap_or_else(|error| panic!("{name} connection failed: {error}"))
}

async fn reset_disposable_support_fixture(write: &PgPool, read: &PgPool) {
    sqlx::query(
        "DELETE FROM erpx_control.outbox_events
         WHERE aggregate_type IN (
             'support_session', 'support_capability', 'incident',
             'support_diagnostics_audit'
         )",
    )
    .execute(write)
    .await
    .expect("clear disposable support outbox fixtures");
    sqlx::query(
        "TRUNCATE TABLE erpx_control.support_tickets,
            erpx_control.support_sessions,
            erpx_control.support_command_idempotency,
            erpx_control.incidents CASCADE",
    )
    .execute(write)
    .await
    .expect("clear disposable support source fixtures");
    sqlx::query(
        "TRUNCATE TABLE erpx_control.support_access_projection,
            erpx_control.support_diagnostics_projection,
            erpx_control.support_projection_event_history,
            erpx_control.incidents CASCADE",
    )
    .execute(read)
    .await
    .expect("clear disposable support read fixtures");
}

#[tokio::test]
#[ignore = "requires disposable ERPX_TEST_CONTROL_WRITE_DATABASE_URL and ERPX_TEST_CONTROL_READ_DATABASE_URL"]
async fn support_workflow_is_atomic_redacted_projected_reconciled_and_rebuildable() {
    let write = pool_from_env("ERPX_TEST_CONTROL_WRITE_DATABASE_URL").await;
    let read = pool_from_env("ERPX_TEST_CONTROL_READ_DATABASE_URL").await;
    migrate_control_plane(&write)
        .await
        .expect("control write migrations");
    migrate_control_read(&read)
        .await
        .expect("control read migrations");

    let support = SupportPersistence::new(write.clone());
    support.apply_migration().await.expect("support migrations");
    let workflow = SupportWorkflowStore::new(write.clone(), read.clone());
    workflow
        .ensure_read_model()
        .await
        .expect("support read adapter");
    reset_disposable_support_fixture(&write, &read).await;

    let now = Utc::now();
    let customer_account_id = Uuid::new_v4();
    let ticket = SupportTicket {
        id: Uuid::new_v4(),
        customer_account_id,
        status: TicketStatus::New,
        severity: "sev2".into(),
        description: "durable support workflow acceptance".into(),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    support
        .insert_ticket(
            &ticket,
            &SupportIdempotencyKey::new("workflow-ticket-acceptance", "ticket-v1"),
        )
        .await
        .expect("ticket source row");
    let scope = Scope::Holding {
        holding_id: Uuid::new_v4(),
    };
    let (command, secret_hex) = grant_fixture(ticket.id, customer_account_id, &scope, now);
    let session_id = command.session.id;
    let capability_id = command.capability.id;
    let receipt = workflow
        .persist_approved_grant(command.clone())
        .await
        .expect("atomic approved grant");
    assert_eq!(receipt.event_ids.len(), 2);
    let replay = workflow
        .persist_approved_grant(command)
        .await
        .expect("idempotent approved grant replay");
    assert!(replay.replayed);
    assert_eq!(replay.event_ids, receipt.event_ids);

    let secret_in_outbox: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.outbox_events
         WHERE aggregate_type IN ('support_session','support_capability')
           AND payload::text LIKE $1",
    )
    .bind(format!("%{secret_hex}%"))
    .fetch_one(&write)
    .await
    .expect("outbox redaction query");
    assert_eq!(secret_in_outbox, 0);
    let secret_hash_length: i32 = sqlx::query_scalar(
        "SELECT octet_length(secret_hash) FROM erpx_control.support_capabilities WHERE id = $1",
    )
    .bind(capability_id)
    .fetch_one(&write)
    .await
    .expect("capability hash");
    assert_eq!(secret_hash_length, 32);

    let mut incident = Incident::new(
        "incident-commander",
        vec!["incident-lead".into()],
        vec!["support-api".into()],
        vec![customer_account_id],
        now,
    )
    .expect("incident");
    let incident_id = incident.id;
    support
        .insert_incident(
            &incident,
            &SupportIdempotencyKey::new("workflow-incident-insert", "incident-v1"),
        )
        .await
        .expect("incident source insert");
    incident
        .add_broadcast(now, "incident-commander", "customer", "redacted broadcast")
        .expect("incident broadcast");
    incident
        .record_root_cause("internal root cause")
        .expect("incident root cause");
    incident
        .transition(IncidentStatus::Identified, now, "incident-commander")
        .expect("incident transition");
    support
        .update_incident(
            &incident,
            1,
            &SupportIdempotencyKey::new("workflow-incident-update", "incident-v2"),
        )
        .await
        .expect("incident source update");

    let mut diagnostics = DiagnosticsSnapshotInput::new(now);
    diagnostics.add_version("worker", "acceptance");
    diagnostics.set_projection_lag_seconds(0);
    let diagnostics = diagnostics
        .build(DiagnosticsLimits::default())
        .expect("allow-listed diagnostics");
    let diagnostics_scope = scope.canonical_key();
    diagnostics
        .persist_audit_snapshot(
            &write,
            &SupportDiagnosticsAuditRequest::new(
                &diagnostics_scope,
                "support-agent-acceptance",
                "support.diagnostics.acceptance",
                Uuid::new_v4(),
                "workflow-diagnostics-acceptance",
                now,
            ),
        )
        .await
        .expect("diagnostics audit source event");

    let projected = workflow
        .rebuild_read_models()
        .await
        .expect("rebuild support read models");
    assert!(
        projected >= 5,
        "grant, incident, and diagnostics source events"
    );
    let access = workflow
        .access_projection(session_id)
        .await
        .expect("access read query")
        .expect("access projection");
    assert_eq!(access.capability_id, Some(capability_id));
    assert_eq!(access.status, "active");
    assert_eq!(access.device_ref, "device-acceptance");
    assert_eq!(access.scope, scope.canonical_key());

    let incident_payload: Value =
        sqlx::query_scalar("SELECT payload FROM erpx_control.incidents WHERE id = $1")
            .bind(incident_id)
            .fetch_one(&read)
            .await
            .expect("incident read payload");
    let incident_text = incident_payload.to_string();
    for forbidden in ["timeline", "broadcasts", "root_cause", "corrective_actions"] {
        assert!(!incident_text.contains(forbidden), "redacted {forbidden}");
    }
    assert!(incident_text.contains("source_event_id"));

    let diagnostics_row = workflow
        .diagnostics_projection(&diagnostics_scope)
        .await
        .expect("diagnostics read query")
        .expect("diagnostics projection");
    assert_eq!(diagnostics_row.scope, diagnostics_scope);
    assert!(!diagnostics_row.payload.to_string().contains("credential"));

    let healthy = workflow.reconcile().await.expect("healthy reconciliation");
    assert!(healthy.healthy, "{healthy:?}");
    assert_eq!(healthy.missing_projection_events, 0);
    assert_eq!(healthy.authoritative_access_mismatches, 0);

    sqlx::query(
        "UPDATE erpx_control.support_access_projection
         SET status = 'tampered' WHERE session_id = $1",
    )
    .bind(session_id)
    .execute(&read)
    .await
    .expect("tamper read projection for reconciliation acceptance");
    let tampered = workflow.reconcile().await.expect("tampered reconciliation");
    assert!(!tampered.healthy, "tampered access row must fail closed");
    assert_eq!(tampered.authoritative_access_mismatches, 1);

    workflow
        .rebuild_read_models()
        .await
        .expect("repair tampered access projection");
    assert_eq!(
        workflow
            .reconcile()
            .await
            .expect("reconciled access projection")
            .authoritative_access_mismatches,
        0
    );

    workflow
        .clear_read_models()
        .await
        .expect("clear read models for rollback");
    assert!(
        !workflow
            .reconcile()
            .await
            .expect("unhealthy reconciliation")
            .healthy
    );
    workflow
        .rebuild_read_models()
        .await
        .expect("rollback rebuild");
    assert!(
        workflow
            .reconcile()
            .await
            .expect("reconciled rebuild")
            .healthy
    );

    let revoked = workflow
        .revoke_grant(
            RevokeSupportGrantCommand::new(
                session_id,
                "support-agent-acceptance",
                "customer requested immediate revocation",
                Utc::now(),
            )
            .expect("revocation command"),
        )
        .await
        .expect("append-only revocation");
    assert!(!revoked.event_ids.is_empty());
    workflow
        .rebuild_read_models()
        .await
        .expect("rebuild revoked access");
    let revoked_access = workflow
        .access_projection(session_id)
        .await
        .expect("revoked access query")
        .expect("revoked access projection");
    assert_eq!(revoked_access.status, "revoked");
    let write_status: String =
        sqlx::query_scalar("SELECT status FROM erpx_control.support_capabilities WHERE id = $1")
            .bind(capability_id)
            .fetch_one(&write)
            .await
            .expect("revoked capability write state");
    assert_eq!(write_status, "revoked");

    let missing_ticket = Uuid::new_v4();
    let (rollback_command, _) = grant_fixture(missing_ticket, customer_account_id, &scope, now);
    let rollback_session_id = rollback_command.session.id;
    let rollback_error = workflow
        .persist_approved_grant(rollback_command)
        .await
        .expect_err("missing external ticket must fail closed");
    assert!(matches!(
        rollback_error,
        SupportWorkflowPersistenceError::NotFound {
            entity: "support_ticket",
            ..
        }
    ));
    let rollback_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM erpx_control.support_sessions WHERE id = $1")
            .bind(rollback_session_id)
            .fetch_one(&write)
            .await
            .expect("rollback session query");
    assert_eq!(rollback_rows, 0);
}
