use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::{SalesOrderItem, SalesOrderItems, SalesQuoteItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SalesContent {
    Quote(SalesQuoteItem),
    Order(SalesOrderItem),
    Orders(SalesOrderItems),
}

#[derive(Debug, Default)]
pub struct SalesPresenter {
    inner: Presenter<SalesContent>,
}

impl SalesPresenter {
    pub fn state(&self) -> &PresenterState<SalesContent> {
        self.inner.state()
    }

    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }

    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }

    pub fn resolved(&mut self, result: QueryResult<Option<SalesContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result));
    }

    pub fn apply(&mut self, event: PresenterEvent<SalesContent>) {
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
    use erpx_ui_foundation::UiErrorKind;

    #[test]
    fn presenter_supports_all_bounded_recovery_states() {
        let mut value = SalesPresenter::default();
        value.loading();
        assert!(matches!(value.state(), PresenterState::Loading));
        value.submitting();
        assert!(matches!(value.state(), PresenterState::Submitting));
        for kind in [
            UiErrorKind::Denied,
            UiErrorKind::Conflict,
            UiErrorKind::Offline,
            UiErrorKind::NotReady,
        ] {
            value.failed(UiError::bounded(kind, "sales", "x", true, None));
            assert!(!matches!(value.state(), PresenterState::Failed { .. }));
        }
    }

    #[test]
    fn presenter_has_no_wire_or_domain_dependency() {
        let source = include_str!("presenter.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("erpx_", "sales")));
    }
}
