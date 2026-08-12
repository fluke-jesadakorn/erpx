use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::{InventoryStockItem, InventoryStockItems};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryContent {
    Item(InventoryStockItem),
    Items(InventoryStockItems),
}

#[derive(Debug, Default)]
pub struct InventoryPresenter {
    inner: Presenter<InventoryContent>,
}

impl InventoryPresenter {
    pub fn state(&self) -> &PresenterState<InventoryContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }
    pub fn resolved(&mut self, result: QueryResult<Option<InventoryContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result));
    }
    pub fn apply(&mut self, event: PresenterEvent<InventoryContent>) {
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
        let mut presenter = InventoryPresenter::default();
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
