use erpx_commercial::{CatalogPlanView, EntitlementView, Page, SubscriptionView};
use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommercialContent {
    CatalogPlan(CatalogPlanView),
    CatalogPlans(Page<CatalogPlanView>),
    Subscription(SubscriptionView),
    Subscriptions(Page<SubscriptionView>),
    Entitlement(EntitlementView),
    Entitlements(Page<EntitlementView>),
}

#[derive(Debug, Default)]
pub struct CommercialPresenter {
    inner: Presenter<CommercialContent>,
}
impl CommercialPresenter {
    pub fn state(&self) -> &PresenterState<CommercialContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }
    pub fn resolved(&mut self, result: QueryResult<Option<CommercialContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result));
    }
    pub fn apply(&mut self, event: PresenterEvent<CommercialContent>) {
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
    use super::*;
    use erpx_cqrs::{FreshnessState, ProjectionMetadata, QueryConsistency, QueryMetadata};
    use erpx_kernel::CorrelationId;
    use erpx_ui_foundation::UiErrorKind;
    use uuid::Uuid;
    fn result(
        value: Option<CommercialContent>,
        freshness: FreshnessState,
    ) -> QueryResult<Option<CommercialContent>> {
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
    fn progress_and_bounded_failures_are_explicit() {
        let mut presenter = CommercialPresenter::default();
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
    fn query_results_preserve_empty_and_stale() {
        let mut presenter = CommercialPresenter::default();
        presenter.resolved(result(None, FreshnessState::Current));
        assert!(matches!(presenter.state(), PresenterState::Empty { .. }));
        let content = CommercialContent::Subscriptions(Page {
            records: vec![],
            next_cursor: None,
        });
        presenter.resolved(result(Some(content.clone()), FreshnessState::Current));
        assert!(matches!(presenter.state(),PresenterState::Success{value,..}if value==&content));
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
