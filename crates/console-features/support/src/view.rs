use erpx_ui_foundation::{PresenterState, UiError};

use super::presenter::SupportContent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Content(SupportContent),
    Denied(String),
    Stale {
        value: Option<SupportContent>,
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

pub fn render_support_state(state: &PresenterState<SupportContent>) -> SupportRenderedState {
    match state {
        PresenterState::Idle => SupportRenderedState::Idle,
        PresenterState::Loading => SupportRenderedState::Loading,
        PresenterState::Submitting => SupportRenderedState::Submitting,
        PresenterState::Empty { .. } => SupportRenderedState::Empty,
        PresenterState::Success { value, .. } => SupportRenderedState::Content(value.clone()),
        PresenterState::Denied { error } => SupportRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => SupportRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}

fn recovery(error: &UiError, kind: u8) -> SupportRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => SupportRenderedState::Conflict { message, retryable },
        1 => SupportRenderedState::Offline { message, retryable },
        2 => SupportRenderedState::NotReady { message, retryable },
        _ => SupportRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_ui_foundation::{PresenterState, UiErrorKind};

    #[test]
    fn view_renders_every_recovery_state() {
        for (kind, discriminator) in [
            (UiErrorKind::Denied, 0),
            (UiErrorKind::Conflict, 1),
            (UiErrorKind::Offline, 2),
            (UiErrorKind::NotReady, 3),
        ] {
            let error = UiError::bounded(kind, "support", "support", true, None);
            let state = match discriminator {
                0 => PresenterState::Denied { error },
                1 => PresenterState::Conflict { error },
                2 => PresenterState::Offline { error },
                _ => PresenterState::NotReady { error },
            };
            assert!(!matches!(
                render_support_state(&state),
                SupportRenderedState::Failed { .. }
            ));
        }
    }

    #[test]
    fn presenter_view_and_models_are_wire_and_aggregate_free() {
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "support")));
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
        }
    }
}
