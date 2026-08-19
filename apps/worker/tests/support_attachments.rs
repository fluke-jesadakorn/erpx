use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    contracts::AuthorityScope,
    support::{
        SupportAttachment, SupportAttachmentUpload, SupportAttachmentVisibility, SupportCapability,
        SupportCapabilityBinding, SupportMode, SupportSlaNotification, SupportTicket, TicketStatus,
    },
};
use erpx_infrastructure::{
    PersistedSupportAttachment, SupportIdempotencyKey, SupportPersistence, SupportPersistenceError,
};
use erpx_migration::migrate_control_plane;
use erpx_worker::{
    NamedWorkerHandler, SupportAttachmentExpiryHandler, SupportAttachmentExpiryStore,
    SupportSlaNotificationHandler, SupportSlaNotificationSource,
};
use sqlx::{Row, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Clone)]
struct ExpiryStore {
    rows: Vec<PersistedSupportAttachment>,
}

#[async_trait]
impl SupportAttachmentExpiryStore for ExpiryStore {
    async fn sweep_expired_attachments(
        &self,
        _at: DateTime<Utc>,
        _batch_size: u32,
    ) -> Result<Vec<PersistedSupportAttachment>, SupportPersistenceError> {
        Ok(self.rows.clone())
    }
}

#[derive(Clone, Default)]
struct NotificationSource {
    claims: Arc<AtomicUsize>,
}

#[async_trait]
impl SupportSlaNotificationSource for NotificationSource {
    fn supports_durable_claim_identity(&self) -> bool {
        false
    }

    async fn claim_due_sla_notifications(
        &self,
        _claimant_id: Uuid,
        _at: DateTime<Utc>,
        _batch_size: u32,
    ) -> Result<
        Vec<(SupportSlaNotification, Uuid, DateTime<Utc>, DateTime<Utc>)>,
        SupportPersistenceError,
    > {
        self.claims.fetch_add(1, Ordering::Relaxed);
        Ok(Vec::new())
    }

    async fn acknowledge_sla_notifications(
        &self,
        _claimant_id: Uuid,
        _claims: &[(SupportSlaNotification, Uuid, DateTime<Utc>, DateTime<Utc>)],
        _at: DateTime<Utc>,
    ) -> Result<u64, SupportPersistenceError> {
        Ok(0)
    }
}

fn attachment(at: DateTime<Utc>, scope: AuthorityScope) -> PersistedSupportAttachment {
    PersistedSupportAttachment {
        value: SupportAttachment {
            id: Uuid::from_u128(10),
            ticket_id: Uuid::from_u128(11),
            customer_account_id: Uuid::from_u128(12),
            object_ref: "support/ticket-11/object-10.bin".into(),
            content_type: "application/octet-stream".into(),
            content_size_bytes: 1,
            sha256_hex: "ab".repeat(32),
            visibility: SupportAttachmentVisibility::Internal,
            uploader_ref: "agent".into(),
            scope,
            uploaded_at: at - Duration::hours(1),
            expires_at: at + Duration::hours(1),
        },
        status: "active".into(),
        version: 1,
        ticket_version: 1,
        reused_idempotency: false,
    }
}

#[tokio::test]
async fn expiry_worker_refuses_to_acknowledge_non_terminal_store_rows() {
    let at = Utc::now();
    let mut row = attachment(at, AuthorityScope::Platform);
    row.status = "active".into();
    let handler = SupportAttachmentExpiryHandler::new(ExpiryStore { rows: vec![row] }, 10);

    let error = handler
        .run_once_at(at)
        .await
        .expect_err("active row must not be counted as expired");
    assert!(error.to_string().contains("non-terminal attachment"));
}

#[tokio::test]
async fn expiry_worker_rejects_duplicate_terminal_rows_before_reporting_progress() {
    let at = Utc::now();
    let mut row = attachment(at, AuthorityScope::Platform);
    row.status = "expired".into();
    row.version = 2;
    row.value.expires_at = at - Duration::minutes(1);
    let handler = SupportAttachmentExpiryHandler::new(
        ExpiryStore {
            rows: vec![row.clone(), row],
        },
        10,
    );

    let error = handler
        .run_once_at(at)
        .await
        .expect_err("duplicate terminal rows must fail closed");
    assert!(error.to_string().contains("duplicate attachment identity"));
}

#[tokio::test]
async fn expiry_worker_rejects_terminal_rows_that_break_sqlx_order() {
    let at = Utc::now();
    let mut first = attachment(at, AuthorityScope::Platform);
    first.status = "expired".into();
    first.version = 2;
    first.value.expires_at = at - Duration::minutes(1);
    let mut second = first.clone();
    second.value.id = Uuid::from_u128(11);
    second.value.object_ref = "support/ticket-11/object-11.bin".into();
    second.value.expires_at = at - Duration::minutes(2);
    let handler = SupportAttachmentExpiryHandler::new(
        ExpiryStore {
            rows: vec![first, second],
        },
        10,
    );

    let error = handler
        .run_once_at(at)
        .await
        .expect_err("out-of-order terminal rows must fail closed");
    assert!(error.to_string().contains("out-of-order batch"));
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database URL"]
async fn expiry_worker_reconciles_real_sqlx_transition_and_retry_is_empty() {
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

    let sweep_at = Utc::now();
    let ticket = SupportTicket {
        id: Uuid::new_v4(),
        customer_account_id: Uuid::new_v4(),
        status: TicketStatus::New,
        severity: "sev3".into(),
        description: "worker expiry reconciliation".into(),
        recurrence_of: None,
        messages: Vec::new(),
        assignment: None,
        escalation: None,
        sla_deadline: None,
    };
    persistence
        .insert_ticket(
            &ticket,
            &SupportIdempotencyKey::new("worker-expiry-ticket", "worker-expiry-ticket-v1"),
        )
        .await
        .expect("insert ticket");
    let attachment = SupportAttachment::from_upload(
        ticket.id,
        ticket.customer_account_id,
        SupportAttachmentUpload {
            id: Uuid::new_v4(),
            object_ref: "support/worker-expiry/reconcile.bin".into(),
            content_type: "application/octet-stream".into(),
            content_size_bytes: 1,
            sha256_hex: "ab".repeat(32),
            visibility: SupportAttachmentVisibility::Internal,
            uploader_ref: "worker-test".into(),
            scope: AuthorityScope::Platform,
            expires_at: sweep_at - Duration::minutes(1),
        },
        sweep_at - Duration::hours(1),
    )
    .expect("construct expired attachment");
    persistence
        .insert_attachment(
            &ticket,
            &attachment,
            1,
            &AuthorityScope::Platform,
            sweep_at - Duration::hours(1),
            &SupportIdempotencyKey::new("worker-expiry-attachment", "worker-expiry-attachment-v1"),
        )
        .await
        .expect("insert attachment");

    let handler = SupportAttachmentExpiryHandler::new(persistence.clone(), 10);
    let report = handler
        .run_once_at(sweep_at)
        .await
        .expect("real SQLx expiry sweep");
    assert_eq!(report.processed, 1);
    let row = sqlx::query(
        "SELECT status, version
         FROM erpx_control.support_attachments
         WHERE id = $1",
    )
    .bind(attachment.id)
    .fetch_one(&pool)
    .await
    .expect("read authoritative attachment state");
    assert_eq!(row.get::<String, _>("status"), "expired");
    assert_eq!(row.get::<i64, _>("version"), 2);
    let lifecycle_event = sqlx::query(
        "SELECT event_type, aggregate_version, payload
         FROM erpx_control.outbox_events
         WHERE aggregate_type = 'support_attachment' AND aggregate_id = $1
           AND event_type = 'support_attachment.expired'",
    )
    .bind(attachment.id)
    .fetch_one(&pool)
    .await
    .expect("read attachment lifecycle event");
    assert_eq!(
        lifecycle_event.get::<String, _>("event_type"),
        "support_attachment.expired"
    );
    assert_eq!(lifecycle_event.get::<i64, _>("aggregate_version"), 2);
    assert_eq!(
        lifecycle_event.get::<serde_json::Value, _>("payload")["status"],
        serde_json::Value::String("expired".into())
    );

    let retry = handler
        .run_once_at(sweep_at)
        .await
        .expect("retry expiry sweep");
    assert_eq!(retry.processed, 0);
    assert!(!retry.has_work);

    sqlx::query("DELETE FROM erpx_control.support_attachments WHERE id = $1")
        .bind(attachment.id)
        .execute(&pool)
        .await
        .expect("cleanup attachment");
    sqlx::query("DELETE FROM erpx_control.support_tickets WHERE id = $1")
        .bind(ticket.id)
        .execute(&pool)
        .await
        .expect("cleanup ticket");
    sqlx::query(
        "DELETE FROM erpx_control.support_command_idempotency
         WHERE idempotency_key IN ('worker-expiry-ticket', 'worker-expiry-attachment')",
    )
    .execute(&pool)
    .await
    .expect("cleanup idempotency rows");
    pool.close().await;
}

#[test]
fn object_access_requires_exact_binding_fresh_authorization_and_live_state() {
    let at = Utc::now();
    let holding_id = Uuid::from_u128(1);
    let company_id = Uuid::from_u128(2);
    let scope = AuthorityScope::Company {
        holding_id,
        company_id,
    };
    let row = attachment(at, scope.clone());
    let customer_id = row.value.customer_account_id;
    let ticket_id = row.value.ticket_id;
    let binding = SupportCapabilityBinding::new(
        customer_id,
        "agent",
        "device",
        Uuid::from_u128(13),
        ticket_id,
        SupportMode::DiagnosticRead,
        vec!["support.read".into()],
        format!("holding:{holding_id}"),
        "support",
    )
    .expect("valid capability binding");
    let (capability, secret) = SupportCapability::issue(
        binding.clone(),
        at - Duration::minutes(1),
        Duration::hours(1),
    )
    .expect("valid capability");

    SupportAttachmentExpiryHandler::<SupportPersistence>::authorize_object_access(
        &row,
        &capability,
        &secret,
        &binding,
        &row.value.object_ref,
        ticket_id,
        customer_id,
        &scope,
        at,
        at,
    )
    .expect("holding capability covers company attachment");

    SupportAttachmentExpiryHandler::<SupportPersistence>::authorize_high_risk_object_access(
        &row,
        &capability,
        &secret,
        &binding,
        &row.value.object_ref,
        ticket_id,
        customer_id,
        &scope,
        at,
        Uuid::from_u128(14),
        at - Duration::minutes(1),
        at,
    )
    .expect("high-risk object access requires and accepts fresh MFA");
    assert!(
        SupportAttachmentExpiryHandler::<SupportPersistence>::authorize_high_risk_object_access(
            &row,
            &capability,
            &secret,
            &binding,
            &row.value.object_ref,
            ticket_id,
            customer_id,
            &scope,
            at,
            Uuid::nil(),
            at - Duration::minutes(1),
            at,
        )
        .is_err()
    );
    assert!(
        SupportAttachmentExpiryHandler::<SupportPersistence>::authorize_high_risk_object_access(
            &row,
            &capability,
            &secret,
            &binding,
            &row.value.object_ref,
            ticket_id,
            customer_id,
            &scope,
            at,
            Uuid::from_u128(14),
            at - Duration::minutes(6),
            at,
        )
        .is_err()
    );

    assert!(
        SupportAttachmentExpiryHandler::<SupportPersistence>::authorize_object_access(
            &row,
            &capability,
            &secret,
            &binding,
            "support/ticket-11/other.bin",
            ticket_id,
            customer_id,
            &scope,
            at,
            at,
        )
        .is_err()
    );
    assert!(
        SupportAttachmentExpiryHandler::<SupportPersistence>::authorize_object_access(
            &row,
            &capability,
            &secret,
            &binding,
            &row.value.object_ref,
            ticket_id,
            customer_id,
            &scope,
            at - Duration::minutes(6),
            at,
        )
        .is_err()
    );

    let mut revoked_row = row.clone();
    revoked_row.status = "revoked".into();
    assert!(
        SupportAttachmentExpiryHandler::<SupportPersistence>::authorize_object_access(
            &revoked_row,
            &capability,
            &secret,
            &binding,
            &row.value.object_ref,
            ticket_id,
            customer_id,
            &scope,
            at,
            at,
        )
        .is_err()
    );

    let mut revoked = capability;
    revoked
        .revoke(
            erpx_domain::support::SupportCapabilityRevocationReason::SessionRevoked,
            at,
        )
        .expect("capability can be revoked once");
    assert!(
        SupportAttachmentExpiryHandler::<SupportPersistence>::authorize_object_access(
            &row,
            &revoked,
            &secret,
            &binding,
            &row.value.object_ref,
            ticket_id,
            customer_id,
            &scope,
            at,
            at,
        )
        .is_err()
    );
}

#[tokio::test]
async fn unconfigured_sla_handler_fails_before_claiming_or_acknowledging() {
    let source = NotificationSource::default();
    let claims = Arc::clone(&source.claims);
    let handler = SupportSlaNotificationHandler::new(source, 10);

    let error = handler
        .run_once(Uuid::from_u128(99))
        .await
        .expect_err("delivery must fail closed without an explicit publisher");
    assert!(
        error
            .to_string()
            .contains("delivery boundary is not configured")
    );
    assert_eq!(claims.load(Ordering::Relaxed), 0);
}
