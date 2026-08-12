use super::models::{FinanceItem, FinanceItems};
use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinanceContent {
    Item(FinanceItem),
    Items(FinanceItems),
    Summary {
        heading: String,
        amount_minor: i64,
        currency: String,
    },
}
#[derive(Debug, Default)]
pub struct FinancePresenter {
    inner: Presenter<FinanceContent>,
}
impl FinancePresenter {
    pub fn state(&self) -> &PresenterState<FinanceContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted)
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted)
    }
    pub fn resolved(&mut self, r: QueryResult<Option<FinanceContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(r))
    }
    pub fn apply(&mut self, e: PresenterEvent<FinanceContent>) {
        self.inner.transition(e)
    }
    pub fn failed(&mut self, e: UiError) {
        self.inner.transition(PresenterEvent::Failed(e))
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
    fn all_recovery_states_are_bounded() {
        let mut p = FinancePresenter::default();
        p.loading();
        assert!(matches!(p.state(), PresenterState::Loading));
        p.submitting();
        assert!(matches!(p.state(), PresenterState::Submitting));
        for kind in [
            UiErrorKind::Denied,
            UiErrorKind::Conflict,
            UiErrorKind::Offline,
            UiErrorKind::NotReady,
        ] {
            p.failed(UiError::bounded(kind, "finance", "x", true, None));
            assert!(!matches!(p.state(), PresenterState::Failed { .. }));
        }
    }
    #[test]
    fn presenter_has_no_wire_or_domain_dependency() {
        let source = include_str!("presenter.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("erpx_", "finance")));
    }
}
