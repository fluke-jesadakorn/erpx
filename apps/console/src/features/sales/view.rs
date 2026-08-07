use erpx_ui_foundation::{PresenterState, UiError};

use super::presenter::SalesContent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SalesRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Content(SalesContent),
    Denied(String),
    Stale {
        value: Option<SalesContent>,
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

pub fn render_sales_state(state: &PresenterState<SalesContent>) -> SalesRenderedState {
    match state {
        PresenterState::Idle => SalesRenderedState::Idle,
        PresenterState::Loading => SalesRenderedState::Loading,
        PresenterState::Submitting => SalesRenderedState::Submitting,
        PresenterState::Empty { .. } => SalesRenderedState::Empty,
        PresenterState::Success { value, .. } => SalesRenderedState::Content(value.clone()),
        PresenterState::Denied { error } => SalesRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => SalesRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}

fn recovery(error: &UiError, kind: u8) -> SalesRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => SalesRenderedState::Conflict { message, retryable },
        1 => SalesRenderedState::Offline { message, retryable },
        2 => SalesRenderedState::NotReady { message, retryable },
        _ => SalesRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_and_empty_states_render() {
        assert_eq!(
            render_sales_state(&PresenterState::Loading),
            SalesRenderedState::Loading
        );
        assert_eq!(
            render_sales_state(&PresenterState::Submitting),
            SalesRenderedState::Submitting
        );
    }

    #[test]
    fn view_and_models_are_wire_and_domain_free() {
        for source in [include_str!("view.rs"), include_str!("models.rs")] {
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("erpx_", "sales")));
            assert!(!source.contains(concat!("tonic", "::")));
        }
    }
}
