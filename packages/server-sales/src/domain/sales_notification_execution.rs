//! Native Sales customer-notice durable delivery rules.
//!
//! A notice is claimed once through `commerce_customer_notifications` and one
//! immutable `sales_notification_execution_attempts` row. The source image,
//! consent/order facts, queued command authority and purchased Sales capability
//! are re-read before provider I/O and again before the terminal commit. A
//! process that dies after the provider accepted a message leaves an `unknown`
//! attempt for reconciliation instead of authorizing an automatic resend.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;

pub const MAX_ATTEMPTS: i64 = 8;
pub const LEASE_MILLIS: i64 = 90_000;
/// Terminal/held rows keep the queue from selecting them again.
pub const HELD_UNTIL: i64 = 253_402_300_799_000;
pub const SOURCE_CHANGED: &str = "SALES_NOTIFICATION_SOURCE_CHANGED";
pub const LEASE_CHANGED: &str = "SALES_NOTIFICATION_LEASE_CHANGED";
pub const ATTEMPT_UNCERTAIN: &str = "CUSTOMER_EMAIL_ATTEMPT_UNCERTAIN";
pub const AUTHORIZATION_REQUIRED: &str = "CUSTOMER_NOTIFICATION_AUTHORIZATION_REQUIRED";
pub const NOT_CONFIGURED: &str = "CUSTOMER_EMAIL_NOT_CONFIGURED";
pub const AUTHORITY_CHANGED: &str = "QUEUED_AUTHORITY_CHANGED";
pub const TRANSACTION_REQUIRED: &str = "SALES_NOTIFICATION_TRANSACTION_REQUIRED";

/// Immutable notice snapshot plus its current independently owned sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationSource {
    pub id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub order_id: String,
    pub order_state: Option<String>,
    pub fulfillment_id: String,
    pub party_id: String,
    pub recipient_email: String,
    pub subject: String,
    pub body_text: String,
    pub adapter: String,
    pub current_adapter: Option<String>,
    pub status: String,
    pub version: i64,
    pub attempts: i64,
    pub next_attempt_at: i64,
    pub lease_until: Option<i64>,
    pub provider_message_id: Option<String>,
    pub created_by: String,
    pub source_company: Option<String>,
    pub customer_company: Option<String>,
    pub customer_status: Option<String>,
    pub notification_email: Option<String>,
    pub notification_consent: Option<String>,
    pub release_ready: bool,
    pub source_hash: String,
}

/// Immutable queued-command authority captured by the original queue/retry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionAuthorization {
    pub command_id: String,
    pub notification_id: String,
    pub company_id: String,
    pub kind: String,
    pub actor_id: String,
    pub assignment_id: String,
    pub authority_json: String,
    pub source_hash: String,
    pub notification_version: i64,
}

/// One durable send attempt; the immutable prefix is never rewritten.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionAttempt {
    pub id: String,
    pub notification_id: String,
    pub company_id: String,
    pub attempt: i64,
    pub lease_until: i64,
    pub source_hash: String,
    pub state: String,
}

/// The exact queued identity re-read from the original authority snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueuedActor {
    pub user_id: String,
    pub assignment_id: String,
    pub role: Option<String>,
    pub company_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub session_id: Option<String>,
    pub queued_assignment_version: Option<i64>,
}

impl QueuedActor {
    /// The queued identity must still be the source-bound actor. A populated
    /// branch/department on the original context narrows the source scope.
    pub fn binds(
        &self,
        source: &NotificationSource,
        authorization: &ExecutionAuthorization,
    ) -> bool {
        self.user_id == authorization.actor_id
            && self.assignment_id == authorization.assignment_id
            && self.company_id == source.company_id
            && self
                .branch_id
                .as_ref()
                .is_none_or(|branch| Some(branch) == source.branch_id.as_ref())
            && self
                .department_id
                .as_ref()
                .is_none_or(|department| Some(department) == source.department_id.as_ref())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorityDocument {
    user_id: Option<String>,
    assignment_id: Option<String>,
    role: Option<String>,
    company_id: Option<String>,
    branch_id: Option<String>,
    department_id: Option<String>,
    iam: Option<AuthorityIam>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthorityIam {
    session_id: Option<String>,
    queued_assignment_version: Option<i64>,
}

pub fn parse_queued_actor(authority_json: &str) -> Result<QueuedActor, BusinessError> {
    let document: AuthorityDocument = serde_json::from_str(authority_json).map_err(|_| {
        BusinessError::new(
            FailureKind::Conflict,
            SOURCE_CHANGED,
            "The original queued identity binding changed.",
        )
    })?;
    let required = |value: Option<String>| {
        value
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Conflict,
                    SOURCE_CHANGED,
                    "The original queued identity binding changed.",
                )
            })
    };
    Ok(QueuedActor {
        user_id: required(document.user_id)?,
        assignment_id: required(document.assignment_id)?,
        role: document.role,
        company_id: required(document.company_id)?,
        branch_id: document.branch_id,
        department_id: document.department_id,
        session_id: document.iam.as_ref().and_then(|iam| iam.session_id.clone()),
        queued_assignment_version: document
            .iam
            .as_ref()
            .and_then(|iam| iam.queued_assignment_version),
    })
}

/// Current source, consent, order and provider facts. Every branch matches the
/// retained TypeScript predicate; a missing current source is a mismatch.
pub fn invalid_reason(source: &NotificationSource) -> Option<&'static str> {
    if source.source_company.as_deref() != Some(source.company_id.as_str())
        || source.customer_company.as_deref() != Some(source.company_id.as_str())
        || !matches!(
            source.order_state.as_deref(),
            Some("confirmed" | "partially_fulfilled" | "fulfilled")
        )
    {
        return Some(SOURCE_CHANGED);
    }
    if source.customer_status.as_deref() != Some("active")
        || source.notification_consent.as_deref() != Some("granted")
        || source
            .notification_email
            .as_deref()
            .map(str::to_lowercase)
            .as_deref()
            != Some(source.recipient_email.as_str())
    {
        return Some("CUSTOMER_CONSENT_OR_EMAIL_CHANGED");
    }
    if source.current_adapter.as_deref() != Some(source.adapter.as_str()) {
        return Some("CUSTOMER_NOTIFICATION_PROVIDER_CHANGED");
    }
    None
}

/// `sending` at or after its lease is a crashed attempt, never a fresh send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimDecision {
    Ready,
    LeaseHeld,
    Takeover,
    Exhausted,
    NotQueued,
}

pub fn claim_decision(source: &NotificationSource, now: i64) -> ClaimDecision {
    match source.status.as_str() {
        "sending" => match source.lease_until {
            Some(lease) if lease >= now => ClaimDecision::LeaseHeld,
            _ => ClaimDecision::Takeover,
        },
        "pending" | "failed" => {
            if source.attempts >= MAX_ATTEMPTS || source.next_attempt_at > now {
                ClaimDecision::Exhausted
            } else {
                ClaimDecision::Ready
            }
        }
        _ => ClaimDecision::NotQueued,
    }
}

/// The durable attempt and its notice row must still be the exact same unit.
pub fn attempt_current(source: &NotificationSource, attempt: &ExecutionAttempt, now: i64) -> bool {
    source.status == "sending"
        && source.attempts == attempt.attempt
        && source.lease_until == Some(attempt.lease_until)
        && attempt.state == "sending"
        && attempt.lease_until >= now
        && attempt.source_hash == source.source_hash
        && source.release_ready
        && invalid_reason(source).is_none()
}

pub fn authorization_current(
    source: &NotificationSource,
    authorization: &ExecutionAuthorization,
    actor: &QueuedActor,
) -> bool {
    authorization.notification_id == source.id
        && authorization.company_id == source.company_id
        && authorization.source_hash == source.source_hash
        && authorization.notification_version <= source.version
        && actor.binds(source, authorization)
}

pub fn lease_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        LEASE_CHANGED,
        "The original customer-notice attempt changed.",
    )
}

pub fn source_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        SOURCE_CHANGED,
        "The exact original customer notice changed.",
    )
}

pub fn attempt_uncertain() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        ATTEMPT_UNCERTAIN,
        "Reconcile the original provider attempt before another delivery.",
    )
}

pub fn authority_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        AUTHORITY_CHANGED,
        "The original queued assignment or principal is no longer current.",
    )
}

pub fn not_configured() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        NOT_CONFIGURED,
        "Configure the original customer-notice provider before retrying.",
    )
}

/// Persisted `last_error` for a delivery that failed before provider I/O.
pub fn terminal_code(invoked: bool) -> (&'static str, &'static str) {
    if invoked {
        ("unknown", ATTEMPT_UNCERTAIN)
    } else {
        ("failed", AUTHORIZATION_REQUIRED)
    }
}
