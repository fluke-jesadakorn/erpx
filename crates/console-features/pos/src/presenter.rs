use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::{PosRefundItem, PosRefundItems, PosSaleItem, PosSaleItems};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PosContent {
    Sale(PosSaleItem),
    Sales(PosSaleItems),
    Refund(PosRefundItem),
    Refunds(PosRefundItems),
}

#[derive(Debug, Default)]
pub struct PosPresenter {
    inner: Presenter<PosContent>,
}

impl PosPresenter {
    pub fn state(&self) -> &PresenterState<PosContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }
    pub fn resolved(&mut self, value: QueryResult<Option<PosContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(value));
    }
    pub fn apply(&mut self, value: PresenterEvent<PosContent>) {
        self.inner.transition(value);
    }
    pub fn failed(&mut self, value: UiError) {
        self.inner.transition(PresenterEvent::Failed(value));
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
    fn progress_and_bounded_recovery_states_are_explicit() {
        let mut presenter = PosPresenter::default();
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
            presenter.failed(UiError::bounded(kind, "pos", "pos", true, None));
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
