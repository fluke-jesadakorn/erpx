use super::model::PlatformAdministrationContent;
use erpx_ui_foundation::{PresenterState, UiError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformAdministrationRenderedState {
    Idle,
    Loading {
        message: &'static str,
    },
    Submitting {
        message: &'static str,
    },
    Empty {
        message: &'static str,
    },
    Success {
        value: PlatformAdministrationContent,
    },
    Denied {
        message: String,
    },
    Stale {
        value: Option<PlatformAdministrationContent>,
        message: &'static str,
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
pub fn render_platform_administration_state(
    state: &PresenterState<PlatformAdministrationContent>,
) -> PlatformAdministrationRenderedState {
    match state {
        PresenterState::Idle => PlatformAdministrationRenderedState::Idle,
        PresenterState::Loading => PlatformAdministrationRenderedState::Loading {
            message: "Loading platform administration…",
        },
        PresenterState::Submitting => PlatformAdministrationRenderedState::Submitting {
            message: "Saving platform administration…",
        },
        PresenterState::Empty { .. } => PlatformAdministrationRenderedState::Empty {
            message: "No platform records match this view.",
        },
        PresenterState::Success { value, .. } => PlatformAdministrationRenderedState::Success {
            value: value.clone(),
        },
        PresenterState::Denied { error } => PlatformAdministrationRenderedState::Denied {
            message: error.message().into(),
        },
        PresenterState::Stale { value, metadata } => PlatformAdministrationRenderedState::Stale {
            value: value.clone(),
            message: "Platform projections may be out of date.",
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}
fn recovery(error: &UiError, kind: u8) -> PlatformAdministrationRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => PlatformAdministrationRenderedState::Conflict { message, retryable },
        1 => PlatformAdministrationRenderedState::Offline { message, retryable },
        2 => PlatformAdministrationRenderedState::NotReady { message, retryable },
        _ => PlatformAdministrationRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn presenter_and_view_have_no_transport_or_domain_imports() {
        for source in [include_str!("presenter.rs"), include_str!("view.rs")] {
            for forbidden in [
                concat!("erpx_", "proto"),
                concat!("tonic", "::"),
                concat!("prost", "::"),
                concat!("erpx_platform_", "administration"),
            ] {
                assert!(!source.contains(forbidden), "forbidden {forbidden}");
            }
        }
    }
    #[test]
    fn required_render_states_remain_explicit() {
        let source = include_str!("view.rs");
        for state in [
            "Loading",
            "Submitting",
            "Empty",
            "Success",
            "Denied",
            "Stale",
            "Conflict",
            "Offline",
            "NotReady",
            "Failed",
        ] {
            assert!(source.contains(state));
        }
    }
}
