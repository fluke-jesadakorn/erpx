use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use erpx_integration_contracts::{
    StableRecordRef,
    catalog::ContextId,
    customer, identity_access,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{AuthorityScope, HumanDecisionProof, RecordId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod platform_case;
pub use platform_case::{PlatformCaseStatus, PlatformSupportCase};

pub const MAX_CODE_BYTES: usize = 128;
pub const MAX_REFERENCE_BYTES: usize = 256;
pub const MAX_MESSAGE_BYTES: usize = 16_000;
pub const MAX_DESCRIPTION_BYTES: usize = 16_000;
pub const MAX_REASON_BYTES: usize = 2_000;
pub const MAX_PERMISSIONS: usize = 256;
pub const MAX_INCIDENT_ITEMS: usize = 1_000;
pub const HIGH_RISK_MFA_MAX_AGE: Duration = Duration::minutes(5);

macro_rules! support_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, SupportDomainError> {
                if value.is_nil() {
                    return Err(SupportDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = SupportDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

support_id!(SupportTicketId, "support_ticket_id");
support_id!(IncidentId, "incident_id");
support_id!(SessionId, "support_session_id");
support_id!(CapabilityId, "support_capability_id");
support_id!(MessageId, "message_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SupportDomainError {
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("{field} is required or exceeds its safe length")]
    InvalidText { field: &'static str },
    #[error("Support records require an exact company or branch scope")]
    InvalidScope,
    #[error("the customer fact is not eligible for Support")]
    CustomerUnavailable,
    #[error("support ticket lifecycle transition is not allowed")]
    InvalidTicketTransition,
    #[error("closed support tickets are immutable")]
    ClosedTicket,
    #[error("support ticket messages must have unique identifiers and valid timestamps")]
    InvalidMessage,
    #[error("incident lifecycle transition is not allowed")]
    InvalidIncidentTransition,
    #[error("completed incident postmortems are immutable")]
    IncidentTerminal,
    #[error("support session lifetime or lifecycle is invalid")]
    InvalidSession,
    #[error("impersonation requires exactly one represented holding principal")]
    InvalidRepresentedPrincipal,
    #[error("support approval violates segregation of duties")]
    ApprovalSeparation,
    #[error("support session approval requires exact authenticated-human decision evidence")]
    HumanApprovalRequired,
    #[error("support capability binding is invalid")]
    InvalidCapabilityBinding,
    #[error("high-risk support access requires recent MFA evidence")]
    MfaRequired,
    #[error("support segregation-of-duties evidence was denied")]
    SegregationOfDutiesDenied,
    #[error("support authority scope does not contain the approved scope")]
    ScopeDenied,
    #[error("support capability is expired or revoked")]
    CapabilityInactive,
    #[error("support capability permission is not granted")]
    PermissionDenied,
    #[error("support aggregate timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("support aggregate version overflowed")]
    VersionOverflow,
    #[error("platform support case is invalid")]
    InvalidPlatformCase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageAudience {
    CustomerVisible,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketMessage {
    id: MessageId,
    author_ref: String,
    audience: MessageAudience,
    body: String,
    created_at: DateTime<Utc>,
}

impl TicketMessage {
    pub fn new(
        id: MessageId,
        author_ref: String,
        audience: MessageAudience,
        body: String,
        created_at: DateTime<Utc>,
    ) -> Result<Self, SupportDomainError> {
        validate_text(&author_ref, "message_author", MAX_REFERENCE_BYTES)?;
        validate_text(&body, "message_body", MAX_MESSAGE_BYTES)?;
        Ok(Self {
            id,
            author_ref,
            audience,
            body,
            created_at,
        })
    }

    pub const fn id(&self) -> MessageId {
        self.id
    }
    pub fn author_ref(&self) -> &str {
        &self.author_ref
    }
    pub const fn audience(&self) -> MessageAudience {
        self.audience
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    TicketChanged {
        ticket_id: SupportTicketId,
        case_code: String,
        customer: customer::CustomerRef,
        status: TicketStatus,
        category: String,
        at: DateTime<Utc>,
    },
    IncidentChanged {
        incident_id: IncidentId,
        incident_code: String,
        status: IncidentStatus,
        severity: String,
        affected_services: Vec<String>,
        affected_customer_accounts: Vec<customer::CustomerAccountRef>,
        at: DateTime<Utc>,
    },
    Session,
    CapabilityChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportTicket {
    id: SupportTicketId,
    version: u64,
    scope: AuthorityScope,
    customer: customer::CustomerRef,
    customer_account: customer::CustomerAccountRef,
    case_code: String,
    category: String,
    severity: String,
    description: String,
    status: TicketStatus,
    messages: Vec<TicketMessage>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl SupportTicket {
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        id: SupportTicketId,
        scope: AuthorityScope,
        customer_fact: customer::CustomerProfileFact,
        case_code: String,
        category: String,
        severity: String,
        description: String,
        at: DateTime<Utc>,
    ) -> Result<Self, SupportDomainError> {
        validate_customer_scope(&scope, &customer_fact)?;
        if matches!(
            customer_fact.status,
            customer::CustomerStatus::Blocked | customer::CustomerStatus::Closed
        ) {
            return Err(SupportDomainError::CustomerUnavailable);
        }
        validate_text(&case_code, "case_code", MAX_CODE_BYTES)?;
        validate_text(&category, "category", MAX_CODE_BYTES)?;
        validate_text(&severity, "severity", MAX_CODE_BYTES)?;
        validate_text(&description, "description", MAX_DESCRIPTION_BYTES)?;
        let mut value = Self {
            id,
            version: 1,
            scope,
            customer: customer_fact.customer,
            customer_account: customer_fact.account,
            case_code,
            category,
            severity,
            description,
            status: TicketStatus::New,
            messages: Vec::new(),
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.record_change(at);
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: SupportTicketId,
        version: u64,
        scope: AuthorityScope,
        customer: customer::CustomerRef,
        customer_account: customer::CustomerAccountRef,
        case_code: String,
        category: String,
        severity: String,
        description: String,
        status: TicketStatus,
        messages: Vec<TicketMessage>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, SupportDomainError> {
        validate_scope(&scope)?;
        validate_text(&case_code, "case_code", MAX_CODE_BYTES)?;
        validate_text(&category, "category", MAX_CODE_BYTES)?;
        validate_text(&severity, "severity", MAX_CODE_BYTES)?;
        validate_text(&description, "description", MAX_DESCRIPTION_BYTES)?;
        if version == 0
            || updated_at < created_at
            || version
                < u64::try_from(messages.len())
                    .unwrap_or(u64::MAX)
                    .saturating_add(1)
        {
            return Err(SupportDomainError::InvalidMessage);
        }
        let mut message_ids = BTreeSet::new();
        for message in &messages {
            validate_text(message.author_ref(), "message_author", MAX_REFERENCE_BYTES)?;
            validate_text(message.body(), "message_body", MAX_MESSAGE_BYTES)?;
            if message.created_at() > updated_at || !message_ids.insert(message.id()) {
                return Err(SupportDomainError::InvalidMessage);
            }
        }
        Ok(Self {
            id,
            version,
            scope,
            customer,
            customer_account,
            case_code,
            category,
            severity,
            description,
            status,
            messages,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        })
    }

    pub fn transition(
        &mut self,
        next: TicketStatus,
        at: DateTime<Utc>,
    ) -> Result<(), SupportDomainError> {
        self.ensure_mutable()?;
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
            return Err(SupportDomainError::InvalidTicketTransition);
        }
        self.advance(at)?;
        self.status = next;
        self.record_change(at);
        Ok(())
    }

    pub fn add_message(
        &mut self,
        message: TicketMessage,
        at: DateTime<Utc>,
    ) -> Result<(), SupportDomainError> {
        self.ensure_mutable()?;
        if message.created_at() > at
            || self
                .messages
                .iter()
                .any(|current| current.id() == message.id())
        {
            return Err(SupportDomainError::InvalidMessage);
        }
        self.advance(at)?;
        self.messages.push(message);
        self.record_change(at);
        Ok(())
    }

    fn ensure_mutable(&self) -> Result<(), SupportDomainError> {
        if self.status == TicketStatus::Closed {
            Err(SupportDomainError::ClosedTicket)
        } else {
            Ok(())
        }
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), SupportDomainError> {
        if at < self.updated_at {
            return Err(SupportDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SupportDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    fn record_change(&mut self, at: DateTime<Utc>) {
        self.uncommitted_events.push(DomainEvent::TicketChanged {
            ticket_id: self.id,
            case_code: self.case_code.clone(),
            customer: self.customer,
            status: self.status,
            category: self.category.clone(),
            at,
        });
    }

    pub const fn id(&self) -> SupportTicketId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn customer(&self) -> customer::CustomerRef {
        self.customer
    }
    pub const fn customer_account(&self) -> customer::CustomerAccountRef {
        self.customer_account
    }
    pub fn case_code(&self) -> &str {
        &self.case_code
    }
    pub fn category(&self) -> &str {
        &self.category
    }
    pub fn severity(&self) -> &str {
        &self.severity
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub const fn status(&self) -> TicketStatus {
        self.status
    }
    pub fn messages(&self) -> &[TicketMessage] {
        &self.messages
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        support_record_ref(self.id.as_uuid(), "support.customer-case")
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Investigating,
    Identified,
    Monitoring,
    Resolved,
    PostmortemComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastAudience {
    CustomerVisible,
    Internal,
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
    pub audience: BroadcastAudience,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    id: IncidentId,
    version: u64,
    scope: AuthorityScope,
    incident_code: String,
    severity: String,
    commander_ref: String,
    affected_services: Vec<String>,
    affected_customer_accounts: Vec<customer::CustomerAccountRef>,
    status: IncidentStatus,
    timeline: Vec<IncidentTimelineEntry>,
    broadcasts: Vec<IncidentBroadcast>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl Incident {
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        id: IncidentId,
        scope: AuthorityScope,
        incident_code: String,
        severity: String,
        commander_ref: String,
        affected_services: Vec<String>,
        affected_customer_accounts: Vec<customer::CustomerAccountRef>,
        at: DateTime<Utc>,
    ) -> Result<Self, SupportDomainError> {
        validate_scope(&scope)?;
        validate_text(&incident_code, "incident_code", MAX_CODE_BYTES)?;
        validate_text(&severity, "severity", MAX_CODE_BYTES)?;
        validate_text(&commander_ref, "commander_ref", MAX_REFERENCE_BYTES)?;
        validate_unique_texts(&affected_services, "affected_services")?;
        if affected_services.is_empty()
            || affected_services.len() > MAX_INCIDENT_ITEMS
            || affected_customer_accounts.len() > MAX_INCIDENT_ITEMS
        {
            return Err(SupportDomainError::InvalidText {
                field: "incident_impact",
            });
        }
        let mut value = Self {
            id,
            version: 1,
            scope,
            incident_code,
            severity,
            commander_ref: commander_ref.clone(),
            affected_services,
            affected_customer_accounts,
            status: IncidentStatus::Investigating,
            timeline: vec![IncidentTimelineEntry {
                at,
                actor_ref: commander_ref,
                message: "incident opened".into(),
            }],
            broadcasts: Vec::new(),
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.record_change(at);
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: IncidentId,
        version: u64,
        scope: AuthorityScope,
        incident_code: String,
        severity: String,
        commander_ref: String,
        affected_services: Vec<String>,
        affected_customer_accounts: Vec<customer::CustomerAccountRef>,
        status: IncidentStatus,
        timeline: Vec<IncidentTimelineEntry>,
        broadcasts: Vec<IncidentBroadcast>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, SupportDomainError> {
        validate_scope(&scope)?;
        validate_text(&incident_code, "incident_code", MAX_CODE_BYTES)?;
        validate_text(&severity, "severity", MAX_CODE_BYTES)?;
        validate_text(&commander_ref, "commander_ref", MAX_REFERENCE_BYTES)?;
        validate_unique_texts(&affected_services, "affected_services")?;
        if affected_services.is_empty()
            || affected_services.len() > MAX_INCIDENT_ITEMS
            || affected_customer_accounts.len() > MAX_INCIDENT_ITEMS
            || timeline.is_empty()
            || updated_at < created_at
        {
            return Err(SupportDomainError::InvalidText {
                field: "incident_impact",
            });
        }
        for entry in &timeline {
            validate_text(&entry.actor_ref, "incident_actor", MAX_REFERENCE_BYTES)?;
            validate_text(&entry.message, "incident_timeline", MAX_REASON_BYTES)?;
            if entry.at < created_at || entry.at > updated_at {
                return Err(SupportDomainError::InvalidTimestamp);
            }
        }
        for broadcast in &broadcasts {
            validate_text(&broadcast.actor_ref, "incident_actor", MAX_REFERENCE_BYTES)?;
            validate_text(&broadcast.message, "incident_broadcast", MAX_REASON_BYTES)?;
            if broadcast.at < created_at || broadcast.at > updated_at {
                return Err(SupportDomainError::InvalidTimestamp);
            }
        }
        let item_count = timeline
            .len()
            .checked_add(broadcasts.len())
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(SupportDomainError::VersionOverflow)?;
        if version != item_count
            || !timeline.iter().any(|entry| entry.at == updated_at)
                && !broadcasts
                    .iter()
                    .any(|broadcast| broadcast.at == updated_at)
        {
            return Err(SupportDomainError::InvalidTimestamp);
        }
        Ok(Self {
            id,
            version,
            scope,
            incident_code,
            severity,
            commander_ref,
            affected_services,
            affected_customer_accounts,
            status,
            timeline,
            broadcasts,
            created_at,
            updated_at,
            uncommitted_events: Vec::new(),
        })
    }

    pub fn transition(
        &mut self,
        next: IncidentStatus,
        actor_ref: String,
        at: DateTime<Utc>,
    ) -> Result<(), SupportDomainError> {
        self.ensure_mutable()?;
        let valid = matches!(
            (self.status, next),
            (IncidentStatus::Investigating, IncidentStatus::Identified)
                | (IncidentStatus::Identified, IncidentStatus::Monitoring)
                | (IncidentStatus::Monitoring, IncidentStatus::Resolved)
                | (IncidentStatus::Resolved, IncidentStatus::PostmortemComplete)
        );
        if !valid {
            return Err(SupportDomainError::InvalidIncidentTransition);
        }
        validate_text(&actor_ref, "incident_actor", MAX_REFERENCE_BYTES)?;
        self.advance(at)?;
        self.timeline.push(IncidentTimelineEntry {
            at,
            actor_ref,
            message: format!("status changed to {next:?}"),
        });
        self.status = next;
        self.record_change(at);
        Ok(())
    }

    pub fn add_timeline(
        &mut self,
        actor_ref: String,
        message: String,
        at: DateTime<Utc>,
    ) -> Result<(), SupportDomainError> {
        self.ensure_mutable()?;
        validate_text(&actor_ref, "incident_actor", MAX_REFERENCE_BYTES)?;
        validate_text(&message, "incident_timeline", MAX_REASON_BYTES)?;
        self.advance(at)?;
        self.timeline.push(IncidentTimelineEntry {
            at,
            actor_ref,
            message,
        });
        self.record_change(at);
        Ok(())
    }

    pub fn broadcast(
        &mut self,
        actor_ref: String,
        audience: BroadcastAudience,
        message: String,
        at: DateTime<Utc>,
    ) -> Result<(), SupportDomainError> {
        self.ensure_mutable()?;
        validate_text(&actor_ref, "incident_actor", MAX_REFERENCE_BYTES)?;
        validate_text(&message, "incident_broadcast", MAX_REASON_BYTES)?;
        self.advance(at)?;
        self.broadcasts.push(IncidentBroadcast {
            at,
            actor_ref,
            audience,
            message,
        });
        self.record_change(at);
        Ok(())
    }

    fn ensure_mutable(&self) -> Result<(), SupportDomainError> {
        if self.status == IncidentStatus::PostmortemComplete {
            Err(SupportDomainError::IncidentTerminal)
        } else {
            Ok(())
        }
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), SupportDomainError> {
        if at < self.updated_at {
            return Err(SupportDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SupportDomainError::VersionOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    fn record_change(&mut self, at: DateTime<Utc>) {
        self.uncommitted_events.push(DomainEvent::IncidentChanged {
            incident_id: self.id,
            incident_code: self.incident_code.clone(),
            status: self.status,
            severity: self.severity.clone(),
            affected_services: self.affected_services.clone(),
            affected_customer_accounts: self.affected_customer_accounts.clone(),
            at,
        });
    }

    pub const fn id(&self) -> IncidentId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn incident_code(&self) -> &str {
        &self.incident_code
    }
    pub fn severity(&self) -> &str {
        &self.severity
    }
    pub fn commander_ref(&self) -> &str {
        &self.commander_ref
    }
    pub fn affected_services(&self) -> &[String] {
        &self.affected_services
    }
    pub fn affected_customer_accounts(&self) -> &[customer::CustomerAccountRef] {
        &self.affected_customer_accounts
    }
    pub const fn status(&self) -> IncidentStatus {
        self.status
    }
    pub fn timeline(&self) -> &[IncidentTimelineEntry] {
        &self.timeline
    }
    pub fn broadcasts(&self) -> &[IncidentBroadcast] {
        &self.broadcasts
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        support_record_ref(self.id.as_uuid(), "support.incident")
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportMode {
    DiagnosticRead,
    HoldingMutation,
    Impersonation,
}

impl SupportMode {
    pub const fn maximum_duration(self) -> Duration {
        match self {
            Self::DiagnosticRead => Duration::hours(8),
            Self::HoldingMutation | Self::Impersonation => Duration::hours(2),
        }
    }
    pub const fn requires_high_risk_mfa(self) -> bool {
        !matches!(self, Self::DiagnosticRead)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Requested,
    Approved,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportSession {
    id: SessionId,
    version: u64,
    scope: AuthorityScope,
    ticket_id: SupportTicketId,
    customer: customer::CustomerRef,
    customer_account: customer::CustomerAccountRef,
    agent: identity_access::PrincipalRef,
    represented_principal: Option<identity_access::PrincipalRef>,
    mode: SupportMode,
    permissions: Vec<String>,
    module: String,
    reason: String,
    status: SessionStatus,
    requested_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    approved_by: Option<String>,
    approved_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    revoked_reason: Option<String>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl SupportSession {
    #[allow(clippy::too_many_arguments)]
    pub fn request(
        id: SessionId,
        ticket: &SupportTicket,
        agent: identity_access::PrincipalRef,
        represented_principal: Option<identity_access::PrincipalRef>,
        mode: SupportMode,
        permissions: Vec<String>,
        scope: AuthorityScope,
        module: String,
        reason: String,
        duration: Duration,
        at: DateTime<Utc>,
    ) -> Result<Self, SupportDomainError> {
        if ticket.status() != TicketStatus::AccessRequested || ticket.scope() != &scope {
            return Err(SupportDomainError::InvalidSession);
        }
        if mode == SupportMode::Impersonation && represented_principal.is_none()
            || mode != SupportMode::Impersonation && represented_principal.is_some()
        {
            return Err(SupportDomainError::InvalidRepresentedPrincipal);
        }
        if duration <= Duration::zero() || duration > mode.maximum_duration() {
            return Err(SupportDomainError::InvalidSession);
        }
        validate_permissions(&permissions)?;
        validate_text(&module, "module", MAX_CODE_BYTES)?;
        validate_text(&reason, "session_reason", MAX_REASON_BYTES)?;
        let mut permissions = permissions;
        permissions.sort();
        let value = Self {
            id,
            version: 1,
            scope,
            ticket_id: ticket.id(),
            customer: ticket.customer(),
            customer_account: ticket.customer_account(),
            agent,
            represented_principal,
            mode,
            permissions,
            module,
            reason,
            status: SessionStatus::Requested,
            requested_at: at,
            expires_at: at + duration,
            approved_by: None,
            approved_at: None,
            revoked_at: None,
            revoked_reason: None,
            uncommitted_events: vec![DomainEvent::Session],
        };
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: SessionId,
        version: u64,
        scope: AuthorityScope,
        ticket_id: SupportTicketId,
        customer: customer::CustomerRef,
        customer_account: customer::CustomerAccountRef,
        agent: identity_access::PrincipalRef,
        represented_principal: Option<identity_access::PrincipalRef>,
        mode: SupportMode,
        permissions: Vec<String>,
        module: String,
        reason: String,
        status: SessionStatus,
        requested_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        approved_by: Option<String>,
        approved_at: Option<DateTime<Utc>>,
        revoked_at: Option<DateTime<Utc>>,
        revoked_reason: Option<String>,
    ) -> Result<Self, SupportDomainError> {
        validate_scope(&scope)?;
        validate_permissions(&permissions)?;
        if !permissions.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err(SupportDomainError::InvalidSession);
        }
        validate_text(&module, "module", MAX_CODE_BYTES)?;
        validate_text(&reason, "session_reason", MAX_REASON_BYTES)?;
        if expires_at <= requested_at
            || expires_at.signed_duration_since(requested_at) > mode.maximum_duration()
            || (mode == SupportMode::Impersonation) != represented_principal.is_some()
        {
            return Err(SupportDomainError::InvalidSession);
        }
        if let Some(approver) = approved_by.as_deref() {
            validate_text(approver, "approver_ref", MAX_REFERENCE_BYTES)?;
            if approver == agent.as_uuid().to_string()
                || represented_principal
                    .is_some_and(|value| approver == value.as_uuid().to_string())
            {
                return Err(SupportDomainError::ApprovalSeparation);
            }
        }
        if let Some(reason) = revoked_reason.as_deref() {
            validate_text(reason, "revocation_reason", MAX_REASON_BYTES)?;
        }
        let lifecycle_valid = match status {
            SessionStatus::Requested => {
                version == 1
                    && approved_by.is_none()
                    && approved_at.is_none()
                    && revoked_at.is_none()
                    && revoked_reason.is_none()
            }
            SessionStatus::Approved => {
                version == 2
                    && approved_by.is_some()
                    && approved_at.is_some_and(|at| at >= requested_at && at < expires_at)
                    && revoked_at.is_none()
                    && revoked_reason.is_none()
            }
            SessionStatus::Revoked => {
                let expected_version = if approved_at.is_some() { 3 } else { 2 };
                version == expected_version
                    && approved_by.is_some() == approved_at.is_some()
                    && approved_at.is_none_or(|at| at >= requested_at && at < expires_at)
                    && revoked_at.is_some_and(|at| at >= requested_at)
                    && revoked_reason.is_some()
            }
        };
        if !lifecycle_valid {
            return Err(SupportDomainError::InvalidSession);
        }
        Ok(Self {
            id,
            version,
            scope,
            ticket_id,
            customer,
            customer_account,
            agent,
            represented_principal,
            mode,
            permissions,
            module,
            reason,
            status,
            requested_at,
            expires_at,
            approved_by,
            approved_at,
            revoked_at,
            revoked_reason,
            uncommitted_events: Vec::new(),
        })
    }

    pub fn approve(&mut self, decision: &HumanDecisionProof) -> Result<(), SupportDomainError> {
        let at = decision.decided_at();
        let expected_ref = support_session_approval_decision_ref(self.id);
        if decision.authority_scope() != &self.scope
            || decision.decision_ref() != expected_ref
            || !decision.actor().is_human()
            || decision.actor().session_id.is_none()
        {
            return Err(SupportDomainError::HumanApprovalRequired);
        }
        if self.status != SessionStatus::Requested
            || at < self.requested_at
            || at >= self.expires_at
        {
            return Err(SupportDomainError::InvalidSession);
        }
        let approver_ref = decision.principal_ref();
        validate_text(approver_ref, "approver_ref", MAX_REFERENCE_BYTES)?;
        if approver_ref == self.agent.as_uuid().to_string()
            || self
                .represented_principal
                .is_some_and(|value| approver_ref == value.as_uuid().to_string())
        {
            return Err(SupportDomainError::ApprovalSeparation);
        }
        self.advance()?;
        self.status = SessionStatus::Approved;
        self.approved_by = Some(approver_ref.to_owned());
        self.approved_at = Some(at);
        self.uncommitted_events.push(DomainEvent::Session);
        Ok(())
    }

    pub fn revoke(&mut self, reason: String, at: DateTime<Utc>) -> Result<(), SupportDomainError> {
        if self.status == SessionStatus::Revoked || at < self.requested_at {
            return Err(SupportDomainError::InvalidSession);
        }
        validate_text(&reason, "revocation_reason", MAX_REASON_BYTES)?;
        self.advance()?;
        self.status = SessionStatus::Revoked;
        self.revoked_at = Some(at);
        self.revoked_reason = Some(reason);
        self.uncommitted_events.push(DomainEvent::Session);
        Ok(())
    }

    pub fn is_active(&self, at: DateTime<Utc>) -> bool {
        self.status == SessionStatus::Approved && self.requested_at <= at && at < self.expires_at
    }

    fn advance(&mut self) -> Result<(), SupportDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SupportDomainError::VersionOverflow)?;
        Ok(())
    }

    pub const fn id(&self) -> SessionId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn ticket_id(&self) -> SupportTicketId {
        self.ticket_id
    }
    pub const fn customer(&self) -> customer::CustomerRef {
        self.customer
    }
    pub const fn customer_account(&self) -> customer::CustomerAccountRef {
        self.customer_account
    }
    pub const fn agent(&self) -> identity_access::PrincipalRef {
        self.agent
    }
    pub const fn represented_principal(&self) -> Option<identity_access::PrincipalRef> {
        self.represented_principal
    }
    pub const fn mode(&self) -> SupportMode {
        self.mode
    }
    pub fn permissions(&self) -> &[String] {
        &self.permissions
    }
    pub fn module(&self) -> &str {
        &self.module
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub const fn status(&self) -> SessionStatus {
        self.status
    }
    pub const fn requested_at(&self) -> DateTime<Utc> {
        self.requested_at
    }
    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
    pub fn approved_by(&self) -> Option<&str> {
        self.approved_by.as_deref()
    }
    pub const fn approved_at(&self) -> Option<DateTime<Utc>> {
        self.approved_at
    }
    pub const fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }
    pub fn revoked_reason(&self) -> Option<&str> {
        self.revoked_reason.as_deref()
    }
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        support_record_ref(self.id.as_uuid(), "support.access-session")
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

pub fn support_session_approval_decision_ref(session_id: SessionId) -> String {
    format!("support.session:{}:approve", session_id.as_uuid())
}

fn support_record_ref(
    id: Uuid,
    record_type: &'static str,
) -> Result<StableRecordRef, ReferenceValidationError> {
    let record_id = RecordId::try_from_uuid(id).expect("validated Support identifier is non-nil");
    StableRecordRef::try_new(
        record_id,
        ContextId::Support,
        RecordTypeId::try_new(record_type)?,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityRevocationReason {
    CustomerRequested,
    AgentDisabled,
    DeviceRevoked,
    SessionRevoked,
    TicketClosed,
    Administrative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityEvidence {
    pub device_ref: String,
    pub mfa_verified_at: Option<DateTime<Utc>>,
    pub segregation_of_duties_passed: bool,
    pub authority_scope: AuthorityScope,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportCapability {
    id: CapabilityId,
    version: u64,
    session_id: SessionId,
    ticket_id: SupportTicketId,
    customer: customer::CustomerRef,
    customer_account: customer::CustomerAccountRef,
    agent: identity_access::PrincipalRef,
    device_ref: String,
    mode: SupportMode,
    permissions: Vec<String>,
    scope: AuthorityScope,
    module: String,
    secret_hash_sha256: [u8; 32],
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    revocation: Option<(CapabilityRevocationReason, DateTime<Utc>)>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl std::fmt::Debug for SupportCapability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SupportCapability")
            .field("id", &self.id)
            .field("version", &self.version)
            .field("session_id", &self.session_id)
            .field("ticket_id", &self.ticket_id)
            .field("agent", &self.agent)
            .field("device_ref", &self.device_ref)
            .field("mode", &self.mode)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("secret_hash_sha256", &"REDACTED")
            .field("revocation", &self.revocation)
            .finish_non_exhaustive()
    }
}

impl SupportCapability {
    pub fn issue(
        id: CapabilityId,
        session: &SupportSession,
        evidence: CapabilityEvidence,
        secret_hash_sha256: [u8; 32],
        at: DateTime<Utc>,
    ) -> Result<Self, SupportDomainError> {
        if !session.is_active(at) || !evidence.authority_scope.contains(session.scope()) {
            return Err(SupportDomainError::ScopeDenied);
        }
        validate_text(&evidence.device_ref, "device_ref", MAX_REFERENCE_BYTES)?;
        if !evidence.segregation_of_duties_passed {
            return Err(SupportDomainError::SegregationOfDutiesDenied);
        }
        validate_mfa(session.mode(), evidence.mfa_verified_at, at)?;
        if secret_hash_sha256.iter().all(|byte| *byte == 0) {
            return Err(SupportDomainError::InvalidCapabilityBinding);
        }
        Ok(Self {
            id,
            version: 1,
            session_id: session.id(),
            ticket_id: session.ticket_id(),
            customer: session.customer(),
            customer_account: session.customer_account(),
            agent: session.agent(),
            device_ref: evidence.device_ref,
            mode: session.mode(),
            permissions: session.permissions().to_vec(),
            scope: session.scope().clone(),
            module: session.module().into(),
            secret_hash_sha256,
            issued_at: at,
            expires_at: session.expires_at(),
            revocation: None,
            uncommitted_events: vec![DomainEvent::CapabilityChanged],
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_rehydrate(
        id: CapabilityId,
        version: u64,
        session_id: SessionId,
        ticket_id: SupportTicketId,
        customer: customer::CustomerRef,
        customer_account: customer::CustomerAccountRef,
        agent: identity_access::PrincipalRef,
        device_ref: String,
        mode: SupportMode,
        permissions: Vec<String>,
        scope: AuthorityScope,
        module: String,
        secret_hash_sha256: [u8; 32],
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        revocation: Option<(CapabilityRevocationReason, DateTime<Utc>)>,
    ) -> Result<Self, SupportDomainError> {
        validate_scope(&scope)?;
        validate_text(&device_ref, "device_ref", MAX_REFERENCE_BYTES)?;
        validate_text(&module, "module", MAX_CODE_BYTES)?;
        validate_permissions(&permissions)?;
        if !permissions.windows(2).all(|pair| pair[0] < pair[1])
            || secret_hash_sha256.iter().all(|byte| *byte == 0)
            || expires_at <= issued_at
            || expires_at.signed_duration_since(issued_at) > mode.maximum_duration()
            || version != if revocation.is_some() { 2 } else { 1 }
            || revocation.is_some_and(|(_, at)| at < issued_at)
        {
            return Err(SupportDomainError::InvalidCapabilityBinding);
        }
        Ok(Self {
            id,
            version,
            session_id,
            ticket_id,
            customer,
            customer_account,
            agent,
            device_ref,
            mode,
            permissions,
            scope,
            module,
            secret_hash_sha256,
            issued_at,
            expires_at,
            revocation,
            uncommitted_events: Vec::new(),
        })
    }

    pub fn authorize(
        &self,
        session: &SupportSession,
        permission: &str,
        requested_scope: &AuthorityScope,
        evidence: &CapabilityEvidence,
        at: DateTime<Utc>,
    ) -> Result<(), SupportDomainError> {
        if self.revocation.is_some()
            || at < self.issued_at
            || at >= self.expires_at
            || !session.is_active(at)
        {
            return Err(SupportDomainError::CapabilityInactive);
        }
        if session.id() != self.session_id
            || session.agent() != self.agent
            || evidence.device_ref != self.device_ref
            || &self.scope != requested_scope
            || !evidence.authority_scope.contains(requested_scope)
        {
            return Err(SupportDomainError::InvalidCapabilityBinding);
        }
        if !evidence.segregation_of_duties_passed {
            return Err(SupportDomainError::SegregationOfDutiesDenied);
        }
        validate_mfa(self.mode, evidence.mfa_verified_at, at)?;
        if !self.permissions.iter().any(|value| value == permission) {
            return Err(SupportDomainError::PermissionDenied);
        }
        Ok(())
    }

    pub fn revoke(
        &mut self,
        reason: CapabilityRevocationReason,
        at: DateTime<Utc>,
    ) -> Result<(), SupportDomainError> {
        if self.revocation.is_some() || at < self.issued_at {
            return Err(SupportDomainError::CapabilityInactive);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(SupportDomainError::VersionOverflow)?;
        self.revocation = Some((reason, at));
        self.uncommitted_events.push(DomainEvent::CapabilityChanged);
        Ok(())
    }

    pub const fn id(&self) -> CapabilityId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }
    pub const fn ticket_id(&self) -> SupportTicketId {
        self.ticket_id
    }
    pub const fn customer(&self) -> customer::CustomerRef {
        self.customer
    }
    pub const fn customer_account(&self) -> customer::CustomerAccountRef {
        self.customer_account
    }
    pub const fn agent(&self) -> identity_access::PrincipalRef {
        self.agent
    }
    pub fn device_ref(&self) -> &str {
        &self.device_ref
    }
    pub const fn mode(&self) -> SupportMode {
        self.mode
    }
    pub fn permissions(&self) -> &[String] {
        &self.permissions
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn module(&self) -> &str {
        &self.module
    }
    pub const fn secret_hash_sha256(&self) -> &[u8; 32] {
        &self.secret_hash_sha256
    }
    pub const fn issued_at(&self) -> DateTime<Utc> {
        self.issued_at
    }
    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
    pub const fn revocation(&self) -> Option<(CapabilityRevocationReason, DateTime<Utc>)> {
        self.revocation
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum SupportAggregate {
    Ticket(SupportTicket),
    Incident(Incident),
    Session(SupportSession),
    Capability(SupportCapability),
}

impl SupportAggregate {
    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Ticket(_) => "support_ticket",
            Self::Incident(_) => "support_incident",
            Self::Session(_) => "support_session",
            Self::Capability(_) => "support_capability",
        }
    }
    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Ticket(v) => v.id().as_uuid(),
            Self::Incident(v) => v.id().as_uuid(),
            Self::Session(v) => v.id().as_uuid(),
            Self::Capability(v) => v.id().as_uuid(),
        }
    }
    pub const fn version(&self) -> u64 {
        match self {
            Self::Ticket(v) => v.version(),
            Self::Incident(v) => v.version(),
            Self::Session(v) => v.version(),
            Self::Capability(v) => v.version(),
        }
    }
    pub const fn scope(&self) -> &AuthorityScope {
        match self {
            Self::Ticket(v) => v.scope(),
            Self::Incident(v) => v.scope(),
            Self::Session(v) => v.scope(),
            Self::Capability(v) => v.scope(),
        }
    }
    pub const fn occurred_at(&self) -> DateTime<Utc> {
        match self {
            Self::Ticket(value) => value.updated_at(),
            Self::Incident(value) => value.updated_at(),
            Self::Session(value) => match value.revoked_at() {
                Some(at) => at,
                None => match value.approved_at() {
                    Some(at) => at,
                    None => value.requested_at(),
                },
            },
            Self::Capability(value) => match value.revocation() {
                Some((_, at)) => at,
                None => value.issued_at(),
            },
        }
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Ticket(v) => v.take_uncommitted_events(),
            Self::Incident(v) => v.take_uncommitted_events(),
            Self::Session(v) => v.take_uncommitted_events(),
            Self::Capability(v) => v.take_uncommitted_events(),
        }
    }
}

fn validate_scope(scope: &AuthorityScope) -> Result<(), SupportDomainError> {
    scope
        .validate()
        .map_err(|_| SupportDomainError::InvalidScope)?;
    if matches!(
        scope,
        AuthorityScope::Company { .. } | AuthorityScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(SupportDomainError::InvalidScope)
    }
}

fn validate_customer_scope(
    scope: &AuthorityScope,
    fact: &customer::CustomerProfileFact,
) -> Result<(), SupportDomainError> {
    validate_scope(scope)?;
    let owning_company = fact.owning_company.as_uuid();
    match scope {
        AuthorityScope::Company { company_id, .. } | AuthorityScope::Branch { company_id, .. }
            if *company_id == owning_company =>
        {
            Ok(())
        }
        _ => Err(SupportDomainError::InvalidScope),
    }
}

fn validate_text(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<(), SupportDomainError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        Err(SupportDomainError::InvalidText { field })
    } else {
        Ok(())
    }
}

fn validate_unique_texts(values: &[String], field: &'static str) -> Result<(), SupportDomainError> {
    let mut unique = BTreeSet::new();
    for value in values {
        validate_text(value, field, MAX_REFERENCE_BYTES)?;
        if !unique.insert(value) {
            return Err(SupportDomainError::InvalidText { field });
        }
    }
    Ok(())
}

fn validate_permissions(values: &[String]) -> Result<(), SupportDomainError> {
    if values.is_empty() || values.len() > MAX_PERMISSIONS {
        return Err(SupportDomainError::InvalidSession);
    }
    validate_unique_texts(values, "permissions")
}

fn validate_mfa(
    mode: SupportMode,
    verified_at: Option<DateTime<Utc>>,
    at: DateTime<Utc>,
) -> Result<(), SupportDomainError> {
    if !mode.requires_high_risk_mfa() {
        return Ok(());
    }
    let Some(verified_at) = verified_at else {
        return Err(SupportDomainError::MfaRequired);
    };
    if verified_at > at || at.signed_duration_since(verified_at) > HIGH_RISK_MFA_MAX_AGE {
        Err(SupportDomainError::MfaRequired)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
