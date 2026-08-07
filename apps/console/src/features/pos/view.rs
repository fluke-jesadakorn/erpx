use erpx_ui_foundation::{PresenterState, UiError};

use super::{
    models::{PosRefundItem, PosRefundItems, PosSaleItem, PosSaleItems},
    presenter::PosContent,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PosRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Sale(PosSaleItem),
    Sales(PosSaleItems),
    Refund(PosRefundItem),
    Refunds(PosRefundItems),
    Denied(String),
    Stale {
        value: Option<PosContent>,
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

pub fn render_pos_state(state: &PresenterState<PosContent>) -> PosRenderedState {
    match state {
        PresenterState::Idle => PosRenderedState::Idle,
        PresenterState::Loading => PosRenderedState::Loading,
        PresenterState::Submitting => PosRenderedState::Submitting,
        PresenterState::Empty { .. } => PosRenderedState::Empty,
        PresenterState::Success {
            value: PosContent::Sale(value),
            ..
        } => PosRenderedState::Sale(value.clone()),
        PresenterState::Success {
            value: PosContent::Sales(value),
            ..
        } => PosRenderedState::Sales(value.clone()),
        PresenterState::Success {
            value: PosContent::Refund(value),
            ..
        } => PosRenderedState::Refund(value.clone()),
        PresenterState::Success {
            value: PosContent::Refunds(value),
            ..
        } => PosRenderedState::Refunds(value.clone()),
        PresenterState::Denied { error } => PosRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => PosRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}

fn recovery(error: &UiError, kind: u8) -> PosRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => PosRenderedState::Conflict { message, retryable },
        1 => PosRenderedState::Offline { message, retryable },
        2 => PosRenderedState::NotReady { message, retryable },
        _ => PosRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn presenter_view_and_models_do_not_import_transport_or_aggregate_types() {
        for source in [
            include_str!("presenter.rs"),
            include_str!("view.rs"),
            include_str!("models.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
            assert!(!source.contains(concat!("erpx_", "pos")));
        }
    }
}
