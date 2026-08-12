use erpx_cqrs::QueryResult;
use erpx_organization::{NodeList, NodeView};
use erpx_ui_foundation::{Presenter, PresenterEvent, PresenterState, UiError};

/// Disposable read-model content; write aggregates and transport messages are
/// not renderable values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrganizationContent {
    Node(NodeView),
    Nodes(NodeList),
}

#[derive(Debug, Default)]
pub struct OrganizationPresenter {
    inner: Presenter<OrganizationContent>,
}

impl OrganizationPresenter {
    pub fn state(&self) -> &PresenterState<OrganizationContent> {
        self.inner.state()
    }

    pub fn loading(&mut self) {
        self.inner.transition(PresenterEvent::LoadStarted);
    }

    pub fn submitting(&mut self) {
        self.inner.transition(PresenterEvent::SubmitStarted);
    }

    pub fn resolved(&mut self, result: QueryResult<Option<OrganizationContent>>) {
        self.inner.transition(PresenterEvent::QueryResolved(result));
    }

    pub fn apply(&mut self, event: PresenterEvent<OrganizationContent>) {
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
    use erpx_ui_foundation::{PresenterState, UiErrorKind};

    use super::*;

    #[test]
    fn progress_and_bounded_recovery_states_are_explicit() {
        let mut presenter = OrganizationPresenter::default();
        presenter.loading();
        assert!(matches!(presenter.state(), PresenterState::Loading));
        presenter.submitting();
        assert!(matches!(presenter.state(), PresenterState::Submitting));

        for (kind, expected) in [
            (UiErrorKind::Denied, "denied"),
            (UiErrorKind::Conflict, "conflict"),
            (UiErrorKind::Offline, "offline"),
            (UiErrorKind::NotReady, "not_ready"),
        ] {
            presenter.failed(UiError::bounded(kind, expected, expected, true, None));
            let actual = match presenter.state() {
                PresenterState::Denied { .. } => "denied",
                PresenterState::Conflict { .. } => "conflict",
                PresenterState::Offline { .. } => "offline",
                PresenterState::NotReady { .. } => "not_ready",
                _ => "unexpected",
            };
            assert_eq!(actual, expected);
        }
    }
}
