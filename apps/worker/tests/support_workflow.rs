//! Real-PostgreSQL worker acceptance for the opt-in G4 support projector.

use std::sync::Arc;

use chrono::{Duration, Utc};
use erpx_domain::{
    contracts::AuthorityScope as Scope,
    support::{
        SupportCapability, SupportCapabilityBinding, SupportDeviceSessionBinding, SupportMode,
        SupportSession, SupportTicket, TicketStatus,
    },
};
use erpx_infrastructure::{
    ControlPlanePools, SqlxControlPlane, SupportIdempotencyKey, SupportPersistence,
    SupportWorkflowStore,
};
use erpx_migration::{migrate_control_plane, migrate_control_read};
use erpx_worker::SupportWorkflowOutboxHandler;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires disposable ERPX_TEST_CONTROL_WRITE_DATABASE_URL and ERPX_TEST_CONTROL_READ_DATABASE_URL"]
async fn support_worker_claims_projects_and_checkpoints_support_events() {
    let write_url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required");
    let read_url = std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
        .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required");
    let write = PgPoolOptions::new()
        .max_connections(5)
        .connect(&write_url)
        .await
        .expect("write database");
    let read = PgPoolOptions::new()
        .max_connections(5)
        .connect(&read_url)
        .await
        .expect("read database");
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
        .expect("support read model");

    let now = Utc::now();
    let customer_account_id = Uuid::new_v4();
    let ticket = SupportTicket {
        id: Uuid::new_v4(),
        customer_account_id,
        status: TicketStatus::New,
        severity: "sev2".into(),
        description: "worker support projection acceptance".into(),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    support
        .insert_ticket(
            &ticket,
            &SupportIdempotencyKey::new("worker-support-ticket", "ticket-v1"),
        )
        .await
        .expect("ticket source");
    let scope = Scope::Holding {
        holding_id: Uuid::new_v4(),
    };
    let mut session = SupportSession::request_with_module(
        ticket.id,
        customer_account_id,
        "worker-support-agent",
        SupportMode::DiagnosticRead,
        vec!["ticket.read".into()],
        scope.canonical_key(),
        "support",
        "worker projector acceptance",
        now - Duration::minutes(2),
        Duration::minutes(30),
        None,
    )
    .expect("session");
    session
        .approve("worker-customer-approver", now - Duration::minutes(1))
        .expect("approval");
    let device = SupportDeviceSessionBinding::new(
        session.id,
        "worker-support-agent",
        "worker-device",
        None,
        now - Duration::minutes(1),
    )
    .expect("device");
    let binding = SupportCapabilityBinding::new(
        customer_account_id,
        "worker-support-agent",
        "worker-device",
        session.id,
        ticket.id,
        SupportMode::DiagnosticRead,
        vec!["ticket.read".into()],
        scope.canonical_key(),
        "support",
    )
    .expect("binding");
    let (capability, _) =
        SupportCapability::issue(binding, now - Duration::seconds(30), Duration::minutes(10))
            .expect("capability");
    let command = erpx_application::PersistApprovedSupportGrantCommand::new(
        session,
        device,
        capability,
        scope.clone(),
        scope,
        true,
        "worker-approval-1",
        now,
    )
    .expect("approved grant command");
    let session_id = command.session.id;
    workflow
        .persist_approved_grant(command)
        .await
        .expect("support source events");

    let control = Arc::new(SqlxControlPlane::new(ControlPlanePools { write, read }));
    let handler = SupportWorkflowOutboxHandler::new(control, workflow.clone(), Uuid::new_v4(), 100);
    let report = handler.run_once().await.expect("worker pass");
    assert!(report.claimed >= 2);
    assert!(report.published >= 2);
    let projection = workflow
        .access_projection(session_id)
        .await
        .expect("read projection query")
        .expect("read projection");
    assert_eq!(projection.status, "active");
    assert_eq!(projection.device_ref, "worker-device");
}
