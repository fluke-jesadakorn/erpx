use super::presenter::MaintenanceContent;
use erpx_ui_foundation::{PresenterState, UiError};

use super::models::{WorkOrderItem, WorkOrderItems};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaintenanceRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    WorkOrder(Box<WorkOrderItem>),
    WorkOrders(WorkOrderItems),
    Denied(String),
    Stale {
        value: Option<MaintenanceContent>,
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
pub fn render_maintenance_state(
    state: &PresenterState<MaintenanceContent>,
) -> MaintenanceRenderedState {
    match state {
        PresenterState::Idle => MaintenanceRenderedState::Idle,
        PresenterState::Loading => MaintenanceRenderedState::Loading,
        PresenterState::Submitting => MaintenanceRenderedState::Submitting,
        PresenterState::Empty { .. } => MaintenanceRenderedState::Empty,
        PresenterState::Success {
            value: MaintenanceContent::WorkOrder(value),
            ..
        } => MaintenanceRenderedState::WorkOrder(Box::new(value.as_ref().clone())),
        PresenterState::Success {
            value: MaintenanceContent::WorkOrders(value),
            ..
        } => MaintenanceRenderedState::WorkOrders(value.clone()),
        PresenterState::Denied { error } => {
            MaintenanceRenderedState::Denied(error.message().into())
        }
        PresenterState::Stale { value, metadata } => MaintenanceRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}
fn recovery(error: &UiError, kind: u8) -> MaintenanceRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => MaintenanceRenderedState::Conflict { message, retryable },
        1 => MaintenanceRenderedState::Offline { message, retryable },
        2 => MaintenanceRenderedState::NotReady { message, retryable },
        _ => MaintenanceRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn presenter_and_view_are_transport_and_aggregate_free() {
        for source in [include_str!("presenter.rs"), include_str!("view.rs")] {
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
            assert!(!source.contains(concat!("erpx_", "maintenance")));
        }
    }
}
