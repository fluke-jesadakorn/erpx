use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::{CrmLeadItem, CrmOpportunityItem, CrmOpportunityItems};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrmContent {
    Lead(CrmLeadItem),
    Opportunity(CrmOpportunityItem),
    Opportunities(CrmOpportunityItems),
}
#[derive(Debug, Default)]
pub struct CrmPresenter {
    inner: Presenter<CrmContent>,
}
impl CrmPresenter {
    pub fn state(&self) -> &PresenterState<CrmContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted)
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted)
    }
    pub fn resolved(&mut self, result: QueryResult<Option<CrmContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result))
    }
    pub fn apply(&mut self, event: PresenterEvent<CrmContent>) {
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
    use erpx_ui_foundation::{PresenterState, UiErrorKind};
    #[test]
    fn presenter_has_bounded_recovery_states() {
        let mut value = CrmPresenter::default();
        value.loading();
        assert!(matches!(value.state(), PresenterState::Loading));
        for kind in [
            UiErrorKind::Denied,
            UiErrorKind::Conflict,
            UiErrorKind::Offline,
            UiErrorKind::NotReady,
        ] {
            value.failed(UiError::bounded(kind, "x", "x", true, None));
            assert!(!matches!(value.state(), PresenterState::Failed { .. }));
        }
    }
    #[test]
    fn presenter_is_wire_free() {
        let source = include_str!("presenter.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("erpx_", "crm")));
    }
}
