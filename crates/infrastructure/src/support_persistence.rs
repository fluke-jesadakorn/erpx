//! Bounded SQLx persistence for support access and incident aggregates.
//!
//! This module is intentionally standalone for the G4 persistence packet. The
//! infrastructure module wiring and command/outbox integration belong to the
//! main integration agent. It writes only to the control-plane write pool.

use chrono::{DateTime, Utc};
use erpx_domain::contracts::TenantScope;
use erpx_domain::support::{
    CustomerVisibleSupportAttachment, Incident, IncidentStatus, MAX_SUPPORT_ATTACHMENTS_PER_TICKET,
    SupportAttachment, SupportAttachmentSet, SupportAttachmentVisibility, SupportCapability,
    SupportCapabilityBinding, SupportCapabilityRevocationReason, SupportMode,
    SupportNotificationAudience, SupportSession, SupportSlaNotification,
    SupportSlaNotificationClassification, SupportSlaNotificationPolicy, SupportTicket,
    SupportTicketCommand, TicketStatus,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use thiserror::Error;
use uuid::Uuid;

const SUPPORT_MIGRATION: &str =
    include_str!("../../migration/migrations/0005_support_incidents.sql");
const SUPPORT_ATTACHMENT_MIGRATION: &str =
    include_str!("../../migration/migrations/0016_support_attachments.sql");
const SUPPORT_SLA_NOTIFICATION_MIGRATION: &str =
    include_str!("../../migration/migrations/0017_support_sla_notifications.sql");
const SUPPORT_SLA_CLAIM_IDENTITY_MIGRATION: &str =
    include_str!("../../migration/migrations/0060_support_sla_claim_identity.sql");

/// Maximum number of capabilities changed by one expiration sweep.
///
/// Callers can repeatedly invoke the sweep until it returns an empty batch;
/// keeping the transaction bounded prevents a large control plane from
/// holding locks for an unbounded duration.
pub const MAX_EXPIRATION_SWEEP_BATCH_SIZE: u32 = 1_000;

/// Maximum number of active capabilities changed by one cascade-revocation
/// command. Callers can drain additional batches explicitly; a single
/// transaction never takes an unbounded number of capability locks.
pub const MAX_CASCADE_REVOCATION_BATCH_SIZE: u32 = 1_000;

/// Maximum number of incident summaries returned by one control-plane read.
/// Callers can request a smaller page; the repository never executes an
/// unbounded incident query.
pub const MAX_INCIDENT_LIST_PAGE_SIZE: u32 = 100;

/// Maximum number of support ticket summaries returned by one query.
pub const MAX_SUPPORT_TICKET_LIST_PAGE_SIZE: u32 = 100;

/// Maximum number of active attachment records returned by one page.
/// Callers can continue from their own cursor; the repository never executes
/// an unbounded metadata query.
pub const MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE: u32 = 100;

/// Maximum number of due SLA notifications claimed by one transaction.
pub const MAX_SUPPORT_SLA_NOTIFICATION_CLAIM_BATCH_SIZE: u32 = 100;

#[derive(Debug, Error)]
pub enum SupportPersistenceError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("idempotency key {key} was already used with a different command")]
    IdempotencyConflict { key: String },
    #[error("support resource {entity}/{id} was not found")]
    NotFound { entity: &'static str, id: Uuid },
    #[error(
        "optimistic concurrency conflict for {entity}/{id}: expected {expected}, actual {actual:?}"
    )]
    OptimisticConcurrency {
        entity: &'static str,
        id: Uuid,
        expected: u64,
        actual: Option<u64>,
    },
    #[error("invalid support persistence value: {0}")]
    InvalidData(String),
    #[error("support attachment is outside the authorized scope")]
    AttachmentScopeDenied,
    #[error("support attachment is not bound to the requested ticket and customer")]
    AttachmentBindingDenied,
    #[error("support attachment cannot be added to a closed ticket")]
    AttachmentClosedTicket,
    #[error("support attachment is expired or no longer active")]
    AttachmentNotActive,
    #[error("support attachment has not reached its expiry time")]
    AttachmentNotExpired,
    #[error("support attachment limit has been reached")]
    AttachmentLimitExceeded,
    #[error("support access request is not bound to its ticket's customer")]
    AccessRequestTicketBindingDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey {
    pub key: String,
    pub request_hash: String,
}

impl IdempotencyKey {
    pub fn new(key: impl Into<String>, request_hash: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            request_hash: request_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Persisted<T> {
    pub value: T,
    pub version: u64,
    pub reused_idempotency: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncidentPage {
    pub incidents: Vec<Persisted<Incident>>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportTicketPage {
    pub tickets: Vec<Persisted<SupportTicket>>,
    pub has_more: bool,
    /// The validated keyset position after the last returned ticket.
    ///
    /// This is deliberately typed metadata rather than an API token. The
    /// server integration may later encode it as an opaque cursor without
    /// exposing ticket payloads or credentials.
    pub next_cursor: Option<SupportTicketListCursor>,
}

/// Opaque keyset position for the support ticket queue.
///
/// Only the PostgreSQL ordering keys are retained: `updated_at` and `id`.
/// Fields remain private so callers cannot construct an unchecked cursor;
/// use [`SupportTicketListCursor::new`] and pass the value back to the
/// cursor-aware list method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportTicketListCursor {
    updated_at: DateTime<Utc>,
    id: Uuid,
}

impl SupportTicketListCursor {
    pub fn new(updated_at: DateTime<Utc>, id: Uuid) -> Result<Self, SupportPersistenceError> {
        if id.is_nil() {
            return Err(SupportPersistenceError::InvalidData(
                "support ticket list cursor id cannot be nil".into(),
            ));
        }
        // PostgreSQL TIMESTAMPTZ has microsecond precision. Reject values
        // that would be truncated by SQLx so continuation remains exclusive
        // and deterministic at the database boundary.
        if DateTime::from_timestamp_micros(updated_at.timestamp_micros()) != Some(updated_at) {
            return Err(SupportPersistenceError::InvalidData(
                "support ticket list cursor timestamp is not a valid database value".into(),
            ));
        }
        Ok(Self { updated_at, id })
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn id(&self) -> Uuid {
        self.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedSupportAttachment {
    pub value: SupportAttachment,
    pub status: String,
    pub version: u64,
    pub ticket_version: u64,
    pub reused_idempotency: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportAttachmentPage {
    pub attachments: Vec<PersistedSupportAttachment>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerVisibleSupportAttachmentPage {
    pub attachments: Vec<CustomerVisibleSupportAttachment>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportApproval {
    pub approver_ref: String,
    pub approval_ref: String,
    pub approved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityMetadata {
    pub id: Uuid,
    pub binding: SupportCapabilityBinding,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub reissued_from: Option<Uuid>,
    pub status: String,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revocation_reason: Option<String>,
    pub version: u64,
    pub secret_hash_length: usize,
}

#[derive(Debug, Clone)]
pub struct SupportPersistence {
    write: PgPool,
}

impl SupportPersistence {
    pub fn new(write: PgPool) -> Self {
        Self { write }
    }

    /// Apply this packet's additive migration. The production migrator must
    /// register the same file before this repository is composed at runtime.
    pub async fn apply_migration(&self) -> Result<(), SupportPersistenceError> {
        sqlx::raw_sql(SUPPORT_MIGRATION)
            .execute(&self.write)
            .await?;
        sqlx::raw_sql(SUPPORT_ATTACHMENT_MIGRATION)
            .execute(&self.write)
            .await?;
        sqlx::raw_sql(SUPPORT_SLA_NOTIFICATION_MIGRATION)
            .execute(&self.write)
            .await?;
        sqlx::raw_sql(SUPPORT_SLA_CLAIM_IDENTITY_MIGRATION)
            .execute(&self.write)
            .await?;
        Ok(())
    }

    pub async fn insert_ticket(
        &self,
        ticket: &SupportTicket,
        idempotency: &IdempotencyKey,
    ) -> Result<Persisted<SupportTicket>, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        validate_ticket(ticket)?;
        let payload = serde_json::to_value(ticket)?;
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "ticket_insert", ticket.id).await?;
        if reservation {
            sqlx::query(
                "INSERT INTO erpx_control.support_tickets
                    (id, customer_account_id, status, severity, version, payload)
                 VALUES ($1, $2, $3, $4, 1, $5)",
            )
            .bind(ticket.id)
            .bind(ticket.customer_account_id)
            .bind(ticket_status(ticket.status))
            .bind(&ticket.severity)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
            let notifications = scheduled_sla_notifications(ticket);
            enqueue_sla_notifications_tx(&mut tx, &notifications).await?;
            append_support_ticket_event(&mut tx, ticket, 1, "support_ticket.created").await?;
            tx.commit().await?;
            return Ok(Persisted {
                value: ticket.clone(),
                version: 1,
                reused_idempotency: false,
            });
        }
        let stored = fetch_ticket_tx(&mut tx, ticket.id).await?.ok_or(
            SupportPersistenceError::NotFound {
                entity: "support_ticket",
                id: ticket.id,
            },
        )?;
        tx.commit().await?;
        Ok(Persisted {
            version: stored.1,
            value: stored.0,
            reused_idempotency: true,
        })
    }

    pub async fn get_ticket(
        &self,
        id: Uuid,
    ) -> Result<Option<Persisted<SupportTicket>>, SupportPersistenceError> {
        fetch_ticket_pool(&self.write, id).await.map(|result| {
            result.map(|(value, version)| Persisted {
                value,
                version,
                reused_idempotency: false,
            })
        })
    }

    /// Read a bounded, deterministic support queue page. The query is kept on
    /// the control-plane write pool because the ticket aggregate is still the
    /// authoritative source during projection transition. Authorization and
    /// customer-scope filtering remain at the application boundary.
    pub async fn list_tickets(
        &self,
        status: Option<TicketStatus>,
        customer_account_id: Option<Uuid>,
        page_size: u32,
    ) -> Result<SupportTicketPage, SupportPersistenceError> {
        self.list_tickets_after(status, customer_account_id, page_size, None)
            .await
    }

    /// Read a bounded support queue page after an optional validated keyset
    /// cursor. Ordering is `(updated_at DESC, id DESC)` and the cursor is
    /// exclusive, so rows at the cursor position are never repeated.
    ///
    /// The query remains on the control-plane write pool because the ticket
    /// aggregate is still authoritative during projection transition.
    /// Authorization and customer-scope filtering remain at the application
    /// boundary; the customer filter is nevertheless applied in SQL so a
    /// page cannot return another customer's ticket.
    pub async fn list_tickets_after(
        &self,
        status: Option<TicketStatus>,
        customer_account_id: Option<Uuid>,
        page_size: u32,
        cursor: Option<&SupportTicketListCursor>,
    ) -> Result<SupportTicketPage, SupportPersistenceError> {
        let page_limit = validate_ticket_list_page_size(page_size)?;
        validate_ticket_customer_filter(customer_account_id)?;
        let query_limit = i64::from(page_size) + 1;
        let rows = sqlx::query(
            "SELECT id, updated_at, payload, version
             FROM erpx_control.support_tickets
             WHERE ($1::text IS NULL OR status = $1)
               AND ($2::uuid IS NULL OR customer_account_id = $2)
               AND (
                    $3::timestamptz IS NULL
                    OR updated_at < $3
                    OR (updated_at = $3 AND id < $4)
               )
             ORDER BY updated_at DESC, id DESC
             LIMIT $5",
        )
        .bind(status.map(ticket_status))
        .bind(customer_account_id)
        .bind(cursor.map(SupportTicketListCursor::updated_at))
        .bind(cursor.map(SupportTicketListCursor::id))
        .bind(query_limit)
        .fetch_all(&self.write)
        .await?;

        let has_more = rows.len() > page_limit;
        let tickets = rows
            .into_iter()
            .take(page_limit)
            .map(|row| {
                let id: Uuid = row.try_get("id")?;
                let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
                let cursor = SupportTicketListCursor::new(updated_at, id)?;
                Ok((
                    Persisted {
                        value: validated_ticket(row.try_get("payload")?)?,
                        version: positive_u64(row.try_get("version")?, "ticket version")?,
                        reused_idempotency: false,
                    },
                    cursor,
                ))
            })
            .collect::<Result<Vec<_>, SupportPersistenceError>>()?;
        let next_cursor = has_more
            .then(|| tickets.last().map(|(_, cursor)| cursor.clone()))
            .flatten();
        let tickets = tickets.into_iter().map(|(ticket, _)| ticket).collect();
        Ok(SupportTicketPage {
            tickets,
            has_more,
            next_cursor,
        })
    }

    pub async fn update_ticket(
        &self,
        ticket: &SupportTicket,
        expected_version: u64,
        idempotency: &IdempotencyKey,
    ) -> Result<Persisted<SupportTicket>, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        validate_ticket(ticket)?;
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "ticket_update", ticket.id).await?;
        if !reservation {
            let stored = fetch_ticket_tx(&mut tx, ticket.id).await?.ok_or(
                SupportPersistenceError::NotFound {
                    entity: "support_ticket",
                    id: ticket.id,
                },
            )?;
            tx.commit().await?;
            return Ok(Persisted {
                value: stored.0,
                version: stored.1,
                reused_idempotency: true,
            });
        }
        let current = fetch_ticket_tx(&mut tx, ticket.id).await?.ok_or(
            SupportPersistenceError::NotFound {
                entity: "support_ticket",
                id: ticket.id,
            },
        )?;
        if current.0.customer_account_id != ticket.customer_account_id
            || current.0.recurrence_of != ticket.recurrence_of
        {
            return Err(SupportPersistenceError::InvalidData(
                "support ticket customer binding is immutable".into(),
            ));
        }
        let next_version = checked_next(expected_version)?;
        let result = sqlx::query(
            "UPDATE erpx_control.support_tickets
             SET status = $2, severity = $3, payload = $4, version = $5, updated_at = now()
             WHERE id = $1 AND version = $6",
        )
        .bind(ticket.id)
        .bind(ticket_status(ticket.status))
        .bind(&ticket.severity)
        .bind(serde_json::to_value(ticket)?)
        .bind(as_i64(next_version, "ticket version")?)
        .bind(as_i64(expected_version, "ticket version")?)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(
                concurrency_error(&mut tx, "support_ticket", ticket.id, expected_version).await?,
            );
        }
        let notifications = scheduled_sla_notifications(ticket);
        enqueue_sla_notifications_tx(&mut tx, &notifications).await?;
        append_support_ticket_event(&mut tx, ticket, next_version, "support_ticket.updated")
            .await?;
        tx.commit().await?;
        Ok(Persisted {
            value: ticket.clone(),
            version: next_version,
            reused_idempotency: false,
        })
    }

    /// Insert attachment metadata and advance the ticket aggregate version in
    /// one transaction. The ticket row is locked before the attachment count
    /// is checked, so concurrent uploads cannot bypass the domain limit.
    /// Object bytes and bearer credentials are intentionally not accepted or
    /// written by this method.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_attachment(
        &self,
        ticket: &SupportTicket,
        attachment: &SupportAttachment,
        expected_ticket_version: u64,
        actor_scope: &TenantScope,
        now: DateTime<Utc>,
        idempotency: &IdempotencyKey,
    ) -> Result<PersistedSupportAttachment, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        validate_ticket(ticket)?;
        attachment
            .validate()
            .map_err(|error| SupportPersistenceError::InvalidData(error.to_string()))?;
        if attachment.ticket_id != ticket.id
            || attachment.customer_account_id != ticket.customer_account_id
        {
            return Err(SupportPersistenceError::AttachmentBindingDenied);
        }
        ensure_attachment_scope(actor_scope, &attachment.scope)?;

        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "attachment_insert", attachment.id).await?;
        if !reservation {
            let stored = fetch_attachment_tx(&mut tx, attachment.id, false)
                .await?
                .ok_or(SupportPersistenceError::NotFound {
                    entity: "support_attachment",
                    id: attachment.id,
                })?;
            ensure_attachment_binding(&stored.0, ticket.id, ticket.customer_account_id)?;
            ensure_attachment_scope(actor_scope, &stored.0.scope)?;
            let ticket_version = fetch_ticket_version_tx(&mut tx, ticket.id).await?;
            tx.commit().await?;
            return Ok(PersistedSupportAttachment {
                value: stored.0,
                status: stored.1,
                version: stored.2,
                ticket_version,
                reused_idempotency: true,
            });
        }

        let (stored_ticket, current_ticket_version) =
            fetch_ticket_for_update(&mut tx, ticket.id).await?;
        if stored_ticket.customer_account_id != ticket.customer_account_id {
            return Err(SupportPersistenceError::AttachmentBindingDenied);
        }
        if stored_ticket.status == TicketStatus::Closed {
            return Err(SupportPersistenceError::AttachmentClosedTicket);
        }
        if current_ticket_version != expected_ticket_version {
            return Err(concurrency_error(
                &mut tx,
                "support_ticket",
                ticket.id,
                expected_ticket_version,
            )
            .await?);
        }

        let active_rows = sqlx::query(
            "SELECT id, ticket_id, customer_account_id, object_ref, content_type,
                    content_size_bytes, sha256_hex, visibility, uploader_ref, scope,
                    uploaded_at, expires_at, status, version
             FROM erpx_control.support_attachments
             WHERE ticket_id = $1 AND customer_account_id = $2
               AND status = 'active' AND expires_at > $3
             ORDER BY uploaded_at, id
             LIMIT $4",
        )
        .bind(ticket.id)
        .bind(ticket.customer_account_id)
        .bind(now)
        .bind(i64::from(MAX_SUPPORT_ATTACHMENTS_PER_TICKET as u32 + 1))
        .fetch_all(&mut *tx)
        .await?;
        let existing = active_rows
            .iter()
            .map(|row| attachment_from_row(row).map(|value| value.0))
            .collect::<Result<Vec<_>, _>>()?;
        SupportAttachmentSet::append(&stored_ticket, &existing, attachment.clone(), now)
            .map_err(map_attachment_domain_error)?;

        let next_ticket_version = checked_next(expected_ticket_version)?;
        sqlx::query(
            "INSERT INTO erpx_control.support_attachments
                (id, ticket_id, customer_account_id, object_ref, content_type,
                 content_size_bytes, sha256_hex, visibility, uploader_ref, scope,
                 uploaded_at, expires_at, status, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, 'active', 1)",
        )
        .bind(attachment.id)
        .bind(attachment.ticket_id)
        .bind(attachment.customer_account_id)
        .bind(&attachment.object_ref)
        .bind(&attachment.content_type)
        .bind(as_i64(attachment.content_size_bytes, "attachment size")?)
        .bind(&attachment.sha256_hex)
        .bind(attachment_visibility(attachment.visibility))
        .bind(&attachment.uploader_ref)
        .bind(serde_json::to_value(&attachment.scope)?)
        .bind(attachment.uploaded_at)
        .bind(attachment.expires_at)
        .execute(&mut *tx)
        .await?;

        append_support_attachment_event(
            &mut tx,
            attachment,
            "active",
            1,
            attachment.uploaded_at,
            next_ticket_version,
        )
        .await?;

        let ticket_update = sqlx::query(
            "UPDATE erpx_control.support_tickets
             SET version = $2, updated_at = now()
             WHERE id = $1 AND version = $3 AND status <> 'closed'",
        )
        .bind(ticket.id)
        .bind(as_i64(next_ticket_version, "ticket version")?)
        .bind(as_i64(expected_ticket_version, "ticket version")?)
        .execute(&mut *tx)
        .await?;
        if ticket_update.rows_affected() != 1 {
            return Err(concurrency_error(
                &mut tx,
                "support_ticket",
                ticket.id,
                expected_ticket_version,
            )
            .await?);
        }
        tx.commit().await?;
        Ok(PersistedSupportAttachment {
            value: attachment.clone(),
            status: "active".into(),
            version: 1,
            ticket_version: next_ticket_version,
            reused_idempotency: false,
        })
    }

    /// List only active, unexpired attachment metadata for a ticket. The
    /// result is filtered by the caller's typed scope before it is returned;
    /// expired rows are hidden even if a worker has not swept them yet.
    pub async fn list_attachments_for_ticket(
        &self,
        ticket_id: Uuid,
        customer_account_id: Uuid,
        actor_scope: &TenantScope,
        now: DateTime<Utc>,
        page_size: u32,
    ) -> Result<SupportAttachmentPage, SupportPersistenceError> {
        validate_attachment_page_size(page_size)?;
        let (ticket, ticket_version) = fetch_ticket_pool(&self.write, ticket_id).await?.ok_or(
            SupportPersistenceError::NotFound {
                entity: "support_ticket",
                id: ticket_id,
            },
        )?;
        if ticket.customer_account_id != customer_account_id {
            return Err(SupportPersistenceError::AttachmentBindingDenied);
        }
        let rows = sqlx::query(
            "SELECT id, ticket_id, customer_account_id, object_ref, content_type,
                    content_size_bytes, sha256_hex, visibility, uploader_ref, scope,
                    uploaded_at, expires_at, status, version
             FROM erpx_control.support_attachments
             WHERE ticket_id = $1 AND customer_account_id = $2
               AND status = 'active' AND expires_at > $3
             ORDER BY uploaded_at DESC, id DESC
             LIMIT $4",
        )
        .bind(ticket_id)
        .bind(customer_account_id)
        .bind(now)
        .bind(i64::from(MAX_SUPPORT_ATTACHMENTS_PER_TICKET as u32 + 1))
        .fetch_all(&self.write)
        .await?;
        let mut attachments = rows
            .iter()
            .filter_map(|row| match attachment_from_row(row) {
                Ok((value, status, version)) if actor_scope.contains(&value.scope) => {
                    Some(Ok(PersistedSupportAttachment {
                        value,
                        status,
                        version,
                        ticket_version,
                        reused_idempotency: false,
                    }))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let page_limit = usize::try_from(page_size)
            .map_err(|_| SupportPersistenceError::InvalidData("attachment page size".into()))?;
        let has_more = attachments.len() > page_limit;
        if has_more {
            attachments.truncate(page_limit);
        }
        Ok(SupportAttachmentPage {
            attachments,
            has_more,
        })
    }

    /// Customer-safe attachment metadata. Internal attachments and rows that
    /// are expired at the supplied clock are excluded at both SQL and domain
    /// projection boundaries.
    pub async fn list_customer_visible_attachments(
        &self,
        ticket_id: Uuid,
        customer_account_id: Uuid,
        now: DateTime<Utc>,
        page_size: u32,
    ) -> Result<CustomerVisibleSupportAttachmentPage, SupportPersistenceError> {
        validate_attachment_page_size(page_size)?;
        let (ticket, _) = fetch_ticket_pool(&self.write, ticket_id).await?.ok_or(
            SupportPersistenceError::NotFound {
                entity: "support_ticket",
                id: ticket_id,
            },
        )?;
        if ticket.customer_account_id != customer_account_id {
            return Err(SupportPersistenceError::AttachmentBindingDenied);
        }
        let rows = sqlx::query(
            "SELECT id, ticket_id, customer_account_id, object_ref, content_type,
                    content_size_bytes, sha256_hex, visibility, uploader_ref, scope,
                    uploaded_at, expires_at, status, version
             FROM erpx_control.support_attachments
             WHERE ticket_id = $1 AND customer_account_id = $2
               AND visibility = 'customer_visible'
               AND status = 'active' AND expires_at > $3
             ORDER BY uploaded_at DESC, id DESC
             LIMIT $4",
        )
        .bind(ticket_id)
        .bind(customer_account_id)
        .bind(now)
        .bind(i64::from(MAX_SUPPORT_ATTACHMENTS_PER_TICKET as u32 + 1))
        .fetch_all(&self.write)
        .await?;
        let mut attachments = rows
            .iter()
            .map(|row| {
                let value = attachment_from_row(row)?.0;
                value.customer_visible_view(&ticket, now).ok_or_else(|| {
                    SupportPersistenceError::InvalidData(
                        "customer-visible attachment failed safe projection".into(),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let page_limit = usize::try_from(page_size)
            .map_err(|_| SupportPersistenceError::InvalidData("attachment page size".into()))?;
        let has_more = attachments.len() > page_limit;
        if has_more {
            attachments.truncate(page_limit);
        }
        Ok(CustomerVisibleSupportAttachmentPage {
            attachments,
            has_more,
        })
    }

    /// Revoke one active attachment with ticket and attachment optimistic
    /// versions. A closed ticket cannot be mutated; an idempotent replay of a
    /// successful revocation remains safe after the ticket later closes.
    #[allow(clippy::too_many_arguments)]
    pub async fn revoke_attachment(
        &self,
        ticket_id: Uuid,
        customer_account_id: Uuid,
        attachment_id: Uuid,
        expected_ticket_version: u64,
        expected_attachment_version: u64,
        actor_scope: &TenantScope,
        at: DateTime<Utc>,
        idempotency: &IdempotencyKey,
    ) -> Result<PersistedSupportAttachment, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "attachment_revoke", attachment_id).await?;
        if !reservation {
            let stored = fetch_attachment_tx(&mut tx, attachment_id, false)
                .await?
                .ok_or(SupportPersistenceError::NotFound {
                    entity: "support_attachment",
                    id: attachment_id,
                })?;
            ensure_attachment_binding(&stored.0, ticket_id, customer_account_id)?;
            ensure_attachment_scope(actor_scope, &stored.0.scope)?;
            let ticket_version = fetch_ticket_version_tx(&mut tx, ticket_id).await?;
            tx.commit().await?;
            return Ok(PersistedSupportAttachment {
                value: stored.0,
                status: stored.1,
                version: stored.2,
                ticket_version,
                reused_idempotency: true,
            });
        }

        let (ticket, current_ticket_version) = fetch_ticket_for_update(&mut tx, ticket_id).await?;
        if ticket.customer_account_id != customer_account_id {
            return Err(SupportPersistenceError::AttachmentBindingDenied);
        }
        if ticket.status == TicketStatus::Closed {
            return Err(SupportPersistenceError::AttachmentClosedTicket);
        }
        if current_ticket_version != expected_ticket_version {
            return Err(concurrency_error(
                &mut tx,
                "support_ticket",
                ticket_id,
                expected_ticket_version,
            )
            .await?);
        }
        let stored = fetch_attachment_tx(&mut tx, attachment_id, true)
            .await?
            .ok_or(SupportPersistenceError::NotFound {
                entity: "support_attachment",
                id: attachment_id,
            })?;
        ensure_attachment_binding(&stored.0, ticket_id, customer_account_id)?;
        ensure_attachment_scope(actor_scope, &stored.0.scope)?;
        if stored.1 != "active" || !stored.0.is_active_at(at) {
            return Err(SupportPersistenceError::AttachmentNotActive);
        }
        if stored.2 != expected_attachment_version {
            return Err(concurrency_error(
                &mut tx,
                "support_attachment",
                attachment_id,
                expected_attachment_version,
            )
            .await?);
        }
        let next_attachment_version = checked_next(expected_attachment_version)?;
        let next_ticket_version = checked_next(expected_ticket_version)?;
        let result = sqlx::query(
            "UPDATE erpx_control.support_attachments
             SET status = 'revoked', revoked_at = $2, version = $3, updated_at = $2
             WHERE id = $1 AND ticket_id = $4 AND customer_account_id = $5
               AND status = 'active' AND version = $6 AND expires_at > $2",
        )
        .bind(attachment_id)
        .bind(at)
        .bind(as_i64(next_attachment_version, "attachment version")?)
        .bind(ticket_id)
        .bind(customer_account_id)
        .bind(as_i64(expected_attachment_version, "attachment version")?)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(concurrency_error(
                &mut tx,
                "support_attachment",
                attachment_id,
                expected_attachment_version,
            )
            .await?);
        }
        let ticket_update = sqlx::query(
            "UPDATE erpx_control.support_tickets
             SET version = $2, updated_at = now()
             WHERE id = $1 AND version = $3 AND status <> 'closed'",
        )
        .bind(ticket_id)
        .bind(as_i64(next_ticket_version, "ticket version")?)
        .bind(as_i64(expected_ticket_version, "ticket version")?)
        .execute(&mut *tx)
        .await?;
        if ticket_update.rows_affected() != 1 {
            return Err(concurrency_error(
                &mut tx,
                "support_ticket",
                ticket_id,
                expected_ticket_version,
            )
            .await?);
        }
        let updated = fetch_attachment_tx(&mut tx, attachment_id, false)
            .await?
            .ok_or(SupportPersistenceError::NotFound {
                entity: "support_attachment",
                id: attachment_id,
            })?;
        append_support_attachment_event(
            &mut tx,
            &updated.0,
            "revoked",
            updated.2,
            at,
            next_ticket_version,
        )
        .await?;
        tx.commit().await?;
        Ok(PersistedSupportAttachment {
            value: updated.0,
            status: updated.1,
            version: updated.2,
            ticket_version: next_ticket_version,
            reused_idempotency: false,
        })
    }

    /// Mark one attachment expired after its half-open expiry boundary. This
    /// is deliberately separate from revocation: expiry can run for a closed
    /// ticket, while a user mutation cannot.
    pub async fn expire_attachment(
        &self,
        ticket_id: Uuid,
        customer_account_id: Uuid,
        attachment_id: Uuid,
        expected_version: u64,
        at: DateTime<Utc>,
        idempotency: &IdempotencyKey,
    ) -> Result<PersistedSupportAttachment, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "attachment_expire", attachment_id).await?;
        if !reservation {
            let stored = fetch_attachment_tx(&mut tx, attachment_id, false)
                .await?
                .ok_or(SupportPersistenceError::NotFound {
                    entity: "support_attachment",
                    id: attachment_id,
                })?;
            ensure_attachment_binding(&stored.0, ticket_id, customer_account_id)?;
            let ticket_version = fetch_ticket_version_tx(&mut tx, ticket_id).await?;
            tx.commit().await?;
            return Ok(PersistedSupportAttachment {
                value: stored.0,
                status: stored.1,
                version: stored.2,
                ticket_version,
                reused_idempotency: true,
            });
        }
        let stored = fetch_attachment_tx(&mut tx, attachment_id, true)
            .await?
            .ok_or(SupportPersistenceError::NotFound {
                entity: "support_attachment",
                id: attachment_id,
            })?;
        ensure_attachment_binding(&stored.0, ticket_id, customer_account_id)?;
        if stored.1 != "active" {
            return Err(SupportPersistenceError::AttachmentNotActive);
        }
        if stored.0.expires_at > at {
            return Err(SupportPersistenceError::AttachmentNotExpired);
        }
        if stored.2 != expected_version {
            return Err(concurrency_error(
                &mut tx,
                "support_attachment",
                attachment_id,
                expected_version,
            )
            .await?);
        }
        let next_version = checked_next(expected_version)?;
        let result = sqlx::query(
            "UPDATE erpx_control.support_attachments
             SET status = 'expired', version = $2, updated_at = $3
             WHERE id = $1 AND status = 'active' AND version = $4
               AND expires_at <= $3",
        )
        .bind(attachment_id)
        .bind(as_i64(next_version, "attachment version")?)
        .bind(at)
        .bind(as_i64(expected_version, "attachment version")?)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(concurrency_error(
                &mut tx,
                "support_attachment",
                attachment_id,
                expected_version,
            )
            .await?);
        }
        let updated = fetch_attachment_tx(&mut tx, attachment_id, false)
            .await?
            .ok_or(SupportPersistenceError::NotFound {
                entity: "support_attachment",
                id: attachment_id,
            })?;
        let ticket_version = fetch_ticket_version_tx(&mut tx, ticket_id).await?;
        append_support_attachment_event(
            &mut tx,
            &updated.0,
            "expired",
            updated.2,
            at,
            ticket_version,
        )
        .await?;
        tx.commit().await?;
        Ok(PersistedSupportAttachment {
            value: updated.0,
            status: updated.1,
            version: updated.2,
            ticket_version,
            reused_idempotency: false,
        })
    }

    /// Expire a bounded batch using row locks and a status/clock predicate.
    /// Re-running the same sweep cannot increment a version twice.
    pub async fn sweep_expired_attachments(
        &self,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<PersistedSupportAttachment>, SupportPersistenceError> {
        if batch_size == 0 || batch_size > MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE {
            return Err(SupportPersistenceError::InvalidData(format!(
                "attachment expiry batch size must be 1..={MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE}"
            )));
        }
        let mut tx = self.write.begin().await?;
        let rows = sqlx::query(
            "SELECT id
             FROM erpx_control.support_attachments
             WHERE status = 'active' AND expires_at <= $1
             ORDER BY expires_at, id
             FOR UPDATE SKIP LOCKED
             LIMIT $2",
        )
        .bind(at)
        .bind(i64::from(batch_size))
        .fetch_all(&mut *tx)
        .await?;
        let mut expired = Vec::with_capacity(rows.len());
        for row in rows {
            let id: Uuid = row.try_get("id")?;
            let result = sqlx::query(
                "UPDATE erpx_control.support_attachments
                 SET status = 'expired', version = version + 1, updated_at = $2
                 WHERE id = $1 AND status = 'active' AND expires_at <= $2",
            )
            .bind(id)
            .bind(at)
            .execute(&mut *tx)
            .await?;
            if result.rows_affected() == 1 {
                let stored = fetch_attachment_tx(&mut tx, id, false).await?.ok_or(
                    SupportPersistenceError::NotFound {
                        entity: "support_attachment",
                        id,
                    },
                )?;
                let ticket_version = fetch_ticket_version_tx(&mut tx, stored.0.ticket_id).await?;
                append_support_attachment_event(
                    &mut tx,
                    &stored.0,
                    "expired",
                    stored.2,
                    at,
                    ticket_version,
                )
                .await?;
                expired.push(PersistedSupportAttachment {
                    value: stored.0,
                    status: stored.1,
                    version: stored.2,
                    ticket_version,
                    reused_idempotency: false,
                });
            }
        }
        tx.commit().await?;
        Ok(expired)
    }

    /// Insert the deterministic notifications derived from a ticket SLA.
    /// Existing identical keys are harmless; a changed payload for an
    /// existing key is an idempotency conflict rather than a second send.
    pub async fn enqueue_sla_notifications(
        &self,
        notifications: &[SupportSlaNotification],
    ) -> Result<u64, SupportPersistenceError> {
        let mut tx = self.write.begin().await?;
        let inserted = enqueue_sla_notifications_tx(&mut tx, notifications).await?;
        tx.commit().await?;
        Ok(inserted)
    }

    /// Claim due notifications with a short lease. Pending rows and expired
    /// claims are both eligible; `SKIP LOCKED` allows multiple workers to
    /// drain the queue without duplicate active claims.
    pub async fn claim_due_sla_notifications(
        &self,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<SupportSlaNotification>, SupportPersistenceError> {
        if batch_size == 0 || batch_size > MAX_SUPPORT_SLA_NOTIFICATION_CLAIM_BATCH_SIZE {
            return Err(SupportPersistenceError::InvalidData(format!(
                "SLA notification claim batch must be 1..={MAX_SUPPORT_SLA_NOTIFICATION_CLAIM_BATCH_SIZE}"
            )));
        }
        let lease_expires_at = at + chrono::Duration::minutes(15);
        let mut tx = self.write.begin().await?;
        let rows = sqlx::query(
            "WITH due AS (
                 SELECT notification_key
                 FROM erpx_control.support_sla_notifications
                 WHERE expires_at > $1
                   AND EXISTS (
                       SELECT 1
                       FROM erpx_control.support_tickets ticket
                       WHERE ticket.id = support_sla_notifications.ticket_id
                         AND ticket.status <> 'closed'
                   )
                   AND ((status = 'pending' AND not_before <= $1)
                     OR (status = 'claimed' AND claim_expires_at <= $1))
                 ORDER BY not_before, notification_key
                 FOR UPDATE SKIP LOCKED
                 LIMIT $2
             )
             UPDATE erpx_control.support_sla_notifications AS notification
             SET status = 'claimed', claimed_at = $1,
                 claim_expires_at = $3, updated_at = $1
             FROM due
             WHERE notification.notification_key = due.notification_key
             RETURNING notification.payload",
        )
        .bind(at)
        .bind(i64::from(batch_size))
        .bind(lease_expires_at)
        .fetch_all(&mut *tx)
        .await?;
        let claimed = rows
            .into_iter()
            .map(
                |row| -> Result<SupportSlaNotification, SupportPersistenceError> {
                    let payload: Value = row.try_get("payload")?;
                    let notification = serde_json::from_value(payload)?;
                    validate_sla_notification(&notification)?;
                    Ok(notification)
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(claimed)
    }

    /// Claim due SLA notifications with an immutable worker/lease identity.
    /// The returned tuple is `(notification, claim_token, claimed_at,
    /// claim_expires_at)` and must be passed back unchanged to the matching
    /// acknowledgement method. The token is persisted before the payload is
    /// returned, so a reclaimed row cannot be acknowledged by an old worker.
    pub async fn claim_due_sla_notifications_for_worker(
        &self,
        claimant_id: Uuid,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<
        Vec<(SupportSlaNotification, Uuid, DateTime<Utc>, DateTime<Utc>)>,
        SupportPersistenceError,
    > {
        if claimant_id.is_nil() {
            return Err(SupportPersistenceError::InvalidData(
                "SLA notification claimant is required".into(),
            ));
        }
        if batch_size == 0 || batch_size > MAX_SUPPORT_SLA_NOTIFICATION_CLAIM_BATCH_SIZE {
            return Err(SupportPersistenceError::InvalidData(format!(
                "SLA notification claim batch must be 1..={MAX_SUPPORT_SLA_NOTIFICATION_CLAIM_BATCH_SIZE}"
            )));
        }
        let lease_expires_at = at + chrono::Duration::minutes(15);
        let claim_stamp = at.timestamp_nanos_opt().unwrap_or_default();
        let mut tx = self.write.begin().await?;
        let rows = sqlx::query(
            "WITH due AS (
                 SELECT notification_key
                 FROM erpx_control.support_sla_notifications
                 WHERE expires_at > $1
                   AND EXISTS (
                       SELECT 1
                       FROM erpx_control.support_tickets ticket
                       WHERE ticket.id = support_sla_notifications.ticket_id
                         AND ticket.status <> 'closed'
                   )
                   AND ((status = 'pending' AND not_before <= $1)
                     OR (status = 'claimed' AND claim_expires_at <= $1))
                 ORDER BY not_before, notification_key
                 FOR UPDATE SKIP LOCKED
                 LIMIT $2
             )
             UPDATE erpx_control.support_sla_notifications AS notification
             SET status = 'claimed', claimed_by = $4,
                 claim_token = md5(notification.notification_key || ':' || $4::text || ':' || $5::text)::uuid,
                 claimed_at = $1, claim_expires_at = $3, updated_at = $1
             FROM due
             WHERE notification.notification_key = due.notification_key
             RETURNING notification.payload, notification.claim_token,
                       notification.claimed_at, notification.claim_expires_at",
        )
        .bind(at)
        .bind(i64::from(batch_size))
        .bind(lease_expires_at)
        .bind(claimant_id)
        .bind(claim_stamp)
        .fetch_all(&mut *tx)
        .await?;
        let claimed = rows
            .into_iter()
            .map(
                |row| -> Result<
                    (SupportSlaNotification, Uuid, DateTime<Utc>, DateTime<Utc>),
                    SupportPersistenceError,
                > {
                    let payload: Value = row.try_get("payload")?;
                    let notification = serde_json::from_value(payload)?;
                    validate_sla_notification(&notification)?;
                    Ok((
                        notification,
                        row.try_get("claim_token")?,
                        row.try_get("claimed_at")?,
                        row.try_get("claim_expires_at")?,
                    ))
                },
            )
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(claimed)
    }

    /// Mark a bounded set of claimed notifications as delivered. The worker
    /// or channel adapter must call this only after the provider accepts the
    /// message; an expired lease remains retryable until then.
    pub async fn mark_sla_notifications_delivered(
        &self,
        keys: &[String],
        at: DateTime<Utc>,
    ) -> Result<u64, SupportPersistenceError> {
        let _ = (keys, at);
        Err(SupportPersistenceError::InvalidData(
            "SLA delivery requires a durable worker claim plus provider and channel identity"
                .into(),
        ))
    }

    /// Acknowledge only the exact durable worker claims returned by
    /// `claim_due_sla_notifications_for_worker`. A stale worker, token, or
    /// lease timestamp affects zero rows and therefore remains retryable.
    pub async fn mark_sla_notifications_delivered_for_worker(
        &self,
        claimant_id: Uuid,
        claims: &[(SupportSlaNotification, Uuid, DateTime<Utc>, DateTime<Utc>)],
        at: DateTime<Utc>,
    ) -> Result<u64, SupportPersistenceError> {
        let _ = (claimant_id, claims, at);
        Err(SupportPersistenceError::InvalidData(
            "SLA delivery requires an explicit provider and channel route".into(),
        ))
    }

    /// Acknowledge exact durable worker claims after a provider accepts them.
    /// The provider and channel are persisted in an append-only outbox event
    /// in the same transaction as the terminal notification transition. This
    /// keeps delivery reconciliation possible without adding provider secrets
    /// or mutable delivery metadata to the SLA aggregate.
    pub async fn mark_sla_notifications_delivered_for_worker_with_route(
        &self,
        claimant_id: Uuid,
        claims: &[(SupportSlaNotification, Uuid, DateTime<Utc>, DateTime<Utc>)],
        at: DateTime<Utc>,
        provider_ref: &str,
        channel: &str,
    ) -> Result<u64, SupportPersistenceError> {
        if claimant_id.is_nil()
            || claims.is_empty()
            || claims.len() > MAX_SUPPORT_SLA_NOTIFICATION_CLAIM_BATCH_SIZE as usize
        {
            return Err(SupportPersistenceError::InvalidData(
                "SLA durable acknowledgement batch is outside the supported bound".into(),
            ));
        }
        validate_delivery_route(provider_ref, channel)?;
        let mut tx = self.write.begin().await?;
        let mut affected = 0_u64;
        for (notification, claim_token, claimed_at, claim_expires_at) in claims {
            if notification.key.trim().is_empty()
                || claim_token.is_nil()
                || *claim_expires_at <= *claimed_at
            {
                return Err(SupportPersistenceError::InvalidData(
                    "SLA durable claim is invalid".into(),
                ));
            }
            let count = sqlx::query(
                "UPDATE erpx_control.support_sla_notifications
                 SET status = 'delivered', delivered_at = $5, updated_at = $5,
                     claimed_at = NULL, claim_expires_at = NULL,
                     claimed_by = NULL, claim_token = NULL
                 WHERE notification_key = $1 AND status = 'claimed'
                   AND claimed_by = $2 AND claim_token = $3
                   AND claimed_at = $4 AND claim_expires_at = $6
                   AND claim_expires_at > $5",
            )
            .bind(&notification.key)
            .bind(claimant_id)
            .bind(*claim_token)
            .bind(*claimed_at)
            .bind(at)
            .bind(*claim_expires_at)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            affected = affected.saturating_add(count);
            if count == 1 {
                append_sla_delivery_event(&mut tx, notification, at, provider_ref, channel).await?;
            }
        }
        tx.commit().await?;
        Ok(affected)
    }

    /// Atomically persist the result of one typed ticket command. Ticket
    /// metadata lives in the aggregate JSONB payload, so replies, internal
    /// notes, assignment, escalation, and SLA changes share the existing
    /// ticket table without a schema mutation. The idempotency reservation and
    /// optimistic version check occur in the same transaction.
    pub async fn apply_ticket_command(
        &self,
        ticket: &SupportTicket,
        command: &SupportTicketCommand,
        expected_version: u64,
        idempotency: &IdempotencyKey,
    ) -> Result<Persisted<SupportTicket>, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        validate_ticket(ticket)?;
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "ticket_update", ticket.id).await?;
        if !reservation {
            let stored = fetch_ticket_tx(&mut tx, ticket.id).await?.ok_or(
                SupportPersistenceError::NotFound {
                    entity: "support_ticket",
                    id: ticket.id,
                },
            )?;
            tx.commit().await?;
            return Ok(Persisted {
                value: stored.0,
                version: stored.1,
                reused_idempotency: true,
            });
        }
        let current = fetch_ticket_tx(&mut tx, ticket.id).await?.ok_or(
            SupportPersistenceError::NotFound {
                entity: "support_ticket",
                id: ticket.id,
            },
        )?;
        if !command.is_exactly_applied_to(&current.0, ticket) {
            return Err(SupportPersistenceError::InvalidData(format!(
                "ticket command result does not match aggregate state: {}",
                command.kind()
            )));
        }
        if current.0.status == TicketStatus::Closed {
            return Err(SupportPersistenceError::InvalidData(
                "closed support tickets are terminal".into(),
            ));
        }
        let next_version = checked_next(expected_version)?;
        let result = sqlx::query(
            "UPDATE erpx_control.support_tickets
             SET status = $2, severity = $3, payload = $4, version = $5, updated_at = now()
             WHERE id = $1 AND version = $6 AND status <> 'closed'",
        )
        .bind(ticket.id)
        .bind(ticket_status(ticket.status))
        .bind(&ticket.severity)
        .bind(serde_json::to_value(ticket)?)
        .bind(as_i64(next_version, "ticket version")?)
        .bind(as_i64(expected_version, "ticket version")?)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(
                concurrency_error(&mut tx, "support_ticket", ticket.id, expected_version).await?,
            );
        }
        let notifications = scheduled_sla_notifications(ticket);
        enqueue_sla_notifications_tx(&mut tx, &notifications).await?;
        append_support_ticket_event(&mut tx, ticket, next_version, command.event_type()).await?;
        tx.commit().await?;
        Ok(Persisted {
            value: ticket.clone(),
            version: next_version,
            reused_idempotency: false,
        })
    }

    pub async fn insert_access_request(
        &self,
        session: &SupportSession,
        idempotency: &IdempotencyKey,
    ) -> Result<Persisted<SupportSession>, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        validate_session(session)?;
        let mut tx = self.write.begin().await?;
        ensure_access_request_ticket_binding(&mut tx, session).await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "access_request_insert", session.id).await?;
        if reservation {
            sqlx::query(
                "INSERT INTO erpx_control.support_access_requests
                    (id, ticket_id, customer_account_id, agent_ref,
                     represented_tenant_user, mode, permissions, scope, reason,
                     requested_at, expires_at, status, version, payload)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'requested', 1, $12)",
            )
            .bind(session.id)
            .bind(session.ticket_id)
            .bind(session.customer_account_id)
            .bind(&session.agent_ref)
            .bind(&session.represented_tenant_user)
            .bind(support_mode(session.mode))
            .bind(serde_json::to_value(&session.permissions)?)
            .bind(&session.scope)
            .bind(&session.reason)
            .bind(session.requested_at)
            .bind(session.expires_at)
            .bind(serde_json::to_value(session)?)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(Persisted {
                value: session.clone(),
                version: 1,
                reused_idempotency: false,
            });
        }
        let stored = fetch_access_request_tx(&mut tx, session.id).await?;
        tx.commit().await?;
        Ok(Persisted {
            value: stored.0,
            version: stored.1,
            reused_idempotency: true,
        })
    }

    pub async fn approve_access_request(
        &self,
        id: Uuid,
        expected_version: u64,
        approval: &SupportApproval,
        idempotency: &IdempotencyKey,
    ) -> Result<Persisted<SupportSession>, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        if approval.approver_ref.trim().is_empty() || approval.approval_ref.trim().is_empty() {
            return Err(SupportPersistenceError::InvalidData(
                "approval references cannot be empty".into(),
            ));
        }
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "access_request_approve", id).await?;
        if !reservation {
            let stored = fetch_access_request_tx(&mut tx, id).await?;
            tx.commit().await?;
            return Ok(Persisted {
                value: stored.0,
                version: stored.1,
                reused_idempotency: true,
            });
        }
        let next_version = checked_next(expected_version)?;
        let approval_decision = json!({
            "approved_by": approval.approver_ref,
            "approver_ref": approval.approver_ref,
            "approval_ref": approval.approval_ref,
            "approved_at": approval.approved_at,
        });
        let result = sqlx::query(
            "UPDATE erpx_control.support_access_requests
             SET status = 'approved', approved_by = $2, approved_at = $3,
                 approval_ref = $4, payload = payload || $5,
                 version = $6, updated_at = now()
             WHERE id = $1 AND version = $7 AND status = 'requested' AND approved_at IS NULL
               AND $3 >= requested_at AND $3 < expires_at",
        )
        .bind(id)
        .bind(&approval.approver_ref)
        .bind(approval.approved_at)
        .bind(&approval.approval_ref)
        .bind(&approval_decision)
        .bind(as_i64(next_version, "access request version")?)
        .bind(as_i64(expected_version, "access request version")?)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(
                concurrency_error(&mut tx, "support_access_request", id, expected_version).await?,
            );
        }
        let (mut session, _) = fetch_access_request_tx(&mut tx, id).await?;
        session.approved_by = Some(approval.approver_ref.clone());
        session.approved_at = Some(approval.approved_at);
        tx.commit().await?;
        Ok(Persisted {
            value: session,
            version: next_version,
            reused_idempotency: false,
        })
    }

    pub async fn issue_capability(
        &self,
        capability: &SupportCapability,
        idempotency: &IdempotencyKey,
    ) -> Result<CapabilityMetadata, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        capability
            .binding
            .validate()
            .map_err(|error| SupportPersistenceError::InvalidData(error.to_string()))?;
        if capability.id.is_nil()
            || capability.revocation.is_some()
            || !capability.is_active(capability.issued_at)
        {
            return Err(SupportPersistenceError::InvalidData(
                "support capability must be active at issuance".into(),
            ));
        }
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "capability_issue", capability.id).await?;
        if reservation {
            ensure_capability_issue_binding(&mut tx, capability).await?;
            let mut payload = serde_json::to_value(capability)?;
            if let Some(object) = payload.as_object_mut() {
                object.remove("secret_hash");
            }
            let hash = secret_hash_bytes(capability)?;
            sqlx::query(
                "INSERT INTO erpx_control.support_capabilities
                    (id, access_request_id, customer_account_id, agent_ref, device_ref,
                     ticket_id, mode, permissions, scope, module, issued_at, expires_at,
                     secret_hash, reissued_from, status, version, payload)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                         $13, $14, 'active', 1, $15)",
            )
            .bind(capability.id)
            .bind(capability.binding.session_id)
            .bind(capability.binding.customer_account_id)
            .bind(&capability.binding.agent_ref)
            .bind(&capability.binding.device_ref)
            .bind(capability.binding.ticket_id)
            .bind(support_mode(capability.binding.mode))
            .bind(serde_json::to_value(&capability.binding.permissions)?)
            .bind(&capability.binding.scope)
            .bind(&capability.binding.module)
            .bind(capability.issued_at)
            .bind(capability.expires_at)
            .bind(hash)
            .bind(capability.reissued_from)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
            let metadata = fetch_capability(&mut tx, capability.id).await?;
            append_capability_issued_event(&mut tx, &metadata).await?;
            tx.commit().await?;
            return Ok(metadata);
        }
        let metadata = fetch_capability(&mut tx, capability.id).await?;
        tx.commit().await?;
        Ok(metadata)
    }

    pub async fn get_capability(
        &self,
        id: Uuid,
    ) -> Result<Option<CapabilityMetadata>, SupportPersistenceError> {
        let row = sqlx::query(
            "SELECT id, access_request_id, customer_account_id, agent_ref, device_ref,
                    ticket_id, mode, permissions, scope, module, issued_at, expires_at,
                    reissued_from, status, revoked_at, revocation_reason, version,
                    octet_length(secret_hash) AS secret_hash_length
             FROM erpx_control.support_capabilities WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.write)
        .await?;
        row.map(|row| capability_from_row(&row)).transpose()
    }

    /// Atomically revoke one bounded batch of active capabilities whose
    /// half-open validity window ended at or before `now`.
    ///
    /// The injected timestamp is used for both `revoked_at` and
    /// `updated_at`, making retries deterministic. `SKIP LOCKED` allows
    /// concurrent sweepers to make progress without claiming the same rows;
    /// the active/status predicate makes a repeated call a no-op for rows
    /// already handled by an earlier attempt.
    pub async fn sweep_expired_capabilities(
        &self,
        now: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<CapabilityMetadata>, SupportPersistenceError> {
        if batch_size == 0 || batch_size > MAX_EXPIRATION_SWEEP_BATCH_SIZE {
            return Err(SupportPersistenceError::InvalidData(format!(
                "expiration sweep batch size must be 1..={MAX_EXPIRATION_SWEEP_BATCH_SIZE}"
            )));
        }

        let mut tx = self.write.begin().await?;
        let ids = sqlx::query(
            "SELECT id
             FROM erpx_control.support_capabilities
             WHERE status = 'active' AND expires_at <= $1
             ORDER BY expires_at, id
             FOR UPDATE SKIP LOCKED
             LIMIT $2",
        )
        .bind(now)
        .bind(i64::from(batch_size))
        .fetch_all(&mut *tx)
        .await?;

        let mut expired = Vec::with_capacity(ids.len());
        for row in ids {
            let id: Uuid = row.try_get("id")?;
            let result = sqlx::query(
                "UPDATE erpx_control.support_capabilities
                 SET status = 'revoked', revoked_at = $2, revocation_reason = 'expired',
                     version = version + 1, updated_at = $2
                 WHERE id = $1 AND status = 'active' AND expires_at <= $2",
            )
            .bind(id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            if result.rows_affected() == 1 {
                let metadata = fetch_capability(&mut tx, id).await?;
                append_capability_revocation_event(&mut tx, &metadata).await?;
                expired.push(metadata);
            }
        }

        tx.commit().await?;
        Ok(expired)
    }

    pub async fn revoke_capability(
        &self,
        id: Uuid,
        expected_version: u64,
        reason: SupportCapabilityRevocationReason,
        at: DateTime<Utc>,
        idempotency: &IdempotencyKey,
    ) -> Result<CapabilityMetadata, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "capability_revoke", id).await?;
        if !reservation {
            let metadata = fetch_capability(&mut tx, id).await?;
            tx.commit().await?;
            return Ok(metadata);
        }
        let current = fetch_capability(&mut tx, id).await?;
        if at < current.issued_at {
            return Err(SupportPersistenceError::InvalidData(
                "support capability revocation cannot precede issuance".into(),
            ));
        }
        let next_version = checked_next(expected_version)?;
        let result = sqlx::query(
            "UPDATE erpx_control.support_capabilities
             SET status = 'revoked', revoked_at = $2, revocation_reason = $3,
                 version = $4, updated_at = now()
             WHERE id = $1 AND version = $5 AND revoked_at IS NULL AND status = 'active'",
        )
        .bind(id)
        .bind(at)
        .bind(capability_revocation_reason(reason))
        .bind(as_i64(next_version, "capability version")?)
        .bind(as_i64(expected_version, "capability version")?)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(
                concurrency_error(&mut tx, "support_capability", id, expected_version).await?,
            );
        }
        let metadata = fetch_capability(&mut tx, id).await?;
        append_capability_revocation_event(&mut tx, &metadata).await?;
        tx.commit().await?;
        Ok(metadata)
    }

    /// Revoke one bounded batch of active capabilities matching the supplied
    /// customer, agent, and/or ticket scope.
    ///
    /// A scope must contain at least one selector. The active predicate makes
    /// repeated delivery idempotent: capabilities already revoked by an
    /// earlier delivery are not updated again and therefore do not receive a
    /// second version increment. `FOR UPDATE SKIP LOCKED` permits concurrent
    /// workers to drain the same logical cascade without claiming the same
    /// capability row.
    pub async fn revoke_active_capabilities_by_scope(
        &self,
        customer_account_id: Option<Uuid>,
        agent_ref: Option<&str>,
        ticket_id: Option<Uuid>,
        reason: SupportCapabilityRevocationReason,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<CapabilityMetadata>, SupportPersistenceError> {
        validate_cascade_scope(customer_account_id, agent_ref, ticket_id)?;
        if batch_size == 0 || batch_size > MAX_CASCADE_REVOCATION_BATCH_SIZE {
            return Err(SupportPersistenceError::InvalidData(format!(
                "cascade revocation batch size must be 1..={MAX_CASCADE_REVOCATION_BATCH_SIZE}"
            )));
        }
        let reason = cascade_revocation_reason(reason)?;
        let mut tx = self.write.begin().await?;
        let rows = sqlx::query(
            "WITH candidates AS (
                 SELECT id
                 FROM erpx_control.support_capabilities
                 WHERE status = 'active'
                   AND ($2::uuid IS NULL OR customer_account_id = $2)
                   AND ($3::text IS NULL OR agent_ref = $3)
                   AND ($4::uuid IS NULL OR ticket_id = $4)
                   AND issued_at <= $5
                 ORDER BY customer_account_id, agent_ref, ticket_id, id
                 FOR UPDATE SKIP LOCKED
                 LIMIT $1
             )
             UPDATE erpx_control.support_capabilities AS capability
             SET status = 'revoked', revoked_at = $5, revocation_reason = $6,
                 version = capability.version + 1, updated_at = $5
             FROM candidates
             WHERE capability.id = candidates.id
             RETURNING capability.id, capability.access_request_id,
                       capability.customer_account_id, capability.agent_ref,
                       capability.device_ref, capability.ticket_id,
                       capability.mode, capability.permissions, capability.scope,
                       capability.module, capability.issued_at,
                       capability.expires_at, capability.reissued_from,
                       capability.status, capability.revoked_at,
                       capability.revocation_reason, capability.version,
                       octet_length(capability.secret_hash) AS secret_hash_length",
        )
        .bind(i64::from(batch_size))
        .bind(customer_account_id)
        .bind(agent_ref)
        .bind(ticket_id)
        .bind(at)
        .bind(reason)
        .fetch_all(&mut *tx)
        .await?;

        let metadata = rows
            .iter()
            .map(capability_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        for capability in &metadata {
            append_capability_revocation_event(&mut tx, capability).await?;
        }
        tx.commit().await?;
        Ok(metadata)
    }

    /// Revoke active capabilities for a suspended customer.
    pub async fn revoke_active_capabilities_by_customer(
        &self,
        customer_account_id: Uuid,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<CapabilityMetadata>, SupportPersistenceError> {
        self.revoke_active_capabilities_by_scope(
            Some(customer_account_id),
            None,
            None,
            SupportCapabilityRevocationReason::CustomerSuspended,
            at,
            batch_size,
        )
        .await
    }

    /// Revoke active capabilities granted under an expired agent role.
    pub async fn revoke_active_capabilities_by_agent(
        &self,
        agent_ref: &str,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<CapabilityMetadata>, SupportPersistenceError> {
        self.revoke_active_capabilities_by_scope(
            None,
            Some(agent_ref),
            None,
            SupportCapabilityRevocationReason::AgentRoleExpired,
            at,
            batch_size,
        )
        .await
    }

    /// Revoke active capabilities inherited from an expired parent role.
    pub async fn revoke_active_capabilities_by_parent_role(
        &self,
        agent_ref: &str,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<CapabilityMetadata>, SupportPersistenceError> {
        self.revoke_active_capabilities_by_scope(
            None,
            Some(agent_ref),
            None,
            SupportCapabilityRevocationReason::ParentRoleExpired,
            at,
            batch_size,
        )
        .await
    }

    /// Revoke active capabilities attached to a closed ticket.
    pub async fn revoke_active_capabilities_by_ticket(
        &self,
        ticket_id: Uuid,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<Vec<CapabilityMetadata>, SupportPersistenceError> {
        self.revoke_active_capabilities_by_scope(
            None,
            None,
            Some(ticket_id),
            SupportCapabilityRevocationReason::TicketClosed,
            at,
            batch_size,
        )
        .await
    }

    pub async fn insert_incident(
        &self,
        incident: &Incident,
        idempotency: &IdempotencyKey,
    ) -> Result<Persisted<Incident>, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        validate_incident(incident)?;
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "incident_insert", incident.id).await?;
        if reservation {
            sqlx::query(
                "INSERT INTO erpx_control.incidents (id, status, version, payload)
                 VALUES ($1, $2, 1, $3)",
            )
            .bind(incident.id)
            .bind(incident_status(incident.status))
            .bind(serde_json::to_value(incident)?)
            .execute(&mut *tx)
            .await?;
            append_incident_event(&mut tx, incident, 1, "incident.created").await?;
            tx.commit().await?;
            return Ok(Persisted {
                value: incident.clone(),
                version: 1,
                reused_idempotency: false,
            });
        }
        let stored = fetch_incident_tx(&mut tx, incident.id).await?;
        tx.commit().await?;
        Ok(Persisted {
            value: stored.0,
            version: stored.1,
            reused_idempotency: true,
        })
    }

    pub async fn get_incident(
        &self,
        id: Uuid,
    ) -> Result<Option<Persisted<Incident>>, SupportPersistenceError> {
        let row = sqlx::query("SELECT payload, version FROM erpx_control.incidents WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.write)
            .await?;
        row.map(|row| {
            Ok(Persisted {
                value: validated_incident(row.try_get("payload")?)?,
                version: positive_u64(row.try_get("version")?, "incident version")?,
                reused_idempotency: false,
            })
        })
        .transpose()
    }

    /// Read safe incident aggregate summaries in deterministic newest-first
    /// order. The payload is rehydrated only to reuse the aggregate's safe
    /// DTO mapping; internal timeline and postmortem fields never leave this
    /// persistence boundary.
    pub async fn list_incidents(
        &self,
        status: Option<IncidentStatus>,
        page_size: u32,
    ) -> Result<IncidentPage, SupportPersistenceError> {
        if page_size == 0 || page_size > MAX_INCIDENT_LIST_PAGE_SIZE {
            return Err(SupportPersistenceError::InvalidData(
                "incident list page size is outside the supported bound".into(),
            ));
        }
        let query_limit = i64::from(page_size) + 1;
        let status_name = status.map(incident_status);
        let rows = sqlx::query(
            "SELECT payload, version
             FROM erpx_control.incidents
             WHERE ($1::text IS NULL OR status = $1)
             ORDER BY updated_at DESC, id DESC
             LIMIT $2",
        )
        .bind(status_name)
        .bind(query_limit)
        .fetch_all(&self.write)
        .await?;

        let mut incidents = rows
            .into_iter()
            .map(|row| {
                Ok(Persisted {
                    value: validated_incident(row.try_get("payload")?)?,
                    version: positive_u64(row.try_get("version")?, "incident version")?,
                    reused_idempotency: false,
                })
            })
            .collect::<Result<Vec<_>, SupportPersistenceError>>()?;
        let has_more = incidents.len()
            > usize::try_from(page_size).map_err(|_| {
                SupportPersistenceError::InvalidData("incident list page size is invalid".into())
            })?;
        if has_more {
            incidents.truncate(usize::try_from(page_size).map_err(|_| {
                SupportPersistenceError::InvalidData("incident list page size is invalid".into())
            })?);
        }
        Ok(IncidentPage {
            incidents,
            has_more,
        })
    }

    pub async fn update_incident(
        &self,
        incident: &Incident,
        expected_version: u64,
        idempotency: &IdempotencyKey,
    ) -> Result<Persisted<Incident>, SupportPersistenceError> {
        validate_idempotency(idempotency)?;
        validate_incident(incident)?;
        let mut tx = self.write.begin().await?;
        let reservation =
            reserve_idempotency(&mut tx, idempotency, "incident_update", incident.id).await?;
        if !reservation {
            let stored = fetch_incident_tx(&mut tx, incident.id).await?;
            tx.commit().await?;
            return Ok(Persisted {
                value: stored.0,
                version: stored.1,
                reused_idempotency: true,
            });
        }
        let current = fetch_incident_tx(&mut tx, incident.id).await?;
        if current.0.commander_ref != incident.commander_ref
            || current.0.leads != incident.leads
            || current.0.affected_services != incident.affected_services
            || current.0.affected_customer_accounts != incident.affected_customer_accounts
        {
            return Err(SupportPersistenceError::InvalidData(
                "incident identity and affected-customer binding are immutable".into(),
            ));
        }
        let next_version = checked_next(expected_version)?;
        let result = sqlx::query(
            "UPDATE erpx_control.incidents
             SET status = $2, payload = $3, version = $4, updated_at = now()
             WHERE id = $1 AND version = $5",
        )
        .bind(incident.id)
        .bind(incident_status(incident.status))
        .bind(serde_json::to_value(incident)?)
        .bind(as_i64(next_version, "incident version")?)
        .bind(as_i64(expected_version, "incident version")?)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(
                concurrency_error(&mut tx, "incident", incident.id, expected_version).await?,
            );
        }
        append_incident_event(&mut tx, incident, next_version, "incident.updated").await?;
        tx.commit().await?;
        Ok(Persisted {
            value: incident.clone(),
            version: next_version,
            reused_idempotency: false,
        })
    }
}

async fn reserve_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    idempotency: &IdempotencyKey,
    command_type: &str,
    resource_id: Uuid,
) -> Result<bool, SupportPersistenceError> {
    let result = sqlx::query(
        "INSERT INTO erpx_control.support_command_idempotency
            (idempotency_key, request_hash, command_type, resource_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(&idempotency.key)
    .bind(&idempotency.request_hash)
    .bind(command_type)
    .bind(resource_id)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() == 1 {
        return Ok(true);
    }
    let row = sqlx::query(
        "SELECT request_hash, command_type, resource_id
         FROM erpx_control.support_command_idempotency
         WHERE idempotency_key = $1 FOR UPDATE",
    )
    .bind(&idempotency.key)
    .fetch_one(&mut **tx)
    .await?;
    if row.try_get::<String, _>("request_hash")? != idempotency.request_hash
        || row.try_get::<String, _>("command_type")? != command_type
        || row.try_get::<Uuid, _>("resource_id")? != resource_id
    {
        return Err(SupportPersistenceError::IdempotencyConflict {
            key: idempotency.key.clone(),
        });
    }
    Ok(false)
}

async fn fetch_ticket_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<Option<(SupportTicket, u64)>, SupportPersistenceError> {
    let row =
        sqlx::query("SELECT payload, version FROM erpx_control.support_tickets WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await?;
    row.map(|row| {
        Ok((
            validated_ticket(row.try_get("payload")?)?,
            positive_u64(row.try_get("version")?, "ticket version")?,
        ))
    })
    .transpose()
}

async fn fetch_ticket_pool(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<(SupportTicket, u64)>, SupportPersistenceError> {
    let row =
        sqlx::query("SELECT payload, version FROM erpx_control.support_tickets WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    row.map(|row| {
        Ok((
            validated_ticket(row.try_get("payload")?)?,
            positive_u64(row.try_get("version")?, "ticket version")?,
        ))
    })
    .transpose()
}

async fn fetch_ticket_for_update(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<(SupportTicket, u64), SupportPersistenceError> {
    let row = sqlx::query(
        "SELECT payload, version
         FROM erpx_control.support_tickets
         WHERE id = $1
         FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(SupportPersistenceError::NotFound {
        entity: "support_ticket",
        id,
    })?;
    Ok((
        validated_ticket(row.try_get("payload")?)?,
        positive_u64(row.try_get("version")?, "ticket version")?,
    ))
}

async fn fetch_ticket_version_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<u64, SupportPersistenceError> {
    let row = sqlx::query("SELECT version FROM erpx_control.support_tickets WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(SupportPersistenceError::NotFound {
            entity: "support_ticket",
            id,
        })?;
    positive_u64(row.try_get("version")?, "ticket version")
}

/// A support access request may only authorize the customer that owns its
/// ticket. The ticket foreign key enforces existence, but it cannot enforce
/// this cross-aggregate customer binding; check the authoritative ticket row
/// before reserving idempotency or writing the request.
async fn ensure_access_request_ticket_binding(
    tx: &mut Transaction<'_, Postgres>,
    session: &SupportSession,
) -> Result<(), SupportPersistenceError> {
    if session.ticket_id.is_nil() || session.customer_account_id.is_nil() {
        return Err(SupportPersistenceError::InvalidData(
            "support access request ticket and customer IDs are required".into(),
        ));
    }
    let ticket = sqlx::query(
        "SELECT customer_account_id, status
         FROM erpx_control.support_tickets
         WHERE id = $1
         FOR SHARE",
    )
    .bind(session.ticket_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(SupportPersistenceError::NotFound {
        entity: "support_ticket",
        id: session.ticket_id,
    })?;
    let ticket_customer_id: Uuid = ticket.try_get("customer_account_id")?;
    validate_ticket_support_access_status(ticket.try_get("status")?)?;
    if ticket_customer_id != session.customer_account_id {
        return Err(SupportPersistenceError::AccessRequestTicketBindingDenied);
    }
    Ok(())
}

/// Re-check the approval and ticket source immediately before capability
/// issuance. The database trigger protects the immutable columns, but it does
/// not fence a capability against a ticket that became terminal after the
/// access request was approved. Keeping this check in the same transaction as
/// the insert closes that stale-grant window without changing the schema.
async fn ensure_capability_issue_binding(
    tx: &mut Transaction<'_, Postgres>,
    capability: &SupportCapability,
) -> Result<(), SupportPersistenceError> {
    let request = sqlx::query(
        "SELECT ticket_id, customer_account_id, agent_ref, mode, permissions,
                scope, payload, approved_at, expires_at, status
         FROM erpx_control.support_access_requests
         WHERE id = $1
         FOR SHARE",
    )
    .bind(capability.binding.session_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(SupportPersistenceError::NotFound {
        entity: "support_access_request",
        id: capability.binding.session_id,
    })?;

    let status: String = request.try_get("status")?;
    let approved_at: Option<DateTime<Utc>> = request.try_get("approved_at")?;
    if status != "approved" || approved_at.is_none() {
        return Err(SupportPersistenceError::InvalidData(
            "support capability requires an approved access request".into(),
        ));
    }
    let ticket_id: Uuid = request.try_get("ticket_id")?;
    let customer_account_id: Uuid = request.try_get("customer_account_id")?;
    let agent_ref: String = request.try_get("agent_ref")?;
    let mode: String = request.try_get("mode")?;
    let permissions: Value = request.try_get("permissions")?;
    let scope: String = request.try_get("scope")?;
    let payload: Value = request.try_get("payload")?;
    let approved_module = payload
        .get("module")
        .and_then(Value::as_str)
        .unwrap_or("support");
    let request_expires_at: DateTime<Utc> = request.try_get("expires_at")?;

    if ticket_id != capability.binding.ticket_id
        || customer_account_id != capability.binding.customer_account_id
        || agent_ref != capability.binding.agent_ref
        || mode != support_mode(capability.binding.mode)
        || permissions != serde_json::to_value(&capability.binding.permissions)?
        || scope != capability.binding.scope
        || approved_module != capability.binding.module
        || capability.issued_at < approved_at.expect("checked above")
        || capability.expires_at > request_expires_at
    {
        return Err(SupportPersistenceError::InvalidData(
            "support capability binding exceeds customer approval".into(),
        ));
    }

    let ticket_status = sqlx::query_scalar::<_, String>(
        "SELECT status
         FROM erpx_control.support_tickets
         WHERE id = $1
         FOR SHARE",
    )
    .bind(ticket_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(SupportPersistenceError::NotFound {
        entity: "support_ticket",
        id: ticket_id,
    })?;
    validate_ticket_support_access_status(&ticket_status)
}

fn validate_ticket_support_access_status(status: &str) -> Result<(), SupportPersistenceError> {
    match status {
        "new"
        | "triaged"
        | "waiting_for_customer"
        | "access_requested"
        | "approved"
        | "in_progress"
        | "resolved"
        | "customer_confirmed" => Ok(()),
        "closed" => Err(SupportPersistenceError::InvalidData(
            "closed support tickets cannot receive support access".into(),
        )),
        _ => Err(SupportPersistenceError::InvalidData(
            "support ticket status is invalid for support access".into(),
        )),
    }
}

async fn fetch_attachment_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    for_update: bool,
) -> Result<Option<(SupportAttachment, String, u64)>, SupportPersistenceError> {
    let query = if for_update {
        "SELECT id, ticket_id, customer_account_id, object_ref, content_type,
                content_size_bytes, sha256_hex, visibility, uploader_ref, scope,
                uploaded_at, expires_at, status, version
         FROM erpx_control.support_attachments
         WHERE id = $1
         FOR UPDATE"
    } else {
        "SELECT id, ticket_id, customer_account_id, object_ref, content_type,
                content_size_bytes, sha256_hex, visibility, uploader_ref, scope,
                uploaded_at, expires_at, status, version
         FROM erpx_control.support_attachments
         WHERE id = $1"
    };
    let row = sqlx::query(query)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?;
    row.map(|row| attachment_from_row(&row)).transpose()
}

fn attachment_from_row(
    row: &PgRow,
) -> Result<(SupportAttachment, String, u64), SupportPersistenceError> {
    let status: String = row.try_get("status")?;
    if !matches!(status.as_str(), "active" | "revoked" | "expired") {
        return Err(SupportPersistenceError::InvalidData(format!(
            "unknown support attachment status {status}"
        )));
    }
    let visibility = parse_attachment_visibility(row.try_get::<String, _>("visibility")?.as_str())?;
    let content_size_bytes = positive_u64(
        row.try_get("content_size_bytes")?,
        "attachment content size",
    )?;
    let scope = serde_json::from_value(row.try_get("scope")?)?;
    let attachment = SupportAttachment {
        id: row.try_get("id")?,
        ticket_id: row.try_get("ticket_id")?,
        customer_account_id: row.try_get("customer_account_id")?,
        object_ref: row.try_get("object_ref")?,
        content_type: row.try_get("content_type")?,
        content_size_bytes,
        sha256_hex: row.try_get("sha256_hex")?,
        visibility,
        uploader_ref: row.try_get("uploader_ref")?,
        scope,
        uploaded_at: row.try_get("uploaded_at")?,
        expires_at: row.try_get("expires_at")?,
    };
    attachment
        .validate()
        .map_err(|error| SupportPersistenceError::InvalidData(error.to_string()))?;
    Ok((
        attachment,
        status,
        positive_u64(row.try_get("version")?, "attachment version")?,
    ))
}

fn ensure_attachment_binding(
    attachment: &SupportAttachment,
    ticket_id: Uuid,
    customer_account_id: Uuid,
) -> Result<(), SupportPersistenceError> {
    if attachment.ticket_id != ticket_id || attachment.customer_account_id != customer_account_id {
        return Err(SupportPersistenceError::AttachmentBindingDenied);
    }
    Ok(())
}

fn ensure_attachment_scope(
    actor_scope: &TenantScope,
    attachment_scope: &TenantScope,
) -> Result<(), SupportPersistenceError> {
    if actor_scope.contains(attachment_scope) {
        Ok(())
    } else {
        Err(SupportPersistenceError::AttachmentScopeDenied)
    }
}

pub(crate) fn validate_ticket_list_page_size(
    page_size: u32,
) -> Result<usize, SupportPersistenceError> {
    if page_size == 0 || page_size > MAX_SUPPORT_TICKET_LIST_PAGE_SIZE {
        return Err(SupportPersistenceError::InvalidData(
            "support ticket list page size is outside the supported bound".into(),
        ));
    }
    usize::try_from(page_size).map_err(|_| {
        SupportPersistenceError::InvalidData("support ticket list page size is invalid".into())
    })
}

pub(crate) fn validate_ticket_customer_filter(
    customer_account_id: Option<Uuid>,
) -> Result<(), SupportPersistenceError> {
    if customer_account_id.is_some_and(|id| id.is_nil()) {
        return Err(SupportPersistenceError::InvalidData(
            "support ticket customer filter cannot be nil".into(),
        ));
    }
    Ok(())
}

fn validate_attachment_page_size(page_size: u32) -> Result<(), SupportPersistenceError> {
    if page_size == 0 || page_size > MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE {
        return Err(SupportPersistenceError::InvalidData(format!(
            "attachment list page size must be 1..={MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn map_attachment_domain_error(
    error: erpx_domain::support::SupportError,
) -> SupportPersistenceError {
    match error {
        erpx_domain::support::SupportError::ClosedTicketTerminal => {
            SupportPersistenceError::AttachmentClosedTicket
        }
        erpx_domain::support::SupportError::SupportAttachmentLimitExceeded => {
            SupportPersistenceError::AttachmentLimitExceeded
        }
        erpx_domain::support::SupportError::SupportAttachmentTicketBindingDenied => {
            SupportPersistenceError::AttachmentBindingDenied
        }
        erpx_domain::support::SupportError::SupportAttachmentExpired => {
            SupportPersistenceError::AttachmentNotActive
        }
        other => SupportPersistenceError::InvalidData(other.to_string()),
    }
}

async fn fetch_access_request_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<(SupportSession, u64), SupportPersistenceError> {
    let row = sqlx::query(
        "SELECT payload, version FROM erpx_control.support_access_requests WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(SupportPersistenceError::NotFound {
        entity: "support_access_request",
        id,
    })?;
    Ok((
        validated_session(row.try_get("payload")?)?,
        positive_u64(row.try_get("version")?, "access request version")?,
    ))
}

async fn fetch_incident_tx(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<(Incident, u64), SupportPersistenceError> {
    let row = sqlx::query("SELECT payload, version FROM erpx_control.incidents WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(SupportPersistenceError::NotFound {
            entity: "incident",
            id,
        })?;
    Ok((
        validated_incident(row.try_get("payload")?)?,
        positive_u64(row.try_get("version")?, "incident version")?,
    ))
}

async fn fetch_capability(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<CapabilityMetadata, SupportPersistenceError> {
    let row = sqlx::query(
        "SELECT id, access_request_id, customer_account_id, agent_ref, device_ref,
                ticket_id, mode, permissions, scope, module, issued_at, expires_at,
                reissued_from, status, revoked_at, revocation_reason, version,
                octet_length(secret_hash) AS secret_hash_length
         FROM erpx_control.support_capabilities WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(SupportPersistenceError::NotFound {
        entity: "support_capability",
        id,
    })?;
    capability_from_row(&row)
}

fn capability_issued_event_payload(metadata: &CapabilityMetadata) -> Value {
    json!({
        "capability_id": metadata.id,
        "access_request_id": metadata.binding.session_id,
        "customer_account_id": metadata.binding.customer_account_id,
        "agent_ref": metadata.binding.agent_ref,
        "device_ref": metadata.binding.device_ref,
        "ticket_id": metadata.binding.ticket_id,
        "mode": support_mode(metadata.binding.mode),
        "permissions": metadata.binding.permissions,
        "scope": metadata.binding.scope,
        "module": metadata.binding.module,
        "issued_at": metadata.issued_at,
        "expires_at": metadata.expires_at,
        "reissued_from": metadata.reissued_from,
        "status": metadata.status,
        "version": metadata.version,
    })
}

async fn append_capability_issued_event(
    tx: &mut Transaction<'_, Postgres>,
    metadata: &CapabilityMetadata,
) -> Result<(), SupportPersistenceError> {
    if metadata.id.is_nil()
        || metadata.binding.session_id.is_nil()
        || metadata.version != 1
        || metadata.status != "active"
        || metadata.revoked_at.is_some()
        || metadata.revocation_reason.is_some()
    {
        return Err(SupportPersistenceError::InvalidData(
            "support capability issuance event identity is invalid".into(),
        ));
    }
    let tenant_scope = json!({ "support_scope": metadata.binding.scope });
    let actor = json!({ "principal_ref": "support-capability-issuance" });
    let idempotency_key = format!(
        "support-capability-issued:{}:{}",
        metadata.id, metadata.version
    );
    let payload = capability_issued_event_payload(metadata);
    let occurred_at = metadata.issued_at;
    let event_id = Uuid::new_v4();
    let inserted = sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, tenant_scope, actor, correlation_id,
             idempotency_key, payload, occurred_at, available_at)
         VALUES ($1, 'support_capability.issued', 1, 'support_capability',
                 $2, $3, $4, $5, $1, $6, $7, $8, $8)
         ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(event_id)
    .bind(metadata.id)
    .bind(as_i64(metadata.version, "capability event version")?)
    .bind(&tenant_scope)
    .bind(&actor)
    .bind(&idempotency_key)
    .bind(&payload)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok(());
    }

    let existing = sqlx::query(
        "SELECT event_type, event_version, aggregate_type, aggregate_id,
                aggregate_version, tenant_scope, actor, payload, occurred_at
         FROM erpx_control.outbox_events
         WHERE idempotency_key = $1
         FOR SHARE",
    )
    .bind(&idempotency_key)
    .fetch_one(&mut **tx)
    .await?;
    let matches_source = existing.try_get::<String, _>("event_type")?
        == "support_capability.issued"
        && existing.try_get::<i32, _>("event_version")? == 1
        && existing.try_get::<String, _>("aggregate_type")? == "support_capability"
        && existing.try_get::<Uuid, _>("aggregate_id")? == metadata.id
        && positive_u64(
            existing.try_get("aggregate_version")?,
            "capability event version",
        )? == metadata.version
        && existing.try_get::<Value, _>("tenant_scope")? == tenant_scope
        && existing.try_get::<Value, _>("actor")? == actor
        && existing.try_get::<Value, _>("payload")? == payload
        && existing.try_get::<DateTime<Utc>, _>("occurred_at")? == occurred_at;
    if !matches_source {
        return Err(SupportPersistenceError::IdempotencyConflict {
            key: idempotency_key,
        });
    }
    Ok(())
}

async fn append_capability_revocation_event(
    tx: &mut Transaction<'_, Postgres>,
    metadata: &CapabilityMetadata,
) -> Result<(), SupportPersistenceError> {
    let revoked_at = metadata.revoked_at.ok_or_else(|| {
        SupportPersistenceError::InvalidData(
            "revoked support capability has no revocation timestamp".into(),
        )
    })?;
    let reason = metadata.revocation_reason.as_deref().ok_or_else(|| {
        SupportPersistenceError::InvalidData(
            "revoked support capability has no revocation reason".into(),
        )
    })?;
    if metadata.status != "revoked" || metadata.version == 0 {
        return Err(SupportPersistenceError::InvalidData(
            "support capability revocation event identity is invalid".into(),
        ));
    }
    let event_id = Uuid::new_v4();
    let payload = json!({
        "capability_id": metadata.id,
        "access_request_id": metadata.binding.session_id,
        "customer_account_id": metadata.binding.customer_account_id,
        "agent_ref": metadata.binding.agent_ref,
        "device_ref": metadata.binding.device_ref,
        "ticket_id": metadata.binding.ticket_id,
        "mode": support_mode(metadata.binding.mode),
        "permissions": metadata.binding.permissions,
        "scope": metadata.binding.scope,
        "module": metadata.binding.module,
        "version": metadata.version,
        "status": metadata.status,
        "revocation_reason": reason,
        "revoked_at": revoked_at,
    });
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, tenant_scope, actor, correlation_id,
             idempotency_key, payload,
             occurred_at, available_at)
         VALUES ($1, 'support_capability.revoked', 1, 'support_capability',
                 $2, $3, $4, $5, $1, $6, $7, $8, $8)",
    )
    .bind(event_id)
    .bind(metadata.id)
    .bind(as_i64(metadata.version, "capability event version")?)
    .bind(json!({ "support_scope": metadata.binding.scope }))
    .bind(json!({ "principal_ref": "support-capability-revocation" }))
    .bind(format!(
        "support-capability-revoked:{}:{}",
        metadata.id, metadata.version
    ))
    .bind(payload)
    .bind(revoked_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Append the immutable source fact for a support-ticket aggregate mutation.
///
/// Ticket rows remain authoritative on the control-plane write side, but a
/// projector and reconciliation pass need a durable event boundary. The
/// event is written in the same transaction as the aggregate row and uses
/// the aggregate version as its idempotency identity, so a retry cannot
/// manufacture a second source fact or advance the read model twice.
async fn append_support_ticket_event(
    tx: &mut Transaction<'_, Postgres>,
    ticket: &SupportTicket,
    version: u64,
    event_type: &str,
) -> Result<(), SupportPersistenceError> {
    if version == 0 || event_type.trim().is_empty() || event_type.len() > 128 {
        return Err(SupportPersistenceError::InvalidData(
            "support ticket event identity is invalid".into(),
        ));
    }
    let event_id = Uuid::new_v4();
    let payload = json!({
        "ticket": serde_json::to_value(ticket)?,
        "ticket_id": ticket.id,
        "customer_account_id": ticket.customer_account_id,
        "status": ticket_status(ticket.status),
        "version": version,
    });
    let occurred_at = Utc::now();
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, correlation_id, idempotency_key, payload,
             occurred_at, available_at)
         VALUES ($1, $2, 1, 'support_ticket', $3, $4, $1, $5, $6, $7, $7)",
    )
    .bind(event_id)
    .bind(event_type)
    .bind(ticket.id)
    .bind(as_i64(version, "support ticket event version")?)
    .bind(format!("support-ticket:{}:{}", ticket.id, version))
    .bind(payload)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Append the immutable lifecycle fact for attachment metadata. Attachment
/// bytes are never copied into the outbox; the opaque object reference is
/// retained only in the authoritative metadata payload for an authorized
/// projector or reconciliation process.
async fn append_support_attachment_event(
    tx: &mut Transaction<'_, Postgres>,
    attachment: &SupportAttachment,
    status: &str,
    version: u64,
    occurred_at: DateTime<Utc>,
    ticket_version: u64,
) -> Result<(), SupportPersistenceError> {
    if attachment.id.is_nil()
        || version == 0
        || ticket_version == 0
        || !matches!(status, "active" | "revoked" | "expired")
    {
        return Err(SupportPersistenceError::InvalidData(
            "support attachment event identity is invalid".into(),
        ));
    }
    let event_id = Uuid::new_v4();
    let event_type = match status {
        "active" => "support_attachment.created",
        "revoked" => "support_attachment.revoked",
        "expired" => "support_attachment.expired",
        _ => unreachable!("status was validated above"),
    };
    let payload = json!({
        "attachment": serde_json::to_value(attachment)?,
        "attachment_id": attachment.id,
        "ticket_id": attachment.ticket_id,
        "customer_account_id": attachment.customer_account_id,
        "status": status,
        "version": version,
        "ticket_version": ticket_version,
    });
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, tenant_scope, actor, correlation_id,
             idempotency_key, payload, occurred_at, available_at)
         VALUES ($1, $2, 1, 'support_attachment', $3, $4, $5, $6, $1, $7, $8, $9, $9)",
    )
    .bind(event_id)
    .bind(event_type)
    .bind(attachment.id)
    .bind(as_i64(version, "support attachment event version")?)
    .bind(serde_json::to_value(&attachment.scope)?)
    .bind(json!({ "principal_ref": attachment.uploader_ref }))
    .bind(format!("support-attachment:{}:{}", attachment.id, version))
    .bind(payload)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Append the complete incident source fact in the same transaction as the
/// authoritative incident row. The support read adapter performs the
/// redaction; this event is never exposed as a customer-facing read payload.
async fn append_incident_event(
    tx: &mut Transaction<'_, Postgres>,
    incident: &Incident,
    version: u64,
    event_type: &str,
) -> Result<(), SupportPersistenceError> {
    if incident.id.is_nil() || version == 0 || event_type.trim().is_empty() {
        return Err(SupportPersistenceError::InvalidData(
            "incident event identity is invalid".into(),
        ));
    }
    let event_id = Uuid::new_v4();
    let occurred_at = Utc::now();
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, correlation_id, idempotency_key, payload,
             occurred_at, available_at)
         VALUES ($1, $2, 1, 'incident', $3, $4, $1, $5, $6, $7, $7)",
    )
    .bind(event_id)
    .bind(event_type)
    .bind(incident.id)
    .bind(as_i64(version, "incident event version")?)
    .bind(format!("support-incident:{}:{}", incident.id, version))
    .bind(serde_json::to_value(incident)?)
    .bind(occurred_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn append_sla_delivery_event(
    tx: &mut Transaction<'_, Postgres>,
    notification: &SupportSlaNotification,
    at: DateTime<Utc>,
    provider_ref: &str,
    channel: &str,
) -> Result<(), SupportPersistenceError> {
    validate_delivery_route(provider_ref, channel)?;
    let aggregate_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:support_sla_notification:{}", notification.key).as_bytes(),
    );
    let event_id = Uuid::new_v4();
    let payload = json!({
        "notification_key": notification.key,
        "ticket_id": notification.ticket_id,
        "customer_account_id": notification.customer_account_id,
        "classification": sla_classification(notification.classification),
        "audience": sla_audience(notification.audience),
        "provider_ref": provider_ref,
        "channel": channel,
        "delivered_at": at,
    });
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, correlation_id, idempotency_key, payload,
             occurred_at, available_at)
         VALUES ($1, 'support_sla_notification.delivered', 1,
                 'support_sla_notification', $2, 1, $1, $3, $4, $5, $5)",
    )
    .bind(event_id)
    .bind(aggregate_id)
    .bind(format!("support-sla-delivered:{aggregate_id}"))
    .bind(payload)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_delivery_route(
    provider_ref: &str,
    channel: &str,
) -> Result<(), SupportPersistenceError> {
    for (value, field) in [(provider_ref, "provider reference"), (channel, "channel")] {
        if value.trim().is_empty()
            || value.len() > 128
            || value.chars().any(char::is_control)
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
            })
        {
            return Err(SupportPersistenceError::InvalidData(format!(
                "SLA delivery {field} is invalid"
            )));
        }
    }
    Ok(())
}

fn capability_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<CapabilityMetadata, SupportPersistenceError> {
    let permissions: Vec<String> = serde_json::from_value(row.try_get("permissions")?)?;
    let binding = SupportCapabilityBinding::new(
        row.try_get("customer_account_id")?,
        row.try_get::<String, _>("agent_ref")?,
        row.try_get::<String, _>("device_ref")?,
        row.try_get("access_request_id")?,
        row.try_get("ticket_id")?,
        parse_support_mode(row.try_get::<String, _>("mode")?.as_str())?,
        permissions,
        row.try_get::<String, _>("scope")?,
        row.try_get::<String, _>("module")?,
    )
    .map_err(|error| SupportPersistenceError::InvalidData(error.to_string()))?;
    Ok(CapabilityMetadata {
        id: row.try_get("id")?,
        binding,
        issued_at: row.try_get("issued_at")?,
        expires_at: row.try_get("expires_at")?,
        reissued_from: row.try_get("reissued_from")?,
        status: row.try_get("status")?,
        revoked_at: row.try_get("revoked_at")?,
        revocation_reason: row.try_get("revocation_reason")?,
        version: positive_u64(row.try_get("version")?, "capability version")?,
        secret_hash_length: usize::try_from(row.try_get::<i32, _>("secret_hash_length")?).map_err(
            |_| SupportPersistenceError::InvalidData("negative secret hash length".into()),
        )?,
    })
}

pub(crate) fn validate_idempotency(
    idempotency: &IdempotencyKey,
) -> Result<(), SupportPersistenceError> {
    if idempotency.key.trim().is_empty() || idempotency.key.len() > 255 {
        return Err(SupportPersistenceError::InvalidData(
            "idempotency key must be 1..=255 characters".into(),
        ));
    }
    if idempotency.request_hash.trim().is_empty() {
        return Err(SupportPersistenceError::InvalidData(
            "idempotency request hash cannot be empty".into(),
        ));
    }
    Ok(())
}

fn checked_next(version: u64) -> Result<u64, SupportPersistenceError> {
    version
        .checked_add(1)
        .ok_or_else(|| SupportPersistenceError::InvalidData("aggregate version overflow".into()))
}

pub(crate) fn validate_ticket(ticket: &SupportTicket) -> Result<(), SupportPersistenceError> {
    ticket
        .validate()
        .map_err(|error| SupportPersistenceError::InvalidData(error.to_string()))
}

fn validated_ticket(value: Value) -> Result<SupportTicket, SupportPersistenceError> {
    let ticket: SupportTicket = serde_json::from_value(value)?;
    validate_ticket(&ticket)?;
    Ok(ticket)
}

fn validate_session(session: &SupportSession) -> Result<(), SupportPersistenceError> {
    session
        .validate()
        .map_err(|error| SupportPersistenceError::InvalidData(error.to_string()))
}

fn validated_session(value: Value) -> Result<SupportSession, SupportPersistenceError> {
    let session: SupportSession = serde_json::from_value(value)?;
    validate_session(&session)?;
    Ok(session)
}

pub(crate) fn validate_incident(incident: &Incident) -> Result<(), SupportPersistenceError> {
    incident
        .validate()
        .map_err(|error| SupportPersistenceError::InvalidData(error.to_string()))
}

fn validated_incident(value: Value) -> Result<Incident, SupportPersistenceError> {
    let incident: Incident = serde_json::from_value(value)?;
    validate_incident(&incident)?;
    Ok(incident)
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, SupportPersistenceError> {
    i64::try_from(value)
        .map_err(|_| SupportPersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn positive_u64(value: i64, field: &'static str) -> Result<u64, SupportPersistenceError> {
    u64::try_from(value)
        .map_err(|_| SupportPersistenceError::InvalidData(format!("{field} is negative")))
}

async fn concurrency_error(
    tx: &mut Transaction<'_, Postgres>,
    entity: &'static str,
    id: Uuid,
    expected: u64,
) -> Result<SupportPersistenceError, SupportPersistenceError> {
    let table = match entity {
        "support_ticket" => "support_tickets",
        "support_access_request" => "support_access_requests",
        "support_capability" => "support_capabilities",
        "support_attachment" => "support_attachments",
        "incident" => "incidents",
        _ => {
            return Err(SupportPersistenceError::InvalidData(
                "unknown support entity".into(),
            ));
        }
    };
    let query = format!("SELECT version FROM erpx_control.{table} WHERE id = $1 FOR UPDATE");
    let actual = sqlx::query(&query)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| row.try_get::<i64, _>("version"))
        .transpose()?
        .map(|value| positive_u64(value, "aggregate version"))
        .transpose()?;
    if actual.is_none() {
        return Ok(SupportPersistenceError::NotFound { entity, id });
    }
    Ok(SupportPersistenceError::OptimisticConcurrency {
        entity,
        id,
        expected,
        actual,
    })
}

fn secret_hash_bytes(capability: &SupportCapability) -> Result<Vec<u8>, SupportPersistenceError> {
    let value = serde_json::to_value(capability.secret_hash())?;
    let bytes: Vec<u8> = serde_json::from_value(value).map_err(|_| {
        SupportPersistenceError::InvalidData("capability secret hash is not a byte array".into())
    })?;
    if bytes.len() != 32 {
        return Err(SupportPersistenceError::InvalidData(
            "capability secret hash must be exactly 32 bytes".into(),
        ));
    }
    Ok(bytes)
}

/// Build the durable warning and breach rows for a ticket's SLA deadline.
///
/// The domain policy normally evaluates a live clock. Persistence instead
/// evaluates the same policy at each deterministic boundary so both rows are
/// present before either one becomes due. This lets the registered worker
/// claim future work without a scheduler racing the deadline.
fn scheduled_sla_notifications(ticket: &SupportTicket) -> Vec<SupportSlaNotification> {
    let Some(deadline) = ticket.sla_deadline else {
        return Vec::new();
    };
    if ticket.status == TicketStatus::Closed {
        return Vec::new();
    }
    let policy = SupportSlaNotificationPolicy::default();
    let mut notifications = policy.notifications_for(ticket, deadline - policy.warning_before);
    notifications.extend(policy.notifications_for(ticket, deadline));
    notifications
}

async fn enqueue_sla_notifications_tx(
    tx: &mut Transaction<'_, Postgres>,
    notifications: &[SupportSlaNotification],
) -> Result<u64, SupportPersistenceError> {
    if notifications.len() > MAX_SUPPORT_SLA_NOTIFICATION_CLAIM_BATCH_SIZE as usize {
        return Err(SupportPersistenceError::InvalidData(
            "SLA notification enqueue batch is too large".into(),
        ));
    }
    let mut inserted = 0_u64;
    for notification in notifications {
        validate_sla_notification(notification)?;
        let payload = serde_json::to_value(notification)?;
        let result = sqlx::query(
            "INSERT INTO erpx_control.support_sla_notifications
                (notification_key, ticket_id, customer_account_id,
                 classification, audience, deadline, not_before, expires_at,
                 status, payload)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'pending',$9)
             ON CONFLICT (notification_key) DO NOTHING",
        )
        .bind(&notification.key)
        .bind(notification.ticket_id)
        .bind(notification.customer_account_id)
        .bind(sla_classification(notification.classification))
        .bind(sla_audience(notification.audience))
        .bind(notification.deadline)
        .bind(notification.not_before)
        .bind(notification.expires_at)
        .bind(&payload)
        .execute(&mut **tx)
        .await?;
        if result.rows_affected() == 1 {
            inserted += 1;
            continue;
        }
        let existing: Value = sqlx::query_scalar(
            "SELECT payload FROM erpx_control.support_sla_notifications
             WHERE notification_key = $1 FOR UPDATE",
        )
        .bind(&notification.key)
        .fetch_one(&mut **tx)
        .await?;
        if existing != payload {
            return Err(SupportPersistenceError::IdempotencyConflict {
                key: notification.key.clone(),
            });
        }
    }
    Ok(inserted)
}

fn ticket_status(status: TicketStatus) -> &'static str {
    match status {
        TicketStatus::New => "new",
        TicketStatus::Triaged => "triaged",
        TicketStatus::WaitingForCustomer => "waiting_for_customer",
        TicketStatus::AccessRequested => "access_requested",
        TicketStatus::Approved => "approved",
        TicketStatus::InProgress => "in_progress",
        TicketStatus::Resolved => "resolved",
        TicketStatus::CustomerConfirmed => "customer_confirmed",
        TicketStatus::Closed => "closed",
    }
}

fn sla_classification(classification: SupportSlaNotificationClassification) -> &'static str {
    match classification {
        SupportSlaNotificationClassification::DeadlineApproaching => "deadline_approaching",
        SupportSlaNotificationClassification::DeadlineBreached => "deadline_breached",
    }
}

fn sla_audience(audience: SupportNotificationAudience) -> &'static str {
    match audience {
        SupportNotificationAudience::CustomerVisible => "customer_visible",
        SupportNotificationAudience::Internal => "internal",
    }
}

fn validate_sla_notification(
    notification: &SupportSlaNotification,
) -> Result<(), SupportPersistenceError> {
    if notification.key.trim().is_empty()
        || notification.key.len() > 255
        || notification.key.chars().any(char::is_control)
    {
        return Err(SupportPersistenceError::InvalidData(
            "SLA notification key is invalid".into(),
        ));
    }
    if notification.ticket_id.is_nil() || notification.customer_account_id.is_nil() {
        return Err(SupportPersistenceError::InvalidData(
            "SLA notification identifiers are invalid".into(),
        ));
    }
    if notification.not_before > notification.deadline
        || notification.deadline > notification.expires_at
        || notification.not_before >= notification.expires_at
    {
        return Err(SupportPersistenceError::InvalidData(
            "SLA notification window is invalid".into(),
        ));
    }
    Ok(())
}

fn attachment_visibility(visibility: SupportAttachmentVisibility) -> &'static str {
    match visibility {
        SupportAttachmentVisibility::CustomerVisible => "customer_visible",
        SupportAttachmentVisibility::Internal => "internal",
    }
}

fn parse_attachment_visibility(
    visibility: &str,
) -> Result<SupportAttachmentVisibility, SupportPersistenceError> {
    match visibility {
        "customer_visible" => Ok(SupportAttachmentVisibility::CustomerVisible),
        "internal" => Ok(SupportAttachmentVisibility::Internal),
        _ => Err(SupportPersistenceError::InvalidData(format!(
            "unknown support attachment visibility {visibility}"
        ))),
    }
}

fn support_mode(mode: SupportMode) -> &'static str {
    match mode {
        SupportMode::DiagnosticRead => "diagnostic_read",
        SupportMode::TenantMutation => "tenant_mutation",
        SupportMode::Impersonation => "impersonation",
    }
}

fn parse_support_mode(mode: &str) -> Result<SupportMode, SupportPersistenceError> {
    match mode {
        "diagnostic_read" => Ok(SupportMode::DiagnosticRead),
        "tenant_mutation" => Ok(SupportMode::TenantMutation),
        "impersonation" => Ok(SupportMode::Impersonation),
        _ => Err(SupportPersistenceError::InvalidData(format!(
            "unknown support mode {mode}"
        ))),
    }
}

fn incident_status(status: IncidentStatus) -> &'static str {
    match status {
        IncidentStatus::Investigating => "investigating",
        IncidentStatus::Identified => "identified",
        IncidentStatus::Monitoring => "monitoring",
        IncidentStatus::Resolved => "resolved",
        IncidentStatus::PostmortemComplete => "postmortem_complete",
    }
}

fn capability_revocation_reason(reason: SupportCapabilityRevocationReason) -> &'static str {
    match reason {
        SupportCapabilityRevocationReason::Expired => "expired",
        SupportCapabilityRevocationReason::CustomerRequested => "customer_requested",
        SupportCapabilityRevocationReason::AgentRoleExpired => "agent_role_expired",
        SupportCapabilityRevocationReason::ParentRoleExpired => "parent_role_expired",
        SupportCapabilityRevocationReason::SessionRevoked => "session_revoked",
        SupportCapabilityRevocationReason::TicketClosed => "ticket_closed",
        SupportCapabilityRevocationReason::CustomerSuspended => "customer_suspended",
        SupportCapabilityRevocationReason::Administrative => "administrative",
        SupportCapabilityRevocationReason::Reissued => "reissued",
    }
}

pub(crate) fn cascade_revocation_reason(
    reason: SupportCapabilityRevocationReason,
) -> Result<&'static str, SupportPersistenceError> {
    match reason {
        SupportCapabilityRevocationReason::CustomerSuspended
        | SupportCapabilityRevocationReason::AgentRoleExpired
        | SupportCapabilityRevocationReason::ParentRoleExpired
        | SupportCapabilityRevocationReason::TicketClosed => Ok(capability_revocation_reason(reason)),
        _ => Err(SupportPersistenceError::InvalidData(
            "cascade revocation reason must be customer_suspended, agent_role_expired, parent_role_expired, or ticket_closed".into(),
        )),
    }
}

pub(crate) fn validate_cascade_scope(
    customer_account_id: Option<Uuid>,
    agent_ref: Option<&str>,
    ticket_id: Option<Uuid>,
) -> Result<(), SupportPersistenceError> {
    if customer_account_id.is_some_and(|id| id.is_nil()) || ticket_id.is_some_and(|id| id.is_nil())
    {
        return Err(SupportPersistenceError::InvalidData(
            "cascade revocation identifiers cannot be nil".into(),
        ));
    }
    if customer_account_id.is_none() && agent_ref.is_none() && ticket_id.is_none() {
        return Err(SupportPersistenceError::InvalidData(
            "cascade revocation requires a customer, agent, or ticket scope".into(),
        ));
    }
    if let Some(agent_ref) = agent_ref
        && (agent_ref.trim().is_empty()
            || agent_ref.len() > 255
            || agent_ref.chars().any(char::is_control))
    {
        return Err(SupportPersistenceError::InvalidData(
            "cascade agent reference must be 1..=255 characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod attachment_tests {
    use super::*;

    #[test]
    fn attachment_visibility_and_status_are_closed_sets() {
        assert_eq!(
            attachment_visibility(SupportAttachmentVisibility::CustomerVisible),
            "customer_visible"
        );
        assert_eq!(
            parse_attachment_visibility("internal").expect("internal visibility"),
            SupportAttachmentVisibility::Internal
        );
        assert!(parse_attachment_visibility("secret").is_err());
    }

    #[test]
    fn sla_notification_persistence_contract_is_bounded_and_audience_safe() {
        let now = Utc::now();
        let notification = SupportSlaNotification {
            key: "support-sla:test".into(),
            ticket_id: Uuid::from_u128(1),
            customer_account_id: Uuid::from_u128(2),
            classification: SupportSlaNotificationClassification::DeadlineBreached,
            audience: SupportNotificationAudience::CustomerVisible,
            deadline: now,
            not_before: now,
            expires_at: now + chrono::Duration::hours(1),
        };
        validate_sla_notification(&notification).expect("valid breached window");
        assert_eq!(
            sla_classification(notification.classification),
            "deadline_breached"
        );
        assert_eq!(sla_audience(notification.audience), "customer_visible");
        assert!(
            validate_sla_notification(&SupportSlaNotification {
                key: "bad\nkey".into(),
                ..notification
            })
            .is_err()
        );
    }

    #[test]
    fn ticket_sla_persistence_materializes_future_warning_and_breach_boundaries() {
        let now = Utc::now();
        let ticket = SupportTicket {
            id: Uuid::from_u128(10),
            customer_account_id: Uuid::from_u128(11),
            status: TicketStatus::InProgress,
            severity: "sev2".into(),
            description: "bounded SLA fixture".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: Some(now + chrono::Duration::hours(2)),
        };

        let notifications = scheduled_sla_notifications(&ticket);
        assert_eq!(notifications.len(), 2);
        assert_eq!(
            notifications[0].classification,
            SupportSlaNotificationClassification::DeadlineApproaching
        );
        assert_eq!(
            notifications[1].classification,
            SupportSlaNotificationClassification::DeadlineBreached
        );
        assert!(notifications.iter().all(|notification| {
            notification.ticket_id == ticket.id
                && notification.customer_account_id == ticket.customer_account_id
                && notification.not_before < notification.expires_at
        }));
        assert!(
            scheduled_sla_notifications(&SupportTicket {
                status: TicketStatus::Closed,
                ..ticket
            })
            .is_empty()
        );
    }

    #[test]
    fn attachment_reads_and_expiry_sweeps_are_bounded() {
        assert!(validate_attachment_page_size(1).is_ok());
        assert!(validate_attachment_page_size(MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE).is_ok());
        assert!(validate_attachment_page_size(0).is_err());
        assert!(validate_attachment_page_size(MAX_SUPPORT_ATTACHMENT_LIST_PAGE_SIZE + 1).is_err());
    }

    #[test]
    fn attachment_scope_is_containment_not_string_matching() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let holding = TenantScope::Holding { holding_id };
        let branch = TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        };
        assert!(ensure_attachment_scope(&holding, &branch).is_ok());
        assert!(ensure_attachment_scope(&branch, &holding).is_err());
    }

    #[test]
    fn domain_attachment_errors_map_to_safe_persistence_errors() {
        assert!(matches!(
            map_attachment_domain_error(erpx_domain::support::SupportError::ClosedTicketTerminal),
            SupportPersistenceError::AttachmentClosedTicket
        ));
        assert!(matches!(
            map_attachment_domain_error(
                erpx_domain::support::SupportError::SupportAttachmentExpired
            ),
            SupportPersistenceError::AttachmentNotActive
        ));
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    #[test]
    fn capability_issue_event_payload_is_complete_and_secret_free() {
        let issued_at = Utc::now();
        let metadata = CapabilityMetadata {
            id: Uuid::from_u128(1),
            binding: SupportCapabilityBinding::new(
                Uuid::from_u128(2),
                "agent-1",
                "device-1",
                Uuid::from_u128(3),
                Uuid::from_u128(4),
                SupportMode::TenantMutation,
                vec!["support:read".into(), "support:write".into()],
                "holding-1/company-1",
                "support",
            )
            .expect("valid capability binding"),
            issued_at,
            expires_at: issued_at + chrono::Duration::minutes(30),
            reissued_from: None,
            status: "active".into(),
            revoked_at: None,
            revocation_reason: None,
            version: 1,
            secret_hash_length: 32,
        };

        let payload = capability_issued_event_payload(&metadata);
        assert_eq!(payload["capability_id"], json!(metadata.id));
        assert_eq!(
            payload["access_request_id"],
            json!(metadata.binding.session_id)
        );
        assert_eq!(payload["device_ref"], json!("device-1"));
        assert_eq!(payload["status"], json!("active"));
        assert_eq!(payload["version"], json!(1));
        assert!(!payload.to_string().contains("secret_hash"));
    }

    #[test]
    fn capability_issue_ticket_fence_is_closed_set() {
        assert!(validate_ticket_support_access_status("in_progress").is_ok());
        assert!(validate_ticket_support_access_status("closed").is_err());
        assert!(validate_ticket_support_access_status("unknown").is_err());
    }
}
