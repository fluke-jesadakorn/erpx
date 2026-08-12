use erpx_ui_foundation::{PresenterState, UiError};

use super::models::Customer360Display;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Customer360RenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Content(Customer360Display),
    Denied(String),
    Stale {
        value: Option<Customer360Display>,
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

pub fn render_customer360_state(
    state: &PresenterState<Customer360Display>,
) -> Customer360RenderedState {
    match state {
        PresenterState::Idle => Customer360RenderedState::Idle,
        PresenterState::Loading => Customer360RenderedState::Loading,
        PresenterState::Submitting => Customer360RenderedState::Submitting,
        PresenterState::Empty { .. } => Customer360RenderedState::Empty,
        PresenterState::Success { value, .. } => Customer360RenderedState::Content(value.clone()),
        PresenterState::Denied { error } => {
            Customer360RenderedState::Denied(error.message().into())
        }
        PresenterState::Stale { value, metadata } => Customer360RenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}

fn recovery(error: &UiError, kind: u8) -> Customer360RenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => Customer360RenderedState::Conflict { message, retryable },
        1 => Customer360RenderedState::Offline { message, retryable },
        2 => Customer360RenderedState::NotReady { message, retryable },
        _ => Customer360RenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn presenter_and_view_do_not_import_transport_or_domain_types() {
        for source in [include_str!("presenter.rs"), include_str!("view.rs")] {
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
            assert!(!source.contains(concat!("erpx_", "customer_360")));
            assert!(!source.contains(concat!("erpx_", "integration_contracts")));
        }
    }
}
