//! Pure, authenticated-boundary contract for Marketing provider callbacks.
//!
//! This module deliberately stops at the boundary between a raw-body HTTP
//! ingress and the Marketing write model. It does not verify a provider
//! signature, parse provider-specific payloads, query a delivery, or mutate a
//! database. The ingress must first verify the raw body and then supply the
//! resulting [`ProviderAuthenticationDecision`] together with trusted routing
//! data. The write-side adapter must still bind the accepted callback to the
//! stored [`DeliveryDispatch`] before changing delivery state.

use std::cmp::Ordering;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::AuthorityScope;
use crate::marketing_delivery::DeliveryDispatch;

/// Provider callback fields are opaque references, never provider payloads.
pub const MAX_CALLBACK_REFERENCE_BYTES: usize = 255;
pub const MAX_CALLBACK_HOLDING_BYTES: usize = 128;
pub const MAX_CALLBACK_AUTH_SCHEME_BYTES: usize = 64;
pub const MAX_CALLBACK_AUTH_KEY_REF_BYTES: usize = 255;

/// A callback is accepted at the lower bound and rejected at the upper bound:
/// `[received_at - 24h, received_at + 5m)`.
pub const CALLBACK_MAX_EVENT_AGE_SECONDS: i64 = 24 * 60 * 60;
pub const CALLBACK_MAX_FUTURE_SKEW_SECONDS: i64 = 5 * 60;

const CALLBACK_KEY_VERSION: &str = "erpx-marketing-provider-callback";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingProviderCallbackError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("identifier cannot be nil: {field}")]
    NilIdentifier { field: &'static str },
    #[error("callback scope is invalid or not bound to the holding")]
    InvalidScope,
    #[error("provider callback signature/authentication was not verified")]
    AuthenticationRequired,
    #[error("provider callback authentication decision is invalid")]
    InvalidAuthenticationDecision,
    #[error("callback binding does not match the trusted route: {field}")]
    BindingMismatch { field: &'static str },
    #[error("provider event timestamp is outside the accepted window")]
    TimestampOutsideWindow,
    #[error("delivery identifier does not match the callback")]
    DeliveryMismatch,
    #[error("provider message reference does not match the accepted delivery")]
    ProviderReferenceMismatch,
    #[error("callback event identity conflicts with an existing event")]
    EventIdentityConflict,
}

/// The only authentication result the domain accepts is an explicit result
/// produced by a raw-body verifier. Raw signatures and provider payloads are
/// intentionally absent so they cannot be persisted or accidentally logged by
/// this contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderAuthenticationDecision {
    Verified(VerifiedProviderAuthentication),
    NotVerified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedProviderAuthentication {
    /// Provider-specific verification scheme, for example `stripe_signature`.
    pub scheme: String,
    /// Opaque key/configuration reference used by the verifier, not a secret.
    pub verifier_key_ref: String,
    /// Digest of the exact raw request body that was verified.
    pub raw_body_sha256: [u8; 32],
}

impl VerifiedProviderAuthentication {
    pub fn new(
        scheme: impl Into<String>,
        verifier_key_ref: impl Into<String>,
        raw_body_sha256: [u8; 32],
    ) -> Result<Self, MarketingProviderCallbackError> {
        let value = Self {
            scheme: scheme.into(),
            verifier_key_ref: verifier_key_ref.into(),
            raw_body_sha256,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), MarketingProviderCallbackError> {
        bounded(
            &self.scheme,
            "authentication_scheme",
            MAX_CALLBACK_AUTH_SCHEME_BYTES,
        )?;
        bounded(
            &self.verifier_key_ref,
            "verifier_key_ref",
            MAX_CALLBACK_AUTH_KEY_REF_BYTES,
        )?;
        if self.raw_body_sha256 == [0; 32] {
            return Err(MarketingProviderCallbackError::InvalidAuthenticationDecision);
        }
        Ok(())
    }
}

/// Trusted route data supplied by the control-plane/provider-account lookup.
/// `scope` is the exact delivery scope, while `holding_id` makes the
/// holding-boundary check explicit even when the scope is a company or branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProviderCallbackBinding {
    pub provider_account_ref: String,
    pub holding_id: Uuid,
    pub scope: AuthorityScope,
}

impl MarketingProviderCallbackBinding {
    pub fn new(
        provider_account_ref: impl Into<String>,
        holding_id: Uuid,
        scope: AuthorityScope,
    ) -> Result<Self, MarketingProviderCallbackError> {
        let value = Self {
            provider_account_ref: provider_account_ref.into(),
            holding_id,
            scope,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingProviderCallbackError> {
        bounded(
            &self.provider_account_ref,
            "provider_account_ref",
            MAX_CALLBACK_REFERENCE_BYTES,
        )?;
        require_id(self.holding_id, "holding_id")?;
        if self.scope == AuthorityScope::Platform
            || scope_holding_id(&self.scope) != Some(self.holding_id)
        {
            return Err(MarketingProviderCallbackError::InvalidScope);
        }
        Ok(())
    }
}

/// Only these provider outcomes may cross this callback boundary. Unknown
/// provider event names must be rejected by the provider adapter before a
/// request is constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingProviderCallbackKind {
    Delivered,
    Failed,
    Bounced,
}

impl MarketingProviderCallbackKind {
    fn key(self) -> &'static str {
        match self {
            Self::Delivered => "delivered",
            Self::Failed => "failed",
            Self::Bounced => "bounced",
        }
    }
}

/// Fixed timestamp policy for provider callbacks. The interval is half-open:
/// the lower boundary is valid and the upper boundary is not. A valid,
/// authenticated callback outside this interval is rejected before the
/// infrastructure inbox is touched.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarketingProviderCallbackTimestampPolicy;

impl MarketingProviderCallbackTimestampPolicy {
    pub fn bounds(self, received_at: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
        (
            received_at - Duration::seconds(CALLBACK_MAX_EVENT_AGE_SECONDS),
            received_at + Duration::seconds(CALLBACK_MAX_FUTURE_SKEW_SECONDS),
        )
    }

    pub fn contains(self, provider_event_at: DateTime<Utc>, received_at: DateTime<Utc>) -> bool {
        let (lower, upper) = self.bounds(received_at);
        provider_event_at >= lower && provider_event_at < upper
    }
}

/// Parsed but not yet trusted provider callback input. The caller must use
/// [`Self::verify`] with the binding selected by the trusted ingress route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProviderCallbackRequest {
    pub binding: MarketingProviderCallbackBinding,
    pub delivery_id: Uuid,
    pub provider_message_ref: String,
    pub provider_event_id: String,
    pub provider_event_at: DateTime<Utc>,
    pub kind: MarketingProviderCallbackKind,
    pub authentication: ProviderAuthenticationDecision,
}

impl MarketingProviderCallbackRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        binding: MarketingProviderCallbackBinding,
        delivery_id: Uuid,
        provider_message_ref: impl Into<String>,
        provider_event_id: impl Into<String>,
        provider_event_at: DateTime<Utc>,
        kind: MarketingProviderCallbackKind,
        authentication: ProviderAuthenticationDecision,
    ) -> Result<Self, MarketingProviderCallbackError> {
        let value = Self {
            binding,
            delivery_id,
            provider_message_ref: provider_message_ref.into(),
            provider_event_id: provider_event_id.into(),
            provider_event_at,
            kind,
            authentication,
        };
        value.validate_shape()?;
        Ok(value)
    }

    /// Verifies only the domain-level invariants that are available after raw
    /// body authentication and trusted route lookup. It does not inspect the
    /// database or delivery aggregate.
    pub fn verify(
        &self,
        expected_binding: &MarketingProviderCallbackBinding,
        received_at: DateTime<Utc>,
    ) -> Result<MarketingProviderCallback, MarketingProviderCallbackError> {
        self.validate_shape()?;
        expected_binding.validate()?;
        if &self.binding != expected_binding {
            return Err(binding_mismatch(&self.binding, expected_binding));
        }
        if !MarketingProviderCallbackTimestampPolicy.contains(self.provider_event_at, received_at) {
            return Err(MarketingProviderCallbackError::TimestampOutsideWindow);
        }
        let ProviderAuthenticationDecision::Verified(authentication) = &self.authentication else {
            return Err(MarketingProviderCallbackError::AuthenticationRequired);
        };
        authentication.validate()?;
        Ok(MarketingProviderCallback {
            binding: self.binding.clone(),
            delivery_id: self.delivery_id,
            provider_message_ref: self.provider_message_ref.clone(),
            provider_event_id: self.provider_event_id.clone(),
            provider_event_at: self.provider_event_at,
            kind: self.kind,
            authentication: authentication.clone(),
            idempotency: MarketingProviderCallbackIdempotency::derive(self)?,
        })
    }

    fn validate_shape(&self) -> Result<(), MarketingProviderCallbackError> {
        self.binding.validate()?;
        require_id(self.delivery_id, "delivery_id")?;
        bounded(
            &self.provider_message_ref,
            "provider_message_ref",
            MAX_CALLBACK_REFERENCE_BYTES,
        )?;
        bounded(
            &self.provider_event_id,
            "provider_event_id",
            MAX_CALLBACK_REFERENCE_BYTES,
        )?;
        Ok(())
    }
}

/// Authenticated and time-valid callback event. The event is still a pure
/// value: infrastructure must call [`Self::bind_to_delivery`] and then apply
/// the outcome through the Marketing write-side aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProviderCallback {
    pub binding: MarketingProviderCallbackBinding,
    pub delivery_id: Uuid,
    pub provider_message_ref: String,
    pub provider_event_id: String,
    pub provider_event_at: DateTime<Utc>,
    pub kind: MarketingProviderCallbackKind,
    pub authentication: VerifiedProviderAuthentication,
    pub idempotency: MarketingProviderCallbackIdempotency,
}

impl MarketingProviderCallback {
    /// Confirms the callback was resolved to the exact delivery and to the
    /// provider reference recorded by the accepted submission. This equality
    /// check is intentionally stricter than a provider-message lookup alone.
    pub fn bind_to_delivery(
        &self,
        delivery: &DeliveryDispatch,
    ) -> Result<(), MarketingProviderCallbackError> {
        if delivery.id != self.delivery_id || delivery.scope != self.binding.scope {
            return Err(MarketingProviderCallbackError::DeliveryMismatch);
        }
        if delivery.provider_message_ref.as_deref() != Some(self.provider_message_ref.as_str()) {
            return Err(MarketingProviderCallbackError::ProviderReferenceMismatch);
        }
        Ok(())
    }

    pub fn classify_against(
        &self,
        previous: Option<&MarketingProviderCallbackWatermark>,
    ) -> MarketingProviderCallbackOrder {
        let Some(previous) = previous else {
            return MarketingProviderCallbackOrder::New;
        };
        if self.provider_event_id == previous.provider_event_id {
            if self.idempotency.event_fingerprint == previous.event_fingerprint {
                MarketingProviderCallbackOrder::Duplicate
            } else {
                MarketingProviderCallbackOrder::ConflictingDuplicate
            }
        } else {
            match event_order(self).cmp(&watermark_order(previous)) {
                Ordering::Greater => MarketingProviderCallbackOrder::InOrder,
                Ordering::Less => MarketingProviderCallbackOrder::OutOfOrder,
                Ordering::Equal => MarketingProviderCallbackOrder::ConflictingDuplicate,
            }
        }
    }

    pub fn watermark(&self) -> MarketingProviderCallbackWatermark {
        MarketingProviderCallbackWatermark {
            provider_event_id: self.provider_event_id.clone(),
            provider_event_at: self.provider_event_at,
            event_fingerprint: self.idempotency.event_fingerprint,
        }
    }
}

/// Stable event ordering metadata for a provider account. The infrastructure
/// layer may persist this beside its inbox row; provider timestamps are the
/// primary order and event IDs are the deterministic tie-breaker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProviderCallbackWatermark {
    pub provider_event_id: String,
    pub provider_event_at: DateTime<Utc>,
    pub event_fingerprint: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketingProviderCallbackOrder {
    New,
    InOrder,
    OutOfOrder,
    Duplicate,
    ConflictingDuplicate,
}

/// Deterministic keys that the raw-body ingress can hand to an inbox and a
/// delivery transition command. The key contents are hashed so provider data
/// is not exposed through database key strings or logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingProviderCallbackIdempotency {
    pub inbox_key: MarketingProviderInboxKey,
    pub delivery_idempotency_key: MarketingProviderDeliveryKey,
    pub event_fingerprint: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct MarketingProviderInboxKey(String);

impl MarketingProviderInboxKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct MarketingProviderDeliveryKey(String);

impl MarketingProviderDeliveryKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl MarketingProviderCallbackIdempotency {
    fn derive(
        request: &MarketingProviderCallbackRequest,
    ) -> Result<Self, MarketingProviderCallbackError> {
        let binding = &request.binding;
        let scope = scope_key(&binding.scope);
        let event_at = timestamp_key(request.provider_event_at);
        let kind = request.kind.key();
        let event_fingerprint = digest(&[
            CALLBACK_KEY_VERSION,
            "event-fingerprint",
            &binding.provider_account_ref,
            &binding.holding_id.to_string(),
            &scope,
            &request.delivery_id.to_string(),
            &request.provider_message_ref,
            &request.provider_event_id,
            &event_at,
            kind,
        ]);
        let inbox_digest = digest(&[
            CALLBACK_KEY_VERSION,
            "inbox",
            &binding.provider_account_ref,
            &binding.holding_id.to_string(),
            &request.provider_event_id,
        ]);
        let delivery_digest = digest(&[
            CALLBACK_KEY_VERSION,
            "delivery",
            &binding.provider_account_ref,
            &binding.holding_id.to_string(),
            &scope,
            &request.delivery_id.to_string(),
            &request.provider_message_ref,
            kind,
        ]);
        Ok(Self {
            inbox_key: MarketingProviderInboxKey(format!("mpcb_in_{}", hex::encode(inbox_digest))),
            delivery_idempotency_key: MarketingProviderDeliveryKey(format!(
                "mpcb_del_{}",
                hex::encode(delivery_digest)
            )),
            event_fingerprint,
        })
    }
}

fn event_order(value: &MarketingProviderCallback) -> (DateTime<Utc>, &str) {
    (value.provider_event_at, value.provider_event_id.as_str())
}

fn watermark_order(value: &MarketingProviderCallbackWatermark) -> (DateTime<Utc>, &str) {
    (value.provider_event_at, value.provider_event_id.as_str())
}

fn binding_mismatch(
    actual: &MarketingProviderCallbackBinding,
    expected: &MarketingProviderCallbackBinding,
) -> MarketingProviderCallbackError {
    if actual.provider_account_ref != expected.provider_account_ref {
        MarketingProviderCallbackError::BindingMismatch {
            field: "provider_account_ref",
        }
    } else if actual.holding_id != expected.holding_id {
        MarketingProviderCallbackError::BindingMismatch {
            field: "holding_id",
        }
    } else {
        MarketingProviderCallbackError::BindingMismatch { field: "scope" }
    }
}

fn scope_holding_id(scope: &AuthorityScope) -> Option<Uuid> {
    match scope {
        AuthorityScope::Platform => None,
        AuthorityScope::Holding { holding_id }
        | AuthorityScope::Company { holding_id, .. }
        | AuthorityScope::Branch { holding_id, .. }
        | AuthorityScope::Subtree { holding_id, .. } => Some(*holding_id),
    }
}

fn scope_key(scope: &AuthorityScope) -> String {
    match scope {
        AuthorityScope::Platform => "platform".to_owned(),
        AuthorityScope::Holding { holding_id } => format!("holding:{holding_id}"),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => format!("company:{holding_id}:{company_id}"),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("branch:{holding_id}:{company_id}:{branch_id}"),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => format!("subtree:{holding_id}:{organization_node_id}"),
    }
}

fn timestamp_key(value: DateTime<Utc>) -> String {
    format!("{}:{}", value.timestamp(), value.timestamp_subsec_nanos())
}

fn digest(parts: &[&str]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.finalize().into()
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), MarketingProviderCallbackError> {
    if id.is_nil() {
        Err(MarketingProviderCallbackError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn bounded(
    value: &str,
    field: &'static str,
    max: usize,
) -> Result<(), MarketingProviderCallbackError> {
    if value.trim().is_empty() {
        return Err(MarketingProviderCallbackError::Required { field });
    }
    if value.len() > max {
        return Err(MarketingProviderCallbackError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(MarketingProviderCallbackError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketing_delivery::{ChannelDispatchRecord, DeliveryState, RetryPolicy};

    fn ids() -> (Uuid, Uuid, Uuid, Uuid) {
        (
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
        )
    }

    fn binding() -> MarketingProviderCallbackBinding {
        let (_, holding_id, company_id, _) = ids();
        MarketingProviderCallbackBinding::new(
            "provider-account-1",
            holding_id,
            AuthorityScope::Company {
                holding_id,
                company_id,
            },
        )
        .expect("valid binding")
    }

    fn authentication() -> ProviderAuthenticationDecision {
        ProviderAuthenticationDecision::Verified(
            VerifiedProviderAuthentication::new("hmac-sha256", "key-ref-1", [1; 32])
                .expect("valid authentication"),
        )
    }

    fn request(at: DateTime<Utc>) -> MarketingProviderCallbackRequest {
        let (delivery_id, _, _, _) = ids();
        MarketingProviderCallbackRequest::new(
            binding(),
            delivery_id,
            "provider-message-1",
            "provider-event-1",
            at,
            MarketingProviderCallbackKind::Delivered,
            authentication(),
        )
        .expect("valid request")
    }

    fn verified_callback(at: DateTime<Utc>) -> MarketingProviderCallback {
        let received_at = at + Duration::seconds(1);
        request(at)
            .verify(&binding(), received_at)
            .expect("valid callback")
    }

    #[test]
    fn unverified_callbacks_are_rejected_before_key_derivation() {
        let now = Utc::now();
        let mut value = request(now);
        value.authentication = ProviderAuthenticationDecision::NotVerified;

        assert_eq!(
            value.verify(&binding(), now),
            Err(MarketingProviderCallbackError::AuthenticationRequired)
        );
    }

    #[test]
    fn provider_account_holding_holding_and_scope_must_match_trusted_binding() {
        let now = Utc::now();
        let expected = binding();
        for (field, mut actual) in [
            ("provider_account_ref", expected.clone()),
            ("holding_id", expected.clone()),
            ("scope", expected.clone()),
        ] {
            match field {
                "provider_account_ref" => actual.provider_account_ref = "other-account".into(),
                "holding_id" => {
                    actual.holding_id = Uuid::from_u128(999);
                    actual.scope = AuthorityScope::Company {
                        holding_id: actual.holding_id,
                        company_id: Uuid::from_u128(2),
                    };
                }
                "scope" => {
                    actual.scope = AuthorityScope::Holding {
                        holding_id: actual.holding_id,
                    }
                }
                _ => unreachable!(),
            }
            let mut value = request(now);
            value.binding = actual;
            assert_eq!(
                value.verify(&expected, now),
                Err(MarketingProviderCallbackError::BindingMismatch { field })
            );
        }
    }

    #[test]
    fn inbox_key_and_fingerprint_make_exact_duplicate_identity_deterministic() {
        let now = Utc::now();
        let first = verified_callback(now);
        let duplicate = verified_callback(now);

        assert_eq!(first.idempotency, duplicate.idempotency);
        assert_eq!(
            duplicate.classify_against(Some(&first.watermark())),
            MarketingProviderCallbackOrder::Duplicate
        );
        assert_ne!(first.idempotency.inbox_key.as_str(), "");
        assert_ne!(
            first.idempotency.delivery_idempotency_key.as_str(),
            first.idempotency.inbox_key.as_str()
        );
    }

    #[test]
    fn timestamp_window_is_lower_inclusive_and_upper_exclusive() {
        let received_at = DateTime::parse_from_rfc3339("2026-07-25T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let (lower, upper) = MarketingProviderCallbackTimestampPolicy.bounds(received_at);

        assert!(MarketingProviderCallbackTimestampPolicy.contains(lower, received_at));
        assert!(!MarketingProviderCallbackTimestampPolicy.contains(upper, received_at));
        assert!(request(lower).verify(&binding(), received_at).is_ok());
        assert_eq!(
            request(upper).verify(&binding(), received_at),
            Err(MarketingProviderCallbackError::TimestampOutsideWindow)
        );
    }

    #[test]
    fn callback_binds_to_the_exact_stored_provider_reference_and_delivery() {
        let now = Utc::now();
        let callback = verified_callback(now);
        let (delivery_id, _, company_id, branch_id) = ids();
        let mut delivery = DeliveryDispatch::new(
            delivery_id,
            binding().scope.clone(),
            Uuid::from_u128(10),
            Uuid::from_u128(11),
            crate::marketing::MarketingPurpose::Newsletter,
            crate::marketing::MarketingChannel::Email,
            ChannelDispatchRecord::Email {
                recipient: "person@example.test".into(),
                template_ref: "template-1".into(),
            },
            "message-1",
            1,
            now,
            now + Duration::hours(1),
            RetryPolicy::default(),
        )
        .expect("valid delivery");
        delivery.provider_message_ref = Some(callback.provider_message_ref.clone());
        delivery.state = DeliveryState::ProviderAccepted;
        assert!(callback.bind_to_delivery(&delivery).is_ok());

        delivery.provider_message_ref = Some("other-provider-message".into());
        assert_eq!(
            callback.bind_to_delivery(&delivery),
            Err(MarketingProviderCallbackError::ProviderReferenceMismatch)
        );

        let _ = company_id;
        let _ = branch_id;
    }

    #[test]
    fn bounds_and_errors_never_echo_provider_values() {
        let now = Utc::now();
        let mut value = request(now);
        value.provider_event_id = "x".repeat(MAX_CALLBACK_REFERENCE_BYTES + 1);
        let error = value.verify(&binding(), now).expect_err("bounded event id");
        assert_eq!(
            error,
            MarketingProviderCallbackError::TooLong {
                field: "provider_event_id",
                max: MAX_CALLBACK_REFERENCE_BYTES,
            }
        );
        assert!(
            !error
                .to_string()
                .contains(&"x".repeat(MAX_CALLBACK_REFERENCE_BYTES + 1))
        );

        let mut auth = authentication();
        if let ProviderAuthenticationDecision::Verified(value) = &mut auth {
            value.scheme = "scheme\nwith-control".into();
        }
        let error = request(now)
            .verify(&binding(), now)
            .expect("request auth is still valid");
        assert!(error.authentication.scheme.contains("hmac"));
        assert_eq!(
            VerifiedProviderAuthentication::new("", "key-ref", [1; 32]).expect_err("empty scheme"),
            MarketingProviderCallbackError::Required {
                field: "authentication_scheme",
            }
        );
        let _ = auth;
    }

    #[test]
    fn event_ordering_is_deterministic_and_does_not_apply_stale_events() {
        let base = DateTime::parse_from_rfc3339("2026-07-25T12:00:00Z")
            .expect("timestamp")
            .with_timezone(&Utc);
        let first = verified_callback(base);
        let mut later_request = request(base + Duration::seconds(2));
        later_request.provider_event_id = "provider-event-2".into();
        let later = later_request
            .verify(&binding(), base + Duration::seconds(3))
            .expect("valid later callback");
        assert_eq!(
            later.classify_against(Some(&first.watermark())),
            MarketingProviderCallbackOrder::InOrder
        );

        let mut older_request = request(base - Duration::seconds(2));
        older_request.provider_event_id = "provider-event-0".into();
        let older = older_request
            .verify(&binding(), base)
            .expect("valid older callback");
        assert_eq!(
            older.classify_against(Some(&first.watermark())),
            MarketingProviderCallbackOrder::OutOfOrder
        );

        let mut conflict = request(base);
        conflict.provider_message_ref = "other-message".into();
        let conflict = conflict
            .verify(&binding(), base + Duration::seconds(1))
            .expect("valid conflicting event");
        assert_eq!(
            conflict.classify_against(Some(&first.watermark())),
            MarketingProviderCallbackOrder::ConflictingDuplicate
        );
    }
}
