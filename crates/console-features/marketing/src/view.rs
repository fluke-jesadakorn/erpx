use super::presenter::MarketingContent;
use erpx_ui_foundation::{PresenterState, UiError};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketingRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Candidate(MarketingContent),
    Denied(String),
    Stale {
        value: Option<MarketingContent>,
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
pub fn render_marketing_state(state: &PresenterState<MarketingContent>) -> MarketingRenderedState {
    match state {
        PresenterState::Idle => MarketingRenderedState::Idle,
        PresenterState::Loading => MarketingRenderedState::Loading,
        PresenterState::Submitting => MarketingRenderedState::Submitting,
        PresenterState::Empty { .. } => MarketingRenderedState::Empty,
        PresenterState::Success { value, .. } => MarketingRenderedState::Candidate(value.clone()),
        PresenterState::Denied { error } => MarketingRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => MarketingRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}
fn recovery(error: &UiError, kind: u8) -> MarketingRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => MarketingRenderedState::Conflict { message, retryable },
        1 => MarketingRenderedState::Offline { message, retryable },
        2 => MarketingRenderedState::NotReady { message, retryable },
        _ => MarketingRenderedState::Failed { message, retryable },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use erpx_ui_foundation::{PresenterState, UiErrorKind};
    #[test]
    fn view_renders_every_recovery() {
        for (kind, state) in [
            (UiErrorKind::Denied, 0),
            (UiErrorKind::Conflict, 1),
            (UiErrorKind::Offline, 2),
            (UiErrorKind::NotReady, 3),
        ] {
            let error = UiError::bounded(kind, "x", "x", true, None);
            let input = match state {
                0 => PresenterState::Denied { error },
                1 => PresenterState::Conflict { error },
                2 => PresenterState::Offline { error },
                _ => PresenterState::NotReady { error },
            };
            assert!(!matches!(
                render_marketing_state(&input),
                MarketingRenderedState::Failed { .. }
            ));
        }
    }
    #[test]
    fn view_has_no_wire_or_aggregate_import() {
        for source in [include_str!("view.rs"), include_str!("models.rs")] {
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("erpx_", "marketing")));
            assert!(!source.contains(concat!("tonic", "::")));
        }
    }
}
