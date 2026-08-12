use erpx_ui_foundation::{PresenterState, UiError};

use super::models::{ProcurementOrder, ProcurementOrders};
use super::presenter::ProcurementContent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcurementRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Order(ProcurementOrder),
    Orders(ProcurementOrders),
    Denied(String),
    Stale {
        value: Option<ProcurementContent>,
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

pub fn render_procurement_state(
    state: &PresenterState<ProcurementContent>,
) -> ProcurementRenderedState {
    match state {
        PresenterState::Idle => ProcurementRenderedState::Idle,
        PresenterState::Loading => ProcurementRenderedState::Loading,
        PresenterState::Submitting => ProcurementRenderedState::Submitting,
        PresenterState::Empty { .. } => ProcurementRenderedState::Empty,
        PresenterState::Success { value, .. } => match value.clone() {
            ProcurementContent::Order(value) => ProcurementRenderedState::Order(value),
            ProcurementContent::Orders(value) => ProcurementRenderedState::Orders(value),
        },
        PresenterState::Denied { error } => {
            ProcurementRenderedState::Denied(error.message().into())
        }
        PresenterState::Stale { value, metadata } => ProcurementRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}

fn recovery(error: &UiError, kind: u8) -> ProcurementRenderedState {
    let message = error.message().to_owned();
    let retryable = error.retryable();
    match kind {
        0 => ProcurementRenderedState::Conflict { message, retryable },
        1 => ProcurementRenderedState::Offline { message, retryable },
        2 => ProcurementRenderedState::NotReady { message, retryable },
        _ => ProcurementRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_ui_foundation::PresenterState;
    #[test]
    fn progress_states_render() {
        assert_eq!(
            render_procurement_state(&PresenterState::Loading),
            ProcurementRenderedState::Loading
        );
        assert_eq!(
            render_procurement_state(&PresenterState::Submitting),
            ProcurementRenderedState::Submitting
        );
    }
    #[test]
    fn presenter_and_view_do_not_import_transport_or_aggregates() {
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
            assert!(!source.contains(concat!("erpx_", "procurement")));
        }
    }
}
