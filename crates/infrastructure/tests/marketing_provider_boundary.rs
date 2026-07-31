#[path = "../src/marketing_provider.rs"]
mod marketing_provider;

use chrono::{Duration, Utc};
use erpx_domain::{
    contracts::TenantScope,
    marketing::{MarketingChannel, MarketingPurpose},
    marketing_delivery::{
        AttemptAuthorization, ChannelDispatchRecord, ConsentRecheckDecision, DeliveryDispatch,
        DeliveryRecheckDecision, DeliveryRecheckResult, ProviderResult, RetryPolicy,
        SuppressionRecheckDecision,
    },
};
use marketing_provider::{
    MAX_MARKETING_PROVIDER_RESPONSE_BYTES, MarketingProviderBoundaryError, bind_provider_acceptance,
};
use serde_json::json;
use uuid::Uuid;

fn authorized_attempt() -> (DeliveryDispatch, AttemptAuthorization) {
    let now = Utc::now();
    let mut dispatch = DeliveryDispatch::new(
        Uuid::new_v4(),
        TenantScope::Holding {
            holding_id: Uuid::new_v4(),
        },
        Uuid::new_v4(),
        Uuid::new_v4(),
        MarketingPurpose::Transactional,
        MarketingChannel::Email,
        ChannelDispatchRecord::Email {
            recipient: "recipient@example.test".into(),
            template_ref: "template-1".into(),
        },
        "message-1",
        1,
        now,
        now + Duration::hours(1),
        RetryPolicy::default(),
    )
    .expect("valid dispatch");
    let authorization = dispatch
        .begin_attempt(
            &DeliveryRecheckDecision {
                checked_at: now,
                consent: ConsentRecheckDecision::Granted,
                suppression: SuppressionRecheckDecision::Clear,
                result: DeliveryRecheckResult::Allowed,
            },
            now,
        )
        .expect("authorized attempt");
    (dispatch, authorization)
}

fn acceptance_body(dispatch: &DeliveryDispatch, authorization: &AttemptAuthorization) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "dispatch_id": dispatch.id,
        "attempt": authorization.attempt,
        "idempotency_key": authorization.idempotency_key.as_str(),
        "provider_message_ref": "provider-message-1"
    }))
    .expect("serializable response")
}

#[test]
fn acceptance_requires_exact_dispatch_attempt_and_idempotency_identity() {
    let (dispatch, authorization) = authorized_attempt();
    let result = bind_provider_acceptance(
        &dispatch,
        &authorization,
        &acceptance_body(&dispatch, &authorization),
    )
    .expect("matching provider acceptance");
    assert_eq!(
        result,
        ProviderResult::Accepted {
            provider_message_ref: "provider-message-1".into(),
        }
    );
}

#[test]
fn stale_or_cross_delivery_response_identity_is_rejected() {
    let (dispatch, authorization) = authorized_attempt();
    let cases = [
        json!({
            "dispatch_id": Uuid::new_v4(),
            "attempt": authorization.attempt,
            "idempotency_key": authorization.idempotency_key.as_str(),
            "provider_message_ref": "provider-message-1"
        }),
        json!({
            "dispatch_id": dispatch.id,
            "attempt": authorization.attempt + 1,
            "idempotency_key": authorization.idempotency_key.as_str(),
            "provider_message_ref": "provider-message-1"
        }),
        json!({
            "dispatch_id": dispatch.id,
            "attempt": authorization.attempt,
            "idempotency_key": "mdk-from-another-dispatch",
            "provider_message_ref": "provider-message-1"
        }),
    ];

    for response in cases {
        let error = bind_provider_acceptance(
            &dispatch,
            &authorization,
            &serde_json::to_vec(&response).expect("serializable response"),
        )
        .expect_err("mismatched response identity");
        assert_eq!(
            error,
            MarketingProviderBoundaryError::ResponseIdentityMismatch
        );
    }
}

#[test]
fn acceptance_without_identity_or_with_an_invalid_reference_fails_closed() {
    let (dispatch, authorization) = authorized_attempt();
    let missing_identity = serde_json::to_vec(&json!({
        "provider_message_ref": "provider-message-1"
    }))
    .expect("serializable response");
    assert_eq!(
        bind_provider_acceptance(&dispatch, &authorization, &missing_identity),
        Err(MarketingProviderBoundaryError::InvalidResponse)
    );

    for reference in ["", "   ", "provider\nmessage"] {
        let response = serde_json::to_vec(&json!({
            "dispatch_id": dispatch.id,
            "attempt": authorization.attempt,
            "idempotency_key": authorization.idempotency_key.as_str(),
            "provider_message_ref": reference
        }))
        .expect("serializable response");
        assert_eq!(
            bind_provider_acceptance(&dispatch, &authorization, &response),
            Err(MarketingProviderBoundaryError::InvalidProviderMessageReference)
        );
    }
}

#[test]
fn response_limit_and_failures_do_not_retain_provider_payloads() {
    let (dispatch, authorization) = authorized_attempt();
    let secret_like_payload = "provider-secret-like-payload";
    let oversized = vec![b'x'; MAX_MARKETING_PROVIDER_RESPONSE_BYTES + 1];
    let error = bind_provider_acceptance(&dispatch, &authorization, &oversized)
        .expect_err("oversized response");
    assert_eq!(error, MarketingProviderBoundaryError::ResponseTooLarge);
    assert!(!format!("{error}").contains(secret_like_payload));

    let malformed = format!(
        "{{\"provider_message_ref\":\"{secret_like_payload}\",\"dispatch_id\":\"not-a-uuid\"}}"
    );
    let error = bind_provider_acceptance(&dispatch, &authorization, malformed.as_bytes())
        .expect_err("malformed response");
    assert_eq!(error, MarketingProviderBoundaryError::InvalidResponse);
    assert!(!format!("{error}").contains(secret_like_payload));
}
