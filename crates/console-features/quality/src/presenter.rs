use erpx_cqrs::QueryResult;
use erpx_quality::{InspectionList, InspectionView};
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

/// Data that a Quality screen may render. This is deliberately made from
/// disposable read models rather than protobuf messages or write aggregates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualityContent {
    Inspection(InspectionView),
    Inspections(InspectionList),
}

#[derive(Debug, Default)]
pub struct QualityPresenter {
    inner: Presenter<QualityContent>,
}

impl QualityPresenter {
    pub fn state(&self) -> &PresenterState<QualityContent> {
        self.inner.state()
    }

    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }

    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }

    pub fn resolved(&mut self, result: QueryResult<Option<QualityContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result));
    }

    /// Applies a target-neutral result produced by the Quality controller.
    /// Query metadata remains intact, including stale/rebuilding freshness.
    pub fn apply(&mut self, event: PresenterEvent<QualityContent>) {
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
    use erpx_ui_foundation::{PresenterState, UiError, UiErrorKind};
    use uuid::Uuid;

    use super::*;

    fn result(
        value: Option<QualityContent>,
        freshness: FreshnessState,
    ) -> QueryResult<Option<QualityContent>> {
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
    fn progress_and_bounded_failure_states_are_explicit() {
        let mut presenter = QualityPresenter::default();
        presenter.loading();
        assert!(matches!(presenter.state(), PresenterState::Loading));

        presenter.submitting();
        assert!(matches!(presenter.state(), PresenterState::Submitting));

        for (kind, expected) in [
            (UiErrorKind::Denied, "denied"),
            (UiErrorKind::Conflict, "conflict"),
            (UiErrorKind::Offline, "offline"),
            (UiErrorKind::NotReady, "not_ready"),
        ] {
            presenter.failed(UiError::bounded(kind, expected, expected, true, None));
            let actual = match presenter.state() {
                PresenterState::Denied { .. } => "denied",
                PresenterState::Conflict { .. } => "conflict",
                PresenterState::Offline { .. } => "offline",
                PresenterState::NotReady { .. } => "not_ready",
                _ => "unexpected",
            };
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn query_results_preserve_empty_success_and_stale_states() {
        let mut presenter = QualityPresenter::default();
        presenter.resolved(result(None, FreshnessState::Current));
        assert!(matches!(presenter.state(), PresenterState::Empty { .. }));

        let content = QualityContent::Inspections(InspectionList {
            inspections: Vec::new(),
            next_cursor: None,
        });
        presenter.resolved(result(Some(content.clone()), FreshnessState::Current));
        assert!(matches!(
            presenter.state(),
            PresenterState::Success { value, .. } if value == &content
        ));

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
