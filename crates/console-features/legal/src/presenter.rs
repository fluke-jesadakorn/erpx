use super::models::{
    AccessEntryItem, AccessEntryItems, ContractItem, ContractItems, DocumentItem, DocumentItems,
    MatterItem,
};
use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegalContent {
    Contract(ContractItem),
    Contracts(ContractItems),
    Document(DocumentItem),
    Documents(DocumentItems),
    AccessEntry(AccessEntryItem),
    AccessHistory(AccessEntryItems),
    Matter(MatterItem),
}
#[derive(Debug, Default)]
pub struct LegalPresenter {
    inner: Presenter<LegalContent>,
}
impl LegalPresenter {
    pub fn state(&self) -> &PresenterState<LegalContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted)
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted)
    }
    pub fn resolved(&mut self, result: QueryResult<Option<LegalContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result))
    }
    pub fn apply(&mut self, event: PresenterEvent<LegalContent>) {
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
    fn progress_and_recovery_states_are_explicit() {
        let mut p = LegalPresenter::default();
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
            p.failed(UiError::bounded(kind, "legal", "legal", true, None));
            assert!(matches!(
                p.state(),
                PresenterState::Denied { .. }
                    | PresenterState::Conflict { .. }
                    | PresenterState::Offline { .. }
                    | PresenterState::NotReady { .. }
            ));
        }
    }
}
