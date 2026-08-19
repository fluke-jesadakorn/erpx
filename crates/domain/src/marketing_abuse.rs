//! Pure policy for anonymous Marketing form abuse controls.
//!
//! The caller is responsible for deriving `request_fingerprint` before it
//! reaches this module. This contract never accepts or stores a raw IP,
//! contact value, request body, cookie, or user-agent. The policy only
//! evaluates bounded counters supplied by an adapter that owns the relevant
//! short-lived rate-limit state.

use std::fmt;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// The largest opaque fingerprint accepted by the policy boundary.
pub const MAX_MARKETING_ABUSE_FINGERPRINT_BYTES: usize = 128;
/// The shortest rate-limit window accepted by the policy.
pub const MIN_MARKETING_ABUSE_WINDOW_SECONDS: i64 = 1;
/// The largest rate-limit window accepted by the policy.
pub const MAX_MARKETING_ABUSE_WINDOW_SECONDS: i64 = 60 * 60;
/// The largest burst window accepted by the policy.
pub const MAX_MARKETING_ABUSE_BURST_WINDOW_SECONDS: i64 = 15 * 60;
/// The largest configured requests-per-window limit.
pub const MAX_MARKETING_ABUSE_REQUESTS_PER_WINDOW: u32 = 10_000;
/// The largest configured requests-per-burst limit.
pub const MAX_MARKETING_ABUSE_REQUESTS_PER_BURST: u32 = 1_000;
/// The largest counter an adapter may present to the pure policy.
pub const MAX_MARKETING_ABUSE_OBSERVED_REQUESTS: u32 = 1_000_000;
/// A conservative default window for public form submissions.
pub const DEFAULT_MARKETING_ABUSE_WINDOW_SECONDS: i64 = 60;
/// A conservative default burst window for public form submissions.
pub const DEFAULT_MARKETING_ABUSE_BURST_WINDOW_SECONDS: i64 = 10;
/// A conservative default sustained request limit.
pub const DEFAULT_MARKETING_ABUSE_REQUESTS_PER_WINDOW: u32 = 60;
/// A conservative default burst request limit.
pub const DEFAULT_MARKETING_ABUSE_REQUESTS_PER_BURST: u32 = 5;

/// A validated opaque request fingerprint.
///
/// The value is intentionally hidden from `Debug` output. The application
/// boundary should construct it from a keyed or otherwise privacy-preserving
/// digest; this domain type does not receive the source material.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingRequestFingerprint(String);

impl MarketingRequestFingerprint {
    pub fn new(value: impl Into<String>) -> Result<Self, MarketingAbusePolicyError> {
        let value = value.into();
        validate_fingerprint(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for MarketingRequestFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("MarketingRequestFingerprint(REDACTED)")
    }
}

/// Signals from the ingress/challenge adapter that affect the decision.
///
/// `ChallengeRequired` is an instruction to the caller to obtain a valid
/// challenge response and retry. `ChallengePassed` permits normal policy
/// evaluation; it does not bypass binding or rate limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingAbuseSignal {
    HoneypotTriggered,
    ChallengeRequired,
    ChallengeFailed,
    ChallengePassed,
}

/// Stable reasons for a deterministic rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingAbuseRejectReason {
    HoneypotTriggered,
    ChallengeFailed,
}

/// Stable reasons for a deterministic challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketingAbuseChallengeReason {
    IngressChallengeRequired,
}

/// The only outcomes an anonymous form ingress may receive from this policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum MarketingAbuseDecision {
    Allow,
    Reject {
        reason: MarketingAbuseRejectReason,
    },
    RateLimited {
        retry_after_seconds: i64,
    },
    Challenge {
        reason: MarketingAbuseChallengeReason,
    },
}

/// A bounded request observation supplied by the rate-limit adapter.
///
/// The counters represent requests already observed in the active windows;
/// the policy does not mutate or persist those counters.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnonymousMarketingAbuseInput {
    pub holding_id: Uuid,
    pub form_id: Uuid,
    pub domain_id: Uuid,
    pub request_fingerprint: MarketingRequestFingerprint,
    pub received_at: DateTime<Utc>,
    pub requests_in_window: u32,
    pub requests_in_burst: u32,
    pub signal: Option<MarketingAbuseSignal>,
}

impl fmt::Debug for AnonymousMarketingAbuseInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnonymousMarketingAbuseInput")
            .field("holding_id", &self.holding_id)
            .field("form_id", &self.form_id)
            .field("domain_id", &self.domain_id)
            .field("request_fingerprint", &"REDACTED")
            .field("received_at", &self.received_at)
            .field("requests_in_window", &self.requests_in_window)
            .field("requests_in_burst", &self.requests_in_burst)
            .field("signal", &self.signal)
            .finish()
    }
}

impl AnonymousMarketingAbuseInput {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        holding_id: Uuid,
        form_id: Uuid,
        domain_id: Uuid,
        request_fingerprint: impl Into<String>,
        received_at: DateTime<Utc>,
        requests_in_window: u32,
        requests_in_burst: u32,
        signal: Option<MarketingAbuseSignal>,
    ) -> Result<Self, MarketingAbusePolicyError> {
        let value = Self {
            holding_id,
            form_id,
            domain_id,
            request_fingerprint: MarketingRequestFingerprint::new(request_fingerprint)?,
            received_at,
            requests_in_window,
            requests_in_burst,
            signal,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), MarketingAbusePolicyError> {
        require_identifier(self.holding_id, "holding_id")?;
        require_identifier(self.form_id, "form_id")?;
        require_identifier(self.domain_id, "domain_id")?;
        validate_fingerprint(self.request_fingerprint.as_str())?;
        if self.received_at.timestamp_nanos_opt().is_none() {
            return Err(MarketingAbusePolicyError::InvalidTimestamp);
        }
        if self.requests_in_window > MAX_MARKETING_ABUSE_OBSERVED_REQUESTS
            || self.requests_in_burst > MAX_MARKETING_ABUSE_OBSERVED_REQUESTS
        {
            return Err(MarketingAbusePolicyError::ObservedCounterTooLarge);
        }
        Ok(())
    }
}

/// Immutable configuration for one holding/form/domain submission boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnonymousMarketingAbusePolicy {
    holding_id: Uuid,
    form_id: Uuid,
    domain_id: Uuid,
    window: Duration,
    burst_window: Duration,
    requests_per_window: u32,
    requests_per_burst: u32,
}

impl AnonymousMarketingAbusePolicy {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        holding_id: Uuid,
        form_id: Uuid,
        domain_id: Uuid,
        window: Duration,
        burst_window: Duration,
        requests_per_window: u32,
        requests_per_burst: u32,
    ) -> Result<Self, MarketingAbusePolicyError> {
        let value = Self {
            holding_id,
            form_id,
            domain_id,
            window,
            burst_window,
            requests_per_window,
            requests_per_burst,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn standard(
        holding_id: Uuid,
        form_id: Uuid,
        domain_id: Uuid,
    ) -> Result<Self, MarketingAbusePolicyError> {
        Self::new(
            holding_id,
            form_id,
            domain_id,
            Duration::seconds(DEFAULT_MARKETING_ABUSE_WINDOW_SECONDS),
            Duration::seconds(DEFAULT_MARKETING_ABUSE_BURST_WINDOW_SECONDS),
            DEFAULT_MARKETING_ABUSE_REQUESTS_PER_WINDOW,
            DEFAULT_MARKETING_ABUSE_REQUESTS_PER_BURST,
        )
    }

    pub fn window(&self) -> Duration {
        self.window
    }

    pub fn burst_window(&self) -> Duration {
        self.burst_window
    }

    pub fn requests_per_window(&self) -> u32 {
        self.requests_per_window
    }

    pub fn requests_per_burst(&self) -> u32 {
        self.requests_per_burst
    }

    pub fn validate(&self) -> Result<(), MarketingAbusePolicyError> {
        require_identifier(self.holding_id, "holding_id")?;
        require_identifier(self.form_id, "form_id")?;
        require_identifier(self.domain_id, "domain_id")?;
        validate_duration(
            self.window,
            MIN_MARKETING_ABUSE_WINDOW_SECONDS,
            MAX_MARKETING_ABUSE_WINDOW_SECONDS,
            "window",
        )?;
        validate_duration(
            self.burst_window,
            MIN_MARKETING_ABUSE_WINDOW_SECONDS,
            MAX_MARKETING_ABUSE_BURST_WINDOW_SECONDS,
            "burst_window",
        )?;
        if self.burst_window > self.window {
            return Err(MarketingAbusePolicyError::BurstWindowExceedsWindow);
        }
        if self.requests_per_window == 0
            || self.requests_per_window > MAX_MARKETING_ABUSE_REQUESTS_PER_WINDOW
        {
            return Err(MarketingAbusePolicyError::InvalidRequestLimit {
                field: "requests_per_window",
            });
        }
        if self.requests_per_burst == 0
            || self.requests_per_burst > MAX_MARKETING_ABUSE_REQUESTS_PER_BURST
            || self.requests_per_burst > self.requests_per_window
        {
            return Err(MarketingAbusePolicyError::InvalidRequestLimit {
                field: "requests_per_burst",
            });
        }
        Ok(())
    }

    /// Evaluates a bounded observation without consulting time, storage, or
    /// external state. Equal inputs always produce equal decisions.
    pub fn evaluate(
        &self,
        input: &AnonymousMarketingAbuseInput,
    ) -> Result<MarketingAbuseDecision, MarketingAbusePolicyError> {
        self.validate()?;
        input.validate()?;
        self.validate_binding(input)?;

        match input.signal {
            Some(MarketingAbuseSignal::HoneypotTriggered) => {
                return Ok(MarketingAbuseDecision::Reject {
                    reason: MarketingAbuseRejectReason::HoneypotTriggered,
                });
            }
            Some(MarketingAbuseSignal::ChallengeFailed) => {
                return Ok(MarketingAbuseDecision::Reject {
                    reason: MarketingAbuseRejectReason::ChallengeFailed,
                });
            }
            Some(MarketingAbuseSignal::ChallengeRequired) => {
                return Ok(MarketingAbuseDecision::Challenge {
                    reason: MarketingAbuseChallengeReason::IngressChallengeRequired,
                });
            }
            None | Some(MarketingAbuseSignal::ChallengePassed) => {}
        }

        if input.requests_in_burst >= self.requests_per_burst {
            return Ok(MarketingAbuseDecision::RateLimited {
                retry_after_seconds: self.burst_window.num_seconds(),
            });
        }
        if input.requests_in_window >= self.requests_per_window {
            return Ok(MarketingAbuseDecision::RateLimited {
                retry_after_seconds: self.window.num_seconds(),
            });
        }
        Ok(MarketingAbuseDecision::Allow)
    }

    fn validate_binding(
        &self,
        input: &AnonymousMarketingAbuseInput,
    ) -> Result<(), MarketingAbusePolicyError> {
        for (field, expected, actual) in [
            ("holding_id", self.holding_id, input.holding_id),
            ("form_id", self.form_id, input.form_id),
            ("domain_id", self.domain_id, input.domain_id),
        ] {
            if expected != actual {
                return Err(MarketingAbusePolicyError::BindingMismatch { field });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MarketingAbusePolicyError {
    #[error("identifier cannot be nil: {field}")]
    NilIdentifier { field: &'static str },
    #[error("request fingerprint is required")]
    MissingFingerprint,
    #[error("request fingerprint exceeds {max} bytes")]
    FingerprintTooLong { max: usize },
    #[error("request fingerprint contains an unsafe character")]
    InvalidFingerprint,
    #[error("received timestamp is invalid")]
    InvalidTimestamp,
    #[error("observed request counter exceeds the bounded maximum")]
    ObservedCounterTooLarge,
    #[error("{field} is outside the supported bounds")]
    InvalidDuration { field: &'static str },
    #[error("burst window cannot exceed the sustained request window")]
    BurstWindowExceedsWindow,
    #[error("{field} is outside the supported request limit")]
    InvalidRequestLimit { field: &'static str },
    #[error("policy binding does not match the request: {field}")]
    BindingMismatch { field: &'static str },
}

fn require_identifier(value: Uuid, field: &'static str) -> Result<(), MarketingAbusePolicyError> {
    if value.is_nil() {
        Err(MarketingAbusePolicyError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn validate_fingerprint(value: &str) -> Result<(), MarketingAbusePolicyError> {
    if value.trim().is_empty() {
        return Err(MarketingAbusePolicyError::MissingFingerprint);
    }
    if value.len() > MAX_MARKETING_ABUSE_FINGERPRINT_BYTES {
        return Err(MarketingAbusePolicyError::FingerprintTooLong {
            max: MAX_MARKETING_ABUSE_FINGERPRINT_BYTES,
        });
    }
    if value.chars().any(|character| {
        character.is_control()
            || character.is_whitespace()
            || matches!(character, '@' | '+' | '/' | '?' | '#')
    }) {
        return Err(MarketingAbusePolicyError::InvalidFingerprint);
    }
    Ok(())
}

fn validate_duration(
    value: Duration,
    minimum_seconds: i64,
    maximum_seconds: i64,
    field: &'static str,
) -> Result<(), MarketingAbusePolicyError> {
    let seconds = value.num_seconds();
    if value != Duration::seconds(seconds)
        || !(minimum_seconds..=maximum_seconds).contains(&seconds)
    {
        return Err(MarketingAbusePolicyError::InvalidDuration { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (Uuid, Uuid, Uuid) {
        (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3))
    }

    fn input(signal: Option<MarketingAbuseSignal>) -> AnonymousMarketingAbuseInput {
        let (holding_id, form_id, domain_id) = ids();
        AnonymousMarketingAbuseInput::new(
            holding_id,
            form_id,
            domain_id,
            "fingerprint-opaque-001",
            DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp is valid"),
            0,
            0,
            signal,
        )
        .expect("test input is valid")
    }

    fn policy() -> AnonymousMarketingAbusePolicy {
        let (holding_id, form_id, domain_id) = ids();
        AnonymousMarketingAbusePolicy::new(
            holding_id,
            form_id,
            domain_id,
            Duration::seconds(60),
            Duration::seconds(10),
            10,
            2,
        )
        .expect("test policy is valid")
    }

    #[test]
    fn missing_and_oversized_fingerprints_are_rejected() {
        let (holding_id, form_id, domain_id) = ids();
        let timestamp = DateTime::from_timestamp(1_700_000_000, 0).expect("fixed timestamp");

        assert_eq!(
            AnonymousMarketingAbuseInput::new(
                holding_id, form_id, domain_id, "   ", timestamp, 0, 0, None,
            ),
            Err(MarketingAbusePolicyError::MissingFingerprint)
        );
        assert_eq!(
            MarketingRequestFingerprint::new("x".repeat(MAX_MARKETING_ABUSE_FINGERPRINT_BYTES + 1)),
            Err(MarketingAbusePolicyError::FingerprintTooLong {
                max: MAX_MARKETING_ABUSE_FINGERPRINT_BYTES,
            })
        );
    }

    #[test]
    fn holding_form_and_domain_bindings_are_exact() {
        let policy = policy();
        for (field, request) in [
            {
                let mut request = input(None);
                request.holding_id = Uuid::from_u128(11);
                ("holding_id", request)
            },
            {
                let mut request = input(None);
                request.form_id = Uuid::from_u128(12);
                ("form_id", request)
            },
            {
                let mut request = input(None);
                request.domain_id = Uuid::from_u128(13);
                ("domain_id", request)
            },
        ] {
            assert_eq!(
                policy.evaluate(&request),
                Err(MarketingAbusePolicyError::BindingMismatch { field })
            );
        }
    }

    #[test]
    fn policy_rejects_out_of_bounds_windows_and_limits() {
        let (holding_id, form_id, domain_id) = ids();
        let invalid_windows = [
            Duration::zero(),
            Duration::seconds(MAX_MARKETING_ABUSE_WINDOW_SECONDS + 1),
        ];
        for window in invalid_windows {
            assert_eq!(
                AnonymousMarketingAbusePolicy::new(
                    holding_id,
                    form_id,
                    domain_id,
                    window,
                    Duration::seconds(1),
                    1,
                    1,
                ),
                Err(MarketingAbusePolicyError::InvalidDuration { field: "window" })
            );
        }

        assert_eq!(
            AnonymousMarketingAbusePolicy::new(
                holding_id,
                form_id,
                domain_id,
                Duration::seconds(10),
                Duration::seconds(11),
                1,
                1,
            ),
            Err(MarketingAbusePolicyError::BurstWindowExceedsWindow)
        );
        assert_eq!(
            AnonymousMarketingAbusePolicy::new(
                holding_id,
                form_id,
                domain_id,
                Duration::seconds(10),
                Duration::seconds(1),
                1,
                2,
            ),
            Err(MarketingAbusePolicyError::InvalidRequestLimit {
                field: "requests_per_burst",
            })
        );
    }

    #[test]
    fn equal_inputs_have_deterministic_decisions() {
        let policy = policy();
        let request = input(None);
        assert_eq!(policy.evaluate(&request), Ok(MarketingAbuseDecision::Allow));
        assert_eq!(policy.evaluate(&request), policy.evaluate(&request));

        let mut burst_limited = request.clone();
        burst_limited.requests_in_burst = 2;
        assert_eq!(
            policy.evaluate(&burst_limited),
            Ok(MarketingAbuseDecision::RateLimited {
                retry_after_seconds: 10,
            })
        );

        let mut window_limited = request;
        window_limited.requests_in_window = 10;
        assert_eq!(
            policy.evaluate(&window_limited),
            Ok(MarketingAbuseDecision::RateLimited {
                retry_after_seconds: 60,
            })
        );
    }

    #[test]
    fn honeypot_rejects_before_rate_limit_or_challenge() {
        let policy = policy();
        let mut request = input(Some(MarketingAbuseSignal::HoneypotTriggered));
        request.requests_in_window = 10;
        request.requests_in_burst = 2;
        assert_eq!(
            policy.evaluate(&request),
            Ok(MarketingAbuseDecision::Reject {
                reason: MarketingAbuseRejectReason::HoneypotTriggered,
            })
        );
    }

    #[test]
    fn challenge_signals_are_explicit_and_passed_challenges_do_not_bypass_limits() {
        let policy = policy();
        assert_eq!(
            policy.evaluate(&input(Some(MarketingAbuseSignal::ChallengeRequired))),
            Ok(MarketingAbuseDecision::Challenge {
                reason: MarketingAbuseChallengeReason::IngressChallengeRequired,
            })
        );
        assert_eq!(
            policy.evaluate(&input(Some(MarketingAbuseSignal::ChallengeFailed))),
            Ok(MarketingAbuseDecision::Reject {
                reason: MarketingAbuseRejectReason::ChallengeFailed,
            })
        );

        let mut passed = input(Some(MarketingAbuseSignal::ChallengePassed));
        passed.requests_in_burst = 2;
        assert_eq!(
            policy.evaluate(&passed),
            Ok(MarketingAbuseDecision::RateLimited {
                retry_after_seconds: 10,
            })
        );
    }

    #[test]
    fn debug_and_errors_do_not_expose_fingerprint_or_contact_like_values() {
        let secret = "customer@example.com";
        let fingerprint = MarketingRequestFingerprint::new("opaque-fingerprint").unwrap();
        let debug = format!("{fingerprint:?}");
        assert!(!debug.contains("opaque-fingerprint"));
        assert!(!debug.contains(secret));

        let input = AnonymousMarketingAbuseInput::new(
            ids().0,
            ids().1,
            ids().2,
            "opaque-fingerprint",
            DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            0,
            0,
            None,
        )
        .unwrap();
        let debug = format!("{input:?}");
        assert!(!debug.contains("opaque-fingerprint"));
        assert!(!debug.contains(secret));
        let error = MarketingRequestFingerprint::new(secret).unwrap_err();
        assert!(!error.to_string().contains(secret));
    }
}
