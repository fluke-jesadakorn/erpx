use chrono::{Duration, TimeZone, Utc};
use erpx_integration_contracts::CurrencyCode;
use erpx_kernel::TenantScope;
use uuid::Uuid;

use super::*;

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn at() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap()
}
fn currency() -> CurrencyCode {
    CurrencyCode::try_new("THB").unwrap()
}
fn scope() -> TenantScope {
    TenantScope::Holding { holding_id: id(1) }
}

#[test]
fn catalog_plan_is_platform_scoped_and_requires_positive_price() {
    let result = CatalogPlan::create(
        CatalogPlanId::try_from_uuid(id(10)).unwrap(),
        scope(),
        "growth".into(),
        "Growth".into(),
        0,
        currency(),
        at(),
    );
    assert_eq!(
        result.unwrap_err(),
        CommercialDomainError::InvalidCatalogScope
    );
}

#[test]
fn published_catalog_plan_is_immutable_by_lifecycle() {
    let mut plan = CatalogPlan::create(
        CatalogPlanId::try_from_uuid(id(10)).unwrap(),
        TenantScope::Platform,
        "growth".into(),
        "Growth".into(),
        50_000,
        currency(),
        at(),
    )
    .unwrap();
    plan.publish(at() + Duration::seconds(1)).unwrap();
    assert_eq!(plan.version(), 2);
    assert_eq!(
        plan.publish(at() + Duration::seconds(2)),
        Err(CommercialDomainError::InvalidCatalogTransition)
    );
}

#[test]
fn subscription_accepts_only_valid_ordered_provider_transitions() {
    let mut subscription = Subscription::create(
        SubscriptionId::try_from_uuid(id(20)).unwrap(),
        scope(),
        id(21),
        CatalogPlanId::try_from_uuid(id(10)).unwrap(),
        50_000,
        currency(),
        true,
        at(),
    )
    .unwrap();
    subscription
        .apply_verified_billing_event(
            "evt-1".into(),
            at() + Duration::seconds(1),
            BillingSignal::TrialEnded,
        )
        .unwrap();
    assert_eq!(subscription.status(), SubscriptionStatus::Active);
    assert_eq!(
        subscription.apply_verified_billing_event(
            "evt-old".into(),
            at(),
            BillingSignal::PaymentFailed
        ),
        Err(CommercialDomainError::StaleProviderEvent)
    );
    assert_eq!(
        subscription.apply_verified_billing_event(
            "evt-invalid".into(),
            at() + Duration::seconds(2),
            BillingSignal::TrialEnded
        ),
        Err(CommercialDomainError::InvalidSubscriptionTransition {
            from: SubscriptionStatus::Active,
            signal: BillingSignal::TrialEnded
        })
    );
}

#[test]
fn subscription_enforces_seven_day_grace_before_read_only_suspension() {
    let mut subscription = Subscription::create(
        SubscriptionId::try_from_uuid(id(22)).unwrap(),
        scope(),
        id(23),
        CatalogPlanId::try_from_uuid(id(10)).unwrap(),
        50_000,
        currency(),
        false,
        at(),
    )
    .unwrap();
    let failed_at = at() + Duration::minutes(1);
    subscription
        .apply_verified_billing_event(
            "evt-payment-failed".into(),
            failed_at,
            BillingSignal::PaymentFailed,
        )
        .unwrap();
    assert_eq!(
        subscription.grace_expires_at(),
        Some(failed_at + Duration::days(7))
    );
    assert_eq!(
        subscription.apply_verified_billing_event(
            "evt-grace-early".into(),
            failed_at + Duration::days(6),
            BillingSignal::GraceExpired,
        ),
        Err(CommercialDomainError::GracePeriodNotExpired)
    );
    subscription
        .apply_verified_billing_event(
            "evt-grace-expired".into(),
            failed_at + Duration::days(7),
            BillingSignal::GraceExpired,
        )
        .unwrap();
    assert_eq!(subscription.status(), SubscriptionStatus::Suspended);
    assert_eq!(subscription.grace_expires_at(), None);
}

#[test]
fn entitlement_rejects_capacity_reduction_below_allocated_usage() {
    let mut entitlement = Entitlement::create(
        EntitlementId::try_from_uuid(id(30)).unwrap(),
        scope(),
        SubscriptionId::try_from_uuid(id(20)).unwrap(),
        "finance".into(),
        true,
        EntitlementCapacity {
            seats: 10,
            companies: 2,
            branches: 4,
        },
        at(),
    )
    .unwrap();
    entitlement
        .allocate(
            EntitlementCapacity {
                seats: 8,
                companies: 1,
                branches: 3,
            },
            at() + Duration::seconds(1),
        )
        .unwrap();
    let error = entitlement
        .apply_verified_change(
            "evt-capacity".into(),
            at() + Duration::seconds(2),
            true,
            EntitlementCapacity {
                seats: 7,
                companies: 2,
                branches: 4,
            },
        )
        .unwrap_err();
    assert_eq!(error, CommercialDomainError::CapacityBelowAllocated);
}

#[test]
fn disabled_entitlement_refuses_allocation() {
    let mut entitlement = Entitlement::create(
        EntitlementId::try_from_uuid(id(31)).unwrap(),
        scope(),
        SubscriptionId::try_from_uuid(id(20)).unwrap(),
        "finance".into(),
        false,
        EntitlementCapacity {
            seats: 10,
            companies: 2,
            branches: 4,
        },
        at(),
    )
    .unwrap();
    assert_eq!(
        entitlement.allocate(
            EntitlementCapacity {
                seats: 1,
                companies: 0,
                branches: 0
            },
            at()
        ),
        Err(CommercialDomainError::EntitlementDisabled)
    );
}
