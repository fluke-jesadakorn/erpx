//! Provider-response boundary for Marketing delivery.
//!
//! Provider acceptance is useful only when it is bound to the exact local
//! delivery attempt that made the request. This module consumes a bounded
//! response envelope and returns only the typed provider result; raw provider
//! payloads and failure details do not cross the boundary.

use erpx_domain::marketing_delivery::{AttemptAuthorization, DeliveryDispatch, ProviderResult};
use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;

pub const MAX_MARKETING_PROVIDER_RESPONSE_BYTES: usize = 16 * 1024;
pub const MAX_MARKETING_PROVIDER_MESSAGE_REFERENCE_BYTES: usize = 255;

/// Redacted failures for the provider response boundary. None of the variants
/// retain provider payloads, message references, request identities, or
/// credentials.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MarketingProviderBoundaryError {
    #[error("provider request identity is invalid")]
    InvalidRequestIdentity,
    #[error("provider response exceeds the bounded limit")]
    ResponseTooLarge,
    #[error("provider response is invalid")]
    InvalidResponse,
    #[error("provider response identity does not match the authorized attempt")]
    ResponseIdentityMismatch,
    #[error("provider message reference is invalid")]
    InvalidProviderMessageReference,
}

#[derive(Debug, Deserialize)]
struct ProviderAcceptanceEnvelope {
    dispatch_id: Uuid,
    attempt: u32,
    idempotency_key: String,
    provider_message_ref: String,
}

/// Binds a provider acceptance to the exact dispatch, attempt, and
/// idempotency key authorized by the durable delivery record.
///
/// Callers must invoke this only after they have classified the HTTP status as
/// successful. The parser intentionally accepts no provider-specific response
/// shape: an acceptance without all local identity fields is not an acceptance
/// of this delivery attempt.
pub fn bind_provider_acceptance(
    dispatch: &DeliveryDispatch,
    authorization: &AttemptAuthorization,
    response_body: &[u8],
) -> Result<ProviderResult, MarketingProviderBoundaryError> {
    if authorization.dispatch_id != dispatch.id
        || authorization.attempt == 0
        || authorization.attempt != dispatch.attempt_count
        || authorization.idempotency_key != dispatch.idempotency_key
    {
        return Err(MarketingProviderBoundaryError::InvalidRequestIdentity);
    }
    if response_body.len() > MAX_MARKETING_PROVIDER_RESPONSE_BYTES {
        return Err(MarketingProviderBoundaryError::ResponseTooLarge);
    }
    let response: ProviderAcceptanceEnvelope = serde_json::from_slice(response_body)
        .map_err(|_| MarketingProviderBoundaryError::InvalidResponse)?;
    if response.dispatch_id != dispatch.id
        || response.attempt != authorization.attempt
        || response.idempotency_key != authorization.idempotency_key.as_str()
    {
        return Err(MarketingProviderBoundaryError::ResponseIdentityMismatch);
    }
    if !valid_provider_message_reference(&response.provider_message_ref) {
        return Err(MarketingProviderBoundaryError::InvalidProviderMessageReference);
    }
    Ok(ProviderResult::Accepted {
        provider_message_ref: response.provider_message_ref,
    })
}

fn valid_provider_message_reference(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= MAX_MARKETING_PROVIDER_MESSAGE_REFERENCE_BYTES
        && !value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_message_reference_rejects_ambiguous_whitespace() {
        assert!(valid_provider_message_reference("provider-message-1"));
        assert!(!valid_provider_message_reference("provider message 1"));
        assert!(!valid_provider_message_reference("provider-message-1 "));
        assert!(!valid_provider_message_reference("\u{00a0}"));
        assert!(!valid_provider_message_reference("provider\nmessage"));
    }

    #[test]
    fn provider_message_reference_remains_bounded() {
        assert!(!valid_provider_message_reference(
            &"x".repeat(MAX_MARKETING_PROVIDER_MESSAGE_REFERENCE_BYTES + 1)
        ));
        assert!(!valid_provider_message_reference(" "));
    }
}
