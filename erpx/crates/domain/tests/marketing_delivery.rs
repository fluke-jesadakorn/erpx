use chrono::{DateTime, Duration, Utc};
use erpx_domain::{
    contracts::TenantScope,
    marketing::{MarketingChannel, MarketingPurpose},
    marketing_delivery::{
        ChannelDispatchRecord, ConsentRecheckDecision, DeliveryDispatch, DeliveryRecheckDecision,
        DeliveryRecheckResult, DeliveryState, MarketingDeliveryError, ProviderResult, RetryPolicy,
        SuppressionRecheckDecision,
    },
};
use uuid::Uuid;

fn scope() -> TenantScope {
    TenantScope::Holding {
        holding_id: Uuid::from_u128(1),
    }
}

fn queued_dispatch(now: DateTime<Utc>) -> DeliveryDispatch {
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
        "message-1",
        1,
        now,
        now + Duration::hours(1),
        RetryPolicy::default(),
    )
    .expect("valid dispatch")
}

fn accepted_dispatch(now: DateTime<Utc>) -> DeliveryDispatch {
    let mut delivery = queued_dispatch(now);
    let decision = DeliveryRecheckDecision {
        checked_at: now,
        consent: ConsentRecheckDecision::Granted,
        suppression: SuppressionRecheckDecision::Clear,
        result: DeliveryRecheckResult::Allowed,
    };
    let attempt = delivery.begin_attempt(&decision, now).expect("attempt");
    delivery
        .record_provider_result(
            attempt.attempt,
            now,
            ProviderResult::Accepted {
                provider_message_ref: "provider-message-1".into(),
            },
        )
        .expect("provider acceptance");
    delivery
}

#[test]
fn provider_callbacks_cannot_move_delivery_backwards_in_time() {
    let accepted_at = DateTime::parse_from_rfc3339("2026-07-29T00:00:00Z")
        .expect("timestamp")
        .with_timezone(&Utc);
    let before_acceptance = accepted_at - Duration::nanoseconds(1);

    let mut delivered = accepted_dispatch(accepted_at);
    let before_delivered = delivered.clone();
    assert_eq!(
        delivered.mark_delivered_for_provider("provider-message-1", before_acceptance),
        Err(MarketingDeliveryError::ProviderCallbackBeforeAcceptance)
    );
    assert_eq!(delivered, before_delivered);

    let mut failed = accepted_dispatch(accepted_at);
    let before_failed = failed.clone();
    assert_eq!(
        failed.fail_for_provider("provider-message-1", "provider_bounced", before_acceptance,),
        Err(MarketingDeliveryError::ProviderCallbackBeforeAcceptance)
    );
    assert_eq!(failed, before_failed);
    assert_eq!(failed.state, DeliveryState::ProviderAccepted);
}
