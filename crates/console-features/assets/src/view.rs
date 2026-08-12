use super::presenter::AssetsContent;
use erpx_ui_foundation::{PresenterState, UiError};

use super::models::{AssetItem, AssetItems};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetsRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Asset(Box<AssetItem>),
    Assets(AssetItems),
    Denied(String),
    Stale {
        value: Option<AssetsContent>,
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

pub fn render_assets_state(state: &PresenterState<AssetsContent>) -> AssetsRenderedState {
    match state {
        PresenterState::Idle => AssetsRenderedState::Idle,
        PresenterState::Loading => AssetsRenderedState::Loading,
        PresenterState::Submitting => AssetsRenderedState::Submitting,
        PresenterState::Empty { .. } => AssetsRenderedState::Empty,
        PresenterState::Success {
            value: AssetsContent::Asset(value),
            ..
        } => AssetsRenderedState::Asset(Box::new(value.as_ref().clone())),
        PresenterState::Success {
            value: AssetsContent::Assets(value),
            ..
        } => AssetsRenderedState::Assets(value.clone()),
        PresenterState::Denied { error } => AssetsRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => AssetsRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}

fn recovery(error: &UiError, kind: u8) -> AssetsRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => AssetsRenderedState::Conflict { message, retryable },
        1 => AssetsRenderedState::Offline { message, retryable },
        2 => AssetsRenderedState::NotReady { message, retryable },
        _ => AssetsRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn presenter_and_view_do_not_import_transport_or_aggregate_types() {
        for source in [include_str!("presenter.rs"), include_str!("view.rs")] {
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
            assert!(!source.contains(concat!("erpx_", "assets")));
        }
    }
}
