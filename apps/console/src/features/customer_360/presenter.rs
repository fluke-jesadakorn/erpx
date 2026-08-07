use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::Customer360Display;

#[derive(Debug, Default)]
pub struct Customer360Presenter {
    inner: Presenter<Customer360Display>,
}

impl Customer360Presenter {
    pub fn state(&self) -> &PresenterState<Customer360Display> {
        self.inner.state()
    }

    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }

    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }

    pub fn resolved(&mut self, value: QueryResult<Option<Customer360Display>>) {
        self.inner.transition(PresenterEvent::QueryResolved(value));
    }

    pub fn failed(&mut self, error: UiError) {
        self.inner.transition(PresenterEvent::Failed(error));
    }

    pub fn reset(&mut self) {
        self.inner.transition(PresenterEvent::Reset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_cqrs::{FreshnessState, ProjectionMetadata, QueryConsistency, QueryMetadata};
    use erpx_kernel::CorrelationId;
    use erpx_ui_foundation::UiErrorKind;
    use uuid::Uuid;

    use super::super::models::{
        Customer360AvailabilityDisplay, Customer360CoverageDisplay,
        Customer360SourceProjectionsDisplay, ProjectionFreshnessDisplay,
        SectionDisplayAvailability, SourceProjectionDisplay,
    };

    fn result(
        value: Option<Customer360Display>,
        freshness: FreshnessState,
    ) -> QueryResult<Option<Customer360Display>> {
        QueryResult {
            value,
            metadata: QueryMetadata {
                correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
                projection: ProjectionMetadata {
                    checkpoint: None,
                    freshness,
                    lag_millis: Some(5),
                },
                consistency: QueryConsistency::Eventual,
            },
        }
    }

    fn content() -> Customer360Display {
        let source = SourceProjectionDisplay {
            freshness: ProjectionFreshnessDisplay::Current,
            lag_millis: Some(5),
            checkpoint_sequence: None,
        };
        Customer360Display {
            customer_id: Uuid::from_u128(2).to_string(),
            profile: None,
            subscriptions: vec![],
            opportunities: vec![],
            sales_orders: vec![],
            support_cases: vec![],
            coverage: Customer360CoverageDisplay {
                customer_profile: false,
                commercial_subscriptions: false,
                crm_opportunities: false,
                sales_orders: false,
                support_cases: false,
            },
            availability: Customer360AvailabilityDisplay {
                overview: SectionDisplayAvailability::AwaitingSource,
                commercial: SectionDisplayAvailability::AwaitingSource,
                relationship_activity: SectionDisplayAvailability::AwaitingSource,
                sales_activity: SectionDisplayAvailability::AwaitingSource,
                support: SectionDisplayAvailability::AwaitingSource,
                organization: SectionDisplayAvailability::NotProjected,
                contacts: SectionDisplayAvailability::NotProjected,
                entitlements: SectionDisplayAvailability::NotProjected,
                onboarding: SectionDisplayAvailability::NotProjected,
                health: SectionDisplayAvailability::NotProjected,
                access_audit: SectionDisplayAvailability::NotProjected,
                notes: SectionDisplayAvailability::NotProjected,
            },
            projections: Customer360SourceProjectionsDisplay {
                control_customers: source.clone(),
                tenant_activity: source,
            },
        }
    }

    #[test]
    fn empty_success_and_stale_states_are_explicit() {
        let mut presenter = Customer360Presenter::default();
        presenter.resolved(result(None, FreshnessState::Current));
        assert!(matches!(presenter.state(), PresenterState::Empty { .. }));
        presenter.resolved(result(Some(content()), FreshnessState::Current));
        assert!(matches!(presenter.state(), PresenterState::Success { .. }));
        presenter.resolved(result(Some(content()), FreshnessState::Rebuilding));
        assert!(matches!(presenter.state(), PresenterState::Stale { .. }));
    }

    #[test]
    fn progress_and_bounded_recovery_states_are_explicit() {
        let mut presenter = Customer360Presenter::default();
        presenter.loading();
        assert!(matches!(presenter.state(), PresenterState::Loading));
        presenter.submitting();
        assert!(matches!(presenter.state(), PresenterState::Submitting));
        for kind in [
            UiErrorKind::Denied,
            UiErrorKind::Conflict,
            UiErrorKind::Offline,
            UiErrorKind::NotReady,
        ] {
            presenter.failed(UiError::bounded(
                kind,
                "customer_360",
                "customer 360 query failed",
                true,
                None,
            ));
            assert!(matches!(
                presenter.state(),
                PresenterState::Denied { .. }
                    | PresenterState::Conflict { .. }
                    | PresenterState::Offline { .. }
                    | PresenterState::NotReady { .. }
            ));
        }
    }
}
