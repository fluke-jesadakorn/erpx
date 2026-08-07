use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::{
    CapabilityMetadataItem, CustomerTicketItem, CustomerTicketItems, IncidentItem, IncidentItems,
    InternalTicketItem, InternalTicketItems, SessionItem,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportContent {
    CustomerTicket(CustomerTicketItem),
    CustomerTickets(CustomerTicketItems),
    InternalTicket(InternalTicketItem),
    InternalTickets(InternalTicketItems),
    Incident(IncidentItem),
    Incidents(IncidentItems),
    Session(SessionItem),
    CapabilityMetadata(CapabilityMetadataItem),
}

#[derive(Debug, Default)]
pub struct SupportPresenter {
    inner: Presenter<SupportContent>,
}

impl SupportPresenter {
    pub fn state(&self) -> &PresenterState<SupportContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted)
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted)
    }
    pub fn resolved(&mut self, result: QueryResult<Option<SupportContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result))
    }
    pub fn apply(&mut self, event: PresenterEvent<SupportContent>) {
        self.inner.transition(event)
    }
    pub fn failed(&mut self, error: UiError) {
        self.inner.transition(PresenterEvent::Failed(error))
    }
    pub fn reset(&mut self) {
        self.inner.transition(PresenterEvent::Reset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_cqrs::{FreshnessState, ProjectionMetadata, QueryConsistency, QueryMetadata};
    use erpx_kernel::CorrelationId;
    use erpx_ui_foundation::{PresenterState, UiErrorKind};
    use uuid::Uuid;

    fn result(
        value: Option<SupportContent>,
        freshness: FreshnessState,
    ) -> QueryResult<Option<SupportContent>> {
        QueryResult {
            value,
            metadata: QueryMetadata {
                correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
                projection: ProjectionMetadata {
                    checkpoint: None,
                    freshness,
                    lag_millis: Some(8),
                },
                consistency: QueryConsistency::Eventual,
            },
        }
    }

    fn content() -> SupportContent {
        SupportContent::CustomerTickets(CustomerTicketItems {
            tickets: vec![],
            next_cursor: None,
        })
    }

    #[test]
    fn progress_and_bounded_recovery_states_are_explicit() {
        let mut presenter = SupportPresenter::default();
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
            presenter.failed(UiError::bounded(kind, "support", "support", true, None));
            assert!(matches!(
                presenter.state(),
                PresenterState::Denied { .. }
                    | PresenterState::Conflict { .. }
                    | PresenterState::Offline { .. }
                    | PresenterState::NotReady { .. }
            ));
        }
    }

    #[test]
    fn query_results_preserve_empty_success_and_stale() {
        let mut presenter = SupportPresenter::default();
        presenter.resolved(result(None, FreshnessState::Current));
        assert!(matches!(presenter.state(), PresenterState::Empty { .. }));
        let content = content();
        presenter.resolved(result(Some(content.clone()), FreshnessState::Current));
        assert!(
            matches!(presenter.state(), PresenterState::Success { value, .. } if value == &content)
        );
        presenter.resolved(result(Some(content), FreshnessState::Rebuilding));
        assert!(matches!(
            presenter.state(),
            PresenterState::Stale {
                metadata: QueryMetadata {
                    projection: ProjectionMetadata {
                        lag_millis: Some(8),
                        ..
                    },
                    ..
                },
                ..
            }
        ));
    }
}
