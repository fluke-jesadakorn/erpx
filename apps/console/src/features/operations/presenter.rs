use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::{
    OperationsExceptionItem, OperationsExceptionItems, OperationsSummaryItem, WorkQueuePolicyItem,
    WorkQueuePolicyItems,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationsContent {
    Exception(OperationsExceptionItem),
    Exceptions(OperationsExceptionItems),
    WorkQueuePolicy(WorkQueuePolicyItem),
    WorkQueuePolicies(WorkQueuePolicyItems),
    Summary(OperationsSummaryItem),
}

#[derive(Debug, Default)]
pub struct OperationsPresenter {
    inner: Presenter<OperationsContent>,
}

impl OperationsPresenter {
    pub fn state(&self) -> &PresenterState<OperationsContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted)
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted)
    }
    pub fn resolved(&mut self, value: QueryResult<Option<OperationsContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(value));
    }
    pub fn apply(&mut self, value: PresenterEvent<OperationsContent>) {
        self.inner.transition(value);
    }
    pub fn failed(&mut self, value: UiError) {
        self.inner.transition(PresenterEvent::Failed(value));
    }
    pub fn reset(&mut self) {
        self.inner.transition(PresenterEvent::Reset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_ui_foundation::UiErrorKind;

    #[test]
    fn progress_and_all_bounded_recovery_states_are_explicit() {
        let mut presenter = OperationsPresenter::default();
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
                "operations",
                "operations",
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
