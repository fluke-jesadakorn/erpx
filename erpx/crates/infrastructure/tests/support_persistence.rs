#[path = "../src/support_persistence.rs"]
#[allow(dead_code)]
mod support_persistence;

use chrono::DateTime;
use chrono::{Duration, Utc};
use erpx_domain::contracts::TenantScope;
use erpx_domain::support::{
    Incident, IncidentStatus, SupportCapability, SupportCapabilityBinding, SupportCapabilitySecret,
    SupportMode, SupportSession, SupportTicket, SupportTicketAssignment, SupportTicketCommand,
    SupportTicketEscalation, TicketStatus,
};
use erpx_infrastructure::{
    ControlPlanePools, SqlxControlPlane, SupportCapabilityAuthorizationRequest,
};
use erpx_migration::migrate_control_plane;
use sqlx::{Row, postgres::PgPoolOptions};
use std::collections::HashSet;
use std::sync::OnceLock;
use support_persistence::{
    IdempotencyKey, MAX_CASCADE_REVOCATION_BATCH_SIZE, SupportApproval, SupportPersistence,
    SupportPersistenceError, SupportTicketListCursor,
};
use uuid::Uuid;

static SUPPORT_DATABASE_TEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn support_database_test_guard() -> tokio::sync::MutexGuard<'static, ()> {
    SUPPORT_DATABASE_TEST_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

#[test]
fn idempotency_keys_reject_empty_values() {
    let empty_key = IdempotencyKey::new("", "hash");
    let empty_hash = IdempotencyKey::new("key", "");
    assert!(matches!(
        support_persistence::validate_idempotency(&empty_key),
        Err(SupportPersistenceError::InvalidData(_))
    ));
    assert!(matches!(
        support_persistence::validate_idempotency(&empty_hash),
        Err(SupportPersistenceError::InvalidData(_))
    ));
}

#[test]
fn support_ticket_list_cursor_and_bounds_are_validated_without_database_access() {
    assert!(support_persistence::validate_ticket_list_page_size(1).is_ok());
    assert!(support_persistence::validate_ticket_list_page_size(100).is_ok());
    assert!(support_persistence::validate_ticket_list_page_size(0).is_err());
    assert!(support_persistence::validate_ticket_list_page_size(101).is_err());
    assert!(support_persistence::validate_ticket_customer_filter(None).is_ok());
    assert!(support_persistence::validate_ticket_customer_filter(Some(Uuid::new_v4())).is_ok());
    assert!(support_persistence::validate_ticket_customer_filter(Some(Uuid::nil())).is_err());

    let timestamp = DateTime::<Utc>::from_timestamp(1_700_000_000, 123_000)
        .expect("valid microsecond timestamp");
    let id = Uuid::new_v4();
    let cursor = SupportTicketListCursor::new(timestamp, id).expect("valid cursor");
    assert_eq!(cursor.updated_at(), timestamp);
    assert_eq!(cursor.id(), id);
    assert!(SupportTicketListCursor::new(timestamp, Uuid::nil()).is_err());
    assert!(
        SupportTicketListCursor::new(
            DateTime::<Utc>::from_timestamp(1_700_000_000, 123)
                .expect("valid nanosecond timestamp"),
            id,
        )
        .is_err()
    );
}

#[test]
fn cascade_revocation_validates_scope_batch_and_reason() {
    let customer = Some(Uuid::new_v4());
    assert!(support_persistence::validate_cascade_scope(None, None, None).is_err());
    assert!(support_persistence::validate_cascade_scope(customer, None, None).is_ok());
    assert!(support_persistence::validate_cascade_scope(None, Some(" "), None).is_err());
    assert!(
        support_persistence::validate_cascade_scope(None, Some(&"a".repeat(256)), None).is_err()
    );

    assert!(
        support_persistence::cascade_revocation_reason(
            erpx_domain::support::SupportCapabilityRevocationReason::CustomerSuspended
        )
        .is_ok()
    );
    assert!(
        support_persistence::cascade_revocation_reason(
            erpx_domain::support::SupportCapabilityRevocationReason::AgentRoleExpired
        )
        .is_ok()
    );
    assert!(
        support_persistence::cascade_revocation_reason(
            erpx_domain::support::SupportCapabilityRevocationReason::ParentRoleExpired
        )
        .is_ok()
    );
    assert!(
        support_persistence::cascade_revocation_reason(
            erpx_domain::support::SupportCapabilityRevocationReason::TicketClosed
        )
        .is_ok()
    );
    assert!(
        support_persistence::cascade_revocation_reason(
            erpx_domain::support::SupportCapabilityRevocationReason::Administrative
        )
        .is_err()
    );
}

#[test]
fn persistence_boundary_rejects_malformed_public_aggregates_before_sqlx() {
    let malformed_ticket = SupportTicket {
        id: Uuid::nil(),
        customer_account_id: Uuid::new_v4(),
        status: TicketStatus::New,
        severity: "sev2".into(),
        description: "invalid identity".into(),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    assert!(matches!(
        support_persistence::validate_ticket(&malformed_ticket),
        Err(SupportPersistenceError::InvalidData(_))
    ));

    let malformed_incident = Incident {
        id: Uuid::new_v4(),
        status: IncidentStatus::Investigating,
        commander_ref: "commander".into(),
        leads: vec!["lead".into()],
        affected_services: vec!["support-api".into()],
        affected_customer_accounts: vec![Uuid::nil()],
        timeline: Vec::new(),
        broadcasts: Vec::new(),
        mitigation: None,
        root_cause: None,
        corrective_actions: Vec::new(),
    };
    assert!(matches!(
        support_persistence::validate_incident(&malformed_incident),
        Err(SupportPersistenceError::InvalidData(_))
    ));
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn support_persistence_enforces_idempotency_hash_only_secrets_and_immutable_bindings() {
    let _database_guard = support_database_test_guard().await;
    let database_url = std::env::var("ERPX_SUPPORT_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_SUPPORT_TEST_DATABASE_URL or ERPX_CONTROL_WRITE_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control migration");
    let persistence = SupportPersistence::new(pool.clone());
    persistence
        .apply_migration()
        .await
        .expect("apply G4 migration");

    let now = Utc::now();
    let ticket = SupportTicket {
        id: Uuid::new_v4(),
        customer_account_id: Uuid::new_v4(),
        status: TicketStatus::New,
        severity: "sev2".into(),
        description: "reconcile a failed projection".into(),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    let ticket_key = IdempotencyKey::new("g4-ticket-1", "ticket-request-v1");
    let first_ticket = persistence
        .insert_ticket(&ticket, &ticket_key)
        .await
        .expect("insert ticket");
    let replayed_ticket = persistence
        .insert_ticket(&ticket, &ticket_key)
        .await
        .expect("replay ticket");
    assert!(!first_ticket.reused_idempotency);
    assert!(replayed_ticket.reused_idempotency);
    assert_eq!(replayed_ticket.version, 1);
    let loaded_ticket = persistence
        .get_ticket(ticket.id)
        .await
        .expect("get ticket")
        .expect("ticket exists");
    assert_eq!(loaded_ticket.value.description, ticket.description);

    let mismatched_session = SupportSession::request(
        ticket.id,
        Uuid::new_v4(),
        "platform-agent-1",
        SupportMode::TenantMutation,
        vec!["operations:write".into()],
        "holding-1/company-1/branch-1",
        "attempt cross-customer support access",
        now,
        Duration::hours(1),
        None,
    )
    .expect("construct mismatched support access request");
    assert!(matches!(
        persistence
            .insert_access_request(
                &mismatched_session,
                &IdempotencyKey::new("g4-access-1", "access-request-v1"),
            )
            .await,
        Err(SupportPersistenceError::AccessRequestTicketBindingDenied)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.support_access_requests WHERE id = $1",
        )
        .bind(mismatched_session.id)
        .fetch_one(&pool)
        .await
        .expect("check rejected access request"),
        0
    );

    let mut triaged_ticket = ticket.clone();
    triaged_ticket
        .transition(TicketStatus::Triaged)
        .expect("triage ticket");
    let updated_ticket = persistence
        .update_ticket(
            &triaged_ticket,
            1,
            &IdempotencyKey::new("g4-ticket-update-1", "ticket-update-v1"),
        )
        .await
        .expect("update ticket");
    assert_eq!(updated_ticket.version, 2);

    let session = SupportSession::request_with_module(
        ticket.id,
        ticket.customer_account_id,
        "platform-agent-1",
        SupportMode::TenantMutation,
        vec!["operations:write".into()],
        "holding-1/company-1/branch-1",
        "operations",
        "repair the approved projection issue",
        now,
        Duration::hours(1),
        None,
    )
    .expect("support access request");
    let session_key = IdempotencyKey::new("g4-access-1", "access-request-v1");
    persistence
        .insert_access_request(&session, &session_key)
        .await
        .expect("insert access request");
    let approved = persistence
        .approve_access_request(
            session.id,
            1,
            &SupportApproval {
                approver_ref: "customer-security-head-1".into(),
                approval_ref: "approval-1".into(),
                approved_at: now + Duration::minutes(1),
            },
            &IdempotencyKey::new("g4-approval-1", "approval-v1"),
        )
        .await
        .expect("approve access request");
    assert_eq!(
        approved.value.approved_by.as_deref(),
        Some("customer-security-head-1")
    );

    let binding = SupportCapabilityBinding::new(
        ticket.customer_account_id,
        "platform-agent-1",
        "device-1",
        session.id,
        ticket.id,
        SupportMode::TenantMutation,
        vec!["operations:write".into()],
        "holding-1/company-1/branch-1",
        "operations",
    )
    .expect("capability binding");
    let (capability, secret) =
        SupportCapability::issue(binding, now + Duration::minutes(2), Duration::minutes(30))
            .expect("capability issue");
    let secret_text = secret.as_hex();
    let metadata = persistence
        .issue_capability(
            &capability,
            &IdempotencyKey::new("g4-capability-1", "cap-v1"),
        )
        .await
        .expect("persist capability");
    assert_eq!(metadata.secret_hash_length, 32);
    assert_eq!(metadata.status, "active");
    assert_eq!(
        persistence
            .get_capability(capability.id)
            .await
            .expect("get capability")
            .expect("capability exists")
            .secret_hash_length,
        32
    );

    let stored = sqlx::query(
        "SELECT secret_hash, payload FROM erpx_control.support_capabilities WHERE id = $1",
    )
    .bind(capability.id)
    .fetch_one(&pool)
    .await
    .expect("read persisted capability");
    let hash: Vec<u8> = stored.try_get("secret_hash").expect("hash bytes");
    let payload: serde_json::Value = stored.try_get("payload").expect("capability payload");
    assert_eq!(hash.len(), 32);
    assert!(!payload.to_string().contains(&secret_text));
    assert!(!format!("{metadata:?}").contains(&secret_text));

    let control = SqlxControlPlane::new(ControlPlanePools {
        write: pool.clone(),
        read: pool.clone(),
    });
    let rehydrated = control
        .load_support_capability(capability.id)
        .await
        .expect("rehydrate persisted capability")
        .expect("persisted capability exists");
    let bearer = SupportCapabilitySecret::from_hex(&secret_text)
        .expect("persisted capability bearer format");
    rehydrated
        .authenticate(&bearer, &capability.binding, now + Duration::minutes(3))
        .expect("rehydrated capability authenticates without storing bearer");
    let capability_request = SupportCapabilityAuthorizationRequest {
        capability_id: capability.id,
        customer_account_id: ticket.customer_account_id,
        agent_ref: "platform-agent-1".into(),
        device_ref: "device-1".into(),
        session_id: session.id,
        ticket_id: ticket.id,
        mode: "tenant_mutation".into(),
        permission: "operations:write".into(),
        scope: "holding-1/company-1/branch-1".into(),
        module: "operations".into(),
        secret_hex: secret_text.clone(),
        at: now + Duration::minutes(3),
    };
    assert!(
        control
            .authorize_support_capability(&capability_request)
            .await
            .expect("capability authorization")
    );
    let mut wrong_secret = capability_request.clone();
    wrong_secret.secret_hex = "00".repeat(32);
    assert!(
        !control
            .authorize_support_capability(&wrong_secret)
            .await
            .expect("wrong capability secret is denied")
    );

    let immutable_scope = sqlx::query(
        "UPDATE erpx_control.support_access_requests SET scope = 'other' WHERE id = $1",
    )
    .bind(session.id)
    .execute(&pool)
    .await;
    assert!(
        immutable_scope.is_err(),
        "approved binding must be immutable"
    );

    let immutable_module = sqlx::query(
        "UPDATE erpx_control.support_capabilities SET module = 'finance' WHERE id = $1",
    )
    .bind(capability.id)
    .execute(&pool)
    .await;
    assert!(
        immutable_module.is_err(),
        "capability binding must be immutable"
    );

    let revoked = persistence
        .revoke_capability(
            capability.id,
            1,
            erpx_domain::support::SupportCapabilityRevocationReason::CustomerRequested,
            now + Duration::minutes(4),
            &IdempotencyKey::new("g4-capability-revoke-1", "cap-revoke-v1"),
        )
        .await
        .expect("revoke capability");
    assert_eq!(revoked.status, "revoked");
    assert!(
        !control
            .authorize_support_capability(&capability_request)
            .await
            .expect("revoked capability is denied")
    );

    let mut incident = Incident::new(
        "incident-commander-1",
        vec!["platform-lead-1".into()],
        vec!["projection-worker".into()],
        vec![ticket.customer_account_id],
        now,
    )
    .expect("incident");
    let incident_key = IdempotencyKey::new("g4-incident-1", "incident-v1");
    persistence
        .insert_incident(&incident, &incident_key)
        .await
        .expect("insert incident");
    let incident_replay = persistence
        .insert_incident(&incident, &incident_key)
        .await
        .expect("replay incident");
    assert!(incident_replay.reused_idempotency);
    incident
        .transition(
            IncidentStatus::Identified,
            now + Duration::minutes(3),
            "incident-commander-1",
        )
        .expect("identify incident");
    let updated_incident = persistence
        .update_incident(
            &incident,
            1,
            &IdempotencyKey::new("g4-incident-update-1", "incident-update-v1"),
        )
        .await
        .expect("update incident");
    assert_eq!(updated_incident.version, 2);
    assert_eq!(updated_incident.value.status, IncidentStatus::Identified);
    assert_eq!(
        persistence
            .get_incident(incident.id)
            .await
            .expect("get incident")
            .expect("incident exists")
            .version,
        2
    );

    sqlx::query("DELETE FROM erpx_control.support_capabilities WHERE id = $1")
        .bind(capability.id)
        .execute(&pool)
        .await
        .expect("cleanup capability");
    sqlx::query("DELETE FROM erpx_control.support_access_requests WHERE id = $1")
        .bind(session.id)
        .execute(&pool)
        .await
        .expect("cleanup access request");
    sqlx::query("DELETE FROM erpx_control.support_tickets WHERE id = $1")
        .bind(ticket.id)
        .execute(&pool)
        .await
        .expect("cleanup ticket");
    sqlx::query("DELETE FROM erpx_control.incidents WHERE id = $1")
        .bind(incident.id)
        .execute(&pool)
        .await
        .expect("cleanup incident");
    sqlx::query(
        "DELETE FROM erpx_control.support_command_idempotency
         WHERE idempotency_key LIKE 'g4-%'",
    )
    .execute(&pool)
    .await
    .expect("cleanup idempotency rows");
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn support_ticket_metadata_commands_are_durable_audience_safe_and_replay_safe() {
    let _database_guard = support_database_test_guard().await;
    let database_url = std::env::var("ERPX_SUPPORT_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_SUPPORT_TEST_DATABASE_URL or ERPX_CONTROL_WRITE_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control migration");
    let persistence = SupportPersistence::new(pool.clone());
    persistence
        .apply_migration()
        .await
        .expect("apply support migration");

    let now = Utc::now();
    let ticket = SupportTicket {
        id: Uuid::new_v4(),
        customer_account_id: Uuid::new_v4(),
        status: TicketStatus::New,
        severity: "sev2".into(),
        description: "projection is delayed".into(),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    persistence
        .insert_ticket(
            &ticket,
            &IdempotencyKey::new("g4-ticket-metadata-insert", "g4-ticket-metadata-insert-v1"),
        )
        .await
        .expect("insert ticket");

    let internal_command = SupportTicketCommand::InternalNote {
        message_id: Uuid::new_v4(),
        author_ref: "agent-1".into(),
        body: "internal-only diagnostic detail".into(),
        created_at: now,
    };
    let mut with_internal_note = ticket.clone();
    with_internal_note
        .apply_command(internal_command.clone(), &TenantScope::Platform, now)
        .expect("apply internal note");
    let first = persistence
        .apply_ticket_command(
            &with_internal_note,
            &internal_command,
            1,
            &IdempotencyKey::new("g4-ticket-metadata-note", "g4-ticket-metadata-note-v1"),
        )
        .await
        .expect("persist internal note");
    assert_eq!(first.version, 2);
    let replay = persistence
        .apply_ticket_command(
            &with_internal_note,
            &internal_command,
            1,
            &IdempotencyKey::new("g4-ticket-metadata-note", "g4-ticket-metadata-note-v1"),
        )
        .await
        .expect("replay internal note");
    assert!(replay.reused_idempotency);
    assert_eq!(replay.version, 2);

    let reply_command = SupportTicketCommand::CustomerReply {
        message_id: Uuid::new_v4(),
        author_ref: "agent-1".into(),
        body: "We are investigating this for you".into(),
        created_at: now,
    };
    let mut with_reply = first.value;
    with_reply
        .apply_command(reply_command.clone(), &TenantScope::Platform, now)
        .expect("apply customer reply");
    let second = persistence
        .apply_ticket_command(
            &with_reply,
            &reply_command,
            2,
            &IdempotencyKey::new("g4-ticket-metadata-reply", "g4-ticket-metadata-reply-v1"),
        )
        .await
        .expect("persist customer reply");

    let assignment = SupportTicketAssignment::new(
        "queue-owner",
        "manager",
        TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        },
        now,
    )
    .expect("assignment");
    let assignment_command = SupportTicketCommand::Assign { assignment };
    let mut with_assignment = second.value;
    with_assignment
        .apply_command(assignment_command.clone(), &TenantScope::Platform, now)
        .expect("apply assignment");
    let third = persistence
        .apply_ticket_command(
            &with_assignment,
            &assignment_command,
            3,
            &IdempotencyKey::new(
                "g4-ticket-metadata-assignment",
                "g4-ticket-metadata-assignment-v1",
            ),
        )
        .await
        .expect("persist assignment");

    let escalation_command = SupportTicketCommand::Escalate {
        escalation: SupportTicketEscalation::new(2, "support-lead", "SLA risk", "manager", now)
            .expect("escalation"),
    };
    let mut with_escalation = third.value;
    with_escalation
        .apply_command(escalation_command.clone(), &TenantScope::Platform, now)
        .expect("apply escalation");
    let fourth = persistence
        .apply_ticket_command(
            &with_escalation,
            &escalation_command,
            4,
            &IdempotencyKey::new(
                "g4-ticket-metadata-escalation",
                "g4-ticket-metadata-escalation-v1",
            ),
        )
        .await
        .expect("persist escalation");

    let sla_command = SupportTicketCommand::SetSlaDeadline {
        deadline: now + Duration::hours(2),
    };
    let mut complete = fourth.value;
    complete
        .apply_command(sla_command.clone(), &TenantScope::Platform, now)
        .expect("apply SLA");
    let persisted = persistence
        .apply_ticket_command(
            &complete,
            &sla_command,
            5,
            &IdempotencyKey::new("g4-ticket-metadata-sla", "g4-ticket-metadata-sla-v1"),
        )
        .await
        .expect("persist SLA");
    assert_eq!(persisted.version, 6);

    let ticket_events = sqlx::query(
        "SELECT event_type, aggregate_version, payload
         FROM erpx_control.outbox_events
         WHERE aggregate_type = 'support_ticket' AND aggregate_id = $1
         ORDER BY aggregate_version",
    )
    .bind(ticket.id)
    .fetch_all(&pool)
    .await
    .expect("load support ticket source events");
    assert_eq!(ticket_events.len(), 6);
    assert_eq!(
        ticket_events
            .iter()
            .map(|row| row.try_get::<i64, _>("aggregate_version").unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
    assert_eq!(
        ticket_events[0].try_get::<String, _>("event_type").unwrap(),
        "support_ticket.created"
    );
    assert_eq!(
        ticket_events[1].try_get::<String, _>("event_type").unwrap(),
        "support_ticket.internal_note_added"
    );
    let source_payload: serde_json::Value = ticket_events[5].try_get("payload").unwrap();
    assert_eq!(source_payload["ticket_id"], serde_json::json!(ticket.id));
    assert_eq!(source_payload["version"], serde_json::json!(6));

    let failed_rebuild = persistence
        .apply_ticket_command(
            &complete,
            &sla_command,
            4,
            &IdempotencyKey::new(
                "g4-ticket-metadata-rollback",
                "g4-ticket-metadata-rollback-v1",
            ),
        )
        .await;
    assert!(matches!(
        failed_rebuild,
        Err(SupportPersistenceError::OptimisticConcurrency { .. })
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM erpx_control.outbox_events
             WHERE aggregate_type = 'support_ticket' AND aggregate_id = $1",
        )
        .bind(ticket.id)
        .fetch_one(&pool)
        .await
        .expect("count rolled-back source event"),
        6
    );
    let sla_rows = sqlx::query(
        "SELECT classification, audience, status, not_before, deadline, expires_at
         FROM erpx_control.support_sla_notifications
         WHERE ticket_id = $1
         ORDER BY classification",
    )
    .bind(ticket.id)
    .fetch_all(&pool)
    .await
    .expect("load transactionally scheduled SLA rows");
    assert_eq!(sla_rows.len(), 2);
    assert_eq!(
        sla_rows[0].try_get::<String, _>("classification").unwrap(),
        "deadline_approaching"
    );
    assert_eq!(
        sla_rows[1].try_get::<String, _>("classification").unwrap(),
        "deadline_breached"
    );
    assert!(sla_rows.iter().all(|row| {
        row.try_get::<String, _>("audience").unwrap() == "internal"
            && row.try_get::<String, _>("status").unwrap() == "pending"
            && row.try_get::<DateTime<Utc>, _>("not_before").unwrap()
                < row.try_get::<DateTime<Utc>, _>("expires_at").unwrap()
            && row.try_get::<DateTime<Utc>, _>("deadline").unwrap() == now + Duration::hours(2)
    }));
    let replayed_sla = persistence
        .apply_ticket_command(
            &complete,
            &sla_command,
            5,
            &IdempotencyKey::new("g4-ticket-metadata-sla", "g4-ticket-metadata-sla-v1"),
        )
        .await
        .expect("replay SLA command");
    assert!(replayed_sla.reused_idempotency);
    let claimant = Uuid::from_u128(99);
    let claims = persistence
        .claim_due_sla_notifications_for_worker(claimant, now, 10)
        .await
        .expect("claim due SLA warning");
    assert_eq!(claims.len(), 1);
    assert_eq!(
        claims[0].0.classification,
        erpx_domain::support::SupportSlaNotificationClassification::DeadlineApproaching
    );
    let acknowledged = persistence
        .mark_sla_notifications_delivered_for_worker_with_route(
            claimant,
            &claims,
            now + Duration::minutes(1),
            "test-provider",
            "in_app",
        )
        .await
        .expect("acknowledge SLA warning");
    assert_eq!(acknowledged, 1);
    let delivered_row = sqlx::query(
        "SELECT status, delivered_at, claimed_at, claim_expires_at, claimed_by, claim_token
         FROM erpx_control.support_sla_notifications
         WHERE notification_key = $1",
    )
    .bind(&claims[0].0.key)
    .fetch_one(&pool)
    .await
    .expect("load acknowledged SLA warning");
    assert_eq!(
        delivered_row.try_get::<String, _>("status").unwrap(),
        "delivered"
    );
    assert!(
        delivered_row
            .try_get::<Option<DateTime<Utc>>, _>("delivered_at")
            .unwrap()
            .is_some()
    );
    assert!(
        delivered_row
            .try_get::<Option<DateTime<Utc>>, _>("claimed_at")
            .unwrap()
            .is_none()
    );
    assert!(
        delivered_row
            .try_get::<Option<DateTime<Utc>>, _>("claim_expires_at")
            .unwrap()
            .is_none()
    );
    assert!(
        delivered_row
            .try_get::<Option<Uuid>, _>("claimed_by")
            .unwrap()
            .is_none()
    );
    assert!(
        delivered_row
            .try_get::<Option<Uuid>, _>("claim_token")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        persistence
            .mark_sla_notifications_delivered_for_worker_with_route(
                claimant,
                &claims,
                now + Duration::minutes(2),
                "test-provider",
                "in_app",
            )
            .await
            .expect("replayed SLA acknowledgement is harmless"),
        0
    );
    let delivery_event = sqlx::query(
        "SELECT event_type, aggregate_type, payload
         FROM erpx_control.outbox_events
         WHERE event_type = 'support_sla_notification.delivered'
           AND payload->>'notification_key' = $1",
    )
    .bind(&claims[0].0.key)
    .fetch_one(&pool)
    .await
    .expect("load durable SLA delivery event");
    assert_eq!(
        delivery_event
            .try_get::<String, _>("aggregate_type")
            .unwrap(),
        "support_sla_notification"
    );
    assert_eq!(
        delivery_event.try_get::<String, _>("event_type").unwrap(),
        "support_sla_notification.delivered"
    );
    let delivery_payload: serde_json::Value = delivery_event.try_get("payload").unwrap();
    assert_eq!(
        delivery_payload["provider_ref"],
        serde_json::Value::String("test-provider".into())
    );
    assert_eq!(
        delivery_payload["channel"],
        serde_json::Value::String("in_app".into())
    );

    let loaded = persistence
        .get_ticket(ticket.id)
        .await
        .expect("load ticket")
        .expect("ticket exists")
        .value;
    let customer_view = loaded.customer_visible_view();
    assert_eq!(customer_view.messages.len(), 1);
    assert_eq!(
        customer_view.messages[0].body,
        "We are investigating this for you"
    );
    assert!(
        !serde_json::to_string(&customer_view)
            .expect("serialize customer view")
            .contains("internal-only diagnostic detail")
    );
    assert!(
        serde_json::to_string(&loaded)
            .expect("serialize aggregate")
            .contains("internal-only diagnostic detail")
    );

    let mut stale = loaded.clone();
    let stale_command = SupportTicketCommand::InternalNote {
        message_id: Uuid::new_v4(),
        author_ref: "agent-2".into(),
        body: "stale command".into(),
        created_at: now,
    };
    stale
        .apply_command(stale_command.clone(), &TenantScope::Platform, now)
        .expect("apply stale fixture command");
    assert!(matches!(
        persistence
            .apply_ticket_command(
                &stale,
                &stale_command,
                1,
                &IdempotencyKey::new("g4-ticket-metadata-stale", "g4-ticket-metadata-stale-v1",),
            )
            .await,
        Err(SupportPersistenceError::OptimisticConcurrency { .. })
    ));

    sqlx::query("DELETE FROM erpx_control.support_sla_notifications WHERE ticket_id = $1")
        .bind(ticket.id)
        .execute(&pool)
        .await
        .expect("cleanup SLA notifications");
    sqlx::query("DELETE FROM erpx_control.support_tickets WHERE id = $1")
        .bind(ticket.id)
        .execute(&pool)
        .await
        .expect("cleanup ticket");
    sqlx::query(
        "DELETE FROM erpx_control.support_command_idempotency
         WHERE idempotency_key LIKE 'g4-ticket-metadata-%'",
    )
    .execute(&pool)
    .await
    .expect("cleanup ticket idempotency rows");
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn incident_list_is_bounded_ordered_and_status_filterable() {
    let _database_guard = support_database_test_guard().await;
    let database_url = std::env::var("ERPX_SUPPORT_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_SUPPORT_TEST_DATABASE_URL or ERPX_CONTROL_WRITE_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control migration");
    let persistence = SupportPersistence::new(pool.clone());
    persistence
        .apply_migration()
        .await
        .expect("apply G4 migration");

    let now = Utc::now();
    let older = Incident::new("commander-older", vec![], vec!["api".into()], vec![], now)
        .expect("older incident");
    let mut newer = Incident::new(
        "commander-newer",
        vec![],
        vec!["worker".into()],
        vec![],
        now,
    )
    .expect("newer incident");
    newer
        .transition(
            IncidentStatus::Identified,
            now + Duration::minutes(1),
            "commander-newer",
        )
        .expect("identify newer incident");
    persistence
        .insert_incident(
            &older,
            &IdempotencyKey::new("g4-incident-list-older", "older-v1"),
        )
        .await
        .expect("insert older incident");
    persistence
        .insert_incident(
            &newer,
            &IdempotencyKey::new("g4-incident-list-newer", "newer-v1"),
        )
        .await
        .expect("insert newer incident");
    sqlx::query(
        "UPDATE erpx_control.incidents
         SET updated_at = $2
         WHERE id = $1",
    )
    .bind(older.id)
    .bind(now)
    .execute(&pool)
    .await
    .expect("set older ordering timestamp");
    sqlx::query(
        "UPDATE erpx_control.incidents
         SET updated_at = $2
         WHERE id = $1",
    )
    .bind(newer.id)
    .bind(now + Duration::minutes(2))
    .execute(&pool)
    .await
    .expect("set newer ordering timestamp");

    assert!(persistence.list_incidents(None, 0).await.is_err());
    assert!(
        persistence
            .list_incidents(None, support_persistence::MAX_INCIDENT_LIST_PAGE_SIZE + 1)
            .await
            .is_err()
    );
    let page = persistence
        .list_incidents(None, 1)
        .await
        .expect("bounded incident page");
    assert_eq!(page.incidents.len(), 1);
    assert!(page.has_more);
    assert_eq!(page.incidents[0].value.id, newer.id);
    assert_eq!(page.incidents[0].version, 1);

    let filtered = persistence
        .list_incidents(Some(IncidentStatus::Identified), 1)
        .await
        .expect("status-filtered incident page");
    assert!(!filtered.has_more);
    assert_eq!(filtered.incidents.len(), 1);
    assert_eq!(filtered.incidents[0].value.id, newer.id);
    assert_eq!(
        filtered.incidents[0].value.status,
        IncidentStatus::Identified
    );

    for incident in [older, newer] {
        sqlx::query("DELETE FROM erpx_control.incidents WHERE id = $1")
            .bind(incident.id)
            .execute(&pool)
            .await
            .expect("cleanup incident");
    }
    sqlx::query(
        "DELETE FROM erpx_control.support_command_idempotency
         WHERE idempotency_key LIKE 'g4-incident-list-%'",
    )
    .execute(&pool)
    .await
    .expect("cleanup incident-list idempotency rows");
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn support_ticket_list_is_bounded_ordered_and_status_filterable() {
    let _database_guard = support_database_test_guard().await;
    let database_url = std::env::var("ERPX_SUPPORT_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_SUPPORT_TEST_DATABASE_URL or ERPX_CONTROL_WRITE_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control migration");
    let persistence = SupportPersistence::new(pool.clone());
    persistence
        .apply_migration()
        .await
        .expect("apply G4 migration");

    let customer_id = Uuid::new_v4();
    let other_customer_id = Uuid::new_v4();
    let make_ticket = |customer_account_id, status, id| SupportTicket {
        id,
        customer_account_id,
        status,
        severity: "sev3".into(),
        description: format!("ticket-{id}"),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    let newest = make_ticket(customer_id, TicketStatus::New, Uuid::new_v4());
    let filtered_out = make_ticket(other_customer_id, TicketStatus::Triaged, Uuid::new_v4());
    let resolved = make_ticket(customer_id, TicketStatus::Resolved, Uuid::new_v4());
    for (index, ticket) in [&newest, &filtered_out, &resolved].into_iter().enumerate() {
        persistence
            .insert_ticket(
                ticket,
                &IdempotencyKey::new(
                    format!("ticket-list-{index}"),
                    format!("ticket-list-hash-{index}"),
                ),
            )
            .await
            .expect("insert ticket");
    }
    let now = Utc::now();
    for (ticket, updated_at) in [
        (&newest, now + Duration::minutes(2)),
        (&filtered_out, now + Duration::minutes(1)),
        (&resolved, now),
    ] {
        sqlx::query("UPDATE erpx_control.support_tickets SET updated_at = $2 WHERE id = $1")
            .bind(ticket.id)
            .bind(updated_at)
            .execute(&pool)
            .await
            .expect("set ticket ordering timestamp");
    }

    assert!(persistence.list_tickets(None, None, 0).await.is_err());
    assert!(
        persistence
            .list_tickets(
                None,
                None,
                support_persistence::MAX_SUPPORT_TICKET_LIST_PAGE_SIZE + 1,
            )
            .await
            .is_err()
    );
    let page = persistence
        .list_tickets(None, Some(customer_id), 1)
        .await
        .expect("customer-filtered ticket page");
    assert_eq!(page.tickets.len(), 1);
    assert!(page.has_more);
    assert_eq!(page.tickets[0].value.id, newest.id);

    let filtered = persistence
        .list_tickets(Some(TicketStatus::Resolved), Some(customer_id), 1)
        .await
        .expect("status-filtered ticket page");
    assert!(!filtered.has_more);
    assert_eq!(filtered.tickets.len(), 1);
    assert_eq!(filtered.tickets[0].value.id, resolved.id);

    for ticket in [newest, filtered_out, resolved] {
        sqlx::query("DELETE FROM erpx_control.support_tickets WHERE id = $1")
            .bind(ticket.id)
            .execute(&pool)
            .await
            .expect("cleanup ticket");
    }
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn support_ticket_list_cursor_is_exclusive_deterministic_filtered_and_customer_scoped() {
    let _database_guard = support_database_test_guard().await;
    let database_url = std::env::var("ERPX_SUPPORT_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_SUPPORT_TEST_DATABASE_URL or ERPX_CONTROL_WRITE_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control migration");
    let persistence = SupportPersistence::new(pool.clone());
    persistence
        .apply_migration()
        .await
        .expect("apply G4 migration");

    let customer_id = Uuid::new_v4();
    let other_customer_id = Uuid::new_v4();
    let newest = Uuid::from_u128(0x00000000000000000000000000000004);
    let tie_low = Uuid::from_u128(0x00000000000000000000000000000002);
    let tie_high = Uuid::from_u128(0x00000000000000000000000000000003);
    let oldest = Uuid::from_u128(0x00000000000000000000000000000001);
    let other_customer_ticket = Uuid::from_u128(0x00000000000000000000000000000005);
    let resolved = Uuid::from_u128(0x00000000000000000000000000000006);
    let make_ticket = |id, customer_account_id, status| SupportTicket {
        id,
        customer_account_id,
        status,
        severity: "sev3".into(),
        description: format!("cursor-ticket-{id}"),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    let tickets = [
        make_ticket(newest, customer_id, TicketStatus::New),
        make_ticket(tie_low, customer_id, TicketStatus::New),
        make_ticket(tie_high, customer_id, TicketStatus::New),
        make_ticket(oldest, customer_id, TicketStatus::New),
        make_ticket(other_customer_ticket, other_customer_id, TicketStatus::New),
        make_ticket(resolved, customer_id, TicketStatus::Resolved),
    ];
    for ticket in &tickets {
        persistence
            .insert_ticket(
                ticket,
                &IdempotencyKey::new(
                    format!("g4-cursor-ticket-{}", ticket.id),
                    format!("g4-cursor-hash-{}", ticket.id),
                ),
            )
            .await
            .expect("insert cursor test ticket");
    }

    let base = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).expect("valid base timestamp");
    for (ticket, updated_at) in [
        (&newest, base + Duration::seconds(3)),
        (&tie_low, base + Duration::seconds(2)),
        (&tie_high, base + Duration::seconds(2)),
        (&oldest, base + Duration::seconds(1)),
        (&other_customer_ticket, base + Duration::seconds(4)),
        (&resolved, base + Duration::seconds(5)),
    ] {
        sqlx::query("UPDATE erpx_control.support_tickets SET updated_at = $2 WHERE id = $1")
            .bind(ticket)
            .bind(updated_at)
            .execute(&pool)
            .await
            .expect("set cursor ordering timestamp");
    }

    let first = persistence
        .list_tickets(Some(TicketStatus::New), Some(customer_id), 2)
        .await
        .expect("read first cursor page");
    assert!(first.has_more);
    assert_eq!(
        first
            .tickets
            .iter()
            .map(|ticket| ticket.value.id)
            .collect::<Vec<_>>(),
        vec![newest, tie_high]
    );
    assert!(
        first
            .tickets
            .iter()
            .all(|ticket| ticket.value.customer_account_id == customer_id)
    );
    let cursor = first.next_cursor.clone().expect("next cursor");
    assert_eq!(cursor.id(), tie_high);
    assert_eq!(cursor.updated_at(), base + Duration::seconds(2));

    let repeated_first = persistence
        .list_tickets(Some(TicketStatus::New), Some(customer_id), 2)
        .await
        .expect("repeat first cursor page");
    assert_eq!(repeated_first, first);

    let second = persistence
        .list_tickets_after(Some(TicketStatus::New), Some(customer_id), 2, Some(&cursor))
        .await
        .expect("read continuation cursor page");
    assert!(!second.has_more);
    assert_eq!(
        second
            .tickets
            .iter()
            .map(|ticket| ticket.value.id)
            .collect::<Vec<_>>(),
        vec![tie_low, oldest]
    );
    assert!(
        second
            .tickets
            .iter()
            .all(|ticket| ticket.value.customer_account_id == customer_id)
    );
    assert!(
        !second
            .tickets
            .iter()
            .any(|ticket| ticket.value.id == cursor.id())
    );

    for ticket in tickets {
        sqlx::query("DELETE FROM erpx_control.support_tickets WHERE id = $1")
            .bind(ticket.id)
            .execute(&pool)
            .await
            .expect("cleanup cursor test ticket");
    }
    sqlx::query(
        "DELETE FROM erpx_control.support_command_idempotency
         WHERE idempotency_key LIKE 'g4-cursor-ticket-%'",
    )
    .execute(&pool)
    .await
    .expect("cleanup cursor test idempotency rows");
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn expiration_sweep_is_bounded_deterministic_and_retry_safe() {
    let _database_guard = support_database_test_guard().await;
    let database_url = std::env::var("ERPX_SUPPORT_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_SUPPORT_TEST_DATABASE_URL or ERPX_CONTROL_WRITE_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control migration");
    let persistence = SupportPersistence::new(pool.clone());
    persistence
        .apply_migration()
        .await
        .expect("apply G4 migration");

    let sweep_now = Utc::now() + Duration::hours(1);
    let before = issue_sweep_fixture(
        &persistence,
        sweep_now - Duration::minutes(1),
        Duration::minutes(2),
        "before",
    )
    .await;
    let exactly = issue_sweep_fixture(
        &persistence,
        sweep_now - Duration::minutes(2),
        Duration::minutes(2),
        "exactly",
    )
    .await;
    let after = issue_sweep_fixture(
        &persistence,
        sweep_now - Duration::minutes(3),
        Duration::minutes(2),
        "after",
    )
    .await;
    let after_again = issue_sweep_fixture(
        &persistence,
        sweep_now - Duration::minutes(4),
        Duration::minutes(2),
        "after-again",
    )
    .await;
    let already_revoked = issue_sweep_fixture(
        &persistence,
        sweep_now - Duration::minutes(5),
        Duration::minutes(2),
        "already-revoked",
    )
    .await;

    let manually_revoked_at = sweep_now - Duration::minutes(1);
    let manually_revoked = persistence
        .revoke_capability(
            already_revoked.id,
            1,
            erpx_domain::support::SupportCapabilityRevocationReason::CustomerRequested,
            manually_revoked_at,
            &IdempotencyKey::new("g4-expiry-manual-revoke", "manual-revoke-v1"),
        )
        .await
        .expect("manually revoke fixture");
    assert_eq!(manually_revoked.version, 2);
    assert_eq!(
        manually_revoked.revocation_reason.as_deref(),
        Some("customer_requested")
    );

    assert!(
        persistence
            .sweep_expired_capabilities(sweep_now, 0)
            .await
            .is_err()
    );
    assert!(
        persistence
            .sweep_expired_capabilities(
                sweep_now,
                support_persistence::MAX_EXPIRATION_SWEEP_BATCH_SIZE + 1,
            )
            .await
            .is_err()
    );

    let first_batch = persistence
        .sweep_expired_capabilities(sweep_now, 2)
        .await
        .expect("first bounded expiry sweep");
    assert_eq!(first_batch.len(), 2);
    assert!(first_batch.iter().all(|metadata| {
        metadata.status == "revoked"
            && metadata.revocation_reason.as_deref() == Some("expired")
            && metadata.revoked_at == Some(sweep_now)
            && metadata.version == 2
            && metadata.secret_hash_length == 32
    }));

    let second_batch = persistence
        .sweep_expired_capabilities(sweep_now, 2)
        .await
        .expect("second bounded expiry sweep");
    assert_eq!(second_batch.len(), 1);
    assert_eq!(
        second_batch[0].revocation_reason.as_deref(),
        Some("expired")
    );
    assert_eq!(second_batch[0].version, 2);
    assert_eq!(second_batch[0].secret_hash_length, 32);

    let repeated = persistence
        .sweep_expired_capabilities(sweep_now, 2)
        .await
        .expect("retry expiry sweep");
    assert!(repeated.is_empty(), "completed rows must be retry no-ops");

    let expired_ids: HashSet<_> = first_batch
        .iter()
        .chain(second_batch.iter())
        .map(|metadata| metadata.id)
        .collect();
    assert_eq!(expired_ids.len(), 3);
    assert!(expired_ids.contains(&exactly.id));
    assert!(expired_ids.contains(&after.id));
    assert!(expired_ids.contains(&after_again.id));

    let before_metadata = persistence
        .get_capability(before.id)
        .await
        .expect("load before-expiry capability")
        .expect("before-expiry capability exists");
    assert_eq!(before_metadata.status, "active");
    assert_eq!(before_metadata.version, 1);
    assert!(before_metadata.revoked_at.is_none());
    assert!(before_metadata.revocation_reason.is_none());
    assert_eq!(before_metadata.secret_hash_length, 32);

    let revoked_metadata = persistence
        .get_capability(already_revoked.id)
        .await
        .expect("load manually revoked capability")
        .expect("manually revoked capability exists");
    assert_eq!(revoked_metadata.status, "revoked");
    assert_eq!(revoked_metadata.version, 2);
    assert_eq!(revoked_metadata.revoked_at, Some(manually_revoked_at));
    assert_eq!(
        revoked_metadata.revocation_reason.as_deref(),
        Some("customer_requested")
    );
    assert_eq!(revoked_metadata.secret_hash_length, 32);

    for capability in [before, exactly, after, after_again, already_revoked] {
        sqlx::query("DELETE FROM erpx_control.support_capabilities WHERE id = $1")
            .bind(capability.id)
            .execute(&pool)
            .await
            .expect("cleanup capability");
        sqlx::query("DELETE FROM erpx_control.support_access_requests WHERE id = $1")
            .bind(capability.binding.session_id)
            .execute(&pool)
            .await
            .expect("cleanup access request");
        sqlx::query("DELETE FROM erpx_control.support_tickets WHERE id = $1")
            .bind(capability.binding.ticket_id)
            .execute(&pool)
            .await
            .expect("cleanup ticket");
    }
    sqlx::query(
        "DELETE FROM erpx_control.support_command_idempotency
         WHERE idempotency_key LIKE 'g4-expiry-%'",
    )
    .execute(&pool)
    .await
    .expect("cleanup sweep idempotency rows");
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn cascade_revocation_is_bounded_scoped_and_retry_safe() {
    let _database_guard = support_database_test_guard().await;
    let database_url = std::env::var("ERPX_SUPPORT_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_SUPPORT_TEST_DATABASE_URL or ERPX_CONTROL_WRITE_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control migration");
    let persistence = SupportPersistence::new(pool.clone());
    persistence
        .apply_migration()
        .await
        .expect("apply G4 migration");

    let customer_one = Uuid::new_v4();
    let customer_two = Uuid::new_v4();
    let ticket_one = cascade_ticket(&persistence, customer_one, "g4-cascade-ticket-one").await;
    let ticket_two = cascade_ticket(&persistence, customer_one, "g4-cascade-ticket-two").await;
    let ticket_three = cascade_ticket(&persistence, customer_two, "g4-cascade-ticket-three").await;
    let ticket_four = cascade_ticket(&persistence, customer_two, "g4-cascade-ticket-four").await;
    let base = Utc::now();

    let customer_capability_one = issue_cascade_fixture(
        &persistence,
        &ticket_one,
        "agent-customer",
        base,
        "customer-one",
    )
    .await;
    let customer_capability_two = issue_cascade_fixture(
        &persistence,
        &ticket_two,
        "agent-customer",
        base,
        "customer-two",
    )
    .await;
    let parent_capability = issue_cascade_fixture(
        &persistence,
        &ticket_three,
        "agent-parent",
        base,
        "parent-role",
    )
    .await;
    let ticket_capability = issue_cascade_fixture(
        &persistence,
        &ticket_four,
        "agent-ticket",
        base,
        "ticket-closed",
    )
    .await;
    let agent_capability = issue_cascade_fixture(
        &persistence,
        &ticket_three,
        "agent-expired",
        base,
        "agent-role",
    )
    .await;

    assert!(
        persistence
            .revoke_active_capabilities_by_customer(customer_one, base + Duration::minutes(1), 0,)
            .await
            .is_err()
    );
    assert!(
        persistence
            .revoke_active_capabilities_by_customer(
                customer_one,
                base + Duration::minutes(1),
                MAX_CASCADE_REVOCATION_BATCH_SIZE + 1,
            )
            .await
            .is_err()
    );

    let customer_revoked_at = base + Duration::minutes(2);
    let customer_revoked = persistence
        .revoke_active_capabilities_by_customer(customer_one, customer_revoked_at, 10)
        .await
        .expect("revoke customer capabilities");
    assert_eq!(customer_revoked.len(), 2);
    assert_eq!(
        customer_revoked
            .iter()
            .map(|metadata| metadata.id)
            .collect::<HashSet<_>>(),
        HashSet::from([customer_capability_one.id, customer_capability_two.id,])
    );
    assert!(customer_revoked.iter().all(|metadata| {
        metadata.status == "revoked"
            && metadata.version == 2
            && metadata.revoked_at == Some(customer_revoked_at)
            && metadata.revocation_reason.as_deref() == Some("customer_suspended")
            && metadata.secret_hash_length == 32
    }));
    let customer_retry = persistence
        .revoke_active_capabilities_by_customer(customer_one, customer_revoked_at, 10)
        .await
        .expect("retry customer cascade");
    assert!(customer_retry.is_empty());

    let parent_revoked_at = base + Duration::minutes(3);
    let parent_revoked = persistence
        .revoke_active_capabilities_by_parent_role("agent-parent", parent_revoked_at, 10)
        .await
        .expect("revoke parent-role capabilities");
    assert_eq!(parent_revoked.len(), 1);
    assert_eq!(parent_revoked[0].id, parent_capability.id);
    assert_eq!(
        parent_revoked[0].revocation_reason.as_deref(),
        Some("parent_role_expired")
    );
    assert!(
        persistence
            .revoke_active_capabilities_by_parent_role("agent-parent", parent_revoked_at, 10)
            .await
            .expect("retry parent-role cascade")
            .is_empty()
    );

    let agent_revoked_at = base + Duration::minutes(4);
    let agent_revoked = persistence
        .revoke_active_capabilities_by_agent("agent-expired", agent_revoked_at, 10)
        .await
        .expect("revoke agent capabilities");
    assert_eq!(agent_revoked.len(), 1);
    assert_eq!(agent_revoked[0].id, agent_capability.id);
    assert_eq!(
        agent_revoked[0].revocation_reason.as_deref(),
        Some("agent_role_expired")
    );

    let ticket_revoked_at = base + Duration::minutes(5);
    let ticket_revoked = persistence
        .revoke_active_capabilities_by_ticket(ticket_four.id, ticket_revoked_at, 10)
        .await
        .expect("revoke ticket capabilities");
    assert_eq!(ticket_revoked.len(), 1);
    assert_eq!(ticket_revoked[0].id, ticket_capability.id);
    assert_eq!(
        ticket_revoked[0].revocation_reason.as_deref(),
        Some("ticket_closed")
    );

    let revocation_event_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.outbox_events
         WHERE event_type = 'support_capability.revoked'
           AND aggregate_type = 'support_capability'
           AND aggregate_id = ANY($1)",
    )
    .bind(vec![
        customer_capability_one.id,
        customer_capability_two.id,
        parent_capability.id,
        ticket_capability.id,
        agent_capability.id,
    ])
    .fetch_one(&pool)
    .await
    .expect("count cascade revocation events");
    assert_eq!(revocation_event_count, 5);

    let wrong_scope = persistence
        .revoke_active_capabilities_by_scope(
            Some(customer_two),
            Some("agent-ticket"),
            Some(ticket_three.id),
            erpx_domain::support::SupportCapabilityRevocationReason::TicketClosed,
            base + Duration::minutes(6),
            10,
        )
        .await
        .expect("wrong combined scope is a no-op");
    assert!(wrong_scope.is_empty());

    for capability in [
        customer_capability_one,
        customer_capability_two,
        parent_capability,
        ticket_capability,
        agent_capability,
    ] {
        sqlx::query("DELETE FROM erpx_control.support_capabilities WHERE id = $1")
            .bind(capability.id)
            .execute(&pool)
            .await
            .expect("cleanup cascade capability");
        sqlx::query("DELETE FROM erpx_control.support_access_requests WHERE id = $1")
            .bind(capability.binding.session_id)
            .execute(&pool)
            .await
            .expect("cleanup cascade access request");
    }
    for ticket in [ticket_one, ticket_two, ticket_three, ticket_four] {
        sqlx::query("DELETE FROM erpx_control.support_tickets WHERE id = $1")
            .bind(ticket.id)
            .execute(&pool)
            .await
            .expect("cleanup cascade ticket");
    }
    sqlx::query(
        "DELETE FROM erpx_control.support_command_idempotency
         WHERE idempotency_key LIKE 'g4-cascade-%'",
    )
    .execute(&pool)
    .await
    .expect("cleanup cascade idempotency rows");
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn control_plane_ticket_revocation_drains_active_rows_and_is_retry_safe() {
    let _database_guard = support_database_test_guard().await;
    let database_url = std::env::var("ERPX_SUPPORT_TEST_DATABASE_URL")
        .or_else(|_| std::env::var("ERPX_CONTROL_WRITE_DATABASE_URL"))
        .expect("ERPX_SUPPORT_TEST_DATABASE_URL or ERPX_CONTROL_WRITE_DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect disposable database");
    migrate_control_plane(&pool)
        .await
        .expect("apply control migration");
    let persistence = SupportPersistence::new(pool.clone());
    persistence
        .apply_migration()
        .await
        .expect("apply G4 migration");

    let customer = Uuid::new_v4();
    let ticket = cascade_ticket(&persistence, customer, "g4-control-ticket-revoke").await;
    let capability = issue_cascade_fixture(
        &persistence,
        &ticket,
        "agent-control-ticket",
        Utc::now(),
        "control-ticket-revoke",
    )
    .await;
    let control = SqlxControlPlane::new(ControlPlanePools {
        write: pool.clone(),
        read: pool.clone(),
    });
    let revoked_at = Utc::now() + Duration::minutes(1);
    assert_eq!(
        control
            .revoke_support_capabilities_for_ticket(ticket.id, revoked_at)
            .await
            .expect("revoke active ticket capabilities"),
        1
    );
    assert_eq!(
        control
            .revoke_support_capabilities_for_ticket(ticket.id, revoked_at)
            .await
            .expect("retry ticket capability revocation"),
        0
    );
    let metadata = persistence
        .get_capability(capability.id)
        .await
        .expect("load revoked capability")
        .expect("revoked capability exists");
    assert_eq!(metadata.status, "revoked");
    assert_eq!(metadata.version, 2);
    assert_eq!(metadata.revoked_at, Some(revoked_at));
    assert_eq!(metadata.revocation_reason.as_deref(), Some("ticket_closed"));

    sqlx::query("DELETE FROM erpx_control.support_capabilities WHERE id = $1")
        .bind(capability.id)
        .execute(&pool)
        .await
        .expect("cleanup control ticket capability");
    sqlx::query("DELETE FROM erpx_control.support_access_requests WHERE id = $1")
        .bind(capability.binding.session_id)
        .execute(&pool)
        .await
        .expect("cleanup control ticket access request");
    sqlx::query("DELETE FROM erpx_control.support_tickets WHERE id = $1")
        .bind(ticket.id)
        .execute(&pool)
        .await
        .expect("cleanup control ticket");
    sqlx::query(
        "DELETE FROM erpx_control.support_command_idempotency
         WHERE idempotency_key LIKE 'g4-control-ticket-%'",
    )
    .execute(&pool)
    .await
    .expect("cleanup control ticket idempotency rows");
}

async fn cascade_ticket(
    persistence: &SupportPersistence,
    customer_account_id: Uuid,
    key_suffix: &str,
) -> SupportTicket {
    let ticket = SupportTicket {
        id: Uuid::new_v4(),
        customer_account_id,
        status: TicketStatus::New,
        severity: "sev2".into(),
        description: format!("cascade revocation {key_suffix}"),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    persistence
        .insert_ticket(
            &ticket,
            &IdempotencyKey::new(key_suffix, format!("{key_suffix}-request-v1")),
        )
        .await
        .expect("insert cascade ticket");
    ticket
}

async fn issue_cascade_fixture(
    persistence: &SupportPersistence,
    ticket: &SupportTicket,
    agent_ref: &str,
    issued_at: DateTime<Utc>,
    key_suffix: &str,
) -> SupportCapability {
    let session = SupportSession::request(
        ticket.id,
        ticket.customer_account_id,
        agent_ref,
        SupportMode::DiagnosticRead,
        vec!["support:read".into()],
        "holding-1/company-1/branch-1",
        format!("cascade fixture {key_suffix}"),
        issued_at - Duration::minutes(1),
        Duration::hours(2),
        None,
    )
    .expect("request cascade access");
    persistence
        .insert_access_request(
            &session,
            &IdempotencyKey::new(
                format!("g4-cascade-access-{key_suffix}"),
                format!("access-{key_suffix}-v1"),
            ),
        )
        .await
        .expect("insert cascade access request");
    persistence
        .approve_access_request(
            session.id,
            1,
            &SupportApproval {
                approver_ref: "customer-security-head-cascade".into(),
                approval_ref: format!("g4-cascade-approval-{key_suffix}"),
                approved_at: issued_at,
            },
            &IdempotencyKey::new(
                format!("g4-cascade-approval-command-{key_suffix}"),
                format!("approval-{key_suffix}-v1"),
            ),
        )
        .await
        .expect("approve cascade access request");

    let binding = SupportCapabilityBinding::new(
        ticket.customer_account_id,
        agent_ref,
        format!("device-{key_suffix}"),
        session.id,
        ticket.id,
        SupportMode::DiagnosticRead,
        vec!["support:read".into()],
        "holding-1/company-1/branch-1",
        "support",
    )
    .expect("create cascade capability binding");
    let (capability, _) = SupportCapability::issue(binding, issued_at, Duration::minutes(30))
        .expect("issue cascade capability");
    persistence
        .issue_capability(
            &capability,
            &IdempotencyKey::new(
                format!("g4-cascade-capability-{key_suffix}"),
                format!("capability-{key_suffix}-v1"),
            ),
        )
        .await
        .expect("persist cascade capability");
    capability
}

async fn issue_sweep_fixture(
    persistence: &SupportPersistence,
    issued_at: DateTime<Utc>,
    duration: Duration,
    key_suffix: &str,
) -> SupportCapability {
    let ticket = SupportTicket {
        id: Uuid::new_v4(),
        customer_account_id: Uuid::new_v4(),
        status: TicketStatus::New,
        severity: "sev3".into(),
        description: format!("expiration sweep {key_suffix}"),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    persistence
        .insert_ticket(
            &ticket,
            &IdempotencyKey::new(
                format!("g4-expiry-ticket-{key_suffix}"),
                format!("ticket-{key_suffix}-v1"),
            ),
        )
        .await
        .expect("insert sweep ticket");

    let session = SupportSession::request(
        ticket.id,
        ticket.customer_account_id,
        "platform-agent-expiry",
        SupportMode::DiagnosticRead,
        vec!["support:read".into()],
        "holding-1/company-1/branch-1",
        format!("expiration sweep {key_suffix}"),
        issued_at - Duration::minutes(1),
        Duration::hours(2),
        None,
    )
    .expect("request sweep access");
    persistence
        .insert_access_request(
            &session,
            &IdempotencyKey::new(
                format!("g4-expiry-access-{key_suffix}"),
                format!("access-{key_suffix}-v1"),
            ),
        )
        .await
        .expect("insert sweep access request");
    persistence
        .approve_access_request(
            session.id,
            1,
            &SupportApproval {
                approver_ref: "customer-security-head-expiry".into(),
                approval_ref: format!("approval-{key_suffix}"),
                approved_at: issued_at,
            },
            &IdempotencyKey::new(
                format!("g4-expiry-approval-{key_suffix}"),
                format!("approval-{key_suffix}-v1"),
            ),
        )
        .await
        .expect("approve sweep access request");

    let binding = SupportCapabilityBinding::new(
        ticket.customer_account_id,
        "platform-agent-expiry",
        format!("device-{key_suffix}"),
        session.id,
        ticket.id,
        SupportMode::DiagnosticRead,
        vec!["support:read".into()],
        "holding-1/company-1/branch-1",
        "support",
    )
    .expect("create sweep capability binding");
    let (capability, _) =
        SupportCapability::issue(binding, issued_at, duration).expect("issue sweep capability");
    persistence
        .issue_capability(
            &capability,
            &IdempotencyKey::new(
                format!("g4-expiry-capability-{key_suffix}"),
                format!("capability-{key_suffix}-v1"),
            ),
        )
        .await
        .expect("persist sweep capability");
    capability
}
