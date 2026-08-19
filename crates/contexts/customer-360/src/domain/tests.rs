use chrono::{TimeZone, Utc};
use erpx_integration_contracts::{
    CurrencyCode, DisplayName, MoneyAmount, ReferenceCode,
    commercial::{PlanRef, SubscriptionFact, SubscriptionRef, SubscriptionStatus},
    customer::{Customer360SourceFact, CustomerAccountRef, CustomerProfileFact, CustomerStatus},
    organization::CompanyRef,
};
use erpx_kernel::EventId;
use uuid::Uuid;

use super::*;

fn uuid(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn customer(value: u128) -> CustomerRef {
    CustomerRef::try_from_uuid(uuid(value)).unwrap()
}

fn position(event: u128, version: u64, sequence: u64) -> ProjectionPosition {
    ProjectionPosition {
        event_id: EventId::try_from_uuid(uuid(event)).unwrap(),
        aggregate_version: version,
        global_sequence: sequence,
        occurred_at: Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap(),
    }
}

fn profile(customer: CustomerRef, name: &str) -> Customer360SourceFact {
    Customer360SourceFact::Customer(CustomerProfileFact {
        customer,
        account: CustomerAccountRef::try_from_uuid(uuid(11)).unwrap(),
        customer_code: ReferenceCode::try_new("CUS-10").unwrap(),
        display_name: DisplayName::try_new(name).unwrap(),
        owning_company: CompanyRef::try_from_uuid(uuid(12)).unwrap(),
        status: CustomerStatus::Active,
    })
}

fn subscription(customer: CustomerRef, version_seed: u128) -> Customer360SourceFact {
    Customer360SourceFact::Commercial(SubscriptionFact {
        subscription: SubscriptionRef::try_from_uuid(uuid(version_seed)).unwrap(),
        customer,
        plan: PlanRef::try_from_uuid(uuid(version_seed + 100)).unwrap(),
        status: SubscriptionStatus::Active,
        recurring_amount: MoneyAmount::new(1_000, CurrencyCode::try_new("THB").unwrap()),
        effective_at: Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).single().unwrap(),
    })
}

#[test]
fn rebuild_is_order_independent_and_exposes_only_observed_sources() {
    let id = customer(10);
    let profile_input = Customer360ProjectionInput::try_new(
        Customer360ProjectionStream::ControlCustomers,
        position(1, 1, 1),
        profile(id, "Acme"),
    )
    .unwrap();
    let subscription_input = Customer360ProjectionInput::try_new(
        Customer360ProjectionStream::HoldingActivity,
        position(2, 1, 2),
        subscription(id, 20),
    )
    .unwrap();

    let projection =
        Customer360Projection::rebuild(id, [subscription_input.clone(), profile_input.clone()])
            .unwrap();
    let reversed = Customer360Projection::rebuild(id, [profile_input, subscription_input]).unwrap();
    assert_eq!(projection, reversed);

    let view = projection.view();
    assert_eq!(view.subscriptions.len(), 1);
    assert_eq!(view.availability.overview, SectionAvailability::Ready);
    assert_eq!(view.availability.commercial, SectionAvailability::Ready);
    assert_eq!(
        view.availability.relationship_activity,
        SectionAvailability::AwaitingSource
    );
    assert_eq!(
        view.availability.organization,
        SectionAvailability::NotProjected
    );
    assert_eq!(
        view.availability.entitlements,
        SectionAvailability::NotProjected
    );
    assert_eq!(view.availability.notes, SectionAvailability::NotProjected);
}

#[test]
fn duplicate_out_of_order_and_stale_owner_versions_do_not_replace_current_fact() {
    let id = customer(10);
    let mut projection = Customer360Projection::new(id);
    let current = Customer360ProjectionInput::try_new(
        Customer360ProjectionStream::ControlCustomers,
        position(2, 2, 20),
        profile(id, "Current"),
    )
    .unwrap();
    assert_eq!(
        projection.apply(current.clone()).unwrap(),
        ProjectionApplyOutcome::Applied
    );
    assert_eq!(
        projection.apply(current).unwrap(),
        ProjectionApplyOutcome::Duplicate
    );

    let old_position = Customer360ProjectionInput::try_new(
        Customer360ProjectionStream::ControlCustomers,
        position(1, 1, 10),
        profile(id, "Old"),
    )
    .unwrap();
    assert_eq!(
        projection.apply(old_position).unwrap(),
        ProjectionApplyOutcome::IgnoredOutOfOrder
    );

    let stale_version = Customer360ProjectionInput::try_new(
        Customer360ProjectionStream::ControlCustomers,
        position(3, 1, 30),
        profile(id, "Stale"),
    )
    .unwrap();
    assert_eq!(
        projection.apply(stale_version).unwrap(),
        ProjectionApplyOutcome::IgnoredStaleSourceVersion
    );
    assert_eq!(
        projection.view().profile.unwrap().display_name.as_str(),
        "Current"
    );
    assert_eq!(
        projection
            .last_position(Customer360ProjectionStream::ControlCustomers)
            .unwrap()
            .global_sequence,
        30
    );
}

#[test]
fn facts_for_another_customer_fail_closed_without_advancing_checkpoint() {
    let mut projection = Customer360Projection::new(customer(10));
    let input = Customer360ProjectionInput::try_new(
        Customer360ProjectionStream::HoldingActivity,
        position(1, 1, 1),
        subscription(customer(99), 20),
    )
    .unwrap();
    assert_eq!(
        projection.apply(input),
        Err(Customer360DomainError::CustomerMismatch)
    );
    assert_eq!(
        projection.last_position(Customer360ProjectionStream::HoldingActivity),
        None
    );
    assert_eq!(projection.view().subscriptions, Vec::new());
}

#[test]
fn conflicting_global_positions_fail_instead_of_hiding_data_loss() {
    let id = customer(10);
    let mut projection = Customer360Projection::new(id);
    projection
        .apply(
            Customer360ProjectionInput::try_new(
                Customer360ProjectionStream::ControlCustomers,
                position(1, 1, 1),
                profile(id, "One"),
            )
            .unwrap(),
        )
        .unwrap();
    let conflict = Customer360ProjectionInput::try_new(
        Customer360ProjectionStream::ControlCustomers,
        position(2, 1, 1),
        profile(id, "Conflicting"),
    )
    .unwrap();
    assert_eq!(
        projection.apply(conflict),
        Err(Customer360DomainError::ConflictingGlobalSequence)
    );
}

#[test]
fn control_and_holding_sequences_are_never_compared() {
    let id = customer(10);
    let mut projection = Customer360Projection::new(id);
    projection
        .apply(
            Customer360ProjectionInput::try_new(
                Customer360ProjectionStream::ControlCustomers,
                position(1, 1, 9_000),
                profile(id, "Acme"),
            )
            .unwrap(),
        )
        .unwrap();
    projection
        .apply(
            Customer360ProjectionInput::try_new(
                Customer360ProjectionStream::HoldingActivity,
                position(2, 1, 2),
                subscription(id, 20),
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(
        projection
            .last_position(Customer360ProjectionStream::ControlCustomers)
            .unwrap()
            .global_sequence,
        9_000
    );
    assert_eq!(
        projection
            .last_position(Customer360ProjectionStream::HoldingActivity)
            .unwrap()
            .global_sequence,
        2
    );
}

#[test]
fn source_fact_must_use_its_owner_database_stream() {
    let id = customer(10);
    assert_eq!(
        Customer360ProjectionInput::try_new(
            Customer360ProjectionStream::HoldingActivity,
            position(1, 1, 1),
            profile(id, "Wrong stream"),
        ),
        Err(Customer360DomainError::IncorrectProjectionStream)
    );
}

#[test]
fn source_freshness_not_row_count_controls_empty_section_availability() {
    let coverage = SourceCoverage {
        customer_profile: true,
        commercial_subscriptions: true,
        crm_opportunities: true,
        sales_orders: true,
        support_cases: true,
    };
    let available = Customer360SectionAvailability::from_source_freshness(
        coverage,
        SourceFreshness::Current,
        SourceFreshness::Current,
    );
    assert_eq!(available.commercial, SectionAvailability::Ready);
    assert_eq!(available.support, SectionAvailability::Ready);

    let degraded = Customer360SectionAvailability::from_source_freshness(
        coverage,
        SourceFreshness::Stale,
        SourceFreshness::Unavailable,
    );
    assert_eq!(degraded.overview, SectionAvailability::Stale);
    assert_eq!(degraded.commercial, SectionAvailability::Unavailable);
}
