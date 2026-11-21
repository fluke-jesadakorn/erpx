//! Native Sales customer-notice reconciliation rules.
//!
//! One uncertain provider attempt is settled only by a different current Sales
//! approver who reads the actual provider truth through a bounded read-only
//! port. The immutable attempt is never rewritten; the decision is a separate
//! immutable `sales_notification_reconciliations` row with receipt, audit and
//! outbox evidence.
use crate::domain::sales_notification_execution::NotificationSource;
use erpx_server_shared::public::{BusinessError, FailureKind};

pub const OPERATION: &str = "commerce.customer-notification.reconcile";
pub const MAX_ATTEMPT_ID: usize = 256;
pub const MIN_REASON: usize = 5;
pub const MAX_REASON: usize = 1000;
pub const OBSERVED_WINDOW_MILLIS: i64 = 1_000;

pub fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CUSTOMER_NOTIFICATION_NOT_FOUND",
        "The original customer notice is unavailable in this work scope.",
    )
}

pub fn disabled() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_NOTIFICATION_RECONCILIATION_DISABLED",
        "Enable the reviewed purchase boundary before reconciling durable attempts.",
    )
}

pub fn independent_reviewer_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "INDEPENDENT_NOTIFICATION_REVIEWER_REQUIRED",
        "A different current Sales approver must reconcile the original provider attempt.",
    )
}

pub fn provider_id_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_NOTIFICATION_PROVIDER_ID_REQUIRED",
        "The actual uncertain attempt must already contain its original provider message identity.",
    )
}

pub fn provider_unconfirmed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_NOTIFICATION_PROVIDER_UNCONFIRMED",
        "The original provider did not confirm this uncertain message.",
    )
}

pub fn provider_mismatch() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_NOTIFICATION_PROVIDER_MISMATCH",
        "The actual provider message does not match the immutable original attempt.",
    )
}

pub fn not_uncertain() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CUSTOMER_NOTIFICATION_NOT_UNCERTAIN",
        "Only the exact original uncertain provider attempt can be reconciled.",
    )
}

pub fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "VERSION_CONFLICT",
        "The customer notice changed. Refresh before reconciliation.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SALES_NOTIFICATION_SOURCE_CHANGED",
        "The original reconciliation source changed before commit.",
    )
}

pub fn production_disabled() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "PRODUCTION_MODULE_DISABLED",
        "The original Sales owner release is unavailable.",
    )
}

pub fn authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "QUEUED_AUTHORITY_CHANGED",
        "The original current reviewer authority is no longer current.",
    )
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CUSTOMER_NOTIFICATION_RECONCILIATION_INVALID",
        "Review the reconciliation request.",
    )
}

pub fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "This command key belongs to another reconciliation.",
    )
}

pub fn readback_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SALES_NOTIFICATION_RECONCILIATION_READBACK_INVALID",
        "The original reconciliation evidence is unavailable.",
    )
}

/// Current selected reviewer. `person_user_ids` includes the user's own id and
/// every alias, matching the retained `samePerson` independence rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileActor {
    pub user_id: String,
    pub person_user_ids: Vec<String>,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub assignment_id: String,
    pub role: Option<String>,
    pub session_id: Option<String>,
    pub assignment_version: Option<i64>,
    pub permissions: Vec<String>,
}

impl ReconcileActor {
    pub fn require_approve(&self) -> Result<(), BusinessError> {
        if !self
            .permissions
            .iter()
            .any(|permission| permission == "commerce.approve")
        {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "FORBIDDEN",
                "A current Sales approver must reconcile the original provider attempt.",
            ));
        }
        Ok(())
    }

    pub fn same_person(&self, user_id: &str) -> bool {
        self.user_id == user_id || self.person_user_ids.iter().any(|value| value == user_id)
    }

    pub fn authority_json(&self) -> String {
        serde_json::json!({
            "userId": self.user_id,
            "assignmentId": self.assignment_id,
            "role": self.role,
            "companyId": self.company_id,
            "branchId": self.branch_id,
            "departmentId": self.department_id,
            "iam": {
                "sessionId": self.session_id,
                "queuedAssignmentVersion": self.assignment_version,
            },
        })
        .to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileInput {
    pub expected_version: i64,
    pub attempt_id: String,
    pub reason: String,
}

impl ReconcileInput {
    pub fn validate(self) -> Result<Self, BusinessError> {
        let attempt_id = self.attempt_id.trim().to_owned();
        let reason = self.reason.trim().to_owned();
        if self.expected_version < 1
            || attempt_id.is_empty()
            || attempt_id.chars().count() > MAX_ATTEMPT_ID
            || attempt_id.chars().any(char::is_control)
            || !(MIN_REASON..=MAX_REASON).contains(&reason.chars().count())
        {
            return Err(request_invalid());
        }
        Ok(Self {
            expected_version: self.expected_version,
            attempt_id,
            reason,
        })
    }

    pub fn identity(&self, actor: &ReconcileActor, notification_id: &str, key: &str) -> String {
        serde_json::json!({
            "operation": OPERATION,
            "recordId": notification_id,
            "actorId": actor.user_id,
            "companyId": actor.company_id,
            "idempotencyKey": key,
            "input": {
                "expectedVersion": self.expected_version,
                "attemptId": self.attempt_id,
                "reason": self.reason,
            },
        })
        .to_string()
    }
}

/// Immutable attempt joined to the reviewer who originally authorized it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileAttempt {
    pub id: String,
    pub notification_id: String,
    pub company_id: String,
    pub authorization_command_id: String,
    pub actor_id: String,
    pub source_hash: String,
    pub state: String,
    pub provider_message_id: Option<String>,
    pub attempt: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderEvent {
    Sent,
    Delivered,
    Opened,
    Clicked,
    Bounced,
    Canceled,
    Failed,
    Suppressed,
}

impl ProviderEvent {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "sent" => Some(Self::Sent),
            "delivered" => Some(Self::Delivered),
            "opened" => Some(Self::Opened),
            "clicked" => Some(Self::Clicked),
            "bounced" => Some(Self::Bounced),
            "canceled" => Some(Self::Canceled),
            "failed" => Some(Self::Failed),
            "suppressed" => Some(Self::Suppressed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Delivered => "delivered",
            Self::Opened => "opened",
            Self::Clicked => "clicked",
            Self::Bounced => "bounced",
            Self::Canceled => "canceled",
            Self::Failed => "failed",
            Self::Suppressed => "suppressed",
        }
    }

    pub fn outcome(self) -> &'static str {
        match self {
            Self::Sent | Self::Delivered | Self::Opened | Self::Clicked => "sent",
            _ => "cancelled",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderObservation {
    pub id: String,
    pub to: Vec<String>,
    pub subject: String,
    pub text: String,
    pub created_at: i64,
    pub last_event: ProviderEvent,
}

impl ProviderObservation {
    /// Exact original provider message identity, recipient, subject, text and
    /// attempt-window timestamp. Any ambiguity is refused.
    pub fn validate(
        self,
        source: &NotificationSource,
        attempt: &ReconcileAttempt,
    ) -> Result<Self, BusinessError> {
        let created = self.created_at;
        let recipient = self
            .to
            .first()
            .map(|value| value.to_lowercase())
            .filter(|_| self.to.len() == 1)
            .ok_or_else(provider_unconfirmed)?;
        if self.id != attempt.provider_message_id.clone().unwrap_or_default()
            || recipient != source.recipient_email.to_lowercase()
            || self.subject != source.subject
            || self.text != source.body_text
            || created < attempt.created_at - OBSERVED_WINDOW_MILLIS
            || created > attempt.updated_at + OBSERVED_WINDOW_MILLIS
        {
            return Err(provider_mismatch());
        }
        Ok(Self {
            id: self.id,
            to: vec![recipient],
            subject: self.subject,
            text: self.text,
            created_at: created,
            last_event: self.last_event,
        })
    }

    /// Compact observation persisted with the immutable reconciliation row.
    pub fn wire(&self) -> serde_json::Value {
        serde_json::json!({
            "object": "email",
            "id": self.id,
            "to": self.to,
            "subject": self.subject,
            "text": self.text,
            "createdAt": self.created_at,
            "last_event": self.last_event.as_str(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileReceipt {
    pub id: String,
    pub order_id: String,
    pub status: String,
    pub version: i64,
    pub reconciliation_id: String,
}

impl ReconcileReceipt {
    pub fn wire(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "orderId": self.order_id,
            "status": self.status,
            "version": self.version,
            "reconciliationId": self.reconciliation_id,
        })
    }
}

/// The requested outcome for one exact uncertain source/attempt pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileDecision {
    pub outcome: &'static str,
    pub version: i64,
    pub event_id: String,
    pub observation: ProviderObservation,
}
