use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::access::{RoleAssignment, Scope};
use crate::commercial::{BillingState, ModuleEntitlement, authorize_module};
use crate::contracts::TenantScope;

pub const MAX_TICKET_AUTHOR_REF_LENGTH: usize = 255;
pub const MAX_TICKET_MESSAGE_LENGTH: usize = 16_000;
pub const MAX_TICKET_SEVERITY_LENGTH: usize = 32;
pub const MAX_TICKET_DESCRIPTION_LENGTH: usize = 16_000;
pub const MAX_TICKET_ASSIGNMENT_SCOPE_IDS: usize = 256;
pub const MAX_TICKET_ESCALATION_LEVEL: u8 = 5;
pub const MAX_TICKET_ESCALATION_TARGET_LENGTH: usize = 255;
pub const MAX_TICKET_ESCALATION_REASON_LENGTH: usize = 2_000;
pub const MAX_TICKET_SLA_HORIZON: Duration = Duration::days(365);
pub const MAX_SUPPORT_ATTACHMENTS_PER_TICKET: usize = 20;
pub const MAX_SUPPORT_ATTACHMENT_SIZE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_SUPPORT_ATTACHMENT_OBJECT_REF_LENGTH: usize = 512;
pub const MAX_SUPPORT_ATTACHMENT_CONTENT_TYPE_LENGTH: usize = 255;
pub const MAX_SUPPORT_ATTACHMENT_UPLOADER_REF_LENGTH: usize = 255;
pub const MAX_SUPPORT_ATTACHMENT_LIFETIME: Duration = Duration::days(365);
pub const MAX_SUPPORT_SCOPE_LENGTH: usize = 4_096;
pub const MAX_SUPPORT_PERMISSION_LENGTH: usize = 255;
pub const MAX_SUPPORT_MODULE_LENGTH: usize = 128;
pub const MAX_SUPPORT_REASON_LENGTH: usize = 2_000;
pub const MAX_SUPPORT_SLA_WARNING_HORIZON: Duration = Duration::days(365);
pub const MAX_SUPPORT_SLA_BREACH_NOTIFICATION_WINDOW: Duration = Duration::days(7);
/// High-risk support modes must be backed by a recent MFA ceremony.  The
/// timestamp is persisted as evidence, never as a bearer or authenticator.
pub const SUPPORT_HIGH_RISK_MFA_MAX_AGE: Duration = Duration::minutes(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TicketStatus {
    New,
    Triaged,
    WaitingForCustomer,
    AccessRequested,
    Approved,
    InProgress,
    Resolved,
    CustomerConfirmed,
    Closed,
}

/// The audience is part of the write-side value, rather than inferred from
/// the author or ticket status. This prevents an internal note from being
/// accidentally rendered as a customer reply by a read adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportMessageAudience {
    CustomerVisible,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportTicketMessage {
    pub id: Uuid,
    pub author_ref: String,
    pub audience: SupportMessageAudience,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

impl SupportTicketMessage {
    pub fn customer_reply(
        id: Uuid,
        author_ref: impl Into<String>,
        body: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, SupportError> {
        Self::new(
            id,
            author_ref,
            SupportMessageAudience::CustomerVisible,
            body,
            created_at,
        )
    }

    pub fn internal_note(
        id: Uuid,
        author_ref: impl Into<String>,
        body: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, SupportError> {
        Self::new(
            id,
            author_ref,
            SupportMessageAudience::Internal,
            body,
            created_at,
        )
    }

    fn new(
        id: Uuid,
        author_ref: impl Into<String>,
        audience: SupportMessageAudience,
        body: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, SupportError> {
        if id.is_nil() {
            return Err(SupportError::InvalidTicketMessage);
        }
        let author_ref = bounded_non_empty(
            author_ref.into(),
            MAX_TICKET_AUTHOR_REF_LENGTH,
            SupportError::InvalidTicketMessage,
        )?;
        let body = bounded_non_empty(
            body.into(),
            MAX_TICKET_MESSAGE_LENGTH,
            SupportError::InvalidTicketMessage,
        )?;
        Ok(Self {
            id,
            author_ref,
            audience,
            body,
            created_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerVisibleTicketMessage {
    pub id: Uuid,
    pub author_ref: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerVisibleTicket {
    pub id: Uuid,
    pub customer_account_id: Uuid,
    pub status: TicketStatus,
    pub severity: String,
    pub description: String,
    pub messages: Vec<CustomerVisibleTicketMessage>,
}

/// The visibility is persisted as part of the attachment metadata. It is not
/// inferred from the uploader, support mode, or current ticket status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportAttachmentVisibility {
    CustomerVisible,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportAttachmentUpload {
    pub id: Uuid,
    pub object_ref: String,
    pub content_type: String,
    pub content_size_bytes: u64,
    pub sha256_hex: String,
    pub visibility: SupportAttachmentVisibility,
    pub uploader_ref: String,
    pub scope: TenantScope,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportAttachment {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub customer_account_id: Uuid,
    pub object_ref: String,
    pub content_type: String,
    pub content_size_bytes: u64,
    pub sha256_hex: String,
    pub visibility: SupportAttachmentVisibility,
    pub uploader_ref: String,
    pub scope: TenantScope,
    pub uploaded_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl SupportAttachment {
    #[allow(clippy::too_many_arguments)]
    pub fn from_upload(
        ticket_id: Uuid,
        customer_account_id: Uuid,
        upload: SupportAttachmentUpload,
        uploaded_at: DateTime<Utc>,
    ) -> Result<Self, SupportError> {
        if ticket_id.is_nil() || customer_account_id.is_nil() || upload.id.is_nil() {
            return Err(SupportError::InvalidSupportAttachment);
        }
        validate_object_ref(&upload.object_ref)?;
        validate_content_type(&upload.content_type)?;
        if upload.content_size_bytes == 0
            || upload.content_size_bytes > MAX_SUPPORT_ATTACHMENT_SIZE_BYTES
        {
            return Err(SupportError::InvalidSupportAttachment);
        }
        let sha256_hex = canonical_sha256(&upload.sha256_hex)?;
        let uploader_ref = bounded_non_empty(
            upload.uploader_ref,
            MAX_SUPPORT_ATTACHMENT_UPLOADER_REF_LENGTH,
            SupportError::InvalidSupportAttachment,
        )?;
        validate_tenant_scope(&upload.scope).map_err(|_| SupportError::InvalidSupportAttachment)?;
        if upload.expires_at <= uploaded_at
            || upload.expires_at > uploaded_at + MAX_SUPPORT_ATTACHMENT_LIFETIME
        {
            return Err(SupportError::InvalidSupportAttachment);
        }
        let attachment = Self {
            id: upload.id,
            ticket_id,
            customer_account_id,
            object_ref: upload.object_ref,
            content_type: upload.content_type,
            content_size_bytes: upload.content_size_bytes,
            sha256_hex,
            visibility: upload.visibility,
            uploader_ref,
            scope: upload.scope,
            uploaded_at,
            expires_at: upload.expires_at,
        };
        attachment.validate()?;
        Ok(attachment)
    }

    /// Validate metadata rehydrated from an untrusted persistence boundary.
    /// Upload constructors call the same validator, so SQLx reads cannot
    /// silently turn malformed rows into executable object references.
    pub fn validate(&self) -> Result<(), SupportError> {
        if self.id.is_nil() || self.ticket_id.is_nil() || self.customer_account_id.is_nil() {
            return Err(SupportError::InvalidSupportAttachment);
        }
        validate_object_ref(&self.object_ref)?;
        validate_content_type(&self.content_type)?;
        if self.content_size_bytes == 0
            || self.content_size_bytes > MAX_SUPPORT_ATTACHMENT_SIZE_BYTES
        {
            return Err(SupportError::InvalidSupportAttachment);
        }
        canonical_sha256(&self.sha256_hex)?;
        bounded_non_empty(
            self.uploader_ref.clone(),
            MAX_SUPPORT_ATTACHMENT_UPLOADER_REF_LENGTH,
            SupportError::InvalidSupportAttachment,
        )?;
        validate_tenant_scope(&self.scope).map_err(|_| SupportError::InvalidSupportAttachment)?;
        if self.expires_at <= self.uploaded_at
            || self.expires_at > self.uploaded_at + MAX_SUPPORT_ATTACHMENT_LIFETIME
        {
            return Err(SupportError::InvalidSupportAttachment);
        }
        Ok(())
    }

    pub fn is_active_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.uploaded_at && now < self.expires_at
    }

    fn validate_for_ticket(
        &self,
        ticket: &SupportTicket,
        now: DateTime<Utc>,
    ) -> Result<(), SupportError> {
        if self.ticket_id != ticket.id || self.customer_account_id != ticket.customer_account_id {
            return Err(SupportError::SupportAttachmentTicketBindingDenied);
        }
        if ticket.status == TicketStatus::Closed {
            return Err(SupportError::ClosedTicketTerminal);
        }
        if !self.is_active_at(now) {
            return Err(SupportError::SupportAttachmentExpired);
        }
        Ok(())
    }

    /// Customer reads receive only safe metadata and never receive signed
    /// URLs, credentials, or internal attachment records.
    pub fn customer_visible_view(
        &self,
        ticket: &SupportTicket,
        now: DateTime<Utc>,
    ) -> Option<CustomerVisibleSupportAttachment> {
        (self.visibility == SupportAttachmentVisibility::CustomerVisible
            && self.ticket_id == ticket.id
            && self.customer_account_id == ticket.customer_account_id
            && self.is_active_at(now))
        .then(|| CustomerVisibleSupportAttachment {
            id: self.id,
            object_ref: self.object_ref.clone(),
            content_type: self.content_type.clone(),
            content_size_bytes: self.content_size_bytes,
            sha256_hex: self.sha256_hex.clone(),
            uploader_ref: self.uploader_ref.clone(),
            expires_at: self.expires_at,
        })
    }

    /// The redacted form is suitable for diagnostic logs and audit summaries;
    /// the opaque object reference is deliberately omitted.
    pub fn redacted_for_log(&self) -> RedactedSupportAttachment {
        RedactedSupportAttachment {
            id: self.id,
            ticket_id: self.ticket_id,
            customer_account_id: self.customer_account_id,
            content_type: self.content_type.clone(),
            content_size_bytes: self.content_size_bytes,
            sha256_hex: self.sha256_hex.clone(),
            visibility: self.visibility,
            uploader_ref: self.uploader_ref.clone(),
            expires_at: self.expires_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerVisibleSupportAttachment {
    pub id: Uuid,
    pub object_ref: String,
    pub content_type: String,
    pub content_size_bytes: u64,
    pub sha256_hex: String,
    pub uploader_ref: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedSupportAttachment {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub customer_account_id: Uuid,
    pub content_type: String,
    pub content_size_bytes: u64,
    pub sha256_hex: String,
    pub visibility: SupportAttachmentVisibility,
    pub uploader_ref: String,
    pub expires_at: DateTime<Utc>,
}

/// Collection operations keep attachment count and ticket/customer binding
/// checks at the same boundary as the ticket command.
pub struct SupportAttachmentSet;

impl SupportAttachmentSet {
    pub fn append(
        ticket: &SupportTicket,
        existing: &[SupportAttachment],
        attachment: SupportAttachment,
        now: DateTime<Utc>,
    ) -> Result<Vec<SupportAttachment>, SupportError> {
        if existing.len() >= MAX_SUPPORT_ATTACHMENTS_PER_TICKET {
            return Err(SupportError::SupportAttachmentLimitExceeded);
        }
        for current in existing {
            if current.id == attachment.id || current.object_ref == attachment.object_ref {
                return Err(SupportError::InvalidSupportAttachment);
            }
            if current.ticket_id != ticket.id
                || current.customer_account_id != ticket.customer_account_id
            {
                return Err(SupportError::SupportAttachmentTicketBindingDenied);
            }
        }
        attachment.validate_for_ticket(ticket, now)?;
        let mut result = existing.to_vec();
        result.push(attachment);
        Ok(result)
    }

    pub fn customer_visible(
        ticket: &SupportTicket,
        attachments: &[SupportAttachment],
        now: DateTime<Utc>,
    ) -> Vec<CustomerVisibleSupportAttachment> {
        attachments
            .iter()
            .filter_map(|attachment| attachment.customer_visible_view(ticket, now))
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportNotificationAudience {
    CustomerVisible,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportSlaNotificationClassification {
    DeadlineApproaching,
    DeadlineBreached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportSlaNotificationPolicy {
    pub warning_before: Duration,
    pub breach_notification_window: Duration,
    pub customer_visible_warning: bool,
    pub customer_visible_breach: bool,
    pub internal_warning: bool,
    pub internal_breach: bool,
}

impl SupportSlaNotificationPolicy {
    pub fn new(
        warning_before: Duration,
        breach_notification_window: Duration,
        customer_visible_warning: bool,
        customer_visible_breach: bool,
        internal_warning: bool,
        internal_breach: bool,
    ) -> Result<Self, SupportError> {
        if warning_before <= Duration::zero()
            || warning_before > MAX_SUPPORT_SLA_WARNING_HORIZON
            || breach_notification_window <= Duration::zero()
            || breach_notification_window > MAX_SUPPORT_SLA_BREACH_NOTIFICATION_WINDOW
        {
            return Err(SupportError::InvalidSupportNotificationPolicy);
        }
        Ok(Self {
            warning_before,
            breach_notification_window,
            customer_visible_warning,
            customer_visible_breach,
            internal_warning,
            internal_breach,
        })
    }

    pub fn notifications_for(
        &self,
        ticket: &SupportTicket,
        now: DateTime<Utc>,
    ) -> Vec<SupportSlaNotification> {
        let Some(deadline) = ticket.sla_deadline else {
            return Vec::new();
        };
        if ticket.status == TicketStatus::Closed {
            return Vec::new();
        }

        let (classification, not_before, expires_at) =
            if now < deadline && deadline - now <= self.warning_before {
                (
                    SupportSlaNotificationClassification::DeadlineApproaching,
                    deadline - self.warning_before,
                    deadline,
                )
            } else if now >= deadline && now < deadline + self.breach_notification_window {
                (
                    SupportSlaNotificationClassification::DeadlineBreached,
                    deadline,
                    deadline + self.breach_notification_window,
                )
            } else {
                return Vec::new();
            };

        let customer_enabled = match classification {
            SupportSlaNotificationClassification::DeadlineApproaching => {
                self.customer_visible_warning
            }
            SupportSlaNotificationClassification::DeadlineBreached => self.customer_visible_breach,
        };
        let internal_enabled = match classification {
            SupportSlaNotificationClassification::DeadlineApproaching => self.internal_warning,
            SupportSlaNotificationClassification::DeadlineBreached => self.internal_breach,
        };
        let mut notifications = Vec::with_capacity(2);
        if customer_enabled {
            notifications.push(SupportSlaNotification::new(
                ticket,
                classification,
                SupportNotificationAudience::CustomerVisible,
                deadline,
                not_before,
                expires_at,
            ));
        }
        if internal_enabled {
            notifications.push(SupportSlaNotification::new(
                ticket,
                classification,
                SupportNotificationAudience::Internal,
                deadline,
                not_before,
                expires_at,
            ));
        }
        notifications
    }
}

impl Default for SupportSlaNotificationPolicy {
    fn default() -> Self {
        Self::new(
            Duration::hours(24),
            Duration::hours(24),
            false,
            false,
            true,
            true,
        )
        .expect("default SLA notification policy is valid")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportSlaNotification {
    pub key: String,
    pub ticket_id: Uuid,
    pub customer_account_id: Uuid,
    pub classification: SupportSlaNotificationClassification,
    pub audience: SupportNotificationAudience,
    pub deadline: DateTime<Utc>,
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl SupportSlaNotification {
    #[allow(clippy::too_many_arguments)]
    fn new(
        ticket: &SupportTicket,
        classification: SupportSlaNotificationClassification,
        audience: SupportNotificationAudience,
        deadline: DateTime<Utc>,
        not_before: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        let key = format!(
            "support-sla:{}:{}:{}:{}",
            ticket.id,
            deadline.timestamp_millis(),
            classification.as_key(),
            audience.as_key()
        );
        Self {
            key,
            ticket_id: ticket.id,
            customer_account_id: ticket.customer_account_id,
            classification,
            audience,
            deadline,
            not_before,
            expires_at,
        }
    }

    pub fn is_deliverable_at(&self, ticket: &SupportTicket, now: DateTime<Utc>) -> bool {
        ticket.status != TicketStatus::Closed
            && ticket.id == self.ticket_id
            && ticket.customer_account_id == self.customer_account_id
            && now >= self.not_before
            && now < self.expires_at
    }
}

impl SupportNotificationAudience {
    const fn as_key(self) -> &'static str {
        match self {
            Self::CustomerVisible => "customer",
            Self::Internal => "internal",
        }
    }
}

impl SupportSlaNotificationClassification {
    const fn as_key(self) -> &'static str {
        match self {
            Self::DeadlineApproaching => "approaching",
            Self::DeadlineBreached => "breached",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportTicketAssignment {
    pub assignee_ref: String,
    pub assigned_by: String,
    pub scope: TenantScope,
    pub assigned_at: DateTime<Utc>,
}

impl SupportTicketAssignment {
    pub fn new(
        assignee_ref: impl Into<String>,
        assigned_by: impl Into<String>,
        scope: TenantScope,
        assigned_at: DateTime<Utc>,
    ) -> Result<Self, SupportError> {
        validate_tenant_scope(&scope)?;
        Ok(Self {
            assignee_ref: bounded_non_empty(
                assignee_ref.into(),
                MAX_TICKET_AUTHOR_REF_LENGTH,
                SupportError::InvalidTicketAssignment,
            )?,
            assigned_by: bounded_non_empty(
                assigned_by.into(),
                MAX_TICKET_AUTHOR_REF_LENGTH,
                SupportError::InvalidTicketAssignment,
            )?,
            scope,
            assigned_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportTicketEscalation {
    pub level: u8,
    pub target_ref: String,
    pub reason: String,
    pub escalated_by: String,
    pub escalated_at: DateTime<Utc>,
}

impl SupportTicketEscalation {
    pub fn new(
        level: u8,
        target_ref: impl Into<String>,
        reason: impl Into<String>,
        escalated_by: impl Into<String>,
        escalated_at: DateTime<Utc>,
    ) -> Result<Self, SupportError> {
        if !(1..=MAX_TICKET_ESCALATION_LEVEL).contains(&level) {
            return Err(SupportError::InvalidTicketEscalation);
        }
        Ok(Self {
            level,
            target_ref: bounded_non_empty(
                target_ref.into(),
                MAX_TICKET_ESCALATION_TARGET_LENGTH,
                SupportError::InvalidTicketEscalation,
            )?,
            reason: bounded_non_empty(
                reason.into(),
                MAX_TICKET_ESCALATION_REASON_LENGTH,
                SupportError::InvalidTicketEscalation,
            )?,
            escalated_by: bounded_non_empty(
                escalated_by.into(),
                MAX_TICKET_AUTHOR_REF_LENGTH,
                SupportError::InvalidTicketEscalation,
            )?,
            escalated_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupportTicketCommand {
    Transition {
        next_status: TicketStatus,
    },
    CustomerReply {
        message_id: Uuid,
        author_ref: String,
        body: String,
        created_at: DateTime<Utc>,
    },
    InternalNote {
        message_id: Uuid,
        author_ref: String,
        body: String,
        created_at: DateTime<Utc>,
    },
    Assign {
        assignment: SupportTicketAssignment,
    },
    Escalate {
        escalation: SupportTicketEscalation,
    },
    SetSlaDeadline {
        deadline: DateTime<Utc>,
    },
}

impl SupportTicketCommand {
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Transition { .. } => "support_ticket.transitioned",
            Self::CustomerReply { .. } => "support_ticket.customer_reply_added",
            Self::InternalNote { .. } => "support_ticket.internal_note_added",
            Self::Assign { .. } => "support_ticket.assigned",
            Self::Escalate { .. } => "support_ticket.escalated",
            Self::SetSlaDeadline { .. } => "support_ticket.sla_deadline_set",
        }
    }

    pub const fn audit_action(&self) -> &'static str {
        match self {
            Self::Transition { .. } => "support_ticket_transitioned",
            Self::CustomerReply { .. } => "support_ticket_customer_reply_added",
            Self::InternalNote { .. } => "support_ticket_internal_note_added",
            Self::Assign { .. } => "support_ticket_assigned",
            Self::Escalate { .. } => "support_ticket_escalated",
            Self::SetSlaDeadline { .. } => "support_ticket_sla_deadline_set",
        }
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Transition { .. } => "transition",
            Self::CustomerReply { .. } => "customer_reply",
            Self::InternalNote { .. } => "internal_note",
            Self::Assign { .. } => "assign",
            Self::Escalate { .. } => "escalate",
            Self::SetSlaDeadline { .. } => "set_sla_deadline",
        }
    }

    /// Used by the SQLx command seam to ensure the aggregate supplied for
    /// persistence is the result of this command, not an unrelated snapshot.
    pub fn is_applied_to(&self, ticket: &SupportTicket) -> bool {
        match self {
            Self::Transition { next_status } => ticket.status == *next_status,
            Self::CustomerReply {
                message_id,
                author_ref,
                body,
                created_at,
            } => ticket.messages.iter().any(|message| {
                message.id == *message_id
                    && message.audience == SupportMessageAudience::CustomerVisible
                    && message.author_ref == *author_ref
                    && message.body == *body
                    && message.created_at == *created_at
            }),
            Self::InternalNote {
                message_id,
                author_ref,
                body,
                created_at,
            } => ticket.messages.iter().any(|message| {
                message.id == *message_id
                    && message.audience == SupportMessageAudience::Internal
                    && message.author_ref == *author_ref
                    && message.body == *body
                    && message.created_at == *created_at
            }),
            Self::Assign { assignment } => ticket.assignment.as_ref() == Some(assignment),
            Self::Escalate { escalation } => ticket.escalation.as_ref() == Some(escalation),
            Self::SetSlaDeadline { deadline } => ticket.sla_deadline == Some(*deadline),
        }
    }

    /// Prove that the candidate aggregate is exactly the result of this
    /// command applied to `before`. Presence of a matching field alone is not
    /// sufficient: otherwise unrelated aggregate changes could be smuggled
    /// through an optimistic-concurrency update.
    pub fn is_exactly_applied_to(&self, before: &SupportTicket, after: &SupportTicket) -> bool {
        if before.id != after.id
            || before.customer_account_id != after.customer_account_id
            || before.recurrence_of != after.recurrence_of
            || before.validate().is_err()
            || after.validate().is_err()
        {
            return false;
        }
        match self {
            Self::Transition { next_status } => {
                let mut expected = before.clone();
                expected.transition(*next_status).is_ok() && expected == *after
            }
            Self::CustomerReply {
                message_id,
                author_ref,
                body,
                created_at,
            }
            | Self::InternalNote {
                message_id,
                author_ref,
                body,
                created_at,
            } => {
                let Some((last, prefix)) = after.messages.split_last() else {
                    return false;
                };
                let audience = match self {
                    Self::CustomerReply { .. } => SupportMessageAudience::CustomerVisible,
                    Self::InternalNote { .. } => SupportMessageAudience::Internal,
                    _ => unreachable!(),
                };
                prefix == before.messages.as_slice()
                    && after.messages.len() == before.messages.len() + 1
                    && last.id == *message_id
                    && last.author_ref == *author_ref
                    && last.body == *body
                    && last.created_at == *created_at
                    && last.audience == audience
                    && before.assignment == after.assignment
                    && before.escalation == after.escalation
                    && before.sla_deadline == after.sla_deadline
                    && before.status == after.status
            }
            Self::Assign { assignment } => {
                let mut expected = before.clone();
                expected.assignment = Some(assignment.clone());
                expected == *after
            }
            Self::Escalate { escalation } => {
                let mut expected = before.clone();
                expected.escalation = Some(escalation.clone());
                expected == *after
            }
            Self::SetSlaDeadline { deadline } => {
                let mut expected = before.clone();
                expected.sla_deadline = Some(*deadline);
                expected == *after
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupportMode {
    DiagnosticRead,
    TenantMutation,
    Impersonation,
}

impl SupportMode {
    pub const fn maximum_duration(self) -> Duration {
        match self {
            Self::DiagnosticRead => Duration::hours(8),
            Self::TenantMutation | Self::Impersonation => Duration::hours(2),
        }
    }

    pub const fn requires_represented_user(self) -> bool {
        matches!(self, Self::Impersonation)
    }
}

/// Customer-approved support policy evaluated in addition to the persisted
/// capability/session bindings. Empty permissions or module allowlists deny
/// by default; policy construction is therefore intentionally explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportDomainPolicy {
    pub required_agent_permission: String,
    pub permitted_modules: BTreeSet<String>,
    pub diagnostic_read_allowed: bool,
    pub tenant_mutation_allowed: bool,
    pub impersonation_allowed: bool,
    pub segregation_of_duties_passed: bool,
}

impl SupportDomainPolicy {
    pub fn new(
        required_agent_permission: impl Into<String>,
        permitted_modules: BTreeSet<String>,
        diagnostic_read_allowed: bool,
        tenant_mutation_allowed: bool,
        impersonation_allowed: bool,
        segregation_of_duties_passed: bool,
    ) -> Self {
        Self {
            required_agent_permission: required_agent_permission.into(),
            permitted_modules,
            diagnostic_read_allowed,
            tenant_mutation_allowed,
            impersonation_allowed,
            segregation_of_duties_passed,
        }
    }

    fn permits(&self, mode: SupportMode, module_key: &str) -> bool {
        !self.required_agent_permission.trim().is_empty()
            && self.permitted_modules.contains(module_key)
            && match mode {
                SupportMode::DiagnosticRead => self.diagnostic_read_allowed,
                SupportMode::TenantMutation => self.tenant_mutation_allowed,
                SupportMode::Impersonation => self.impersonation_allowed,
            }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SupportIntersectionDenial {
    #[error("agent identity is inactive")]
    AgentIdentityInactive,
    #[error("agent identity reference is invalid")]
    AgentIdentityMismatch,
    #[error("agent device is inactive")]
    AgentDeviceInactive,
    #[error("agent device binding is invalid")]
    AgentDeviceBindingMismatch,
    #[error("support session agent binding is invalid")]
    SessionAgentMismatch,
    #[error("support session has not been approved")]
    SessionNotApproved,
    #[error("support session is inactive")]
    SessionInactive,
    #[error("approved support scope is invalid")]
    ApprovedScopeDenied,
    #[error("approved support permission is not granted")]
    ApprovedPermissionDenied,
    #[error("support scope encoding does not match the typed scope")]
    ScopeEncodingMismatch,
    #[error("support capability binding is invalid")]
    CapabilityBindingDenied,
    #[error("support capability secret is invalid")]
    CapabilitySecretInvalid,
    #[error("support capability is inactive")]
    CapabilityInactive,
    #[error("agent role is not a platform role")]
    AgentPlatformRoleDenied,
    #[error("agent role is inactive")]
    AgentRoleInactive,
    #[error("agent parent role is inactive or missing")]
    AgentParentInactive,
    #[error("agent role does not grant the support permission")]
    AgentRolePermissionDenied,
    #[error("agent role does not contain the requested scope")]
    AgentRoleScopeDenied,
    #[error("support domain policy denied the request")]
    DomainPolicyDenied,
    #[error("support segregation-of-duties check failed")]
    SegregationOfDutiesFailed,
    #[error("support module entitlement denied: {0}")]
    CommercialDenied(&'static str),
    #[error("represented-user authority is required")]
    RepresentedUserAuthorityMissing,
    #[error("represented-user identity binding is invalid")]
    RepresentedUserIdentityMismatch,
    #[error("represented-user identity is inactive")]
    RepresentedUserIdentityInactive,
    #[error("represented-user role is inactive")]
    RepresentedUserRoleInactive,
    #[error("represented-user parent role is inactive or missing")]
    RepresentedUserParentInactive,
    #[error("represented-user role does not contain the requested scope")]
    RepresentedUserScopeDenied,
    #[error("represented-user role does not grant the requested permission")]
    RepresentedUserPermissionDenied,
    #[error("a represented user is not allowed for this support mode")]
    UnexpectedRepresentedUser,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SupportError {
    #[error("invalid support ticket transition")]
    InvalidTicketTransition,
    #[error("only a closed support ticket can create a recurrence")]
    RecurrenceRequiresClosedTicket,
    #[error("a closed support ticket is terminal and cannot be changed")]
    ClosedTicketTerminal,
    #[error("invalid support ticket message")]
    InvalidTicketMessage,
    #[error("invalid support ticket aggregate")]
    InvalidSupportTicket,
    #[error("support ticket assignment is invalid")]
    InvalidTicketAssignment,
    #[error("support ticket assignment exceeds the actor scope")]
    TicketAssignmentScopeDenied,
    #[error("support ticket escalation is invalid")]
    InvalidTicketEscalation,
    #[error("support ticket escalation cannot move to a lower level")]
    EscalationLevelRegression,
    #[error("support ticket SLA deadline is invalid")]
    InvalidTicketSlaDeadline,
    #[error("support attachment metadata is invalid")]
    InvalidSupportAttachment,
    #[error("support ticket attachment limit exceeded")]
    SupportAttachmentLimitExceeded,
    #[error("support attachment ticket or customer binding is invalid")]
    SupportAttachmentTicketBindingDenied,
    #[error("support attachment is expired or not yet active")]
    SupportAttachmentExpired,
    #[error("support SLA notification policy is invalid")]
    InvalidSupportNotificationPolicy,
    #[error("support session exceeds its mode maximum")]
    ExceedsMaximum,
    #[error("impersonation requires an explicitly represented tenant user")]
    MissingRepresentedUser,
    #[error("a represented tenant user is only valid for impersonation")]
    UnexpectedRepresentedUser,
    #[error("support session has not been approved")]
    NotApproved,
    #[error("support session has already been approved")]
    AlreadyApproved,
    #[error("support session is expired, revoked, or closed")]
    Inactive,
    #[error("support capability does not cover the requested scope")]
    ScopeNotGranted,
    #[error("support capability does not grant the requested permission")]
    PermissionNotGranted,
    #[error("support capability binding contains an empty value")]
    InvalidCapabilityBinding,
    #[error("support capability secret is invalid")]
    InvalidCapabilitySecret,
    #[error("support capability has already been revoked")]
    AlreadyRevoked,
    #[error("high-risk support access requires MFA")]
    HighRiskMfaRequired,
    #[error("high-risk support MFA evidence is stale or from the future")]
    HighRiskMfaExpired,
    #[error("support device is not bound to the approved session")]
    DeviceBindingMismatch,
    #[error("support actor scope does not contain the approved scope")]
    SupportScopeDenied,
    #[error("support segregation-of-duties decision is not currently passed")]
    SupportSegregationOfDutiesDenied,
    #[error("support reference is invalid")]
    InvalidSupportReference,
    #[error("incident field cannot be empty")]
    InvalidIncidentField,
    #[error("invalid incident lifecycle transition")]
    InvalidIncidentTransition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportTicket {
    pub id: Uuid,
    pub customer_account_id: Uuid,
    pub status: TicketStatus,
    pub severity: String,
    pub description: String,
    /// A recurrence is a new aggregate linked to the terminal ticket that
    /// originally reported the issue. Closed tickets remain immutable.
    #[serde(default)]
    pub recurrence_of: Option<Uuid>,
    /// Both audiences are durable in the aggregate, but customer reads must
    /// use `customer_visible_view`, which projects only customer replies.
    #[serde(default)]
    pub messages: Vec<SupportTicketMessage>,
    #[serde(default)]
    pub assignment: Option<SupportTicketAssignment>,
    #[serde(default)]
    pub escalation: Option<SupportTicketEscalation>,
    #[serde(default)]
    pub sla_deadline: Option<DateTime<Utc>>,
}

impl SupportTicket {
    /// Validate the complete aggregate before it crosses an API or
    /// persistence boundary. Public fields are intentional for transport
    /// decoding, so callers must not rely on struct construction alone.
    pub fn validate(&self) -> Result<(), SupportError> {
        if self.id.is_nil()
            || self.customer_account_id.is_nil()
            || self.severity.trim().is_empty()
            || self.severity.len() > MAX_TICKET_SEVERITY_LENGTH
            || self.description.trim().is_empty()
            || self.description.len() > MAX_TICKET_DESCRIPTION_LENGTH
            || self.recurrence_of == Some(self.id)
        {
            return Err(SupportError::InvalidSupportTicket);
        }
        let mut message_ids = BTreeSet::new();
        for message in &self.messages {
            if message.id.is_nil() || !message_ids.insert(message.id) {
                return Err(SupportError::InvalidTicketMessage);
            }
            SupportTicketMessage::new(
                message.id,
                message.author_ref.clone(),
                message.audience,
                message.body.clone(),
                message.created_at,
            )?;
        }
        if let Some(assignment) = &self.assignment {
            validate_tenant_scope(&assignment.scope)?;
            bounded_non_empty(
                assignment.assignee_ref.clone(),
                MAX_TICKET_AUTHOR_REF_LENGTH,
                SupportError::InvalidTicketAssignment,
            )?;
            bounded_non_empty(
                assignment.assigned_by.clone(),
                MAX_TICKET_AUTHOR_REF_LENGTH,
                SupportError::InvalidTicketAssignment,
            )?;
        }
        if let Some(escalation) = &self.escalation {
            if !(1..=MAX_TICKET_ESCALATION_LEVEL).contains(&escalation.level) {
                return Err(SupportError::InvalidTicketEscalation);
            }
            bounded_non_empty(
                escalation.target_ref.clone(),
                MAX_TICKET_ESCALATION_TARGET_LENGTH,
                SupportError::InvalidTicketEscalation,
            )?;
            bounded_non_empty(
                escalation.reason.clone(),
                MAX_TICKET_ESCALATION_REASON_LENGTH,
                SupportError::InvalidTicketEscalation,
            )?;
            bounded_non_empty(
                escalation.escalated_by.clone(),
                MAX_TICKET_AUTHOR_REF_LENGTH,
                SupportError::InvalidTicketEscalation,
            )?;
        }
        Ok(())
    }

    pub fn transition(&mut self, next: TicketStatus) -> Result<(), SupportError> {
        let valid = matches!(
            (self.status, next),
            (TicketStatus::New, TicketStatus::Triaged)
                | (TicketStatus::Triaged, TicketStatus::WaitingForCustomer)
                | (TicketStatus::Triaged, TicketStatus::AccessRequested)
                | (TicketStatus::Triaged, TicketStatus::InProgress)
                | (TicketStatus::WaitingForCustomer, TicketStatus::Triaged)
                | (
                    TicketStatus::WaitingForCustomer,
                    TicketStatus::AccessRequested
                )
                | (TicketStatus::WaitingForCustomer, TicketStatus::InProgress)
                | (TicketStatus::AccessRequested, TicketStatus::Approved)
                | (TicketStatus::AccessRequested, TicketStatus::Triaged)
                | (TicketStatus::Approved, TicketStatus::InProgress)
                | (TicketStatus::InProgress, TicketStatus::WaitingForCustomer)
                | (TicketStatus::InProgress, TicketStatus::AccessRequested)
                | (TicketStatus::InProgress, TicketStatus::Resolved)
                | (TicketStatus::Resolved, TicketStatus::InProgress)
                | (TicketStatus::Resolved, TicketStatus::CustomerConfirmed)
                | (TicketStatus::CustomerConfirmed, TicketStatus::Closed)
        );
        if !valid {
            return Err(SupportError::InvalidTicketTransition);
        }
        self.status = next;
        Ok(())
    }

    /// Apply one bounded support-ticket command. The caller's scope must
    /// contain any assignment scope; a branch can therefore never assign a
    /// ticket to its parent company, holding, or a sibling branch.
    pub fn apply_command(
        &mut self,
        command: SupportTicketCommand,
        actor_scope: &TenantScope,
        now: DateTime<Utc>,
    ) -> Result<(), SupportError> {
        self.validate()?;
        if self.status == TicketStatus::Closed {
            return Err(SupportError::ClosedTicketTerminal);
        }

        match command {
            SupportTicketCommand::Transition { next_status } => self.transition(next_status),
            SupportTicketCommand::CustomerReply {
                message_id,
                author_ref,
                body,
                created_at,
            } => SupportTicketMessage::customer_reply(message_id, author_ref, body, created_at)
                .and_then(|message| {
                    if message.created_at > now {
                        Err(SupportError::InvalidTicketMessage)
                    } else {
                        self.add_message(message)
                    }
                }),
            SupportTicketCommand::InternalNote {
                message_id,
                author_ref,
                body,
                created_at,
            } => SupportTicketMessage::internal_note(message_id, author_ref, body, created_at)
                .and_then(|message| {
                    if message.created_at > now {
                        Err(SupportError::InvalidTicketMessage)
                    } else {
                        self.add_message(message)
                    }
                }),
            SupportTicketCommand::Assign { assignment } => {
                if assignment.assigned_at > now {
                    return Err(SupportError::InvalidTicketAssignment);
                }
                if !actor_scope.contains(&assignment.scope) {
                    return Err(SupportError::TicketAssignmentScopeDenied);
                }
                self.assignment = Some(assignment);
                Ok(())
            }
            SupportTicketCommand::Escalate { escalation } => {
                if escalation.escalated_at > now {
                    return Err(SupportError::InvalidTicketEscalation);
                }
                if self
                    .escalation
                    .as_ref()
                    .is_some_and(|current| escalation.level < current.level)
                {
                    return Err(SupportError::EscalationLevelRegression);
                }
                self.escalation = Some(escalation);
                Ok(())
            }
            SupportTicketCommand::SetSlaDeadline { deadline } => {
                if deadline <= now || deadline > now + MAX_TICKET_SLA_HORIZON {
                    return Err(SupportError::InvalidTicketSlaDeadline);
                }
                self.sla_deadline = Some(deadline);
                Ok(())
            }
        }
    }

    fn add_message(&mut self, message: SupportTicketMessage) -> Result<(), SupportError> {
        if self
            .messages
            .iter()
            .any(|existing| existing.id == message.id)
        {
            return Err(SupportError::InvalidTicketMessage);
        }
        self.messages.push(message);
        Ok(())
    }

    /// Build the only ticket shape intended for customer-visible reads. It
    /// has no internal-note audience or internal metadata field to serialize.
    pub fn customer_visible_view(&self) -> CustomerVisibleTicket {
        CustomerVisibleTicket {
            id: self.id,
            customer_account_id: self.customer_account_id,
            status: self.status,
            severity: self.severity.clone(),
            description: self.description.clone(),
            messages: self
                .messages
                .iter()
                .filter(|message| message.audience == SupportMessageAudience::CustomerVisible)
                .map(|message| CustomerVisibleTicketMessage {
                    id: message.id,
                    author_ref: message.author_ref.clone(),
                    body: message.body.clone(),
                    created_at: message.created_at,
                })
                .collect(),
        }
    }

    /// Create a new support ticket for a later recurrence without reopening
    /// or mutating the closed predecessor.
    pub fn recurrence(
        &self,
        id: Uuid,
        severity: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Self, SupportError> {
        if self.status != TicketStatus::Closed {
            return Err(SupportError::RecurrenceRequiresClosedTicket);
        }
        let recurrence = Self {
            id,
            customer_account_id: self.customer_account_id,
            status: TicketStatus::New,
            severity: severity.into(),
            description: description.into(),
            recurrence_of: Some(self.id),
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        recurrence.validate()?;
        Ok(recurrence)
    }
}

fn bounded_non_empty(
    value: String,
    maximum: usize,
    error: SupportError,
) -> Result<String, SupportError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(error);
    }
    Ok(value)
}

fn validate_object_ref(value: &str) -> Result<(), SupportError> {
    if value.is_empty()
        || value.len() > MAX_SUPPORT_ATTACHMENT_OBJECT_REF_LENGTH
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains("//")
        || value.contains("http:")
        || value.contains("https:")
    {
        return Err(SupportError::InvalidSupportAttachment);
    }
    for segment in value.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(SupportError::InvalidSupportAttachment);
        }
        if !segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(SupportError::InvalidSupportAttachment);
        }
    }
    Ok(())
}

fn validate_content_type(value: &str) -> Result<(), SupportError> {
    if value.is_empty() || value.len() > MAX_SUPPORT_ATTACHMENT_CONTENT_TYPE_LENGTH {
        return Err(SupportError::InvalidSupportAttachment);
    }
    let mut parts = value.split('/');
    let Some(media_type) = parts.next() else {
        return Err(SupportError::InvalidSupportAttachment);
    };
    let Some(subtype) = parts.next() else {
        return Err(SupportError::InvalidSupportAttachment);
    };
    if parts.next().is_some() || media_type.is_empty() || subtype.is_empty() {
        return Err(SupportError::InvalidSupportAttachment);
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-' | b'/'
            )
    }) {
        return Err(SupportError::InvalidSupportAttachment);
    }
    Ok(())
}

fn canonical_sha256(value: &str) -> Result<String, SupportError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SupportError::InvalidSupportAttachment);
    }
    Ok(value.to_ascii_lowercase())
}

fn validate_tenant_scope(scope: &TenantScope) -> Result<(), SupportError> {
    let valid = match scope {
        TenantScope::Platform => true,
        TenantScope::Holding { holding_id } => !holding_id.is_nil(),
        TenantScope::Company {
            holding_id,
            company_id,
        } => !holding_id.is_nil() && !company_id.is_nil(),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => !holding_id.is_nil() && !company_id.is_nil() && !branch_id.is_nil(),
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => {
            !holding_id.is_nil()
                && !company_ids.is_empty()
                && company_ids.len() <= MAX_TICKET_ASSIGNMENT_SCOPE_IDS
                && branch_ids.len() <= MAX_TICKET_ASSIGNMENT_SCOPE_IDS
                && company_ids.iter().all(|id| !id.is_nil())
                && branch_ids.iter().all(|id| !id.is_nil())
        }
    };
    valid
        .then_some(())
        .ok_or(SupportError::InvalidTicketAssignment)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportSession {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub customer_account_id: Uuid,
    pub agent_ref: String,
    pub represented_tenant_user: Option<String>,
    pub mode: SupportMode,
    pub permissions: Vec<String>,
    pub scope: String,
    /// Customer-approved functional module bound into every capability.
    #[serde(default = "default_support_module")]
    pub module: String,
    pub reason: String,
    pub requested_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

fn default_support_module() -> String {
    "support".into()
}

impl SupportSession {
    /// Validate a rehydrated session before it can authorize or be persisted.
    pub fn validate(&self) -> Result<(), SupportError> {
        if self.id.is_nil()
            || self.ticket_id.is_nil()
            || self.customer_account_id.is_nil()
            || self.requested_at >= self.expires_at
            || self.expires_at.signed_duration_since(self.requested_at)
                > self.mode.maximum_duration()
            || self.agent_ref.trim().is_empty()
            || self.agent_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || self.scope.trim().is_empty()
            || self.scope.len() > MAX_SUPPORT_SCOPE_LENGTH
            || self.reason.trim().is_empty()
            || self.reason.len() > MAX_SUPPORT_REASON_LENGTH
            || self.module.trim().is_empty()
            || self.module.len() > MAX_SUPPORT_MODULE_LENGTH
            || self.permissions.is_empty()
            || self.permissions.iter().any(|permission| {
                permission.trim().is_empty()
                    || permission.len() > MAX_SUPPORT_PERMISSION_LENGTH
                    || permission.chars().any(char::is_control)
            })
            || self
                .permissions
                .iter()
                .enumerate()
                .any(|(index, permission)| {
                    self.permissions[..index]
                        .iter()
                        .any(|previous| previous == permission)
                })
            || self.mode.requires_represented_user() != self.represented_tenant_user.is_some()
        {
            return Err(SupportError::InvalidSupportReference);
        }
        for reference in [
            Some(self.agent_ref.as_str()),
            self.represented_tenant_user.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if reference.trim().is_empty()
                || reference.len() > MAX_TICKET_AUTHOR_REF_LENGTH
                || reference.chars().any(char::is_control)
            {
                return Err(SupportError::InvalidSupportReference);
            }
        }
        match (self.approved_by.as_deref(), self.approved_at) {
            (None, None) => {}
            (Some(approver), Some(approved_at))
                if !approver.trim().is_empty()
                    && approver.len() <= MAX_TICKET_AUTHOR_REF_LENGTH
                    && !approver.chars().any(char::is_control)
                    && approved_at >= self.requested_at
                    && approved_at < self.expires_at => {}
            _ => return Err(SupportError::InvalidSupportReference),
        }
        if self
            .revoked_at
            .is_some_and(|revoked_at| revoked_at < self.requested_at)
        {
            return Err(SupportError::InvalidSupportReference);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn request(
        ticket_id: Uuid,
        customer_account_id: Uuid,
        agent_ref: impl Into<String>,
        mode: SupportMode,
        permissions: Vec<String>,
        scope: impl Into<String>,
        reason: impl Into<String>,
        requested_at: DateTime<Utc>,
        duration: Duration,
        represented_tenant_user: Option<String>,
    ) -> Result<Self, SupportError> {
        Self::request_with_module(
            ticket_id,
            customer_account_id,
            agent_ref,
            mode,
            permissions,
            scope,
            "support",
            reason,
            requested_at,
            duration,
            represented_tenant_user,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn request_with_module(
        ticket_id: Uuid,
        customer_account_id: Uuid,
        agent_ref: impl Into<String>,
        mode: SupportMode,
        permissions: Vec<String>,
        scope: impl Into<String>,
        module: impl Into<String>,
        reason: impl Into<String>,
        requested_at: DateTime<Utc>,
        duration: Duration,
        represented_tenant_user: Option<String>,
    ) -> Result<Self, SupportError> {
        if ticket_id.is_nil() || customer_account_id.is_nil() {
            return Err(SupportError::InvalidSupportReference);
        }
        if duration <= Duration::zero() || duration > mode.maximum_duration() {
            return Err(SupportError::ExceedsMaximum);
        }
        if mode.requires_represented_user() && represented_tenant_user.is_none() {
            return Err(SupportError::MissingRepresentedUser);
        }
        if !mode.requires_represented_user() && represented_tenant_user.is_some() {
            return Err(SupportError::UnexpectedRepresentedUser);
        }
        let mut permissions = permissions;
        permissions.sort_unstable();
        permissions.dedup();
        let module = module.into();
        if module.trim().is_empty() || module.len() > 128 {
            return Err(SupportError::InvalidCapabilityBinding);
        }
        let session = Self {
            id: Uuid::new_v4(),
            ticket_id,
            customer_account_id,
            agent_ref: agent_ref.into(),
            represented_tenant_user,
            mode,
            permissions,
            scope: scope.into(),
            module,
            reason: reason.into(),
            requested_at,
            expires_at: requested_at + duration,
            approved_by: None,
            approved_at: None,
            revoked_at: None,
        };
        session.validate()?;
        Ok(session)
    }

    pub fn approve(
        &mut self,
        approver: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), SupportError> {
        self.validate()?;
        if self.revoked_at.is_some() || at >= self.expires_at {
            return Err(SupportError::Inactive);
        }
        if self.approved_at.is_some() {
            return Err(SupportError::AlreadyApproved);
        }
        let approver = approver.into();
        if approver.trim().is_empty()
            || approver.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || approver.chars().any(char::is_control)
            || at < self.requested_at
        {
            return Err(SupportError::InvalidSupportReference);
        }
        self.approved_by = Some(approver);
        self.approved_at = Some(at);
        Ok(())
    }

    pub fn revoke(&mut self, at: DateTime<Utc>) {
        if self.revoked_at.is_none() {
            self.revoked_at = Some(at);
        }
    }

    pub fn authorize(
        &self,
        at: DateTime<Utc>,
        permission: &str,
        scope: &str,
    ) -> Result<(), SupportError> {
        self.validate()?;
        if self.approved_at.is_none() {
            return Err(SupportError::NotApproved);
        }
        if self.revoked_at.is_some() || at < self.requested_at || at >= self.expires_at {
            return Err(SupportError::Inactive);
        }
        if self.scope != scope {
            return Err(SupportError::ScopeNotGranted);
        }
        if !self.permissions.iter().any(|granted| granted == permission) {
            return Err(SupportError::PermissionNotGranted);
        }
        Ok(())
    }

    /// A support session is not executable until its authenticated device and
    /// current high-risk MFA evidence have been re-bound at the action
    /// boundary.  This method intentionally accepts only the opaque device
    /// reference; identity/session liveness is resolved by the infrastructure
    /// adapter against the identity write store.
    pub fn authorize_device(
        &self,
        binding: &SupportDeviceSessionBinding,
        actor_scope: &Scope,
        requested_scope: &Scope,
        sod_passed: bool,
        at: DateTime<Utc>,
    ) -> Result<(), SupportError> {
        self.validate()?;
        binding.validate()?;
        if binding.session_id != self.id || binding.principal_ref != self.agent_ref {
            return Err(SupportError::DeviceBindingMismatch);
        }
        if !binding.is_active_at(at) || self.revoked_at.is_some() || at >= self.expires_at {
            return Err(SupportError::Inactive);
        }
        if self.scope != requested_scope.canonical_key() {
            return Err(SupportError::ScopeNotGranted);
        }
        if !actor_scope.contains(requested_scope) {
            return Err(SupportError::SupportScopeDenied);
        }
        if !sod_passed {
            return Err(SupportError::SupportSegregationOfDutiesDenied);
        }
        if self.mode.requires_high_risk_mfa() {
            let Some(verified_at) = binding.mfa_verified_at else {
                return Err(SupportError::HighRiskMfaRequired);
            };
            if verified_at > at
                || at.signed_duration_since(verified_at) > SUPPORT_HIGH_RISK_MFA_MAX_AGE
            {
                return Err(SupportError::HighRiskMfaExpired);
            }
        }
        Ok(())
    }
}

/// The non-secret device/session evidence attached to an approved support
/// session.  `device_ref` is immutable; revocation is represented by an
/// append-only timestamp and is rechecked for every action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportDeviceSessionBinding {
    pub session_id: Uuid,
    pub principal_ref: String,
    pub device_ref: String,
    pub mfa_verified_at: Option<DateTime<Utc>>,
    pub bound_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl SupportDeviceSessionBinding {
    pub fn validate(&self) -> Result<(), SupportError> {
        if self.session_id.is_nil()
            || self.principal_ref.trim().is_empty()
            || self.device_ref.trim().is_empty()
            || self.principal_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || self.device_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || self.principal_ref.chars().any(char::is_control)
            || self.device_ref.chars().any(char::is_control)
            || self
                .mfa_verified_at
                .is_some_and(|verified_at| verified_at > self.bound_at)
            || self
                .revoked_at
                .is_some_and(|revoked_at| revoked_at < self.bound_at)
        {
            return Err(SupportError::InvalidSupportReference);
        }
        Ok(())
    }

    pub fn new(
        session_id: Uuid,
        principal_ref: impl Into<String>,
        device_ref: impl Into<String>,
        mfa_verified_at: Option<DateTime<Utc>>,
        bound_at: DateTime<Utc>,
    ) -> Result<Self, SupportError> {
        let principal_ref = principal_ref.into();
        let device_ref = device_ref.into();
        if session_id.is_nil()
            || principal_ref.trim().is_empty()
            || device_ref.trim().is_empty()
            || principal_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || device_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || principal_ref.chars().any(char::is_control)
            || device_ref.chars().any(char::is_control)
            || mfa_verified_at.is_some_and(|verified_at| verified_at > bound_at)
        {
            return Err(SupportError::InvalidSupportReference);
        }
        let binding = Self {
            session_id,
            principal_ref,
            device_ref,
            mfa_verified_at,
            bound_at,
            revoked_at: None,
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        at >= self.bound_at && self.revoked_at.is_none_or(|revoked_at| at < revoked_at)
    }

    pub fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }

    pub fn revoke(&mut self, at: DateTime<Utc>) {
        if self.revoked_at.is_none() {
            self.revoked_at = Some(at);
        }
    }
}

impl SupportMode {
    pub const fn requires_high_risk_mfa(self) -> bool {
        matches!(self, Self::TenantMutation | Self::Impersonation)
    }
}

/// The exact approval material bound to a support capability. Keeping these
/// values together makes it difficult for an adapter to accidentally issue a
/// capability that is broader than the customer-approved request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportCapabilityBinding {
    pub customer_account_id: Uuid,
    pub agent_ref: String,
    pub device_ref: String,
    pub session_id: Uuid,
    pub ticket_id: Uuid,
    pub mode: SupportMode,
    pub permissions: Vec<String>,
    pub scope: String,
    pub module: String,
}

impl SupportCapabilityBinding {
    pub fn validate(&self) -> Result<(), SupportError> {
        if self.customer_account_id.is_nil()
            || self.session_id.is_nil()
            || self.ticket_id.is_nil()
            || self.agent_ref.trim().is_empty()
            || self.device_ref.trim().is_empty()
            || self.agent_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || self.device_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || self.scope.trim().is_empty()
            || self.scope.len() > MAX_SUPPORT_SCOPE_LENGTH
            || self.module.trim().is_empty()
            || self.module.len() > MAX_SUPPORT_MODULE_LENGTH
            || self.permissions.is_empty()
            || self.permissions.iter().any(|permission| {
                permission.trim().is_empty()
                    || permission.len() > MAX_SUPPORT_PERMISSION_LENGTH
                    || permission.chars().any(char::is_control)
            })
            || self
                .permissions
                .windows(2)
                .any(|permissions| permissions[0] >= permissions[1])
        {
            return Err(SupportError::InvalidCapabilityBinding);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        customer_account_id: Uuid,
        agent_ref: impl Into<String>,
        device_ref: impl Into<String>,
        session_id: Uuid,
        ticket_id: Uuid,
        mode: SupportMode,
        permissions: Vec<String>,
        scope: impl Into<String>,
        module: impl Into<String>,
    ) -> Result<Self, SupportError> {
        let mut permissions = permissions;
        let agent_ref = agent_ref.into();
        let device_ref = device_ref.into();
        let scope = scope.into();
        let module = module.into();
        if customer_account_id.is_nil()
            || session_id.is_nil()
            || ticket_id.is_nil()
            || agent_ref.trim().is_empty()
            || device_ref.trim().is_empty()
            || scope.trim().is_empty()
            || module.trim().is_empty()
            || permissions.is_empty()
            || permissions.iter().any(|permission| {
                permission.trim().is_empty()
                    || permission.len() > MAX_SUPPORT_PERMISSION_LENGTH
                    || permission.chars().any(char::is_control)
            })
            || scope.len() > MAX_SUPPORT_SCOPE_LENGTH
            || module.len() > MAX_SUPPORT_MODULE_LENGTH
        {
            return Err(SupportError::InvalidCapabilityBinding);
        }
        permissions.sort_unstable();
        permissions.dedup();
        let binding = Self {
            customer_account_id,
            agent_ref,
            device_ref,
            session_id,
            ticket_id,
            mode,
            permissions,
            scope,
            module,
        };
        binding.validate()?;
        Ok(binding)
    }
}

/// The only bearer material returned for a support capability. It is never a
/// field on `SupportCapability`, and its debug representation is redacted.
pub struct SupportCapabilitySecret {
    bytes: [u8; 32],
}

impl SupportCapabilitySecret {
    fn issue() -> Self {
        let mut bytes = [0_u8; 32];
        bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
        bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
        Self { bytes }
    }

    /// Returns the one-time bearer representation. Callers must deliver it
    /// to the agent once and must not persist it.
    pub fn as_hex(&self) -> String {
        encode_hex(&self.bytes)
    }

    /// Reconstruct bearer material only at an authenticated action boundary.
    /// Callers must obtain the value from the request and must never persist
    /// or log the returned secret.
    pub fn from_hex(value: &str) -> Result<Self, SupportError> {
        let bytes = decode_hex(value).ok_or(SupportError::InvalidCapabilitySecret)?;
        Ok(Self { bytes })
    }
}

impl std::fmt::Debug for SupportCapabilitySecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SupportCapabilitySecret(REDACTED)")
    }
}

impl Drop for SupportCapabilitySecret {
    fn drop(&mut self) {
        for byte in &mut self.bytes {
            *byte = 0;
        }
    }
}

/// Persisted hash-only representation of a capability secret. The raw
/// 256-bit bearer value cannot be reconstructed from this value.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SupportCapabilitySecretHash([u8; 32]);

impl std::fmt::Debug for SupportCapabilitySecretHash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SupportCapabilitySecretHash(REDACTED)")
    }
}

impl SupportCapabilitySecretHash {
    fn from_secret(secret: &SupportCapabilitySecret) -> Self {
        Self(sha256(&secret.bytes))
    }

    fn matches(&self, secret: &SupportCapabilitySecret) -> bool {
        let candidate = sha256(&secret.bytes);
        let mut difference = 0_u8;
        for (expected, actual) in self.0.iter().zip(candidate.iter()) {
            difference |= expected ^ actual;
        }
        difference == 0
    }
}

/// Why a customer-approved support capability was terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupportCapabilityRevocationReason {
    Expired,
    CustomerRequested,
    AgentRoleExpired,
    ParentRoleExpired,
    SessionRevoked,
    TicketClosed,
    CustomerSuspended,
    Administrative,
    Reissued,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportCapabilityRevocation {
    pub reason: SupportCapabilityRevocationReason,
    pub revoked_at: DateTime<Utc>,
}

/// An expiring, customer-approved support capability. All fields describing
/// authority are immutable after issuance; revocation is append-only state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportCapability {
    pub id: Uuid,
    pub binding: SupportCapabilityBinding,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    secret_hash: SupportCapabilitySecretHash,
    pub reissued_from: Option<Uuid>,
    pub revocation: Option<SupportCapabilityRevocation>,
}

impl SupportCapability {
    /// Rehydrate the hash-only capability state from the authoritative write
    /// store. The raw bearer is intentionally not accepted here.
    pub fn from_persisted(
        id: Uuid,
        binding: SupportCapabilityBinding,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        secret_hash: [u8; 32],
        reissued_from: Option<Uuid>,
        revocation: Option<SupportCapabilityRevocation>,
    ) -> Result<Self, SupportError> {
        binding.validate()?;
        let duration = expires_at.signed_duration_since(issued_at);
        if duration <= Duration::zero() || duration > binding.mode.maximum_duration() {
            return Err(SupportError::ExceedsMaximum);
        }
        Ok(Self {
            id,
            binding,
            issued_at,
            expires_at,
            secret_hash: SupportCapabilitySecretHash(secret_hash),
            reissued_from,
            revocation,
        })
    }

    pub fn issue(
        binding: SupportCapabilityBinding,
        issued_at: DateTime<Utc>,
        duration: Duration,
    ) -> Result<(Self, SupportCapabilitySecret), SupportError> {
        binding.validate()?;
        if duration <= Duration::zero() || duration > binding.mode.maximum_duration() {
            return Err(SupportError::ExceedsMaximum);
        }
        let expires_at = issued_at
            .checked_add_signed(duration)
            .ok_or(SupportError::ExceedsMaximum)?;
        let secret = SupportCapabilitySecret::issue();
        let capability = Self {
            id: Uuid::new_v4(),
            binding,
            issued_at,
            expires_at,
            secret_hash: SupportCapabilitySecretHash::from_secret(&secret),
            reissued_from: None,
            revocation: None,
        };
        Ok((capability, secret))
    }

    pub fn secret_hash(&self) -> &SupportCapabilitySecretHash {
        &self.secret_hash
    }

    pub fn is_active(&self, at: DateTime<Utc>) -> bool {
        self.revocation.is_none() && self.issued_at <= at && at < self.expires_at
    }

    /// Validates the bearer secret and every immutable binding field. The
    /// expiration is half-open: the capability is invalid at `expires_at`.
    pub fn authenticate(
        &self,
        secret: &SupportCapabilitySecret,
        binding: &SupportCapabilityBinding,
        at: DateTime<Utc>,
    ) -> Result<(), SupportError> {
        if !self.is_active(at) {
            return Err(SupportError::Inactive);
        }
        if &self.binding != binding {
            return Err(SupportError::InvalidCapabilityBinding);
        }
        if !self.secret_hash.matches(secret) {
            return Err(SupportError::InvalidCapabilitySecret);
        }
        Ok(())
    }

    /// Issues a new bearer secret without changing the original approval
    /// expiry. Reissuing is refused after expiry or revocation.
    pub fn reissue(
        &self,
        at: DateTime<Utc>,
    ) -> Result<(Self, SupportCapabilitySecret), SupportError> {
        if !self.is_active(at) {
            return Err(SupportError::Inactive);
        }
        let secret = SupportCapabilitySecret::issue();
        Ok((
            Self {
                id: Uuid::new_v4(),
                binding: self.binding.clone(),
                issued_at: at,
                expires_at: self.expires_at,
                secret_hash: SupportCapabilitySecretHash::from_secret(&secret),
                reissued_from: Some(self.id),
                revocation: None,
            },
            secret,
        ))
    }

    pub fn revoke(
        &mut self,
        reason: SupportCapabilityRevocationReason,
        at: DateTime<Utc>,
    ) -> Result<(), SupportError> {
        if self.revocation.is_some() {
            return Err(SupportError::AlreadyRevoked);
        }
        self.revocation = Some(SupportCapabilityRevocation {
            reason,
            revoked_at: at,
        });
        Ok(())
    }
}

/// Complete support-authority input assembled by an application boundary.
/// The evaluator accepts already-resolved current state and never performs
/// transport, persistence, or identity-provider work itself.
pub struct SupportIntersectionRequest<'a> {
    pub at: DateTime<Utc>,
    pub agent_identity_ref: &'a str,
    pub agent_identity_active: bool,
    pub agent_device_ref: &'a str,
    pub agent_device_active: bool,
    pub session: &'a SupportSession,
    pub capability: &'a SupportCapability,
    pub capability_secret: &'a SupportCapabilitySecret,
    pub agent_role: &'a RoleAssignment,
    pub agent_parent_role: Option<&'a RoleAssignment>,
    pub represented_user_identity_active: bool,
    pub represented_user_role: Option<&'a RoleAssignment>,
    pub represented_user_parent_role: Option<&'a RoleAssignment>,
    pub requested_scope: &'a Scope,
    pub scope_key: &'a str,
    pub module_key: &'a str,
    pub required_permission: &'a str,
    pub billing_state: BillingState,
    pub entitlement: Option<&'a ModuleEntitlement>,
    pub domain_policy: &'a SupportDomainPolicy,
    pub is_mutation: bool,
}

/// Evaluate the required support-authority intersection at one instant.
/// Every input is checked fail-closed; in particular, impersonation cannot
/// proceed without a currently active represented-user role, parent cascade,
/// scope, and permission.
pub fn evaluate_support_intersection(
    request: &SupportIntersectionRequest<'_>,
) -> Result<(), SupportIntersectionDenial> {
    if !request.agent_identity_active {
        return Err(SupportIntersectionDenial::AgentIdentityInactive);
    }
    if request.agent_identity_ref.trim().is_empty() {
        return Err(SupportIntersectionDenial::AgentIdentityMismatch);
    }
    if !request.agent_device_active {
        return Err(SupportIntersectionDenial::AgentDeviceInactive);
    }
    if request.agent_device_ref.trim().is_empty() {
        return Err(SupportIntersectionDenial::AgentDeviceBindingMismatch);
    }
    if request.scope_key != request.requested_scope.canonical_key() {
        return Err(SupportIntersectionDenial::ScopeEncodingMismatch);
    }
    if request.module_key.trim().is_empty() || request.required_permission.trim().is_empty() {
        return Err(SupportIntersectionDenial::DomainPolicyDenied);
    }
    if !request
        .domain_policy
        .permits(request.session.mode, request.module_key)
    {
        return Err(SupportIntersectionDenial::DomainPolicyDenied);
    }
    if !request.domain_policy.segregation_of_duties_passed {
        return Err(SupportIntersectionDenial::SegregationOfDutiesFailed);
    }
    if matches!(request.session.mode, SupportMode::DiagnosticRead) && request.is_mutation {
        return Err(SupportIntersectionDenial::DomainPolicyDenied);
    }

    if request.session.agent_ref != request.agent_identity_ref {
        return Err(SupportIntersectionDenial::SessionAgentMismatch);
    }
    let session_scope = request.session.scope.as_str();
    let session_authorization =
        request
            .session
            .authorize(request.at, request.required_permission, session_scope);
    if session_scope != request.scope_key {
        return Err(SupportIntersectionDenial::ApprovedScopeDenied);
    }
    if let Err(error) = session_authorization {
        return Err(match error {
            SupportError::NotApproved => SupportIntersectionDenial::SessionNotApproved,
            SupportError::Inactive => SupportIntersectionDenial::SessionInactive,
            SupportError::ScopeNotGranted => SupportIntersectionDenial::ApprovedScopeDenied,
            SupportError::PermissionNotGranted => {
                SupportIntersectionDenial::ApprovedPermissionDenied
            }
            _ => SupportIntersectionDenial::SessionInactive,
        });
    }

    let expected_binding = SupportCapabilityBinding::new(
        request.session.customer_account_id,
        request.session.agent_ref.clone(),
        request.agent_device_ref,
        request.session.id,
        request.session.ticket_id,
        request.session.mode,
        request.session.permissions.clone(),
        request.session.scope.clone(),
        request.module_key,
    )
    .map_err(|_| SupportIntersectionDenial::CapabilityBindingDenied)?;
    request
        .capability
        .authenticate(request.capability_secret, &expected_binding, request.at)
        .map_err(|error| match error {
            SupportError::Inactive => SupportIntersectionDenial::CapabilityInactive,
            SupportError::InvalidCapabilitySecret => {
                SupportIntersectionDenial::CapabilitySecretInvalid
            }
            _ => SupportIntersectionDenial::CapabilityBindingDenied,
        })?;

    if request.agent_role.principal_ref != request.agent_identity_ref {
        return Err(SupportIntersectionDenial::AgentIdentityMismatch);
    }
    if !request.agent_role.is_active_at(request.at) {
        return Err(SupportIntersectionDenial::AgentRoleInactive);
    }
    if !request
        .agent_role
        .effective_at(request.at, request.agent_parent_role)
    {
        return Err(SupportIntersectionDenial::AgentParentInactive);
    }
    if request.agent_role.functional_domain != "platform"
        || request.agent_role.scope != Scope::Platform
    {
        return Err(SupportIntersectionDenial::AgentPlatformRoleDenied);
    }
    if !request.agent_role.scope.contains(request.requested_scope) {
        return Err(SupportIntersectionDenial::AgentRoleScopeDenied);
    }
    if !request
        .agent_role
        .permissions
        .iter()
        .any(|permission| permission == &request.domain_policy.required_agent_permission)
    {
        return Err(SupportIntersectionDenial::AgentRolePermissionDenied);
    }

    let is_mutation =
        request.is_mutation || matches!(request.session.mode, SupportMode::TenantMutation);
    if request
        .entitlement
        .is_some_and(|entitlement| entitlement.module_key != request.module_key)
    {
        return Err(SupportIntersectionDenial::CommercialDenied(
            "module_mismatch",
        ));
    }
    match authorize_module(
        request.billing_state,
        request.entitlement,
        true,
        request.at,
        is_mutation,
    ) {
        crate::commercial::EntitlementDecision::Allowed => {}
        crate::commercial::EntitlementDecision::Denied(reason) => {
            return Err(SupportIntersectionDenial::CommercialDenied(reason));
        }
    }

    if request.session.mode.requires_represented_user() {
        let Some(represented_ref) = request.session.represented_tenant_user.as_deref() else {
            return Err(SupportIntersectionDenial::RepresentedUserAuthorityMissing);
        };
        if represented_ref.trim().is_empty() {
            return Err(SupportIntersectionDenial::RepresentedUserAuthorityMissing);
        }
        if !request.represented_user_identity_active {
            return Err(SupportIntersectionDenial::RepresentedUserIdentityInactive);
        }
        let Some(role) = request.represented_user_role else {
            return Err(SupportIntersectionDenial::RepresentedUserAuthorityMissing);
        };
        if role.principal_ref != represented_ref {
            return Err(SupportIntersectionDenial::RepresentedUserIdentityMismatch);
        }
        if !role.is_active_at(request.at) {
            return Err(SupportIntersectionDenial::RepresentedUserRoleInactive);
        }
        if !role.effective_at(request.at, request.represented_user_parent_role) {
            return Err(SupportIntersectionDenial::RepresentedUserParentInactive);
        }
        if !role.scope.contains(request.requested_scope) {
            return Err(SupportIntersectionDenial::RepresentedUserScopeDenied);
        }
        if !role
            .permissions
            .iter()
            .any(|permission| permission == request.required_permission)
        {
            return Err(SupportIntersectionDenial::RepresentedUserPermissionDenied);
        }
    } else if request.session.represented_tenant_user.is_some() {
        return Err(SupportIntersectionDenial::UnexpectedRepresentedUser);
    }

    Ok(())
}

/// Diagnostic output is an allowlist. Unknown fields are discarded so a new
/// producer cannot accidentally exfiltrate credentials or business records.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiagnosticRedactionPolicy;

pub const FORBIDDEN_DIAGNOSTIC_FIELDS: &[&str] = &[
    "password",
    "credential",
    "secret",
    "private_key",
    "payment_instrument",
    "token",
    "authorization",
    "cookie",
    "database_url",
    "connection_string",
    "unrelated_business_data",
];

const ALLOWED_DIAGNOSTIC_FIELDS: &[&str] = &[
    "versions",
    "projection_lag_ms",
    "failed_jobs",
    "integration_errors",
    "worker_health",
    "backup_health",
    "correlation_ids",
    "redacted_configuration",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub fields: BTreeMap<String, String>,
    pub redacted_fields: BTreeSet<String>,
}

impl DiagnosticRedactionPolicy {
    pub fn redact(&self, fields: &BTreeMap<String, String>) -> DiagnosticReport {
        let mut safe_fields = BTreeMap::new();
        let mut redacted_fields = BTreeSet::new();
        for (field, value) in fields {
            let normalized = normalize_field_name(field);
            if ALLOWED_DIAGNOSTIC_FIELDS.contains(&normalized.as_str()) {
                safe_fields.insert(field.clone(), value.clone());
            } else {
                redacted_fields.insert(field.clone());
            }
        }
        DiagnosticReport {
            fields: safe_fields,
            redacted_fields,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentStatus {
    Investigating,
    Identified,
    Monitoring,
    Resolved,
    PostmortemComplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentTimelineEntry {
    pub at: DateTime<Utc>,
    pub actor_ref: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncidentBroadcast {
    pub at: DateTime<Utc>,
    pub actor_ref: String,
    pub audience: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectiveAction {
    pub id: Uuid,
    pub owner_ref: String,
    pub description: String,
    pub due_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Incident aggregate for operational response and postmortem evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub id: Uuid,
    pub status: IncidentStatus,
    pub commander_ref: String,
    pub leads: Vec<String>,
    pub affected_services: Vec<String>,
    pub affected_customer_accounts: Vec<Uuid>,
    pub timeline: Vec<IncidentTimelineEntry>,
    pub broadcasts: Vec<IncidentBroadcast>,
    pub mitigation: Option<String>,
    pub root_cause: Option<String>,
    pub corrective_actions: Vec<CorrectiveAction>,
}

/// Rebuildable incident read data.  Operational timeline, broadcasts,
/// mitigation, root cause, and corrective-action text stay on the write side
/// and are deliberately absent from this DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportIncidentReadSummary {
    pub id: Uuid,
    pub status: IncidentStatus,
    pub commander_ref: String,
    pub leads: Vec<String>,
    pub affected_services: Vec<String>,
    pub affected_customer_accounts: Vec<Uuid>,
    pub source_version: u64,
    pub source_event_id: Uuid,
    pub projected_at: DateTime<Utc>,
}

impl SupportIncidentReadSummary {
    pub fn from_incident(
        incident: &Incident,
        source_version: u64,
        source_event_id: Uuid,
        projected_at: DateTime<Utc>,
    ) -> Result<Self, SupportError> {
        incident.validate()?;
        if incident.id.is_nil()
            || source_version == 0
            || source_event_id.is_nil()
            || incident.commander_ref.trim().is_empty()
            || incident.leads.iter().any(|value| value.trim().is_empty())
            || incident
                .affected_services
                .iter()
                .any(|value| value.trim().is_empty())
            || incident.affected_customer_accounts.iter().any(Uuid::is_nil)
        {
            return Err(SupportError::InvalidIncidentField);
        }
        Ok(Self {
            id: incident.id,
            status: incident.status,
            commander_ref: incident.commander_ref.clone(),
            leads: incident.leads.clone(),
            affected_services: incident.affected_services.clone(),
            affected_customer_accounts: incident.affected_customer_accounts.clone(),
            source_version,
            source_event_id,
            projected_at,
        })
    }
}

impl Incident {
    /// Validate an incident rehydrated from a transport or persistence
    /// boundary. Postmortem evidence is append-only until the terminal state,
    /// after which the aggregate becomes immutable.
    pub fn validate(&self) -> Result<(), SupportError> {
        if self.id.is_nil()
            || self.commander_ref.trim().is_empty()
            || self.commander_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || self.leads.iter().any(|value| {
                value.trim().is_empty()
                    || value.len() > MAX_TICKET_AUTHOR_REF_LENGTH
                    || value.chars().any(char::is_control)
            })
            || self.affected_services.iter().any(|value| {
                value.trim().is_empty()
                    || value.len() > MAX_TICKET_AUTHOR_REF_LENGTH
                    || value.chars().any(char::is_control)
            })
            || self.affected_customer_accounts.iter().any(Uuid::is_nil)
        {
            return Err(SupportError::InvalidIncidentField);
        }
        let mut action_ids = BTreeSet::new();
        for entry in &self.timeline {
            if entry.actor_ref.trim().is_empty()
                || entry.message.trim().is_empty()
                || entry.actor_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
                || entry.message.len() > MAX_SUPPORT_REASON_LENGTH
                || entry.actor_ref.chars().any(char::is_control)
                || entry.message.chars().any(char::is_control)
            {
                return Err(SupportError::InvalidIncidentField);
            }
        }
        for broadcast in &self.broadcasts {
            if broadcast.actor_ref.trim().is_empty()
                || broadcast.audience.trim().is_empty()
                || broadcast.message.trim().is_empty()
                || broadcast.actor_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
                || broadcast.audience.len() > MAX_TICKET_AUTHOR_REF_LENGTH
                || broadcast.message.len() > MAX_SUPPORT_REASON_LENGTH
                || broadcast.actor_ref.chars().any(char::is_control)
                || broadcast.audience.chars().any(char::is_control)
                || broadcast.message.chars().any(char::is_control)
            {
                return Err(SupportError::InvalidIncidentField);
            }
        }
        for value in [&self.mitigation, &self.root_cause].into_iter().flatten() {
            if value.trim().is_empty()
                || value.len() > MAX_SUPPORT_REASON_LENGTH
                || value.chars().any(char::is_control)
            {
                return Err(SupportError::InvalidIncidentField);
            }
        }
        for action in &self.corrective_actions {
            if action.id.is_nil()
                || !action_ids.insert(action.id)
                || action.owner_ref.trim().is_empty()
                || action.description.trim().is_empty()
                || action.owner_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
                || action.description.len() > MAX_SUPPORT_REASON_LENGTH
                || action.owner_ref.chars().any(char::is_control)
                || action.description.chars().any(char::is_control)
                || action
                    .completed_at
                    .zip(action.due_at)
                    .is_some_and(|(completed, due)| completed < due)
            {
                return Err(SupportError::InvalidIncidentField);
            }
        }
        Ok(())
    }

    fn ensure_mutable(&self) -> Result<(), SupportError> {
        if self.status == IncidentStatus::PostmortemComplete {
            Err(SupportError::InvalidIncidentTransition)
        } else {
            Ok(())
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        commander_ref: impl Into<String>,
        leads: Vec<String>,
        affected_services: Vec<String>,
        affected_customer_accounts: Vec<Uuid>,
        at: DateTime<Utc>,
    ) -> Result<Self, SupportError> {
        let commander_ref = commander_ref.into();
        if commander_ref.trim().is_empty()
            || leads.iter().any(|lead| lead.trim().is_empty())
            || affected_services
                .iter()
                .any(|service| service.trim().is_empty())
        {
            return Err(SupportError::InvalidIncidentField);
        }
        let incident = Self {
            id: Uuid::new_v4(),
            status: IncidentStatus::Investigating,
            commander_ref,
            leads,
            affected_services,
            affected_customer_accounts,
            timeline: Vec::new(),
            broadcasts: Vec::new(),
            mitigation: None,
            root_cause: None,
            corrective_actions: Vec::new(),
        };
        incident.validate().and_then(|_| {
            let mut incident = incident;
            incident.add_timeline_entry(at, incident.commander_ref.clone(), "incident opened")?;
            incident.validate()?;
            Ok(incident)
        })
    }

    pub fn transition(
        &mut self,
        next: IncidentStatus,
        at: DateTime<Utc>,
        actor_ref: impl Into<String>,
    ) -> Result<(), SupportError> {
        self.validate()?;
        self.ensure_mutable()?;
        let valid = matches!(
            (self.status, next),
            (IncidentStatus::Investigating, IncidentStatus::Identified)
                | (IncidentStatus::Identified, IncidentStatus::Monitoring)
                | (IncidentStatus::Monitoring, IncidentStatus::Resolved)
                | (IncidentStatus::Resolved, IncidentStatus::PostmortemComplete)
        );
        if !valid {
            return Err(SupportError::InvalidIncidentTransition);
        }
        let actor_ref = actor_ref.into();
        if actor_ref.trim().is_empty() {
            return Err(SupportError::InvalidIncidentField);
        }
        self.add_timeline_entry(at, actor_ref, format!("status changed to {next:?}"))?;
        self.status = next;
        Ok(())
    }

    pub fn add_timeline_entry(
        &mut self,
        at: DateTime<Utc>,
        actor_ref: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<(), SupportError> {
        self.ensure_mutable()?;
        let actor_ref = actor_ref.into();
        let message = message.into();
        if actor_ref.trim().is_empty()
            || message.trim().is_empty()
            || actor_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || message.len() > MAX_SUPPORT_REASON_LENGTH
            || actor_ref.chars().any(char::is_control)
            || message.chars().any(char::is_control)
        {
            return Err(SupportError::InvalidIncidentField);
        }
        self.timeline.push(IncidentTimelineEntry {
            at,
            actor_ref,
            message,
        });
        Ok(())
    }

    pub fn add_broadcast(
        &mut self,
        at: DateTime<Utc>,
        actor_ref: impl Into<String>,
        audience: impl Into<String>,
        message: impl Into<String>,
    ) -> Result<(), SupportError> {
        self.ensure_mutable()?;
        let actor_ref = actor_ref.into();
        let audience = audience.into();
        let message = message.into();
        if actor_ref.trim().is_empty()
            || audience.trim().is_empty()
            || message.trim().is_empty()
            || actor_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || audience.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || message.len() > MAX_SUPPORT_REASON_LENGTH
            || actor_ref.chars().any(char::is_control)
            || audience.chars().any(char::is_control)
            || message.chars().any(char::is_control)
        {
            return Err(SupportError::InvalidIncidentField);
        }
        self.broadcasts.push(IncidentBroadcast {
            at,
            actor_ref,
            audience,
            message,
        });
        Ok(())
    }

    pub fn record_mitigation(&mut self, mitigation: impl Into<String>) -> Result<(), SupportError> {
        self.ensure_mutable()?;
        let mitigation = mitigation.into();
        if mitigation.trim().is_empty()
            || mitigation.len() > MAX_SUPPORT_REASON_LENGTH
            || mitigation.chars().any(char::is_control)
        {
            return Err(SupportError::InvalidIncidentField);
        }
        self.mitigation = Some(mitigation);
        Ok(())
    }

    pub fn record_root_cause(&mut self, root_cause: impl Into<String>) -> Result<(), SupportError> {
        self.ensure_mutable()?;
        let root_cause = root_cause.into();
        if root_cause.trim().is_empty()
            || root_cause.len() > MAX_SUPPORT_REASON_LENGTH
            || root_cause.chars().any(char::is_control)
        {
            return Err(SupportError::InvalidIncidentField);
        }
        self.root_cause = Some(root_cause);
        Ok(())
    }

    pub fn add_corrective_action(
        &mut self,
        owner_ref: impl Into<String>,
        description: impl Into<String>,
        due_at: Option<DateTime<Utc>>,
    ) -> Result<Uuid, SupportError> {
        self.ensure_mutable()?;
        let owner_ref = owner_ref.into();
        let description = description.into();
        if owner_ref.trim().is_empty()
            || description.trim().is_empty()
            || owner_ref.len() > MAX_TICKET_AUTHOR_REF_LENGTH
            || description.len() > MAX_SUPPORT_REASON_LENGTH
            || owner_ref.chars().any(char::is_control)
            || description.chars().any(char::is_control)
        {
            return Err(SupportError::InvalidIncidentField);
        }
        let id = Uuid::new_v4();
        self.corrective_actions.push(CorrectiveAction {
            id,
            owner_ref,
            description,
            due_at,
            completed_at: None,
        });
        Ok(id)
    }

    pub fn complete_corrective_action(
        &mut self,
        action_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), SupportError> {
        self.ensure_mutable()?;
        let action = self
            .corrective_actions
            .iter_mut()
            .find(|action| action.id == action_id)
            .ok_or(SupportError::InvalidIncidentField)?;
        if action.completed_at.is_some() || action.due_at.is_some_and(|due| at < due) {
            return Err(SupportError::InvalidIncidentField);
        }
        action.completed_at = Some(at);
        Ok(())
    }
}

fn normalize_field_name(field: &str) -> String {
    field.trim().to_ascii_lowercase().replace('-', "_")
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn decode_hex(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let bytes = value.as_bytes();
    let mut decoded = [0_u8; 32];
    for (index, output) in decoded.iter_mut().enumerate() {
        let high = hex_digit(bytes[index * 2])?;
        let low = hex_digit(bytes[index * 2 + 1])?;
        *output = (high << 4) | low;
    }
    Some(decoded)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

// Kept private because the identity crate's TokenHash is intentionally not a
// domain dependency. This is SHA-256 over an OS-random 256-bit bearer value;
// only its 32-byte digest is stored on the capability.
fn sha256(input: &[u8]) -> [u8; 32] {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const ROUND_CONSTANTS: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_length = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());

    let mut state = INITIAL;
    for chunk in message.chunks_exact(64) {
        let mut words = [0_u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) = (
            state[0], state[1], state[2], state[3], state[4], state[5], state[6], state[7],
        );
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(ROUND_CONSTANTS[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut digest = [0_u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_and_impersonation_have_two_hour_limits() {
        let now = Utc::now();
        let customer = Uuid::new_v4();
        assert_eq!(
            SupportSession::request(
                Uuid::new_v4(),
                customer,
                "agent",
                SupportMode::TenantMutation,
                vec![],
                "finance",
                "repair",
                now,
                Duration::hours(3),
                None
            ),
            Err(SupportError::ExceedsMaximum)
        );
        assert_eq!(
            SupportSession::request(
                Uuid::new_v4(),
                customer,
                "agent",
                SupportMode::Impersonation,
                vec![],
                "finance",
                "repair",
                now,
                Duration::hours(1),
                None
            ),
            Err(SupportError::MissingRepresentedUser)
        );
    }

    #[test]
    fn support_module_is_part_of_the_approval_and_cannot_be_approved_twice() {
        let now = Utc::now();
        let mut session = SupportSession::request_with_module(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "agent",
            SupportMode::TenantMutation,
            vec!["operations:write".into()],
            "holding:1",
            "operations",
            "repair",
            now,
            Duration::hours(1),
            None,
        )
        .expect("module-bound support session");
        assert_eq!(session.module, "operations");
        session.approve("customer-security", now).unwrap();
        assert_eq!(
            session.approve("second-approver", now),
            Err(SupportError::AlreadyApproved)
        );
    }

    #[test]
    fn approval_expiry_and_revocation_are_rechecked_for_each_action() {
        let now = Utc::now();
        let mut session = SupportSession::request(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "agent",
            SupportMode::DiagnosticRead,
            vec!["finance:view".into()],
            "finance",
            "diagnose",
            now,
            Duration::hours(1),
            None,
        )
        .expect("request");
        assert_eq!(
            session.authorize(now, "finance:view", "finance"),
            Err(SupportError::NotApproved)
        );
        session.approve("customer-security", now).expect("approve");
        session
            .authorize(now + Duration::minutes(20), "finance:view", "finance")
            .expect("active capability");
        session.revoke(now + Duration::minutes(21));
        assert_eq!(
            session.authorize(now + Duration::minutes(22), "finance:view", "finance"),
            Err(SupportError::Inactive)
        );
    }

    #[test]
    fn ticket_lifecycle_is_ordered() {
        let mut ticket = SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id: Uuid::new_v4(),
            status: TicketStatus::New,
            severity: "sev3".into(),
            description: "issue".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        ticket.transition(TicketStatus::Triaged).expect("triage");
        assert_eq!(
            ticket.transition(TicketStatus::Closed),
            Err(SupportError::InvalidTicketTransition)
        );
    }

    #[test]
    fn ticket_lifecycle_matches_the_frozen_support_contract() {
        let mut ticket = SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id: Uuid::new_v4(),
            status: TicketStatus::New,
            severity: "sev3".into(),
            description: "issue".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        for next in [
            TicketStatus::Triaged,
            TicketStatus::InProgress,
            TicketStatus::WaitingForCustomer,
            TicketStatus::AccessRequested,
            TicketStatus::Triaged,
            TicketStatus::InProgress,
            TicketStatus::Resolved,
            TicketStatus::InProgress,
            TicketStatus::Resolved,
            TicketStatus::CustomerConfirmed,
            TicketStatus::Closed,
        ] {
            ticket.transition(next).expect("contract transition");
        }
        assert_eq!(
            ticket.transition(TicketStatus::InProgress),
            Err(SupportError::InvalidTicketTransition)
        );
    }

    #[test]
    fn ticket_lifecycle_rejects_every_forbidden_shortcut() {
        let statuses = [
            TicketStatus::New,
            TicketStatus::Triaged,
            TicketStatus::WaitingForCustomer,
            TicketStatus::AccessRequested,
            TicketStatus::Approved,
            TicketStatus::InProgress,
            TicketStatus::Resolved,
            TicketStatus::CustomerConfirmed,
            TicketStatus::Closed,
        ];
        let allowed = [
            (TicketStatus::New, TicketStatus::Triaged),
            (TicketStatus::Triaged, TicketStatus::WaitingForCustomer),
            (TicketStatus::Triaged, TicketStatus::AccessRequested),
            (TicketStatus::Triaged, TicketStatus::InProgress),
            (TicketStatus::WaitingForCustomer, TicketStatus::Triaged),
            (
                TicketStatus::WaitingForCustomer,
                TicketStatus::AccessRequested,
            ),
            (TicketStatus::WaitingForCustomer, TicketStatus::InProgress),
            (TicketStatus::AccessRequested, TicketStatus::Approved),
            (TicketStatus::AccessRequested, TicketStatus::Triaged),
            (TicketStatus::Approved, TicketStatus::InProgress),
            (TicketStatus::InProgress, TicketStatus::WaitingForCustomer),
            (TicketStatus::InProgress, TicketStatus::AccessRequested),
            (TicketStatus::InProgress, TicketStatus::Resolved),
            (TicketStatus::Resolved, TicketStatus::InProgress),
            (TicketStatus::Resolved, TicketStatus::CustomerConfirmed),
            (TicketStatus::CustomerConfirmed, TicketStatus::Closed),
        ];

        for from in statuses {
            for next in statuses {
                let mut ticket = support_ticket_fixture(from);
                let result = ticket.transition(next);
                assert_eq!(
                    result.is_ok(),
                    allowed.contains(&(from, next)),
                    "unexpected support transition {from:?} -> {next:?}"
                );
            }
        }
    }

    #[test]
    fn closed_ticket_recurrence_is_linked_and_does_not_reopen_the_parent() {
        let mut closed = SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id: Uuid::new_v4(),
            status: TicketStatus::Closed,
            severity: "sev2".into(),
            description: "original issue".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        let parent_id = closed.id;
        let recurrence = closed
            .recurrence(Uuid::new_v4(), "sev1", "issue returned")
            .expect("closed ticket can create a recurrence");
        assert_eq!(recurrence.status, TicketStatus::New);
        assert_eq!(recurrence.customer_account_id, closed.customer_account_id);
        assert_eq!(recurrence.recurrence_of, Some(parent_id));
        assert_eq!(closed.status, TicketStatus::Closed);
        assert_eq!(
            closed.transition(TicketStatus::InProgress),
            Err(SupportError::InvalidTicketTransition)
        );

        closed.status = TicketStatus::Resolved;
        assert_eq!(
            closed.recurrence(Uuid::new_v4(), "sev3", "not closed"),
            Err(SupportError::RecurrenceRequiresClosedTicket)
        );
    }

    #[test]
    fn ticket_audiences_are_explicit_and_customer_projection_excludes_internal_notes() {
        let now = Utc::now();
        let mut ticket = SupportTicket {
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
        let customer_reply = SupportTicketCommand::CustomerReply {
            message_id: Uuid::new_v4(),
            author_ref: "agent-1".into(),
            body: "We are investigating this for you".into(),
            created_at: now,
        };
        ticket
            .apply_command(customer_reply, &TenantScope::Platform, now)
            .expect("customer reply");
        let internal_note = SupportTicketCommand::InternalNote {
            message_id: Uuid::new_v4(),
            author_ref: "agent-1".into(),
            body: "database credentials were rotated".into(),
            created_at: now,
        };
        ticket
            .apply_command(internal_note, &TenantScope::Platform, now)
            .expect("internal note");

        let customer_view = ticket.customer_visible_view();
        assert_eq!(customer_view.messages.len(), 1);
        assert_eq!(
            customer_view.messages[0].body,
            "We are investigating this for you"
        );
        assert!(
            !serde_json::to_string(&customer_view)
                .expect("customer view serializes")
                .contains("database credentials")
        );
        assert!(
            serde_json::to_string(&ticket)
                .expect("durable aggregate serializes")
                .contains("database credentials")
        );
    }

    #[test]
    fn ticket_metadata_commands_enforce_scope_sla_and_escalation_bounds() {
        let now = Utc::now();
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let mut ticket = SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id: Uuid::new_v4(),
            status: TicketStatus::New,
            severity: "sev3".into(),
            description: "needs assignment".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        let assignment_scope = TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        };
        let assignment =
            SupportTicketAssignment::new("queue-owner", "manager", assignment_scope.clone(), now)
                .expect("bounded assignment");
        ticket
            .apply_command(
                SupportTicketCommand::Assign { assignment },
                &TenantScope::Company {
                    holding_id,
                    company_id,
                },
                now,
            )
            .expect("company can assign a contained branch");

        let outside_scope = SupportTicketAssignment::new(
            "sibling-owner",
            "manager",
            TenantScope::Branch {
                holding_id,
                company_id,
                branch_id: Uuid::new_v4(),
            },
            now,
        )
        .expect("bounded sibling assignment");
        assert_eq!(
            ticket.apply_command(
                SupportTicketCommand::Assign {
                    assignment: outside_scope,
                },
                &assignment_scope,
                now,
            ),
            Err(SupportError::TicketAssignmentScopeDenied)
        );

        ticket
            .apply_command(
                SupportTicketCommand::SetSlaDeadline {
                    deadline: now + Duration::days(30),
                },
                &TenantScope::Platform,
                now,
            )
            .expect("bounded SLA deadline");
        assert_eq!(
            ticket.apply_command(
                SupportTicketCommand::SetSlaDeadline {
                    deadline: now + Duration::days(366),
                },
                &TenantScope::Platform,
                now,
            ),
            Err(SupportError::InvalidTicketSlaDeadline)
        );

        ticket
            .apply_command(
                SupportTicketCommand::Escalate {
                    escalation: SupportTicketEscalation::new(
                        3,
                        "support-lead",
                        "SLA risk",
                        "manager",
                        now,
                    )
                    .expect("bounded escalation"),
                },
                &TenantScope::Platform,
                now,
            )
            .expect("escalate");
        assert_eq!(
            ticket.apply_command(
                SupportTicketCommand::Escalate {
                    escalation: SupportTicketEscalation::new(
                        2,
                        "support-lead",
                        "cannot lower escalation",
                        "manager",
                        now,
                    )
                    .expect("valid lower level value"),
                },
                &TenantScope::Platform,
                now,
            ),
            Err(SupportError::EscalationLevelRegression)
        );
    }

    #[test]
    fn closed_ticket_rejects_metadata_commands_as_terminal() {
        let now = Utc::now();
        let mut closed = SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id: Uuid::new_v4(),
            status: TicketStatus::Closed,
            severity: "sev2".into(),
            description: "terminal".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        };
        let before = closed.clone();
        assert_eq!(
            closed.apply_command(
                SupportTicketCommand::InternalNote {
                    message_id: Uuid::new_v4(),
                    author_ref: "agent".into(),
                    body: "must not mutate".into(),
                    created_at: now,
                },
                &TenantScope::Platform,
                now,
            ),
            Err(SupportError::ClosedTicketTerminal)
        );
        assert_eq!(closed, before);
    }

    fn support_ticket_fixture(status: TicketStatus) -> SupportTicket {
        SupportTicket {
            id: Uuid::new_v4(),
            customer_account_id: Uuid::new_v4(),
            status,
            severity: "sev2".into(),
            description: "attachment/SLA fixture".into(),
            recurrence_of: None,
            messages: Vec::new(),
            assignment: None,
            escalation: None,
            sla_deadline: None,
        }
    }

    fn attachment_upload(
        id: Uuid,
        object_ref: &str,
        visibility: SupportAttachmentVisibility,
        expires_at: DateTime<Utc>,
    ) -> SupportAttachmentUpload {
        SupportAttachmentUpload {
            id,
            object_ref: object_ref.into(),
            content_type: "application/pdf".into(),
            content_size_bytes: 128,
            sha256_hex: "AB".repeat(32),
            visibility,
            uploader_ref: "support-agent-1".into(),
            scope: TenantScope::Holding {
                holding_id: Uuid::new_v4(),
            },
            expires_at,
        }
    }

    #[test]
    fn support_attachment_validates_opaque_ref_content_metadata_and_expiry() {
        let now = Utc::now();
        let ticket = support_ticket_fixture(TicketStatus::New);
        let attachment = SupportAttachment::from_upload(
            ticket.id,
            ticket.customer_account_id,
            attachment_upload(
                Uuid::new_v4(),
                "attachments/opaque-object-1",
                SupportAttachmentVisibility::CustomerVisible,
                now + Duration::hours(1),
            ),
            now,
        )
        .expect("valid attachment metadata");
        assert_eq!(attachment.sha256_hex, "ab".repeat(32));
        assert!(
            SupportAttachment::from_upload(
                ticket.id,
                ticket.customer_account_id,
                attachment_upload(
                    Uuid::new_v4(),
                    "../escape",
                    SupportAttachmentVisibility::Internal,
                    now + Duration::hours(1),
                ),
                now,
            )
            .is_err()
        );
        assert!(
            SupportAttachment::from_upload(
                ticket.id,
                ticket.customer_account_id,
                SupportAttachmentUpload {
                    content_type: "application/pdf;secret=true".into(),
                    ..attachment_upload(
                        Uuid::new_v4(),
                        "attachments/opaque-object-2",
                        SupportAttachmentVisibility::Internal,
                        now + Duration::hours(1),
                    )
                },
                now,
            )
            .is_err()
        );
        assert!(
            SupportAttachment::from_upload(
                ticket.id,
                ticket.customer_account_id,
                SupportAttachmentUpload {
                    content_size_bytes: MAX_SUPPORT_ATTACHMENT_SIZE_BYTES + 1,
                    ..attachment_upload(
                        Uuid::new_v4(),
                        "attachments/opaque-object-3",
                        SupportAttachmentVisibility::Internal,
                        now + Duration::hours(1),
                    )
                },
                now,
            )
            .is_err()
        );
        assert!(
            SupportAttachment::from_upload(
                ticket.id,
                ticket.customer_account_id,
                attachment_upload(
                    Uuid::new_v4(),
                    "attachments/opaque-object-4",
                    SupportAttachmentVisibility::Internal,
                    now - Duration::seconds(1),
                ),
                now,
            )
            .is_err()
        );
    }

    #[test]
    fn support_attachment_set_enforces_binding_count_terminal_and_audience_projection() {
        let now = Utc::now();
        let ticket = support_ticket_fixture(TicketStatus::New);
        let customer = SupportAttachment::from_upload(
            ticket.id,
            ticket.customer_account_id,
            attachment_upload(
                Uuid::new_v4(),
                "attachments/customer-visible",
                SupportAttachmentVisibility::CustomerVisible,
                now + Duration::hours(1),
            ),
            now,
        )
        .expect("customer attachment");
        let internal = SupportAttachment::from_upload(
            ticket.id,
            ticket.customer_account_id,
            attachment_upload(
                Uuid::new_v4(),
                "attachments/internal",
                SupportAttachmentVisibility::Internal,
                now + Duration::hours(1),
            ),
            now,
        )
        .expect("internal attachment");
        let expired = SupportAttachment::from_upload(
            ticket.id,
            ticket.customer_account_id,
            attachment_upload(
                Uuid::new_v4(),
                "attachments/expired",
                SupportAttachmentVisibility::CustomerVisible,
                now - Duration::seconds(1),
            ),
            now - Duration::hours(1),
        )
        .expect("historically valid attachment");

        let attachments = SupportAttachmentSet::append(&ticket, &[], customer.clone(), now)
            .and_then(|items| SupportAttachmentSet::append(&ticket, &items, internal.clone(), now))
            .expect("append attachments");
        let mut with_expired = attachments.clone();
        with_expired.push(expired);
        let visible = SupportAttachmentSet::customer_visible(&ticket, &with_expired, now);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].object_ref, customer.object_ref);
        let redacted = internal.redacted_for_log();
        assert!(
            !serde_json::to_string(&redacted)
                .expect("redacted attachment serializes")
                .contains("attachments/internal")
        );

        let mismatch = SupportAttachment::from_upload(
            ticket.id,
            Uuid::new_v4(),
            attachment_upload(
                Uuid::new_v4(),
                "attachments/mismatch",
                SupportAttachmentVisibility::Internal,
                now + Duration::hours(1),
            ),
            now,
        )
        .expect("attachment with a different customer binding");
        assert_eq!(
            SupportAttachmentSet::append(&ticket, &attachments, mismatch, now),
            Err(SupportError::SupportAttachmentTicketBindingDenied)
        );

        let mut maximum = Vec::new();
        for index in 0..MAX_SUPPORT_ATTACHMENTS_PER_TICKET {
            let attachment = SupportAttachment::from_upload(
                ticket.id,
                ticket.customer_account_id,
                attachment_upload(
                    Uuid::new_v4(),
                    &format!("attachments/count-{index}"),
                    SupportAttachmentVisibility::Internal,
                    now + Duration::hours(1),
                ),
                now,
            )
            .expect("bounded count attachment");
            maximum = SupportAttachmentSet::append(&ticket, &maximum, attachment, now)
                .expect("attachment remains within count limit");
        }
        assert_eq!(
            SupportAttachmentSet::append(&ticket, &maximum, customer, now),
            Err(SupportError::SupportAttachmentLimitExceeded)
        );

        let closed = support_ticket_fixture(TicketStatus::Closed);
        let closed_attachment = SupportAttachment::from_upload(
            closed.id,
            closed.customer_account_id,
            attachment_upload(
                Uuid::new_v4(),
                "attachments/closed",
                SupportAttachmentVisibility::Internal,
                now + Duration::hours(1),
            ),
            now,
        )
        .expect("closed ticket fixture attachment");
        assert_eq!(
            SupportAttachmentSet::append(&closed, &[], closed_attachment, now),
            Err(SupportError::ClosedTicketTerminal)
        );
    }

    #[test]
    fn support_sla_notifications_are_deterministic_audience_safe_and_expire() {
        let now = Utc::now();
        let mut ticket = support_ticket_fixture(TicketStatus::InProgress);
        ticket.sla_deadline = Some(now + Duration::hours(2));
        let policy = SupportSlaNotificationPolicy::new(
            Duration::hours(24),
            Duration::hours(2),
            true,
            true,
            true,
            true,
        )
        .expect("bounded notification policy");
        let first = policy.notifications_for(&ticket, now);
        let second = policy.notifications_for(&ticket, now);
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert_ne!(first[0].key, first[1].key);
        assert!(first.iter().all(|notification| {
            notification.ticket_id == ticket.id
                && notification.customer_account_id == ticket.customer_account_id
                && notification.is_deliverable_at(&ticket, now)
        }));
        assert!(first.iter().any(|notification| {
            notification.audience == SupportNotificationAudience::CustomerVisible
        }));

        let deadline = ticket.sla_deadline.expect("deadline");
        let breached = policy.notifications_for(&ticket, deadline);
        assert!(breached.iter().all(|notification| {
            notification.classification == SupportSlaNotificationClassification::DeadlineBreached
                && notification.is_deliverable_at(&ticket, deadline)
        }));
        let after_expiry = deadline + Duration::hours(2);
        assert!(policy.notifications_for(&ticket, after_expiry).is_empty());
        assert!(
            breached
                .iter()
                .all(|notification| !notification.is_deliverable_at(&ticket, after_expiry))
        );

        ticket.status = TicketStatus::Closed;
        assert!(policy.notifications_for(&ticket, now).is_empty());
        assert!(policy.notifications_for(&ticket, deadline).is_empty());
    }

    fn capability_binding(mode: SupportMode) -> SupportCapabilityBinding {
        SupportCapabilityBinding::new(
            Uuid::new_v4(),
            "agent-1",
            "device-1",
            Uuid::new_v4(),
            Uuid::new_v4(),
            mode,
            vec!["diagnostics:read".into(), "support:read".into()],
            "holding-1/company-1/branch-1",
            "operations",
        )
        .expect("valid capability binding")
    }

    #[test]
    fn capability_modes_enforce_their_exact_maximum_duration() {
        let now = Utc::now();
        for (mode, represented_duration) in [
            (SupportMode::DiagnosticRead, Duration::hours(8)),
            (SupportMode::TenantMutation, Duration::hours(2)),
            (SupportMode::Impersonation, Duration::hours(2)),
        ] {
            let binding = capability_binding(mode);
            SupportCapability::issue(binding.clone(), now, represented_duration)
                .expect("mode maximum is allowed");
            assert!(matches!(
                SupportCapability::issue(
                    binding,
                    now,
                    represented_duration + Duration::nanoseconds(1)
                ),
                Err(SupportError::ExceedsMaximum)
            ));
        }
    }

    #[test]
    fn capability_secret_is_hash_only_and_never_disclosed() {
        let now = Utc::now();
        let binding = capability_binding(SupportMode::DiagnosticRead);
        let (capability, secret) =
            SupportCapability::issue(binding.clone(), now, Duration::hours(1)).expect("issue");
        let raw_secret = secret.as_hex();
        capability
            .authenticate(&secret, &binding, now + Duration::minutes(1))
            .expect("valid bearer and binding");
        assert_eq!(format!("{secret:?}"), "SupportCapabilitySecret(REDACTED)");
        assert_eq!(
            format!("{:?}", capability.secret_hash()),
            "SupportCapabilitySecretHash(REDACTED)"
        );
        let serialized = serde_json::to_string(&capability).expect("serialize capability");
        assert!(!serialized.contains(&raw_secret));
        assert!(!format!("{capability:?}").contains(&raw_secret));
        assert_eq!(
            raw_secret.len(),
            64,
            "secret has 256 bits of bearer material"
        );
    }

    #[test]
    fn capability_expiry_revocation_and_reissue_cannot_extend_approval() {
        let now = Utc::now();
        let binding = capability_binding(SupportMode::TenantMutation);
        let (mut capability, secret) =
            SupportCapability::issue(binding.clone(), now, Duration::hours(1)).expect("issue");
        assert_eq!(
            capability.authenticate(&secret, &binding, capability.expires_at),
            Err(SupportError::Inactive)
        );
        let reissue_at = now + Duration::minutes(10);
        let (reissued, reissued_secret) = capability.reissue(reissue_at).expect("reissue");
        assert_eq!(reissued.expires_at, capability.expires_at);
        assert_eq!(reissued.reissued_from, Some(capability.id));
        assert_ne!(secret.as_hex(), reissued_secret.as_hex());
        capability
            .revoke(
                SupportCapabilityRevocationReason::CustomerRequested,
                reissue_at,
            )
            .expect("revoke");
        assert_eq!(
            capability.authenticate(&secret, &binding, reissue_at),
            Err(SupportError::Inactive)
        );
        assert!(matches!(
            capability.reissue(reissue_at),
            Err(SupportError::Inactive)
        ));
        assert_eq!(
            capability.revoke(SupportCapabilityRevocationReason::TicketClosed, reissue_at),
            Err(SupportError::AlreadyRevoked)
        );
    }

    #[test]
    fn diagnostic_policy_forbids_secrets_tokens_and_unrelated_business_data() {
        let input = BTreeMap::from([
            ("versions".into(), "server=1.2.3".into()),
            ("worker-health".into(), "healthy".into()),
            ("password".into(), "do-not-export".into()),
            ("access_token".into(), "do-not-export".into()),
            ("private_key".into(), "do-not-export".into()),
            ("unrelated_business_data".into(), "do-not-export".into()),
        ]);
        let report = DiagnosticRedactionPolicy.redact(&input);
        assert_eq!(report.fields.get("versions"), Some(&"server=1.2.3".into()));
        assert_eq!(report.fields.get("worker-health"), Some(&"healthy".into()));
        for forbidden in FORBIDDEN_DIAGNOSTIC_FIELDS {
            assert!(!report.fields.contains_key(*forbidden));
        }
        assert!(report.redacted_fields.contains("password"));
        assert!(report.redacted_fields.contains("access_token"));
        assert!(
            !serde_json::to_string(&report)
                .expect("serialize diagnostic report")
                .contains("do-not-export")
        );
    }

    #[test]
    fn incident_lifecycle_records_response_and_postmortem_evidence() {
        let now = Utc::now();
        let mut incident = Incident::new(
            "commander-1",
            vec!["lead-1".into(), "lead-2".into()],
            vec!["api".into(), "worker".into()],
            vec![Uuid::new_v4()],
            now,
        )
        .expect("incident");
        incident
            .add_broadcast(now, "commander-1", "customers", "Investigating impact")
            .expect("broadcast");
        incident
            .record_mitigation("paused the failing worker")
            .expect("mitigation");
        incident
            .transition(IncidentStatus::Identified, now, "commander-1")
            .expect("identified");
        incident
            .transition(IncidentStatus::Monitoring, now, "lead-1")
            .expect("monitoring");
        incident
            .record_root_cause("bad projection deployment")
            .expect("root cause");
        let action = incident
            .add_corrective_action("lead-2", "add projection canary", None)
            .expect("corrective action");
        incident
            .complete_corrective_action(action, now)
            .expect("complete action");
        incident
            .transition(IncidentStatus::Resolved, now, "commander-1")
            .expect("resolved");
        incident
            .transition(IncidentStatus::PostmortemComplete, now, "commander-1")
            .expect("postmortem complete");
        assert_eq!(incident.status, IncidentStatus::PostmortemComplete);
        assert_eq!(incident.broadcasts.len(), 1);
        assert!(incident.mitigation.is_some());
        assert!(incident.root_cause.is_some());
        assert_eq!(incident.corrective_actions[0].completed_at, Some(now));
        assert!(matches!(
            incident.transition(IncidentStatus::Investigating, now, "commander-1"),
            Err(SupportError::InvalidIncidentTransition)
        ));
    }

    fn fixed_now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp")
    }

    #[allow(clippy::too_many_arguments)]
    fn role(
        principal_ref: &str,
        role_key: &str,
        permissions: &[&str],
        functional_domain: &str,
        scope: Scope,
        valid_from: DateTime<Utc>,
        valid_until: DateTime<Utc>,
        parent_assignment_id: Option<Uuid>,
    ) -> RoleAssignment {
        RoleAssignment {
            id: Uuid::new_v4(),
            principal_ref: principal_ref.into(),
            role_key: role_key.into(),
            permissions: permissions
                .iter()
                .map(|permission| (*permission).into())
                .collect(),
            functional_domain: functional_domain.into(),
            authority_level: 1,
            approval_limit_minor: 0,
            scope,
            valid_from,
            valid_until,
            timezone: "UTC".into(),
            risk_tier: crate::access::RiskTier::Standard,
            grant_reason: "support intersection test".into(),
            ticket_reference: Some("ticket-1".into()),
            grantor: "grantor".into(),
            approver: None,
            renewal_reviewer: None,
            parent_assignment_id,
            assignment_version: 1,
        }
    }

    struct IntersectionFixture {
        now: DateTime<Utc>,
        scope: Scope,
        scope_key: String,
        session: SupportSession,
        capability: SupportCapability,
        secret: SupportCapabilitySecret,
        agent_role: RoleAssignment,
        represented_user_role: Option<RoleAssignment>,
        policy: SupportDomainPolicy,
        entitlement: ModuleEntitlement,
    }

    impl IntersectionFixture {
        fn new(mode: SupportMode) -> Self {
            let now = fixed_now();
            let scope = Scope::Platform;
            let scope_key = scope.canonical_key();
            let represented_user = mode
                .requires_represented_user()
                .then(|| "represented-user".to_string());
            let session = SupportSession::request(
                Uuid::from_u128(1),
                Uuid::from_u128(2),
                "agent-1",
                mode,
                vec!["support:read".into()],
                scope_key.clone(),
                "approved support repair",
                now,
                Duration::hours(1),
                represented_user.clone(),
            )
            .expect("session request");
            let binding = SupportCapabilityBinding::new(
                session.customer_account_id,
                session.agent_ref.clone(),
                "device-1",
                session.id,
                session.ticket_id,
                session.mode,
                session.permissions.clone(),
                session.scope.clone(),
                "operations",
            )
            .expect("capability binding");
            let (capability, secret) =
                SupportCapability::issue(binding, now, Duration::hours(1)).expect("capability");
            let agent_role = role(
                "agent-1",
                "platform_support",
                &["support:access"],
                "platform",
                Scope::Platform,
                now,
                now + Duration::hours(2),
                None,
            );
            let represented_user_role = represented_user.map(|principal_ref| {
                role(
                    &principal_ref,
                    "tenant_user",
                    &["support:read"],
                    "operations",
                    Scope::Platform,
                    now,
                    now + Duration::hours(2),
                    None,
                )
            });
            let policy = SupportDomainPolicy::new(
                "support:access",
                BTreeSet::from(["operations".into()]),
                true,
                true,
                true,
                true,
            );
            let entitlement = ModuleEntitlement {
                module_key: "operations".into(),
                active: true,
                seats: 1,
                company_limit: 1,
                branch_limit: 1,
                active_until: None,
            };
            Self {
                now,
                scope,
                scope_key,
                session,
                capability,
                secret,
                agent_role,
                represented_user_role,
                policy,
                entitlement,
            }
        }

        fn approve(&mut self) {
            self.session
                .approve("customer-security", self.now)
                .expect("customer approval");
        }

        fn request(&self) -> SupportIntersectionRequest<'_> {
            SupportIntersectionRequest {
                at: self.now,
                agent_identity_ref: "agent-1",
                agent_identity_active: true,
                agent_device_ref: "device-1",
                agent_device_active: true,
                session: &self.session,
                capability: &self.capability,
                capability_secret: &self.secret,
                agent_role: &self.agent_role,
                agent_parent_role: None,
                represented_user_identity_active: true,
                represented_user_role: self.represented_user_role.as_ref(),
                represented_user_parent_role: None,
                requested_scope: &self.scope,
                scope_key: &self.scope_key,
                module_key: "operations",
                required_permission: "support:read",
                billing_state: BillingState::Active,
                entitlement: Some(&self.entitlement),
                domain_policy: &self.policy,
                is_mutation: false,
            }
        }
    }

    #[test]
    fn support_intersection_allows_the_complete_read_path() {
        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        assert_eq!(evaluate_support_intersection(&fixture.request()), Ok(()));
    }

    #[test]
    fn support_intersection_denies_inactive_identity_device_and_session() {
        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        let mut request = fixture.request();
        request.agent_identity_active = false;
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::AgentIdentityInactive)
        );

        request.agent_identity_active = true;
        request.agent_device_active = false;
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::AgentDeviceInactive)
        );

        request.agent_device_active = true;
        fixture.session.expires_at = fixture.now;
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::SessionInactive)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        fixture.session.revoke(fixture.now);
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::SessionInactive)
        );
    }

    #[test]
    fn support_intersection_requires_approved_permission_and_exact_scope() {
        let fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::SessionNotApproved)
        );

        let mut fixture = fixture;
        fixture.approve();
        let mut request = fixture.request();
        request.required_permission = "support:write";
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::ApprovedPermissionDenied)
        );

        let mut request = fixture.request();
        request.scope_key = "holding:wrong";
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::ScopeEncodingMismatch)
        );

        fixture.session.scope = "holding:wrong".into();
        let mut request = fixture.request();
        request.scope_key = fixture.scope_key.as_str();
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::ApprovedScopeDenied)
        );
    }

    #[test]
    fn support_intersection_denies_capability_secret_binding_expiry_and_revocation() {
        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        let (wrong_capability, wrong_secret) = SupportCapability::issue(
            fixture.capability.binding.clone(),
            fixture.now,
            Duration::hours(1),
        )
        .expect("wrong secret capability");
        let mut request = fixture.request();
        request.capability = &wrong_capability;
        request.capability_secret = &fixture.secret;
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::CapabilitySecretInvalid)
        );
        request.capability = &fixture.capability;
        request.capability_secret = &wrong_secret;
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::CapabilitySecretInvalid)
        );

        let mut binding_mismatch = fixture.capability.clone();
        binding_mismatch.binding.module = "finance".into();
        let mut request = fixture.request();
        request.capability = &binding_mismatch;
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::CapabilityBindingDenied)
        );

        fixture.capability.expires_at = fixture.now;
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::CapabilityInactive)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        fixture
            .capability
            .revoke(
                SupportCapabilityRevocationReason::CustomerRequested,
                fixture.now,
            )
            .expect("revoke capability");
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::CapabilityInactive)
        );
    }

    #[test]
    fn support_intersection_checks_platform_role_and_parent_boundaries() {
        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        fixture.agent_role.functional_domain = "operations".into();
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::AgentPlatformRoleDenied)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        fixture.agent_role.valid_until = fixture.now;
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::AgentRoleInactive)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        fixture.agent_role.parent_assignment_id = Some(Uuid::from_u128(99));
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::AgentParentInactive)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        fixture.agent_role.permissions.clear();
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::AgentRolePermissionDenied)
        );
    }

    #[test]
    fn support_intersection_checks_policy_sod_entitlement_and_billing() {
        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        fixture.policy.permitted_modules.clear();
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::DomainPolicyDenied)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::Impersonation);
        fixture.approve();
        fixture.policy.impersonation_allowed = false;
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::DomainPolicyDenied)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        fixture.policy.segregation_of_duties_passed = false;
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::SegregationOfDutiesFailed)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::DiagnosticRead);
        fixture.approve();
        fixture.entitlement.active = false;
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::CommercialDenied(
                "module_not_active"
            ))
        );

        let mut fixture = IntersectionFixture::new(SupportMode::TenantMutation);
        fixture.approve();
        let mut request = fixture.request();
        request.billing_state = BillingState::ReadOnly;
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::CommercialDenied(
                "payment_failure_read_only"
            ))
        );
    }

    #[test]
    fn support_intersection_fails_closed_for_impersonated_user_authority() {
        let mut fixture = IntersectionFixture::new(SupportMode::Impersonation);
        fixture.approve();
        assert_eq!(evaluate_support_intersection(&fixture.request()), Ok(()));

        fixture.represented_user_role = None;
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::RepresentedUserAuthorityMissing)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::Impersonation);
        fixture.approve();
        fixture.represented_user_role.as_mut().unwrap().valid_until = fixture.now;
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::RepresentedUserRoleInactive)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::Impersonation);
        fixture.approve();
        let mut request = fixture.request();
        request.represented_user_identity_active = false;
        assert_eq!(
            evaluate_support_intersection(&request),
            Err(SupportIntersectionDenial::RepresentedUserIdentityInactive)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::Impersonation);
        fixture.approve();
        fixture
            .represented_user_role
            .as_mut()
            .unwrap()
            .permissions
            .clear();
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::RepresentedUserPermissionDenied)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::Impersonation);
        fixture.approve();
        fixture.represented_user_role.as_mut().unwrap().scope = Scope::Holding {
            holding_id: Uuid::from_u128(55),
        };
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::RepresentedUserScopeDenied)
        );

        let mut fixture = IntersectionFixture::new(SupportMode::Impersonation);
        fixture.approve();
        fixture
            .represented_user_role
            .as_mut()
            .unwrap()
            .parent_assignment_id = Some(Uuid::from_u128(56));
        assert_eq!(
            evaluate_support_intersection(&fixture.request()),
            Err(SupportIntersectionDenial::RepresentedUserParentInactive)
        );
    }

    #[test]
    fn private_sha256_matches_known_vector() {
        assert_eq!(
            encode_hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ticket_validation_and_exact_command_delta_fail_closed() {
        let now = Utc::now();
        let mut ticket = support_ticket_fixture(TicketStatus::New);
        assert!(ticket.validate().is_ok());
        let command = SupportTicketCommand::CustomerReply {
            message_id: Uuid::new_v4(),
            author_ref: "agent".into(),
            body: "customer-safe reply".into(),
            created_at: now,
        };
        let before = ticket.clone();
        ticket
            .apply_command(command.clone(), &TenantScope::Platform, now)
            .expect("apply exact command");
        assert!(command.is_exactly_applied_to(&before, &ticket));

        let mut tampered = ticket.clone();
        tampered.customer_account_id = Uuid::new_v4();
        assert!(!command.is_exactly_applied_to(&before, &tampered));

        let mut malformed = support_ticket_fixture(TicketStatus::New);
        malformed.id = Uuid::nil();
        assert_eq!(
            malformed.validate(),
            Err(SupportError::InvalidSupportTicket)
        );
    }

    #[test]
    fn support_session_and_device_revocations_are_append_only() {
        let now = Utc::now();
        let mut session = SupportSession::request(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "agent",
            SupportMode::DiagnosticRead,
            vec!["support.read".into()],
            "platform",
            "diagnose",
            now,
            Duration::hours(1),
            None,
        )
        .expect("session");
        session.revoke(now + Duration::minutes(1));
        session.revoke(now + Duration::minutes(2));
        assert_eq!(session.revoked_at, Some(now + Duration::minutes(1)));

        let mut device = SupportDeviceSessionBinding::new(session.id, "agent", "device", None, now)
            .expect("device");
        device.revoke(now + Duration::minutes(1));
        device.revoke(now + Duration::minutes(2));
        assert_eq!(device.revoked_at(), Some(now + Duration::minutes(1)));
    }

    #[test]
    fn completed_postmortems_are_immutable() {
        let now = Utc::now();
        let mut incident = Incident::new(
            "commander",
            vec!["lead".into()],
            vec!["support-api".into()],
            vec![Uuid::new_v4()],
            now,
        )
        .expect("incident");
        incident
            .transition(IncidentStatus::Identified, now, "commander")
            .expect("identify");
        incident
            .transition(IncidentStatus::Monitoring, now, "commander")
            .expect("monitor");
        incident
            .transition(IncidentStatus::Resolved, now, "commander")
            .expect("resolve");
        incident
            .transition(IncidentStatus::PostmortemComplete, now, "commander")
            .expect("complete postmortem");
        assert_eq!(
            incident.record_root_cause("late root cause"),
            Err(SupportError::InvalidIncidentTransition)
        );
        assert_eq!(
            incident.add_broadcast(now, "commander", "internal", "late broadcast"),
            Err(SupportError::InvalidIncidentTransition)
        );
    }
}
