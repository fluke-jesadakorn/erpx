use erpx_ui_foundation::{PresenterState, UiError};

use super::presenter::OperationsContent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationsRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Content(OperationsContent),
    Denied(String),
    Stale {
        value: Option<OperationsContent>,
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

pub fn render_operations_state(
    state: &PresenterState<OperationsContent>,
) -> OperationsRenderedState {
    match state {
        PresenterState::Idle => OperationsRenderedState::Idle,
        PresenterState::Loading => OperationsRenderedState::Loading,
        PresenterState::Submitting => OperationsRenderedState::Submitting,
        PresenterState::Empty { .. } => OperationsRenderedState::Empty,
        PresenterState::Success { value, .. } => OperationsRenderedState::Content(value.clone()),
        PresenterState::Denied { error } => OperationsRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => OperationsRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}

fn recovery(error: &UiError, kind: u8) -> OperationsRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => OperationsRenderedState::Conflict { message, retryable },
        1 => OperationsRenderedState::Offline { message, retryable },
        2 => OperationsRenderedState::NotReady { message, retryable },
        _ => OperationsRenderedState::Failed { message, retryable },
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
            assert!(!source.contains(concat!("erpx_", "operations")));
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
        }
    }
}
