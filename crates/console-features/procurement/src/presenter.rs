use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::{ProcurementOrder, ProcurementOrders};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcurementContent {
    Order(ProcurementOrder),
    Orders(ProcurementOrders),
}

#[derive(Debug, Default)]
pub struct ProcurementPresenter {
    inner: Presenter<ProcurementContent>,
}

impl ProcurementPresenter {
    pub fn state(&self) -> &PresenterState<ProcurementContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }
    pub fn resolved(&mut self, result: QueryResult<Option<ProcurementContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result));
    }
    pub fn apply(&mut self, event: PresenterEvent<ProcurementContent>) {
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
    use erpx_ui_foundation::{PresenterState, UiErrorKind};

    #[test]
    fn bounded_failure_states_are_explicit() {
        let mut presenter = ProcurementPresenter::default();
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
    }
}
