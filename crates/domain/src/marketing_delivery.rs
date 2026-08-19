//! Pure domain rules for durable Marketing channel delivery.
//!
//! This module authorizes and records delivery intent only. It does not call
//! email, SMS, social, push, or webhook providers and therefore never claims
//! that a provider accepted or delivered a message without an explicit
//! provider result being supplied by a later integration boundary.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::AuthorityScope;
use crate::marketing::{
    ConsentRecord, ConsentState, MarketingChannel, MarketingPurpose, SuppressionRecord,
};

pub const MAX_DELIVERY_TEXT: usize = 255;
pub const MAX_DELIVERY_ATTEMPTS: u32 = 10;
pub const DEFAULT_DELIVERY_ATTEMPTS: u32 = 5;
pub const DEFAULT_RETRY_BASE_SECONDS: u64 = 30;
pub const DEFAULT_RETRY_MAX_SECONDS: u64 = 3_600;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingDeliveryError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("identifier cannot be nil: {field}")]
    NilIdentifier { field: &'static str },
    #[error("delivery scope is invalid")]
    InvalidScope,
    #[error("delivery expiry must be after queue time")]
    InvalidExpiry,
    #[error("retry policy is outside the bounded delivery limits")]
    InvalidRetryPolicy,
    #[error("dispatch channel does not match its channel record")]
    ChannelMismatch,
    #[error("delivery is blocked: {0:?}")]
    Blocked(DeliveryBlockReason),
    #[error("delivery is not due yet")]
    NotDue,
    #[error("delivery has expired")]
    Expired,
    #[error("delivery has reached its maximum attempt count")]
    MaxAttemptsReached,
    #[error("a delivery attempt is already in flight")]
    AttemptAlreadyInFlight,
    #[error("delivery is already terminal")]
    AlreadyTerminal,
    #[error("delivery attempt does not match the current attempt")]
    AttemptMismatch,
    #[error("provider callback timestamp precedes provider acceptance")]
    ProviderCallbackBeforeAcceptance,
    #[error("delivery state does not allow this transition")]
    InvalidStateTransition,
    #[error("delivery idempotency key is already bound to another dispatch")]
    IdempotencyConflict,
    #[error("delivery idempotency key is invalid")]
    InvalidIdempotencyKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentRecheckDecision {
    Granted,
    Missing,
    PendingDoubleOptIn,
    Revoked,
    Expired,
    InvalidProof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionRecheckDecision {
    Clear,
    Active,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryBlockReason {
    ConsentRequired,
    DoubleOptInRequired,
    ConsentRevoked,
    ConsentExpired,
    InvalidConsentProof,
    Suppressed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRecheckDecision {
    pub checked_at: DateTime<Utc>,
    pub consent: ConsentRecheckDecision,
    pub suppression: SuppressionRecheckDecision,
    pub result: DeliveryRecheckResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryRecheckResult {
    Allowed,
    Blocked(DeliveryBlockReason),
}

impl DeliveryRecheckDecision {
    pub fn allows_send(&self) -> bool {
        self.result == DeliveryRecheckResult::Allowed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelDispatchRecord {
    Email {
        recipient: String,
        template_ref: String,
    },
    Sms {
        phone: String,
        template_ref: String,
    },
    Social {
        network: String,
        destination_ref: String,
        content_ref: String,
    },
    Push {
        device_ref: String,
        template_ref: String,
    },
    Webhook {
        destination_ref: String,
        event_type: String,
        payload_ref: String,
    },
}

impl ChannelDispatchRecord {
    pub fn channel(&self) -> MarketingChannel {
        match self {
            Self::Email { .. } => MarketingChannel::Email,
            Self::Sms { .. } => MarketingChannel::Sms,
            Self::Social { .. } => MarketingChannel::Social,
            Self::Push { .. } => MarketingChannel::Push,
            Self::Webhook { .. } => MarketingChannel::Webhook,
        }
    }

    pub fn validate(&self) -> Result<(), MarketingDeliveryError> {
        match self {
            Self::Email {
                recipient,
                template_ref,
            }
            | Self::Sms {
                phone: recipient,
                template_ref,
            } => {
                bounded(recipient, "recipient", MAX_DELIVERY_TEXT)?;
                bounded(template_ref, "template_ref", MAX_DELIVERY_TEXT)?;
            }
            Self::Social {
                network,
                destination_ref,
                content_ref,
            } => {
                bounded(network, "social_network", 64)?;
                bounded(destination_ref, "destination_ref", MAX_DELIVERY_TEXT)?;
                bounded(content_ref, "content_ref", MAX_DELIVERY_TEXT)?;
            }
            Self::Push {
                device_ref,
                template_ref,
            } => {
                bounded(device_ref, "device_ref", MAX_DELIVERY_TEXT)?;
                bounded(template_ref, "template_ref", MAX_DELIVERY_TEXT)?;
            }
            Self::Webhook {
                destination_ref,
                event_type,
                payload_ref,
            } => {
                bounded(destination_ref, "destination_ref", MAX_DELIVERY_TEXT)?;
                bounded(event_type, "event_type", MAX_DELIVERY_TEXT)?;
                bounded(payload_ref, "payload_ref", MAX_DELIVERY_TEXT)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_seconds: u64,
    pub max_delay_seconds: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_DELIVERY_ATTEMPTS,
            base_delay_seconds: DEFAULT_RETRY_BASE_SECONDS,
            max_delay_seconds: DEFAULT_RETRY_MAX_SECONDS,
        }
    }
}

impl RetryPolicy {
    pub fn validate(self) -> Result<(), MarketingDeliveryError> {
        if !(1..=MAX_DELIVERY_ATTEMPTS).contains(&self.max_attempts)
            || self.base_delay_seconds == 0
            || self.base_delay_seconds > self.max_delay_seconds
            || self.max_delay_seconds > DEFAULT_RETRY_MAX_SECONDS
        {
            return Err(MarketingDeliveryError::InvalidRetryPolicy);
        }
        Ok(())
    }

    /// Returns the bounded delay after the supplied one-based attempt.
    pub fn delay_after_attempt(self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(63);
        let multiplier = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
        let seconds = self
            .base_delay_seconds
            .saturating_mul(multiplier)
            .min(self.max_delay_seconds);
        Duration::seconds(i64::try_from(seconds).unwrap_or(i64::MAX))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct DeliveryIdempotencyKey(String);

impl DeliveryIdempotencyKey {
    #[allow(clippy::too_many_arguments)]
    pub fn derive(
        scope: &AuthorityScope,
        profile_id: Uuid,
        campaign_id: Uuid,
        purpose: MarketingPurpose,
        channel: MarketingChannel,
        logical_message_ref: &str,
        template_revision: u64,
    ) -> Result<Self, MarketingDeliveryError> {
        validate_scope(scope)?;
        require_id(profile_id, "profile_id")?;
        require_id(campaign_id, "campaign_id")?;
        bounded(
            logical_message_ref,
            "logical_message_ref",
            MAX_DELIVERY_TEXT,
        )?;
        if template_revision == 0 {
            return Err(MarketingDeliveryError::Required {
                field: "template_revision",
            });
        }
        let canonical = format!(
            "erpx-marketing-delivery|{}|{}|{}|{}|{}|{}|{}",
            scope_key(scope),
            profile_id,
            campaign_id,
            purpose_key(purpose),
            channel_key(channel),
            logical_message_ref.trim(),
            template_revision,
        );
        Ok(Self(format!(
            "mdk_v1_{}",
            hex::encode(Sha256::digest(canonical.as_bytes()))
        )))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), MarketingDeliveryError> {
        if self.0.trim().is_empty()
            || self.0.len() > MAX_DELIVERY_TEXT
            || self.0.chars().any(char::is_control)
        {
            return Err(MarketingDeliveryError::InvalidIdempotencyKey);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Queued,
    Sending,
    RetryScheduled,
    ProviderAccepted,
    Delivered,
    Suppressed,
    FailedTerminal,
}

impl DeliveryState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Delivered | Self::Suppressed | Self::FailedTerminal
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryTerminalReason {
    Suppression,
    Expired,
    MaxAttempts,
    PermanentProviderFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryDispatch {
    pub id: Uuid,
    pub scope: AuthorityScope,
    pub profile_id: Uuid,
    pub campaign_id: Uuid,
    pub purpose: MarketingPurpose,
    pub channel: MarketingChannel,
    pub record: ChannelDispatchRecord,
    pub logical_message_ref: String,
    pub template_revision: u64,
    pub idempotency_key: DeliveryIdempotencyKey,
    pub state: DeliveryState,
    pub attempt_count: u32,
    pub retry_policy: RetryPolicy,
    pub queued_at: DateTime<Utc>,
    pub next_attempt_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub provider_message_ref: Option<String>,
    pub last_failure: Option<String>,
    pub terminal_reason: Option<DeliveryTerminalReason>,
    pub version: u64,
}

impl DeliveryDispatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: AuthorityScope,
        profile_id: Uuid,
        campaign_id: Uuid,
        purpose: MarketingPurpose,
        channel: MarketingChannel,
        record: ChannelDispatchRecord,
        logical_message_ref: impl Into<String>,
        template_revision: u64,
        queued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        retry_policy: RetryPolicy,
    ) -> Result<Self, MarketingDeliveryError> {
        require_id(id, "delivery_id")?;
        validate_scope(&scope)?;
        require_id(profile_id, "profile_id")?;
        require_id(campaign_id, "campaign_id")?;
        if channel != record.channel() {
            return Err(MarketingDeliveryError::ChannelMismatch);
        }
        record.validate()?;
        retry_policy.validate()?;
        if expires_at <= queued_at {
            return Err(MarketingDeliveryError::InvalidExpiry);
        }
        let logical_message_ref = logical_message_ref.into();
        let idempotency_key = DeliveryIdempotencyKey::derive(
            &scope,
            profile_id,
            campaign_id,
            purpose,
            channel,
            &logical_message_ref,
            template_revision,
        )?;
        Ok(Self {
            id,
            scope,
            profile_id,
            campaign_id,
            purpose,
            channel,
            record,
            logical_message_ref,
            template_revision,
            idempotency_key,
            state: DeliveryState::Queued,
            attempt_count: 0,
            retry_policy,
            queued_at,
            next_attempt_at: queued_at,
            expires_at,
            accepted_at: None,
            delivered_at: None,
            provider_message_ref: None,
            last_failure: None,
            terminal_reason: None,
            version: 1,
        })
    }

    pub fn recheck(
        &self,
        consents: &[ConsentRecord],
        suppressions: &[SuppressionRecord],
        at: DateTime<Utc>,
    ) -> DeliveryRecheckDecision {
        let consent = latest_consent_decision(self, consents, at);
        let suppression = suppression_decision(self, suppressions, at);
        let result = if suppression == SuppressionRecheckDecision::Active {
            DeliveryRecheckResult::Blocked(DeliveryBlockReason::Suppressed)
        } else {
            match consent {
                ConsentRecheckDecision::Granted => DeliveryRecheckResult::Allowed,
                ConsentRecheckDecision::Missing => {
                    DeliveryRecheckResult::Blocked(DeliveryBlockReason::ConsentRequired)
                }
                ConsentRecheckDecision::PendingDoubleOptIn => {
                    DeliveryRecheckResult::Blocked(DeliveryBlockReason::DoubleOptInRequired)
                }
                ConsentRecheckDecision::Revoked => {
                    DeliveryRecheckResult::Blocked(DeliveryBlockReason::ConsentRevoked)
                }
                ConsentRecheckDecision::Expired => {
                    DeliveryRecheckResult::Blocked(DeliveryBlockReason::ConsentExpired)
                }
                ConsentRecheckDecision::InvalidProof => {
                    DeliveryRecheckResult::Blocked(DeliveryBlockReason::InvalidConsentProof)
                }
            }
        };
        DeliveryRecheckDecision {
            checked_at: at,
            consent,
            suppression,
            result,
        }
    }

    /// Reserves exactly one attempt for this dispatch. A durable repository
    /// must apply the state/version change atomically before calling a
    /// provider; a second worker sees `AttemptAlreadyInFlight`.
    pub fn begin_attempt(
        &mut self,
        decision: &DeliveryRecheckDecision,
        at: DateTime<Utc>,
    ) -> Result<AttemptAuthorization, MarketingDeliveryError> {
        if self.state.is_terminal() {
            return Err(MarketingDeliveryError::AlreadyTerminal);
        }
        if self.state == DeliveryState::Sending {
            return Err(MarketingDeliveryError::AttemptAlreadyInFlight);
        }
        if at >= self.expires_at {
            self.fail_terminal(DeliveryTerminalReason::Expired, at);
            return Err(MarketingDeliveryError::Expired);
        }
        if self.attempt_count >= self.retry_policy.max_attempts {
            self.fail_terminal(DeliveryTerminalReason::MaxAttempts, at);
            return Err(MarketingDeliveryError::MaxAttemptsReached);
        }
        if at < self.next_attempt_at {
            return Err(MarketingDeliveryError::NotDue);
        }
        if !decision.allows_send() {
            if decision.suppression == SuppressionRecheckDecision::Active {
                self.cancel_for_suppression(at);
            }
            let reason = match decision.result {
                DeliveryRecheckResult::Allowed => unreachable!("allowed decision has no block"),
                DeliveryRecheckResult::Blocked(reason) => reason,
            };
            return Err(MarketingDeliveryError::Blocked(reason));
        }
        self.attempt_count += 1;
        self.state = DeliveryState::Sending;
        self.version += 1;
        Ok(AttemptAuthorization {
            dispatch_id: self.id,
            attempt: self.attempt_count,
            idempotency_key: self.idempotency_key.clone(),
        })
    }

    /// Cancels queued work when suppression is observed. In-flight work is
    /// deliberately not revoked here because the provider boundary may
    /// already have accepted it; a later provider result must still be
    /// recorded against the reserved attempt.
    pub fn cancel_queued_for_suppression(
        &mut self,
        decision: &DeliveryRecheckDecision,
        at: DateTime<Utc>,
    ) -> Result<bool, MarketingDeliveryError> {
        if decision.suppression != SuppressionRecheckDecision::Active {
            return Ok(false);
        }
        match self.state {
            DeliveryState::Queued | DeliveryState::RetryScheduled => {
                self.cancel_for_suppression(at);
                Ok(true)
            }
            DeliveryState::Sending => Err(MarketingDeliveryError::AttemptAlreadyInFlight),
            _ => Ok(false),
        }
    }

    pub fn record_provider_result(
        &mut self,
        attempt: u32,
        at: DateTime<Utc>,
        result: ProviderResult,
    ) -> Result<(), MarketingDeliveryError> {
        if self.state != DeliveryState::Sending {
            return Err(MarketingDeliveryError::InvalidStateTransition);
        }
        if attempt != self.attempt_count {
            return Err(MarketingDeliveryError::AttemptMismatch);
        }
        match result {
            ProviderResult::Accepted {
                provider_message_ref,
            } => {
                bounded(
                    &provider_message_ref,
                    "provider_message_ref",
                    MAX_DELIVERY_TEXT,
                )?;
                self.provider_message_ref = Some(provider_message_ref);
                self.accepted_at = Some(at);
                self.state = DeliveryState::ProviderAccepted;
                self.version += 1;
                Ok(())
            }
            ProviderResult::RetryableFailure { reason } => {
                bounded(&reason, "failure_reason", MAX_DELIVERY_TEXT)?;
                self.last_failure = Some(reason);
                let next_attempt_at = at + self.retry_policy.delay_after_attempt(attempt);
                if attempt >= self.retry_policy.max_attempts {
                    self.fail_terminal(DeliveryTerminalReason::MaxAttempts, at);
                } else if next_attempt_at >= self.expires_at {
                    self.fail_terminal(DeliveryTerminalReason::Expired, at);
                } else {
                    self.next_attempt_at = next_attempt_at;
                    self.state = DeliveryState::RetryScheduled;
                    self.version += 1;
                }
                Ok(())
            }
            ProviderResult::PermanentFailure { reason } => {
                bounded(&reason, "failure_reason", MAX_DELIVERY_TEXT)?;
                self.last_failure = Some(reason);
                self.fail_terminal(DeliveryTerminalReason::PermanentProviderFailure, at);
                Ok(())
            }
        }
    }

    /// Records an explicit provider delivery callback. The domain does not
    /// infer this state from an accepted submission.
    pub fn mark_delivered(&mut self, at: DateTime<Utc>) -> Result<(), MarketingDeliveryError> {
        if self.state != DeliveryState::ProviderAccepted {
            return Err(MarketingDeliveryError::InvalidStateTransition);
        }
        self.validate_provider_callback_at(at)?;
        self.delivered_at = Some(at);
        self.state = DeliveryState::Delivered;
        self.version += 1;
        Ok(())
    }

    /// Binds a delivery callback to the provider reference recorded when the
    /// provider accepted this exact attempt. Signature/authentication remains
    /// the responsibility of the provider ingress boundary.
    pub fn mark_delivered_for_provider(
        &mut self,
        provider_message_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDeliveryError> {
        if self.provider_message_ref.as_deref() != Some(provider_message_ref) {
            return Err(MarketingDeliveryError::AttemptMismatch);
        }
        self.mark_delivered(at)
    }

    /// Records a terminal provider callback after the provider accepted this
    /// exact message. The callback cannot forge a failure for another
    /// delivery because the accepted provider reference is checked first.
    pub fn fail_for_provider(
        &mut self,
        provider_message_ref: &str,
        reason: &str,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDeliveryError> {
        if self.provider_message_ref.as_deref() != Some(provider_message_ref) {
            return Err(MarketingDeliveryError::AttemptMismatch);
        }
        bounded(reason, "failure_reason", MAX_DELIVERY_TEXT)?;
        if self.state != DeliveryState::ProviderAccepted {
            return Err(MarketingDeliveryError::InvalidStateTransition);
        }
        self.validate_provider_callback_at(at)?;
        self.last_failure = Some(reason.to_owned());
        self.fail_terminal(DeliveryTerminalReason::PermanentProviderFailure, at);
        Ok(())
    }

    fn validate_provider_callback_at(
        &self,
        at: DateTime<Utc>,
    ) -> Result<(), MarketingDeliveryError> {
        if self.accepted_at.is_some_and(|accepted_at| at < accepted_at) {
            return Err(MarketingDeliveryError::ProviderCallbackBeforeAcceptance);
        }
        Ok(())
    }

    fn cancel_for_suppression(&mut self, at: DateTime<Utc>) {
        self.state = DeliveryState::Suppressed;
        self.terminal_reason = Some(DeliveryTerminalReason::Suppression);
        self.delivered_at = None;
        self.last_failure = Some("suppressed before provider dispatch".to_owned());
        self.next_attempt_at = at;
        self.version += 1;
    }

    fn fail_terminal(&mut self, reason: DeliveryTerminalReason, at: DateTime<Utc>) {
        self.state = DeliveryState::FailedTerminal;
        self.terminal_reason = Some(reason);
        self.next_attempt_at = at;
        self.version += 1;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptAuthorization {
    pub dispatch_id: Uuid,
    pub attempt: u32,
    pub idempotency_key: DeliveryIdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderResult {
    /// The provider boundary accepted the submission. This is not delivery.
    Accepted {
        provider_message_ref: String,
    },
    RetryableFailure {
        reason: String,
    },
    PermanentFailure {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyRegistration {
    Registered,
    ExistingSameDispatch,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeliveryIdempotencyRegistry {
    dispatches: BTreeMap<DeliveryIdempotencyKey, Uuid>,
}

impl DeliveryIdempotencyRegistry {
    /// Registers the deterministic key before a dispatch is queued. A key
    /// can be replayed for the same dispatch, but can never authorize another
    /// dispatch, which prevents duplicate logical sends after retries or
    /// duplicate queue messages.
    pub fn register(
        &mut self,
        key: DeliveryIdempotencyKey,
        dispatch_id: Uuid,
    ) -> Result<IdempotencyRegistration, MarketingDeliveryError> {
        require_id(dispatch_id, "dispatch_id")?;
        key.validate()?;
        match self.dispatches.get(&key) {
            Some(existing) if *existing == dispatch_id => {
                Ok(IdempotencyRegistration::ExistingSameDispatch)
            }
            Some(_) => Err(MarketingDeliveryError::IdempotencyConflict),
            None => {
                self.dispatches.insert(key, dispatch_id);
                Ok(IdempotencyRegistration::Registered)
            }
        }
    }
}

fn latest_consent_decision(
    dispatch: &DeliveryDispatch,
    consents: &[ConsentRecord],
    at: DateTime<Utc>,
) -> ConsentRecheckDecision {
    let latest = consents
        .iter()
        .filter(|consent| {
            consent.profile_id == dispatch.profile_id
                && consent.purpose == dispatch.purpose
                && consent.channel == dispatch.channel
                && consent.recorded_at <= at
                && consent.scope.contains(&dispatch.scope)
        })
        .max_by_key(|consent| (consent.recorded_at, consent.id));
    let Some(consent) = latest else {
        return ConsentRecheckDecision::Missing;
    };
    match consent.state {
        ConsentState::Revoked => ConsentRecheckDecision::Revoked,
        ConsentState::PendingDoubleOptIn => {
            if consent
                .expires_at
                .is_some_and(|expires_at| at >= expires_at)
            {
                ConsentRecheckDecision::Expired
            } else {
                ConsentRecheckDecision::PendingDoubleOptIn
            }
        }
        ConsentState::Granted => {
            if consent.proof_ref.as_deref().is_none_or(str::is_empty) {
                ConsentRecheckDecision::InvalidProof
            } else if consent
                .expires_at
                .is_some_and(|expires_at| at >= expires_at)
            {
                ConsentRecheckDecision::Expired
            } else {
                ConsentRecheckDecision::Granted
            }
        }
    }
}

fn suppression_decision(
    dispatch: &DeliveryDispatch,
    suppressions: &[SuppressionRecord],
    at: DateTime<Utc>,
) -> SuppressionRecheckDecision {
    let mut relevant = suppressions.iter().filter(|suppression| {
        suppression.profile_id == dispatch.profile_id
            && suppression.created_at <= at
            && suppression.scope.contains(&dispatch.scope)
    });
    if relevant
        .clone()
        .any(|suppression| suppression.is_active_at(at))
    {
        SuppressionRecheckDecision::Active
    } else if relevant.any(|suppression| suppression.expires_at.is_some()) {
        SuppressionRecheckDecision::Expired
    } else {
        SuppressionRecheckDecision::Clear
    }
}

fn validate_scope(scope: &AuthorityScope) -> Result<(), MarketingDeliveryError> {
    match scope {
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. }
            if holding_id.is_nil() =>
        {
            Err(MarketingDeliveryError::InvalidScope)
        }
        AuthorityScope::Company { company_id, .. } if company_id.is_nil() => {
            Err(MarketingDeliveryError::InvalidScope)
        }
        AuthorityScope::Branch {
            company_id,
            branch_id,
            ..
        } if company_id.is_nil() || branch_id.is_nil() => Err(MarketingDeliveryError::InvalidScope),
        AuthorityScope::Subtree {
            organization_node_id,
            ..
        } if organization_node_id.is_nil() => Err(MarketingDeliveryError::InvalidScope),
        AuthorityScope::Platform => Err(MarketingDeliveryError::InvalidScope),
        _ => Ok(()),
    }
}

fn scope_key(scope: &AuthorityScope) -> String {
    match scope {
        AuthorityScope::Holding { holding_id } => format!("h:{holding_id}"),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => format!("c:{holding_id}:{company_id}"),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("b:{holding_id}:{company_id}:{branch_id}"),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => format!("s:{holding_id}:{organization_node_id}"),
        AuthorityScope::Platform => "platform".to_owned(),
    }
}

fn purpose_key(purpose: MarketingPurpose) -> &'static str {
    match purpose {
        MarketingPurpose::Newsletter => "newsletter",
        MarketingPurpose::ProductUpdates => "product_updates",
        MarketingPurpose::Promotional => "promotional",
        MarketingPurpose::Transactional => "transactional",
        MarketingPurpose::LeadNurture => "lead_nurture",
    }
}

fn channel_key(channel: MarketingChannel) -> &'static str {
    match channel {
        MarketingChannel::Email => "email",
        MarketingChannel::Sms => "sms",
        MarketingChannel::Social => "social",
        MarketingChannel::Push => "push",
        MarketingChannel::Webhook => "webhook",
    }
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), MarketingDeliveryError> {
    if id.is_nil() {
        Err(MarketingDeliveryError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), MarketingDeliveryError> {
    if value.trim().is_empty() {
        return Err(MarketingDeliveryError::Required { field });
    }
    if value.len() > max {
        return Err(MarketingDeliveryError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(MarketingDeliveryError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn scope() -> AuthorityScope {
        AuthorityScope::Company {
            holding_id: Uuid::from_u128(1),
            company_id: Uuid::from_u128(2),
        }
    }

    fn dispatch(now: DateTime<Utc>, expires_at: DateTime<Utc>) -> DeliveryDispatch {
        DeliveryDispatch::new(
            Uuid::from_u128(10),
            scope(),
            Uuid::from_u128(11),
            Uuid::from_u128(12),
            MarketingPurpose::Newsletter,
            MarketingChannel::Email,
            ChannelDispatchRecord::Email {
                recipient: "person@example.test".into(),
                template_ref: "newsletter-v1".into(),
            },
            "campaign-message-1",
            1,
            now,
            expires_at,
            RetryPolicy::default(),
        )
        .expect("dispatch")
    }

    fn consent(
        state: ConsentState,
        recorded_at: DateTime<Utc>,
        expires_at: Option<DateTime<Utc>>,
    ) -> ConsentRecord {
        ConsentRecord {
            id: Uuid::from_u128(20),
            profile_id: Uuid::from_u128(11),
            scope: scope(),
            purpose: MarketingPurpose::Newsletter,
            channel: MarketingChannel::Email,
            state,
            source: "double_opt_in".into(),
            jurisdiction: "TH".into(),
            policy_version: "2026-01".into(),
            proof_ref: Some("proof-20".into()),
            recorded_at,
            expires_at,
        }
    }

    fn suppression(now: DateTime<Utc>, expires_at: Option<DateTime<Utc>>) -> SuppressionRecord {
        SuppressionRecord {
            id: Uuid::from_u128(30),
            profile_id: Uuid::from_u128(11),
            scope: scope(),
            reason: "unsubscribe".into(),
            source: "customer".into(),
            created_at: now,
            expires_at,
        }
    }

    #[test]
    fn all_channel_records_validate_and_report_their_channel() {
        let records = [
            (
                MarketingChannel::Email,
                ChannelDispatchRecord::Email {
                    recipient: "a@example.test".into(),
                    template_ref: "email-v1".into(),
                },
            ),
            (
                MarketingChannel::Sms,
                ChannelDispatchRecord::Sms {
                    phone: "+66800000000".into(),
                    template_ref: "sms-v1".into(),
                },
            ),
            (
                MarketingChannel::Social,
                ChannelDispatchRecord::Social {
                    network: "social".into(),
                    destination_ref: "page-1".into(),
                    content_ref: "content-1".into(),
                },
            ),
            (
                MarketingChannel::Push,
                ChannelDispatchRecord::Push {
                    device_ref: "device-1".into(),
                    template_ref: "push-v1".into(),
                },
            ),
            (
                MarketingChannel::Webhook,
                ChannelDispatchRecord::Webhook {
                    destination_ref: "hook-1".into(),
                    event_type: "lead.created".into(),
                    payload_ref: "payload-1".into(),
                },
            ),
        ];
        for (channel, record) in records {
            assert_eq!(record.channel(), channel);
            record.validate().expect("channel record");
        }
    }

    #[test]
    fn consent_and_suppression_rechecks_use_strict_expiry_boundaries() {
        let now = Utc::now();
        let delivery = dispatch(now, now + Duration::hours(1));
        let granted = consent(
            ConsentState::Granted,
            now - Duration::minutes(1),
            Some(now + Duration::seconds(1)),
        );
        assert_eq!(
            delivery
                .recheck(std::slice::from_ref(&granted), &[], now)
                .result,
            DeliveryRecheckResult::Allowed
        );
        assert_eq!(
            delivery
                .recheck(&[granted], &[], now + Duration::seconds(1))
                .consent,
            ConsentRecheckDecision::Expired
        );

        let active = suppression(now, Some(now + Duration::seconds(1)));
        let at_creation = delivery.recheck(
            &[consent(ConsentState::Granted, now, None)],
            std::slice::from_ref(&active),
            now,
        );
        assert_eq!(at_creation.suppression, SuppressionRecheckDecision::Active);
        assert_eq!(
            delivery
                .recheck(
                    &[consent(ConsentState::Granted, now, None)],
                    &[active],
                    now + Duration::seconds(1),
                )
                .suppression,
            SuppressionRecheckDecision::Expired
        );
    }

    #[test]
    fn pending_double_opt_in_and_invalid_proof_fail_closed() {
        let now = Utc::now();
        let delivery = dispatch(now, now + Duration::hours(1));
        let pending = delivery.recheck(
            &[consent(ConsentState::PendingDoubleOptIn, now, None)],
            &[],
            now,
        );
        assert_eq!(pending.consent, ConsentRecheckDecision::PendingDoubleOptIn);
        assert_eq!(
            pending.result,
            DeliveryRecheckResult::Blocked(DeliveryBlockReason::DoubleOptInRequired)
        );

        let mut without_proof = consent(ConsentState::Granted, now, None);
        without_proof.proof_ref = None;
        assert_eq!(
            delivery.recheck(&[without_proof], &[], now).result,
            DeliveryRecheckResult::Blocked(DeliveryBlockReason::InvalidConsentProof)
        );
    }

    #[test]
    fn deterministic_keys_and_registry_prevent_cross_dispatch_duplicates() {
        let first = DeliveryIdempotencyKey::derive(
            &scope(),
            Uuid::from_u128(11),
            Uuid::from_u128(12),
            MarketingPurpose::Newsletter,
            MarketingChannel::Email,
            "message-1",
            1,
        )
        .expect("key");
        let second = DeliveryIdempotencyKey::derive(
            &scope(),
            Uuid::from_u128(11),
            Uuid::from_u128(12),
            MarketingPurpose::Newsletter,
            MarketingChannel::Email,
            "message-1",
            1,
        )
        .expect("same key");
        assert_eq!(first, second);

        let mut registry = DeliveryIdempotencyRegistry::default();
        assert_eq!(
            registry.register(first.clone(), Uuid::from_u128(10)),
            Ok(IdempotencyRegistration::Registered)
        );
        assert_eq!(
            registry.register(first.clone(), Uuid::from_u128(10)),
            Ok(IdempotencyRegistration::ExistingSameDispatch)
        );
        assert_eq!(
            registry.register(first, Uuid::from_u128(99)),
            Err(MarketingDeliveryError::IdempotencyConflict)
        );
    }

    #[test]
    fn retry_backoff_is_bounded_and_exact_attempt_boundaries_hold() {
        let now = Utc::now();
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay_seconds: 10,
            max_delay_seconds: 15,
        };
        assert_eq!(policy.delay_after_attempt(1), Duration::seconds(10));
        assert_eq!(policy.delay_after_attempt(2), Duration::seconds(15));
        assert_eq!(policy.delay_after_attempt(99), Duration::seconds(15));

        let mut delivery = DeliveryDispatch::new(
            Uuid::from_u128(10),
            scope(),
            Uuid::from_u128(11),
            Uuid::from_u128(12),
            MarketingPurpose::Newsletter,
            MarketingChannel::Email,
            ChannelDispatchRecord::Email {
                recipient: "a@example.test".into(),
                template_ref: "v1".into(),
            },
            "message-retry",
            1,
            now,
            now + Duration::minutes(10),
            policy,
        )
        .expect("retry delivery");
        let decision = delivery.recheck(&[consent(ConsentState::Granted, now, None)], &[], now);
        let first = delivery
            .begin_attempt(&decision, now)
            .expect("first attempt");
        assert_eq!(first.attempt, 1);
        assert_eq!(
            delivery.begin_attempt(&decision, now),
            Err(MarketingDeliveryError::AttemptAlreadyInFlight)
        );
        delivery
            .record_provider_result(
                first.attempt,
                now,
                ProviderResult::RetryableFailure {
                    reason: "temporary".into(),
                },
            )
            .expect("retry scheduling");
        let due = now + Duration::seconds(10);
        assert_eq!(
            delivery.begin_attempt(&decision, due - Duration::nanoseconds(1)),
            Err(MarketingDeliveryError::NotDue)
        );
        let second = delivery
            .begin_attempt(&decision, due)
            .expect("exact due boundary");
        assert_eq!(second.attempt, 2);
    }

    #[test]
    fn suppression_cancels_queued_work_and_expiry_is_terminal_at_boundary() {
        let now = Utc::now();
        let mut queued = dispatch(now, now + Duration::hours(1));
        let decision = queued.recheck(
            &[consent(ConsentState::Granted, now, None)],
            &[suppression(now, None)],
            now,
        );
        assert!(
            queued
                .cancel_queued_for_suppression(&decision, now)
                .expect("cancel")
        );
        assert_eq!(queued.state, DeliveryState::Suppressed);
        assert!(queued.state.is_terminal());
        assert_eq!(
            queued.begin_attempt(&decision, now),
            Err(MarketingDeliveryError::AlreadyTerminal)
        );

        let expiry = now + Duration::seconds(5);
        let mut expired = dispatch(now, expiry);
        let allowed = expired.recheck(&[consent(ConsentState::Granted, now, None)], &[], now);
        assert_eq!(
            expired.begin_attempt(&allowed, expiry),
            Err(MarketingDeliveryError::Expired)
        );
        assert_eq!(
            expired.terminal_reason,
            Some(DeliveryTerminalReason::Expired)
        );
    }

    #[test]
    fn provider_acceptance_is_not_delivery_and_terminal_states_are_enforced() {
        let now = Utc::now();
        let mut delivery = dispatch(now, now + Duration::hours(1));
        let allowed = delivery.recheck(&[consent(ConsentState::Granted, now, None)], &[], now);
        let attempt = delivery.begin_attempt(&allowed, now).expect("attempt");
        delivery
            .record_provider_result(
                attempt.attempt,
                now,
                ProviderResult::Accepted {
                    provider_message_ref: "provider-1".into(),
                },
            )
            .expect("accepted");
        assert_eq!(delivery.state, DeliveryState::ProviderAccepted);
        assert!(!delivery.state.is_terminal());
        assert_eq!(
            delivery.mark_delivered_for_provider("wrong-provider-ref", now),
            Err(MarketingDeliveryError::AttemptMismatch)
        );
        delivery
            .mark_delivered_for_provider("provider-1", now)
            .expect("callback");
        assert_eq!(delivery.state, DeliveryState::Delivered);
        assert!(delivery.state.is_terminal());
        assert_eq!(
            delivery.mark_delivered(now),
            Err(MarketingDeliveryError::InvalidStateTransition)
        );

        let mut callback_failed = dispatch(now, now + Duration::hours(1));
        let allowed =
            callback_failed.recheck(&[consent(ConsentState::Granted, now, None)], &[], now);
        let attempt = callback_failed
            .begin_attempt(&allowed, now)
            .expect("attempt");
        callback_failed
            .record_provider_result(
                attempt.attempt,
                now,
                ProviderResult::Accepted {
                    provider_message_ref: "provider-failed".into(),
                },
            )
            .expect("accepted");
        callback_failed
            .fail_for_provider("provider-failed", "provider_bounced", now)
            .expect("callback failure");
        assert_eq!(callback_failed.state, DeliveryState::FailedTerminal);

        let mut failed = dispatch(now, now + Duration::hours(1));
        let allowed = failed.recheck(&[consent(ConsentState::Granted, now, None)], &[], now);
        let attempt = failed.begin_attempt(&allowed, now).expect("attempt");
        failed
            .record_provider_result(
                attempt.attempt,
                now,
                ProviderResult::PermanentFailure {
                    reason: "rejected".into(),
                },
            )
            .expect("terminal failure");
        assert_eq!(failed.state, DeliveryState::FailedTerminal);
        assert!(failed.state.is_terminal());
    }

    #[test]
    fn delivery_constructor_rejects_channel_mismatch_and_platform_scope() {
        let now = Utc::now();
        assert_eq!(
            DeliveryDispatch::new(
                Uuid::from_u128(10),
                scope(),
                Uuid::from_u128(11),
                Uuid::from_u128(12),
                MarketingPurpose::Newsletter,
                MarketingChannel::Sms,
                ChannelDispatchRecord::Email {
                    recipient: "a@example.test".into(),
                    template_ref: "v1".into(),
                },
                "message",
                1,
                now,
                now + Duration::hours(1),
                RetryPolicy::default(),
            )
            .expect_err("mismatch"),
            MarketingDeliveryError::ChannelMismatch
        );
        assert_eq!(
            DeliveryIdempotencyKey::derive(
                &AuthorityScope::Platform,
                Uuid::from_u128(11),
                Uuid::from_u128(12),
                MarketingPurpose::Newsletter,
                MarketingChannel::Email,
                "message",
                1,
            )
            .expect_err("platform scope"),
            MarketingDeliveryError::InvalidScope
        );
    }
}
