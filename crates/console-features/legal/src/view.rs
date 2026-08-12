use super::presenter::LegalContent;
use erpx_ui_foundation::{PresenterState, UiError};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegalRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Content(LegalContent),
    Denied(String),
    Stale {
        value: Option<LegalContent>,
        lag_millis: Option<u64>,
    },
    Conflict {
        message: String,
        retryable: bool,
    },
    Offline {
        message: String,
        retryable: bool,
    },
    NotReady {
        message: String,
        retryable: bool,
    },
    Failed {
        message: String,
        retryable: bool,
    },
}
pub fn render_legal_state(state: &PresenterState<LegalContent>) -> LegalRenderedState {
    match state {
        PresenterState::Idle => LegalRenderedState::Idle,
        PresenterState::Loading => LegalRenderedState::Loading,
        PresenterState::Submitting => LegalRenderedState::Submitting,
        PresenterState::Empty { .. } => LegalRenderedState::Empty,
        PresenterState::Success { value, .. } => LegalRenderedState::Content(value.clone()),
        PresenterState::Denied { error } => LegalRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => LegalRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}
fn recovery(error: &UiError, kind: u8) -> LegalRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => LegalRenderedState::Conflict { message, retryable },
        1 => LegalRenderedState::Offline { message, retryable },
        2 => LegalRenderedState::NotReady { message, retryable },
        _ => LegalRenderedState::Failed { message, retryable },
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn presenter_view_and_models_are_transport_and_aggregate_free() {
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "legal")));
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
        }
    }
}
