use super::model::PlatformAdministrationContent;
use erpx_cqrs::QueryResult;
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

#[derive(Debug, Default)]
pub struct PlatformAdministrationPresenter {
    inner: Presenter<PlatformAdministrationContent>,
}
impl PlatformAdministrationPresenter {
    pub fn state(&self) -> &PresenterState<PlatformAdministrationContent> {
        self.inner.state()
    }
    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }
    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }
    pub fn resolved(&mut self, result: QueryResult<Option<PlatformAdministrationContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result));
    }
    pub fn apply(&mut self, event: PresenterEvent<PlatformAdministrationContent>) {
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
    fn progress_and_bounded_recovery_states_are_explicit() {
        let mut presenter = PlatformAdministrationPresenter::default();
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
            presenter.failed(UiError::bounded(kind, "bounded", "bounded", true, None));
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
