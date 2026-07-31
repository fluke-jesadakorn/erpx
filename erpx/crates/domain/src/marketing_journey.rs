//! Durable Marketing journey definitions and resume-safe execution facts.
//!
//! Marketing owns acquisition, segmentation, consent, and qualification. CRM
//! owns accepted lead records and opportunities. This module only defines a
//! tenant-scoped journey graph and the facts needed to resume its execution;
//! it never sends through a channel, calls an integration, or mutates CRM.

use std::collections::{BTreeSet, VecDeque};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{ActorAttribution, CommandContext, TenantScope};
use crate::marketing::MarketingChannel;
use crate::marketing_crm::{AccountReference, ContactReference, MarketingQualifiedSource};

pub const MARKETING_JOURNEY_CONTRACT_VERSION: u16 = 1;
pub const MAX_JOURNEY_NAME_BYTES: usize = 255;
pub const MAX_JOURNEY_NODES: usize = 256;
pub const MAX_NODE_REFERENCE_BYTES: usize = 160;
pub const MAX_BRANCH_VALUE_BYTES: usize = 255;
pub const MAX_CHANNEL_PAYLOAD_BYTES: usize = 4_096;
pub const MAX_WAIT_SECONDS: u64 = 365 * 24 * 60 * 60;
pub const MAX_EXECUTION_IDEMPOTENCY_KEY_BYTES: usize = 255;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingJourneyError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} contains a URL, secret, token, or other unsafe value")]
    UnsafeValue { field: &'static str },
    #[error("identifier cannot be nil: {field}")]
    NilIdentifier { field: &'static str },
    #[error("journey scope is invalid")]
    InvalidScope,
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("command scope does not contain the journey scope")]
    CommandScopeOutside,
    #[error("journey must contain between one and {max} nodes")]
    InvalidNodeCount { max: usize },
    #[error("journey node identifiers must be unique")]
    DuplicateNodeIdentifier,
    #[error("journey node ordinals must be unique and contiguous from zero")]
    InvalidNodeOrdering,
    #[error("journey must start with exactly one trigger node")]
    InvalidTrigger,
    #[error("journey node references an unknown node")]
    UnknownNodeReference,
    #[error("journey node references itself or a previous node")]
    BackwardNodeReference,
    #[error("journey contains an unreachable node")]
    UnreachableNode,
    #[error("journey node definition is invalid")]
    InvalidNodeDefinition,
    #[error("wait duration must be between one second and {max} seconds")]
    InvalidWaitDuration { max: u64 },
    #[error("channel payload does not match the channel")]
    ChannelMismatch,
    #[error("channel payload exceeds {max} bytes")]
    ChannelPayloadTooLarge { max: usize },
    #[error("conversion value cannot be negative")]
    NegativeConversionValue,
    #[error("conversion currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("journey cannot transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: JourneyStatus,
        to: JourneyStatus,
    },
    #[error("expected aggregate version is required")]
    MissingExpectedVersion,
    #[error("journey version mismatch: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("journey is not running")]
    JourneyNotRunning,
    #[error("execution cursor does not match the journey")]
    CursorJourneyMismatch,
    #[error("execution cursor version mismatch: expected {expected}, actual {actual}")]
    CursorVersionConflict { expected: u64, actual: u64 },
    #[error("execution cursor is invalid")]
    InvalidCursor,
    #[error("execution cursor is waiting until {due_at}")]
    NotDue { due_at: DateTime<Utc> },
    #[error("a branch decision is required for this node")]
    BranchDecisionRequired,
    #[error("a branch decision is not valid for this node")]
    UnexpectedBranchDecision,
    #[error("execution idempotency key is invalid")]
    InvalidExecutionIdempotencyKey,
    #[error("execution timestamp cannot move backwards")]
    InvalidExecutionTimestamp,
    #[error("CRM handoff boundary is invalid")]
    InvalidCrmBoundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JourneyStatus {
    Draft,
    Review,
    Approved,
    Scheduled,
    Running,
    Paused,
    Completed,
    Archived,
}

impl JourneyStatus {
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Review)
                | (Self::Review, Self::Approved | Self::Draft)
                | (Self::Approved, Self::Scheduled | Self::Draft)
                | (Self::Scheduled, Self::Running | Self::Draft)
                | (Self::Running, Self::Paused | Self::Completed)
                | (Self::Paused, Self::Running | Self::Completed)
                | (Self::Completed, Self::Archived)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JourneyTrigger {
    Enrollment,
    SegmentEntered { segment_id: Uuid },
    Event { event_type: String },
    Scheduled { at: DateTime<Utc> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JourneyChannelPayload {
    Email {
        template_ref: String,
        subject: String,
    },
    Sms {
        template_ref: String,
    },
    Social {
        network: String,
        content_ref: String,
    },
    Push {
        template_ref: String,
    },
    Webhook {
        event_type: String,
        payload_ref: String,
    },
}

impl JourneyChannelPayload {
    pub fn channel(&self) -> MarketingChannel {
        match self {
            Self::Email { .. } => MarketingChannel::Email,
            Self::Sms { .. } => MarketingChannel::Sms,
            Self::Social { .. } => MarketingChannel::Social,
            Self::Push { .. } => MarketingChannel::Push,
            Self::Webhook { .. } => MarketingChannel::Webhook,
        }
    }

    fn validate(&self, channel: MarketingChannel) -> Result<(), MarketingJourneyError> {
        if self.channel() != channel {
            return Err(MarketingJourneyError::ChannelMismatch);
        }
        let total_bytes = match self {
            Self::Email {
                template_ref,
                subject,
            } => {
                validate_safe_text(template_ref, "template_ref", MAX_NODE_REFERENCE_BYTES)?;
                validate_safe_text(subject, "subject", MAX_BRANCH_VALUE_BYTES)?;
                template_ref.len() + subject.len()
            }
            Self::Sms { template_ref } | Self::Push { template_ref } => {
                validate_safe_text(template_ref, "template_ref", MAX_NODE_REFERENCE_BYTES)?;
                template_ref.len()
            }
            Self::Social {
                network,
                content_ref,
            } => {
                validate_safe_text(network, "social_network", 64)?;
                validate_safe_text(content_ref, "content_ref", MAX_NODE_REFERENCE_BYTES)?;
                network.len() + content_ref.len()
            }
            Self::Webhook {
                event_type,
                payload_ref,
            } => {
                validate_safe_text(event_type, "event_type", MAX_NODE_REFERENCE_BYTES)?;
                validate_safe_text(payload_ref, "payload_ref", MAX_NODE_REFERENCE_BYTES)?;
                event_type.len() + payload_ref.len()
            }
        };
        if total_bytes > MAX_CHANNEL_PAYLOAD_BYTES {
            return Err(MarketingJourneyError::ChannelPayloadTooLarge {
                max: MAX_CHANNEL_PAYLOAD_BYTES,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JourneyBranchPredicate {
    AttributeEquals { attribute: String, value: String },
    AttributeExists { attribute: String },
}

impl JourneyBranchPredicate {
    fn validate(&self) -> Result<(), MarketingJourneyError> {
        match self {
            Self::AttributeEquals { attribute, value } => {
                validate_safe_text(attribute, "branch_attribute", MAX_NODE_REFERENCE_BYTES)?;
                validate_safe_text(value, "branch_value", MAX_BRANCH_VALUE_BYTES)?;
            }
            Self::AttributeExists { attribute } => {
                validate_safe_text(attribute, "branch_attribute", MAX_NODE_REFERENCE_BYTES)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentChange {
    Add,
    Remove,
}

/// An immutable conversion fact emitted by a journey execution. The
/// conversion reference is the caller-supplied business identity; retries
/// must reproduce this exact value rather than creating a second fact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyConversion {
    pub conversion_ref: Uuid,
    pub journey_id: Uuid,
    pub run_id: Uuid,
    pub profile_id: Uuid,
    pub scope: TenantScope,
    pub revenue_minor: Option<i64>,
    pub currency: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

impl JourneyConversion {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        conversion_ref: Uuid,
        journey_id: Uuid,
        run_id: Uuid,
        profile_id: Uuid,
        scope: TenantScope,
        revenue_minor: Option<i64>,
        currency: Option<String>,
        occurred_at: DateTime<Utc>,
    ) -> Result<Self, MarketingJourneyError> {
        let conversion = Self {
            conversion_ref,
            journey_id,
            run_id,
            profile_id,
            scope,
            revenue_minor,
            currency,
            occurred_at,
        };
        conversion.validate()?;
        Ok(conversion)
    }

    pub fn validate(&self) -> Result<(), MarketingJourneyError> {
        require_id(self.conversion_ref, "conversion_ref")?;
        require_id(self.journey_id, "journey_id")?;
        require_id(self.run_id, "run_id")?;
        require_id(self.profile_id, "profile_id")?;
        validate_scope(&self.scope)?;
        if self.revenue_minor.is_some_and(|value| value < 0) {
            return Err(MarketingJourneyError::NegativeConversionValue);
        }
        if let Some(currency) = &self.currency
            && (currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()))
        {
            return Err(MarketingJourneyError::InvalidCurrency);
        }
        Ok(())
    }
}

/// A durable owner-notification request produced by a journey action. The
/// request is an internal handoff; a downstream notification worker owns
/// provider delivery and may retry it independently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyOwnerNotification {
    pub notification_ref: Uuid,
    pub journey_id: Uuid,
    pub run_id: Uuid,
    pub profile_id: Uuid,
    pub scope: TenantScope,
    pub owner_ref: String,
    pub occurred_at: DateTime<Utc>,
}

/// A durable, profile-redacted handoff from a Marketing journey to an
/// approved holding-bound provider account. The provider-account reference
/// is resolved from authoritative control-plane onboarding state; this value
/// never contains an endpoint, credential, provider payload, or token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyIntegrationInvocation {
    pub invocation_id: Uuid,
    pub integration_ref: String,
    pub journey_id: Uuid,
    pub run_id: Uuid,
    pub node_id: Uuid,
    pub profile_id: Uuid,
    pub scope: TenantScope,
    pub idempotency_key: String,
    pub occurred_at: DateTime<Utc>,
}

impl JourneyIntegrationInvocation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        invocation_id: Uuid,
        integration_ref: impl Into<String>,
        journey_id: Uuid,
        run_id: Uuid,
        node_id: Uuid,
        profile_id: Uuid,
        scope: TenantScope,
        idempotency_key: impl Into<String>,
        occurred_at: DateTime<Utc>,
    ) -> Result<Self, MarketingJourneyError> {
        let invocation = Self {
            invocation_id,
            integration_ref: integration_ref.into(),
            journey_id,
            run_id,
            node_id,
            profile_id,
            scope,
            idempotency_key: idempotency_key.into(),
            occurred_at,
        };
        invocation.validate()?;
        Ok(invocation)
    }

    pub fn validate(&self) -> Result<(), MarketingJourneyError> {
        require_id(self.invocation_id, "invocation_id")?;
        require_id(self.journey_id, "journey_id")?;
        require_id(self.run_id, "run_id")?;
        require_id(self.node_id, "node_id")?;
        require_id(self.profile_id, "profile_id")?;
        validate_scope(&self.scope)?;
        validate_safe_text(
            &self.integration_ref,
            "integration_ref",
            MAX_NODE_REFERENCE_BYTES,
        )?;
        validate_safe_text(
            &self.idempotency_key,
            "integration_idempotency_key",
            MAX_EXECUTION_IDEMPOTENCY_KEY_BYTES,
        )?;
        Ok(())
    }
}

impl JourneyOwnerNotification {
    pub fn new(
        notification_ref: Uuid,
        journey_id: Uuid,
        run_id: Uuid,
        profile_id: Uuid,
        scope: TenantScope,
        owner_ref: String,
        occurred_at: DateTime<Utc>,
    ) -> Result<Self, MarketingJourneyError> {
        let notification = Self {
            notification_ref,
            journey_id,
            run_id,
            profile_id,
            scope,
            owner_ref,
            occurred_at,
        };
        notification.validate()?;
        Ok(notification)
    }

    pub fn validate(&self) -> Result<(), MarketingJourneyError> {
        require_id(self.notification_ref, "notification_ref")?;
        require_id(self.journey_id, "journey_id")?;
        require_id(self.run_id, "run_id")?;
        require_id(self.profile_id, "profile_id")?;
        validate_scope(&self.scope)?;
        validate_safe_text(&self.owner_ref, "owner_ref", MAX_NODE_REFERENCE_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JourneyAction {
    AddProfileTag {
        tag_ref: String,
    },
    RemoveProfileTag {
        tag_ref: String,
    },
    NotifyOwner {
        owner_ref: String,
    },
    /// Invokes the active, verified Marketing provider account whose
    /// authoritative control-plane reference equals `integration_ref`.
    InvokeApprovedIntegration {
        integration_ref: String,
    },
}

impl JourneyAction {
    fn validate(&self) -> Result<(), MarketingJourneyError> {
        match self {
            Self::AddProfileTag { tag_ref } | Self::RemoveProfileTag { tag_ref } => {
                validate_safe_text(tag_ref, "tag_ref", MAX_NODE_REFERENCE_BYTES)?;
            }
            Self::NotifyOwner { owner_ref } => {
                validate_safe_text(owner_ref, "owner_ref", MAX_NODE_REFERENCE_BYTES)?;
            }
            Self::InvokeApprovedIntegration { integration_ref } => {
                validate_safe_text(integration_ref, "integration_ref", MAX_NODE_REFERENCE_BYTES)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JourneyNodeDefinition {
    Trigger {
        trigger: JourneyTrigger,
    },
    Wait {
        duration_seconds: u64,
    },
    Channel {
        channel: MarketingChannel,
        payload: JourneyChannelPayload,
    },
    Branch {
        predicate: JourneyBranchPredicate,
        on_true: Uuid,
        on_false: Uuid,
    },
    SegmentChange {
        segment_id: Uuid,
        change: SegmentChange,
    },
    CrmHandoff {
        source: Box<MarketingQualifiedSource>,
        account: AccountReference,
        contact: ContactReference,
    },
    Conversion {
        conversion_ref: Uuid,
        revenue_minor: Option<i64>,
        currency: Option<String>,
    },
    Action {
        action: JourneyAction,
    },
}

impl JourneyNodeDefinition {
    fn validate(&self, scope: &TenantScope) -> Result<(), MarketingJourneyError> {
        match self {
            Self::Trigger { trigger } => match trigger {
                JourneyTrigger::Enrollment => {}
                JourneyTrigger::SegmentEntered { segment_id } => {
                    require_id(*segment_id, "segment_id")?;
                }
                JourneyTrigger::Event { event_type } => {
                    validate_safe_text(event_type, "event_type", MAX_NODE_REFERENCE_BYTES)?;
                }
                JourneyTrigger::Scheduled { .. } => {}
            },
            Self::Wait { duration_seconds } => {
                if !(1..=MAX_WAIT_SECONDS).contains(duration_seconds) {
                    return Err(MarketingJourneyError::InvalidWaitDuration {
                        max: MAX_WAIT_SECONDS,
                    });
                }
            }
            Self::Channel { channel, payload } => payload.validate(*channel)?,
            Self::Branch {
                predicate,
                on_true,
                on_false,
            } => {
                predicate.validate()?;
                require_id(*on_true, "branch_true_node_id")?;
                require_id(*on_false, "branch_false_node_id")?;
                if on_true == on_false {
                    return Err(MarketingJourneyError::InvalidNodeDefinition);
                }
            }
            Self::SegmentChange { segment_id, .. } => {
                require_id(*segment_id, "segment_id")?;
            }
            Self::CrmHandoff {
                source,
                account,
                contact,
            } => {
                if source.validate_for_scope(scope).is_err()
                    || account.validate().is_err()
                    || contact.validate().is_err()
                    || !scope.contains(account.scope())
                    || !scope.contains(contact.scope())
                    || !safe_crm_source_reference(source)
                    || !safe_crm_reference(account)
                    || !safe_crm_reference(contact)
                {
                    return Err(MarketingJourneyError::InvalidCrmBoundary);
                }
            }
            Self::Conversion {
                conversion_ref,
                revenue_minor,
                currency,
            } => {
                require_id(*conversion_ref, "conversion_ref")?;
                if revenue_minor.is_some_and(|value| value < 0) {
                    return Err(MarketingJourneyError::NegativeConversionValue);
                }
                if let Some(currency) = currency
                    && (currency.len() != 3
                        || !currency.bytes().all(|byte| byte.is_ascii_uppercase()))
                {
                    return Err(MarketingJourneyError::InvalidCurrency);
                }
            }
            Self::Action { action } => action.validate()?,
        }
        Ok(())
    }

    fn is_trigger(&self) -> bool {
        matches!(self, Self::Trigger { .. })
    }

    fn branch_targets(&self) -> Option<(Uuid, Uuid)> {
        match self {
            Self::Branch {
                on_true, on_false, ..
            } => Some((*on_true, *on_false)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyNode {
    pub id: Uuid,
    pub ordinal: u32,
    pub next: Option<Uuid>,
    pub definition: JourneyNodeDefinition,
}

impl JourneyNode {
    pub fn new(
        id: Uuid,
        ordinal: u32,
        next: Option<Uuid>,
        definition: JourneyNodeDefinition,
    ) -> Result<Self, MarketingJourneyError> {
        let node = Self {
            id,
            ordinal,
            next,
            definition,
        };
        require_id(node.id, "journey_node_id")?;
        Ok(node)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journey {
    id: Uuid,
    scope: TenantScope,
    name: String,
    status: JourneyStatus,
    nodes: Vec<JourneyNode>,
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Journey {
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        name: impl Into<String>,
        mut nodes: Vec<JourneyNode>,
        at: DateTime<Utc>,
    ) -> Result<Self, MarketingJourneyError> {
        nodes.sort_by_key(|node| (node.ordinal, node.id));
        let journey = Self {
            id,
            scope,
            name: name.into(),
            status: JourneyStatus::Draft,
            nodes,
            // Persisted aggregates use positive versions so the first durable
            // snapshot and its outbox event can share the database contract.
            version: 1,
            created_at: at,
            updated_at: at,
        };
        journey.validate()?;
        Ok(journey)
    }

    pub fn validate(&self) -> Result<(), MarketingJourneyError> {
        require_id(self.id, "journey_id")?;
        validate_scope(&self.scope)?;
        validate_safe_text(&self.name, "journey_name", MAX_JOURNEY_NAME_BYTES)?;
        if self.nodes.is_empty() || self.nodes.len() > MAX_JOURNEY_NODES {
            return Err(MarketingJourneyError::InvalidNodeCount {
                max: MAX_JOURNEY_NODES,
            });
        }
        if self.updated_at < self.created_at {
            return Err(MarketingJourneyError::InvalidExecutionTimestamp);
        }

        let ids = self
            .nodes
            .iter()
            .map(|node| node.id)
            .collect::<BTreeSet<_>>();
        if ids.len() != self.nodes.len() {
            return Err(MarketingJourneyError::DuplicateNodeIdentifier);
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if node.ordinal != index as u32 {
                return Err(MarketingJourneyError::InvalidNodeOrdering);
            }
            node.definition.validate(&self.scope)?;
            if node.next.is_some_and(|next| !ids.contains(&next)) {
                return Err(MarketingJourneyError::UnknownNodeReference);
            }
            if node.next.is_some_and(|next| next == node.id) {
                return Err(MarketingJourneyError::BackwardNodeReference);
            }
            if let Some((on_true, on_false)) = node.definition.branch_targets() {
                if !ids.contains(&on_true) || !ids.contains(&on_false) {
                    return Err(MarketingJourneyError::UnknownNodeReference);
                }
                if on_true == node.id || on_false == node.id {
                    return Err(MarketingJourneyError::BackwardNodeReference);
                }
            }
        }

        let trigger_count = self
            .nodes
            .iter()
            .filter(|node| node.definition.is_trigger())
            .count();
        if trigger_count != 1 || !self.nodes[0].definition.is_trigger() {
            return Err(MarketingJourneyError::InvalidTrigger);
        }
        let ordinal_by_id = self
            .nodes
            .iter()
            .map(|node| (node.id, node.ordinal))
            .collect::<std::collections::BTreeMap<_, _>>();
        for node in &self.nodes {
            if let Some(next) = node.next
                && ordinal_by_id[&next] <= node.ordinal
            {
                return Err(MarketingJourneyError::BackwardNodeReference);
            }
            if let Some((on_true, on_false)) = node.definition.branch_targets()
                && (ordinal_by_id[&on_true] <= node.ordinal
                    || ordinal_by_id[&on_false] <= node.ordinal)
            {
                return Err(MarketingJourneyError::BackwardNodeReference);
            }
            if node.definition.branch_targets().is_some() && node.next.is_some() {
                return Err(MarketingJourneyError::InvalidNodeDefinition);
            }
        }

        let mut reachable = BTreeSet::new();
        let mut pending = VecDeque::from([self.nodes[0].id]);
        while let Some(node_id) = pending.pop_front() {
            if !reachable.insert(node_id) {
                continue;
            }
            let node = self
                .nodes
                .iter()
                .find(|node| node.id == node_id)
                .ok_or(MarketingJourneyError::UnknownNodeReference)?;
            if let Some(next) = node.next {
                pending.push_back(next);
            }
            if let Some((on_true, on_false)) = node.definition.branch_targets() {
                pending.push_back(on_true);
                pending.push_back(on_false);
            }
        }
        if reachable.len() != self.nodes.len() {
            return Err(MarketingJourneyError::UnreachableNode);
        }
        Ok(())
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn status(&self) -> JourneyStatus {
        self.status
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn nodes(&self) -> &[JourneyNode] {
        &self.nodes
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn transition(
        &mut self,
        command: &JourneyTransitionCommand,
    ) -> Result<JourneyTransitionFact, MarketingJourneyError> {
        command.validate()?;
        if command.journey_id != self.id {
            return Err(MarketingJourneyError::NilIdentifier {
                field: "journey_id_mismatch",
            });
        }
        if !command.context.tenant_scope.contains(&self.scope)
            || matches!(command.context.tenant_scope, TenantScope::Platform)
        {
            return Err(MarketingJourneyError::CommandScopeOutside);
        }
        let expected = command
            .context
            .expected_aggregate_version
            .ok_or(MarketingJourneyError::MissingExpectedVersion)?;
        if expected != self.version {
            return Err(MarketingJourneyError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        if !self.status.can_transition_to(command.next_status) {
            return Err(MarketingJourneyError::InvalidTransition {
                from: self.status,
                to: command.next_status,
            });
        }
        if command.at < self.updated_at {
            return Err(MarketingJourneyError::InvalidExecutionTimestamp);
        }
        let from = self.status;
        self.status = command.next_status;
        self.version =
            self.version
                .checked_add(1)
                .ok_or(MarketingJourneyError::VersionConflict {
                    expected,
                    actual: self.version,
                })?;
        self.updated_at = command.at;
        self.validate()?;
        Ok(JourneyTransitionFact {
            journey_id: self.id,
            tenant_scope: self.scope.clone(),
            from,
            to: self.status,
            expected_version: expected,
            aggregate_version: self.version,
            command_id: command.context.command_id,
            idempotency_key: command.context.idempotency_key.clone(),
            actor: command.context.actor.clone(),
            correlation_id: command.context.correlation_id,
            causation_id: command.context.causation_id,
            occurred_at: command.at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyTransitionCommand {
    pub journey_id: Uuid,
    pub next_status: JourneyStatus,
    pub at: DateTime<Utc>,
    pub context: CommandContext,
}

impl JourneyTransitionCommand {
    pub fn validate(&self) -> Result<(), MarketingJourneyError> {
        require_id(self.journey_id, "journey_id")?;
        self.context
            .validate()
            .map_err(|error| MarketingJourneyError::InvalidCommandContext(error.to_string()))?;
        require_id(self.context.command_id, "command_id")?;
        require_id(self.context.correlation_id, "correlation_id")?;
        validate_safe_text(
            &self.context.actor.principal_ref,
            "actor_principal_ref",
            MAX_NODE_REFERENCE_BYTES,
        )?;
        validate_safe_text(
            &self.context.idempotency_key,
            "idempotency_key",
            MAX_EXECUTION_IDEMPOTENCY_KEY_BYTES,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyTransitionFact {
    pub journey_id: Uuid,
    pub tenant_scope: TenantScope,
    pub from: JourneyStatus,
    pub to: JourneyStatus,
    pub expected_version: u64,
    pub aggregate_version: u64,
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub actor: ActorAttribution,
    pub correlation_id: Uuid,
    pub causation_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JourneyExecutionState {
    Ready,
    Running,
    Waiting,
    Paused,
    Completed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JourneyExecutionPhase {
    WaitingScheduled,
    NodeCompleted,
    Paused,
    Resumed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyExecutionIdempotencyKey(String);

impl JourneyExecutionIdempotencyKey {
    pub fn derive(
        journey_id: Uuid,
        run_id: Uuid,
        profile_id: Uuid,
        node_id: Uuid,
        cursor_version: u64,
        phase: JourneyExecutionPhase,
    ) -> Result<Self, MarketingJourneyError> {
        require_id(journey_id, "journey_id")?;
        require_id(run_id, "run_id")?;
        require_id(profile_id, "profile_id")?;
        require_id(node_id, "node_id")?;
        let value = format!(
            "journey-exec-v1:{journey_id}:{run_id}:{profile_id}:{node_id}:{cursor_version}:{phase:?}"
        );
        if value.len() > MAX_EXECUTION_IDEMPOTENCY_KEY_BYTES {
            return Err(MarketingJourneyError::InvalidExecutionIdempotencyKey);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), MarketingJourneyError> {
        if self.0.trim().is_empty()
            || self.0.len() > MAX_EXECUTION_IDEMPOTENCY_KEY_BYTES
            || self.0.chars().any(char::is_control)
            || self.0.contains("http")
            || self.0.contains("token")
            || self.0.contains("secret")
        {
            return Err(MarketingJourneyError::InvalidExecutionIdempotencyKey);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JourneyNodeOutcome {
    Complete,
    Branch(bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyExecutionCursor {
    journey_id: Uuid,
    run_id: Uuid,
    profile_id: Uuid,
    scope: TenantScope,
    state: JourneyExecutionState,
    next_node_id: Option<Uuid>,
    waiting_until: Option<DateTime<Utc>>,
    version: u64,
    updated_at: DateTime<Utc>,
}

impl JourneyExecutionCursor {
    pub fn start(
        journey: &Journey,
        run_id: Uuid,
        profile_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, MarketingJourneyError> {
        journey.validate()?;
        require_id(run_id, "run_id")?;
        require_id(profile_id, "profile_id")?;
        if journey.status != JourneyStatus::Running {
            return Err(MarketingJourneyError::JourneyNotRunning);
        }
        let cursor = Self {
            journey_id: journey.id,
            run_id,
            profile_id,
            scope: journey.scope.clone(),
            state: JourneyExecutionState::Ready,
            next_node_id: Some(journey.nodes[0].id),
            waiting_until: None,
            version: 0,
            updated_at: at,
        };
        cursor.validate_for_journey(journey)?;
        Ok(cursor)
    }

    pub fn validate_for_journey(&self, journey: &Journey) -> Result<(), MarketingJourneyError> {
        journey.validate()?;
        require_id(self.journey_id, "journey_id")?;
        require_id(self.run_id, "run_id")?;
        require_id(self.profile_id, "profile_id")?;
        if self.journey_id != journey.id || self.scope != journey.scope {
            return Err(MarketingJourneyError::CursorJourneyMismatch);
        }
        if self.updated_at < journey.created_at {
            return Err(MarketingJourneyError::InvalidExecutionTimestamp);
        }
        let Some(next_node_id) = self.next_node_id else {
            if self.state != JourneyExecutionState::Completed || self.waiting_until.is_some() {
                return Err(MarketingJourneyError::InvalidCursor);
            }
            return Ok(());
        };
        if !journey.nodes.iter().any(|node| node.id == next_node_id) {
            return Err(MarketingJourneyError::UnknownNodeReference);
        }
        if self.state == JourneyExecutionState::Completed
            || (self.state == JourneyExecutionState::Waiting && self.waiting_until.is_none())
            || (matches!(
                self.state,
                JourneyExecutionState::Ready | JourneyExecutionState::Running
            ) && self.waiting_until.is_some())
        {
            return Err(MarketingJourneyError::InvalidCursor);
        }
        if self.state == JourneyExecutionState::Running && self.waiting_until.is_some() {
            return Err(MarketingJourneyError::InvalidCursor);
        }
        Ok(())
    }

    pub fn journey_id(&self) -> Uuid {
        self.journey_id
    }

    pub fn run_id(&self) -> Uuid {
        self.run_id
    }

    pub fn profile_id(&self) -> Uuid {
        self.profile_id
    }

    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }

    pub fn state(&self) -> JourneyExecutionState {
        self.state
    }

    pub fn next_node_id(&self) -> Option<Uuid> {
        self.next_node_id
    }

    pub fn waiting_until(&self) -> Option<DateTime<Utc>> {
        self.waiting_until
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn advance(
        &mut self,
        journey: &Journey,
        expected_version: u64,
        outcome: JourneyNodeOutcome,
        at: DateTime<Utc>,
    ) -> Result<JourneyExecutionFact, MarketingJourneyError> {
        self.validate_for_journey(journey)?;
        if journey.status != JourneyStatus::Running {
            return Err(MarketingJourneyError::JourneyNotRunning);
        }
        if expected_version != self.version {
            return Err(MarketingJourneyError::CursorVersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        if at < self.updated_at {
            return Err(MarketingJourneyError::InvalidExecutionTimestamp);
        }
        if self.state == JourneyExecutionState::Paused {
            return Err(MarketingJourneyError::InvalidCursor);
        }
        let node_id = self
            .next_node_id
            .ok_or(MarketingJourneyError::InvalidCursor)?;
        let node = journey
            .nodes
            .iter()
            .find(|node| node.id == node_id)
            .ok_or(MarketingJourneyError::UnknownNodeReference)?;
        let previous_version = self.version;
        let mut next_waiting_until = self.waiting_until;

        if let Some(waiting_until) = self.waiting_until {
            if at < waiting_until {
                return Err(MarketingJourneyError::NotDue {
                    due_at: waiting_until,
                });
            }
            next_waiting_until = None;
        } else if let JourneyNodeDefinition::Wait { duration_seconds } = node.definition {
            let waiting_until = at
                .checked_add_signed(Duration::seconds(i64::try_from(duration_seconds).map_err(
                    |_| MarketingJourneyError::InvalidWaitDuration {
                        max: MAX_WAIT_SECONDS,
                    },
                )?))
                .ok_or(MarketingJourneyError::InvalidWaitDuration {
                    max: MAX_WAIT_SECONDS,
                })?;
            next_waiting_until = Some(waiting_until);
            let next_version = self
                .version
                .checked_add(1)
                .ok_or(MarketingJourneyError::InvalidCursor)?;
            let key = JourneyExecutionIdempotencyKey::derive(
                self.journey_id,
                self.run_id,
                self.profile_id,
                node_id,
                previous_version,
                JourneyExecutionPhase::WaitingScheduled,
            )?;
            self.waiting_until = next_waiting_until;
            self.state = JourneyExecutionState::Waiting;
            self.version = next_version;
            self.updated_at = at;
            return Ok(JourneyExecutionFact {
                journey_id: self.journey_id,
                run_id: self.run_id,
                profile_id: self.profile_id,
                node_id,
                expected_cursor_version: expected_version,
                cursor_version: self.version,
                state: self.state,
                next_node_id: self.next_node_id,
                waiting_until: self.waiting_until,
                idempotency_key: key,
                phase: JourneyExecutionPhase::WaitingScheduled,
                occurred_at: at,
                branch_decision: None,
            });
        }

        let (next_node_id, branch_decision) = match node.definition.branch_targets() {
            Some((on_true, on_false)) => {
                let decision = match outcome {
                    JourneyNodeOutcome::Branch(decision) => decision,
                    JourneyNodeOutcome::Complete => {
                        return Err(MarketingJourneyError::BranchDecisionRequired);
                    }
                };
                (
                    Some(if decision { on_true } else { on_false }),
                    Some(decision),
                )
            }
            None => {
                if matches!(outcome, JourneyNodeOutcome::Branch(_)) {
                    return Err(MarketingJourneyError::UnexpectedBranchDecision);
                }
                (node.next, None)
            }
        };
        let next_state = if next_node_id.is_some() {
            JourneyExecutionState::Running
        } else {
            JourneyExecutionState::Completed
        };
        let next_version = self
            .version
            .checked_add(1)
            .ok_or(MarketingJourneyError::InvalidCursor)?;
        let key = JourneyExecutionIdempotencyKey::derive(
            self.journey_id,
            self.run_id,
            self.profile_id,
            node_id,
            previous_version,
            JourneyExecutionPhase::NodeCompleted,
        )?;
        self.next_node_id = next_node_id;
        self.waiting_until = next_waiting_until;
        self.state = next_state;
        self.version = next_version;
        self.updated_at = at;
        Ok(JourneyExecutionFact {
            journey_id: self.journey_id,
            run_id: self.run_id,
            profile_id: self.profile_id,
            node_id,
            expected_cursor_version: expected_version,
            cursor_version: self.version,
            state: self.state,
            next_node_id: self.next_node_id,
            waiting_until: self.waiting_until,
            idempotency_key: key,
            phase: JourneyExecutionPhase::NodeCompleted,
            occurred_at: at,
            branch_decision,
        })
    }

    pub fn pause(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<JourneyExecutionFact, MarketingJourneyError> {
        self.control(
            expected_version,
            at,
            JourneyExecutionState::Paused,
            JourneyExecutionPhase::Paused,
        )
    }

    pub fn resume(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<JourneyExecutionFact, MarketingJourneyError> {
        let next_state = if self.waiting_until.is_some() {
            JourneyExecutionState::Waiting
        } else {
            JourneyExecutionState::Running
        };
        self.control(
            expected_version,
            at,
            next_state,
            JourneyExecutionPhase::Resumed,
        )
    }

    fn control(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
        state: JourneyExecutionState,
        phase: JourneyExecutionPhase,
    ) -> Result<JourneyExecutionFact, MarketingJourneyError> {
        if expected_version != self.version {
            return Err(MarketingJourneyError::CursorVersionConflict {
                expected: expected_version,
                actual: self.version,
            });
        }
        if at < self.updated_at || self.next_node_id.is_none() {
            return Err(MarketingJourneyError::InvalidExecutionTimestamp);
        }
        if phase == JourneyExecutionPhase::Paused
            && !matches!(
                self.state,
                JourneyExecutionState::Ready
                    | JourneyExecutionState::Running
                    | JourneyExecutionState::Waiting
            )
        {
            return Err(MarketingJourneyError::InvalidCursor);
        }
        if phase == JourneyExecutionPhase::Resumed && self.state != JourneyExecutionState::Paused {
            return Err(MarketingJourneyError::InvalidCursor);
        }
        let node_id = self
            .next_node_id
            .ok_or(MarketingJourneyError::InvalidCursor)?;
        let previous_version = self.version;
        let next_version = self
            .version
            .checked_add(1)
            .ok_or(MarketingJourneyError::InvalidCursor)?;
        let key = JourneyExecutionIdempotencyKey::derive(
            self.journey_id,
            self.run_id,
            self.profile_id,
            node_id,
            previous_version,
            phase,
        )?;
        self.state = state;
        self.version = next_version;
        self.updated_at = at;
        Ok(JourneyExecutionFact {
            journey_id: self.journey_id,
            run_id: self.run_id,
            profile_id: self.profile_id,
            node_id,
            expected_cursor_version: expected_version,
            cursor_version: self.version,
            state: self.state,
            next_node_id: self.next_node_id,
            waiting_until: self.waiting_until,
            idempotency_key: key,
            phase,
            occurred_at: at,
            branch_decision: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JourneyExecutionFact {
    pub journey_id: Uuid,
    pub run_id: Uuid,
    pub profile_id: Uuid,
    pub node_id: Uuid,
    pub expected_cursor_version: u64,
    pub cursor_version: u64,
    pub state: JourneyExecutionState,
    pub next_node_id: Option<Uuid>,
    pub waiting_until: Option<DateTime<Utc>>,
    pub idempotency_key: JourneyExecutionIdempotencyKey,
    pub phase: JourneyExecutionPhase,
    pub occurred_at: DateTime<Utc>,
    pub branch_decision: Option<bool>,
}

fn validate_scope(scope: &TenantScope) -> Result<(), MarketingJourneyError> {
    match scope {
        TenantScope::Platform => Err(MarketingJourneyError::InvalidScope),
        TenantScope::Holding { holding_id } if holding_id.is_nil() => {
            Err(MarketingJourneyError::InvalidScope)
        }
        TenantScope::Company {
            holding_id,
            company_id,
        } if holding_id.is_nil() || company_id.is_nil() => Err(MarketingJourneyError::InvalidScope),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } if holding_id.is_nil() || company_id.is_nil() || branch_id.is_nil() => {
            Err(MarketingJourneyError::InvalidScope)
        }
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } if holding_id.is_nil()
            || (company_ids.is_empty() && branch_ids.is_empty())
            || company_ids.iter().any(Uuid::is_nil)
            || branch_ids.iter().any(Uuid::is_nil)
            || has_duplicates(company_ids)
            || has_duplicates(branch_ids) =>
        {
            Err(MarketingJourneyError::InvalidScope)
        }
        _ => Ok(()),
    }
}

fn has_duplicates(ids: &[Uuid]) -> bool {
    ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
}

fn safe_crm_source_reference(source: &MarketingQualifiedSource) -> bool {
    [
        source.lead.id.as_str(),
        source.acquisition.id.as_str(),
        source.qualification.id.as_str(),
    ]
    .iter()
    .all(|value| validate_safe_text(value, "crm_reference", MAX_NODE_REFERENCE_BYTES).is_ok())
}

fn safe_crm_reference<T>(reference: &T) -> bool
where
    T: CrmReferenceText,
{
    validate_safe_text(
        reference.reference_text(),
        "crm_reference",
        MAX_NODE_REFERENCE_BYTES,
    )
    .is_ok()
}

trait CrmReferenceText {
    fn reference_text(&self) -> &str;
}

impl CrmReferenceText for AccountReference {
    fn reference_text(&self) -> &str {
        self.id.as_str()
    }
}

impl CrmReferenceText for ContactReference {
    fn reference_text(&self) -> &str {
        self.id.as_str()
    }
}

fn validate_safe_text(
    value: &str,
    field: &'static str,
    max: usize,
) -> Result<(), MarketingJourneyError> {
    if value.trim().is_empty() {
        return Err(MarketingJourneyError::Required { field });
    }
    if value.len() > max {
        return Err(MarketingJourneyError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(MarketingJourneyError::ControlCharacter { field });
    }
    let lower = value.to_ascii_lowercase();
    if lower.contains("://")
        || lower.starts_with("www.")
        || lower.starts_with("mailto:")
        || [
            "secret",
            "token",
            "password",
            "api_key",
            "apikey",
            "private_key",
            "bearer ",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return Err(MarketingJourneyError::UnsafeValue { field });
    }
    Ok(())
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), MarketingJourneyError> {
    if id.is_nil() {
        Err(MarketingJourneyError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorKind, AuditContext};

    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }

    fn actor() -> ActorAttribution {
        ActorAttribution {
            principal_ref: "marketing-owner".into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::from_u128(3)),
            represented_principal_ref: None,
            support_capability_id: None,
        }
    }

    fn context(expected: Option<u64>, key: &str) -> CommandContext {
        let correlation_id = Uuid::from_u128(30);
        CommandContext {
            contract_version: 1,
            command_id: Uuid::from_u128(31),
            idempotency_key: key.into(),
            correlation_id,
            causation_id: None,
            actor: actor(),
            tenant_scope: scope(),
            audit: AuditContext {
                purpose: "journey lifecycle".into(),
                customer_visible: false,
                module: Some("marketing".into()),
                correlation_id,
            },
            expected_aggregate_version: expected,
            subscription_id: None,
        }
    }

    fn node(
        id: u128,
        ordinal: u32,
        next: Option<u128>,
        definition: JourneyNodeDefinition,
    ) -> JourneyNode {
        JourneyNode::new(
            Uuid::from_u128(id),
            ordinal,
            next.map(Uuid::from_u128),
            definition,
        )
        .unwrap()
    }

    fn linear_journey() -> Journey {
        Journey::new(
            Uuid::from_u128(100),
            scope(),
            "Nurture",
            vec![
                node(
                    101,
                    0,
                    Some(102),
                    JourneyNodeDefinition::Trigger {
                        trigger: JourneyTrigger::Enrollment,
                    },
                ),
                node(
                    102,
                    1,
                    Some(103),
                    JourneyNodeDefinition::Wait {
                        duration_seconds: 60,
                    },
                ),
                node(
                    103,
                    2,
                    None,
                    JourneyNodeDefinition::Channel {
                        channel: MarketingChannel::Email,
                        payload: JourneyChannelPayload::Email {
                            template_ref: "welcome-v1".into(),
                            subject: "Welcome".into(),
                        },
                    },
                ),
            ],
            DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn deterministic_order_and_expected_version_transition_fact() {
        let mut journey = linear_journey();
        let at = DateTime::from_timestamp(1_700_000_001, 0).unwrap();
        let command = JourneyTransitionCommand {
            journey_id: journey.id(),
            next_status: JourneyStatus::Review,
            at,
            context: context(Some(1), "journey-review-1"),
        };
        let fact = journey.transition(&command).unwrap();
        assert_eq!(journey.nodes()[0].ordinal, 0);
        assert_eq!(journey.nodes()[1].ordinal, 1);
        assert_eq!(fact.expected_version, 1);
        assert_eq!(fact.aggregate_version, 2);
        assert_eq!(fact.from, JourneyStatus::Draft);
        assert_eq!(fact.to, JourneyStatus::Review);
        assert_eq!(
            journey.transition(&JourneyTransitionCommand {
                context: context(Some(1), "stale"),
                next_status: JourneyStatus::Approved,
                journey_id: journey.id(),
                at: at + Duration::seconds(1),
            }),
            Err(MarketingJourneyError::VersionConflict {
                expected: 1,
                actual: 2,
            })
        );
    }

    #[test]
    fn node_validation_fails_closed_for_order_wait_payload_and_unsafe_values() {
        let bad_wait = Journey::new(
            Uuid::from_u128(200),
            scope(),
            "Bad",
            vec![
                node(
                    201,
                    0,
                    Some(202),
                    JourneyNodeDefinition::Trigger {
                        trigger: JourneyTrigger::Enrollment,
                    },
                ),
                node(
                    202,
                    1,
                    None,
                    JourneyNodeDefinition::Wait {
                        duration_seconds: MAX_WAIT_SECONDS + 1,
                    },
                ),
            ],
            DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        );
        assert!(matches!(
            bad_wait,
            Err(MarketingJourneyError::InvalidWaitDuration { .. })
        ));

        let bad_channel = Journey::new(
            Uuid::from_u128(210),
            scope(),
            "Bad",
            vec![
                node(
                    211,
                    0,
                    Some(212),
                    JourneyNodeDefinition::Trigger {
                        trigger: JourneyTrigger::Enrollment,
                    },
                ),
                node(
                    212,
                    1,
                    None,
                    JourneyNodeDefinition::Channel {
                        channel: MarketingChannel::Email,
                        payload: JourneyChannelPayload::Sms {
                            template_ref: "https://provider.invalid".into(),
                        },
                    },
                ),
            ],
            DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        );
        assert_eq!(bad_channel, Err(MarketingJourneyError::ChannelMismatch));

        let unordered = Journey::new(
            Uuid::from_u128(220),
            scope(),
            "Bad",
            vec![
                node(
                    221,
                    0,
                    Some(222),
                    JourneyNodeDefinition::Trigger {
                        trigger: JourneyTrigger::Enrollment,
                    },
                ),
                node(
                    222,
                    2,
                    None,
                    JourneyNodeDefinition::Action {
                        action: JourneyAction::AddProfileTag {
                            tag_ref: "secret-tag".into(),
                        },
                    },
                ),
            ],
            DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        );
        assert!(matches!(
            unordered,
            Err(MarketingJourneyError::InvalidNodeOrdering)
                | Err(MarketingJourneyError::UnsafeValue { .. })
        ));
    }

    #[test]
    fn cursor_schedules_wait_then_resumes_with_stable_idempotency_facts() {
        let mut journey = linear_journey();
        let base = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let mut context = context(Some(1), "review");
        context.command_id = Uuid::from_u128(32);
        let mut command = JourneyTransitionCommand {
            journey_id: journey.id(),
            next_status: JourneyStatus::Review,
            at: base,
            context,
        };
        journey.transition(&command).unwrap();
        command.context.command_id = Uuid::from_u128(33);
        command.context.idempotency_key = "approve".into();
        command.context.expected_aggregate_version = Some(2);
        command.next_status = JourneyStatus::Approved;
        command.at = base + Duration::seconds(1);
        journey.transition(&command).unwrap();
        command.context.command_id = Uuid::from_u128(34);
        command.context.idempotency_key = "schedule".into();
        command.context.expected_aggregate_version = Some(3);
        command.next_status = JourneyStatus::Scheduled;
        command.at = base + Duration::seconds(2);
        journey.transition(&command).unwrap();
        command.context.command_id = Uuid::from_u128(35);
        command.context.idempotency_key = "run".into();
        command.context.expected_aggregate_version = Some(4);
        command.next_status = JourneyStatus::Running;
        command.at = base + Duration::seconds(3);
        journey.transition(&command).unwrap();

        let mut cursor = JourneyExecutionCursor::start(
            &journey,
            Uuid::from_u128(400),
            Uuid::from_u128(401),
            base + Duration::seconds(4),
        )
        .unwrap();
        let trigger = cursor
            .advance(
                &journey,
                0,
                JourneyNodeOutcome::Complete,
                base + Duration::seconds(5),
            )
            .unwrap();
        assert_eq!(trigger.phase, JourneyExecutionPhase::NodeCompleted);
        let wait = cursor
            .advance(
                &journey,
                1,
                JourneyNodeOutcome::Complete,
                base + Duration::seconds(6),
            )
            .unwrap();
        assert_eq!(wait.phase, JourneyExecutionPhase::WaitingScheduled);
        assert_eq!(cursor.state(), JourneyExecutionState::Waiting);
        let due = cursor.waiting_until().unwrap();
        assert_eq!(
            cursor.advance(
                &journey,
                2,
                JourneyNodeOutcome::Complete,
                due - Duration::seconds(1),
            ),
            Err(MarketingJourneyError::NotDue { due_at: due })
        );
        let resumed = cursor
            .advance(&journey, 2, JourneyNodeOutcome::Complete, due)
            .unwrap();
        assert_eq!(resumed.phase, JourneyExecutionPhase::NodeCompleted);
        assert_ne!(wait.idempotency_key, resumed.idempotency_key);
    }

    #[test]
    fn cursor_version_overflow_rejects_without_partial_mutation() {
        let mut journey = linear_journey();
        journey.status = JourneyStatus::Running;
        let base = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let cursor = JourneyExecutionCursor::start(
            &journey,
            Uuid::from_u128(500),
            Uuid::from_u128(501),
            base,
        )
        .unwrap();

        let mut node_completion = cursor.clone();
        node_completion.version = u64::MAX;
        let before_node_completion = node_completion.clone();
        assert_eq!(
            node_completion.advance(
                &journey,
                u64::MAX,
                JourneyNodeOutcome::Complete,
                base + Duration::seconds(1),
            ),
            Err(MarketingJourneyError::InvalidCursor)
        );
        assert_eq!(node_completion, before_node_completion);

        let mut wait_schedule = cursor.clone();
        wait_schedule.next_node_id = Some(Uuid::from_u128(102));
        wait_schedule.state = JourneyExecutionState::Running;
        wait_schedule.version = u64::MAX;
        let before_wait_schedule = wait_schedule.clone();
        assert_eq!(
            wait_schedule.advance(
                &journey,
                u64::MAX,
                JourneyNodeOutcome::Complete,
                base + Duration::seconds(1),
            ),
            Err(MarketingJourneyError::InvalidCursor)
        );
        assert_eq!(wait_schedule, before_wait_schedule);

        let mut pause = cursor;
        pause.version = u64::MAX;
        let before_pause = pause.clone();
        assert_eq!(
            pause.pause(u64::MAX, base + Duration::seconds(1)),
            Err(MarketingJourneyError::InvalidCursor)
        );
        assert_eq!(pause, before_pause);
    }

    #[test]
    fn cursor_validation_rejects_waiting_timestamp_on_ready_state() {
        let mut journey = linear_journey();
        journey.status = JourneyStatus::Running;
        let base = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let mut cursor = JourneyExecutionCursor::start(
            &journey,
            Uuid::from_u128(600),
            Uuid::from_u128(601),
            base,
        )
        .unwrap();
        cursor.waiting_until = Some(base + Duration::seconds(1));

        assert_eq!(
            cursor.validate_for_journey(&journey),
            Err(MarketingJourneyError::InvalidCursor)
        );
    }
}
