use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

use super::models::{MarketingCampaignBudgetItem, MarketingLeadItem, MarketingLeadItems};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketingContent {
    Candidate(MarketingLeadItem),
    Candidates(MarketingLeadItems),
    CampaignBudget(MarketingCampaignBudgetItem),
}
#[derive(Debug, Default)]
pub struct MarketingPresenter {
    inner: Presenter<MarketingContent>,
}
impl MarketingPresenter {
    pub fn state(&self) -> &PresenterState<MarketingContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted)
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted)
    }
    pub fn resolved(&mut self, result: QueryResult<Option<MarketingContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result))
    }
    pub fn apply(&mut self, event: PresenterEvent<MarketingContent>) {
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
    fn presenter_exposes_progress_and_recovery() {
        let mut value = MarketingPresenter::default();
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
            value.failed(UiError::bounded(kind, "code", "message", true, None));
            assert!(!matches!(value.state(), PresenterState::Failed { .. }));
        }
    }
    #[test]
    fn presenter_has_no_transport_dependency() {
        let source = include_str!("presenter.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("tonic", "::")));
        assert!(!source.contains(concat!("erpx_", "marketing")));
    }
}
