use erpx_ui_foundation::{PresenterState, UiError};

use super::models::{InventoryStockItem, InventoryStockItems};
use super::presenter::InventoryContent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Item(InventoryStockItem),
    Items(InventoryStockItems),
    Denied(String),
    Stale {
        value: Option<InventoryContent>,
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

pub fn render_inventory_state(state: &PresenterState<InventoryContent>) -> InventoryRenderedState {
    match state {
        PresenterState::Idle => InventoryRenderedState::Idle,
        PresenterState::Loading => InventoryRenderedState::Loading,
        PresenterState::Submitting => InventoryRenderedState::Submitting,
        PresenterState::Empty { .. } => InventoryRenderedState::Empty,
        PresenterState::Success { value, .. } => match value.clone() {
            InventoryContent::Item(value) => InventoryRenderedState::Item(value),
            InventoryContent::Items(value) => InventoryRenderedState::Items(value),
        },
        PresenterState::Denied { error } => InventoryRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => InventoryRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}

fn recovery(error: &UiError, kind: u8) -> InventoryRenderedState {
    let message = error.message().to_owned();
    let retryable = error.retryable();
    match kind {
        0 => InventoryRenderedState::Conflict { message, retryable },
        1 => InventoryRenderedState::Offline { message, retryable },
        2 => InventoryRenderedState::NotReady { message, retryable },
        _ => InventoryRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_states_render() {
        assert_eq!(
            render_inventory_state(&PresenterState::Loading),
            InventoryRenderedState::Loading
        );
        assert_eq!(
            render_inventory_state(&PresenterState::Submitting),
            InventoryRenderedState::Submitting
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
            assert!(!source.contains(concat!("erpx_", "inventory")));
        }
    }
}
