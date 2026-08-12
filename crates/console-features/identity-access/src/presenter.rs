use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::model::IdentityAccessContent;

#[derive(Debug, Default)]
pub struct IdentityAccessPresenter {
    inner: Presenter<IdentityAccessContent>,
}

impl IdentityAccessPresenter {
    pub fn state(&self) -> &PresenterState<IdentityAccessContent> {
        self.inner.state()
    }

    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }

    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }

    pub fn resolved(&mut self, result: QueryResult<Option<IdentityAccessContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result));
    }

    pub fn apply(&mut self, event: PresenterEvent<IdentityAccessContent>) {
        self.inner.transition(event);
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
    use erpx_cqrs::{
        FreshnessState, ProjectionMetadata, QueryConsistency, QueryMetadata, QueryResult,
    };
    use erpx_kernel::CorrelationId;
    use erpx_ui_foundation::{PresenterState, UiErrorKind};
    use uuid::Uuid;

    use super::*;

    fn result(
        value: Option<IdentityAccessContent>,
        freshness: FreshnessState,
    ) -> QueryResult<Option<IdentityAccessContent>> {
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

    #[test]
    fn all_progress_failure_empty_success_and_stale_states_are_explicit() {
        let mut presenter = IdentityAccessPresenter::default();
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
            presenter.failed(UiError::bounded(kind, "test", "test", true, None));
            assert!(matches!(
                presenter.state(),
                PresenterState::Denied { .. }
                    | PresenterState::Conflict { .. }
                    | PresenterState::Offline { .. }
                    | PresenterState::NotReady { .. }
            ));
        }

        presenter.resolved(result(None, FreshnessState::Current));
        assert!(matches!(presenter.state(), PresenterState::Empty { .. }));
        let content = IdentityAccessContent::Principals {
            values: Vec::new(),
            next_cursor: None,
        };
        presenter.resolved(result(Some(content.clone()), FreshnessState::Current));
        assert!(matches!(presenter.state(), PresenterState::Success { .. }));
        presenter.resolved(result(Some(content), FreshnessState::Rebuilding));
        assert!(matches!(presenter.state(), PresenterState::Stale { .. }));
    }
}
