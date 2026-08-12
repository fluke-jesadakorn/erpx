use super::presenter::CrmContent;
use erpx_ui_foundation::{PresenterState, UiError};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrmRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Content(CrmContent),
    Denied(String),
    Stale {
        value: Option<CrmContent>,
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
pub fn render_crm_state(state: &PresenterState<CrmContent>) -> CrmRenderedState {
    match state {
        PresenterState::Idle => CrmRenderedState::Idle,
        PresenterState::Loading => CrmRenderedState::Loading,
        PresenterState::Submitting => CrmRenderedState::Submitting,
        PresenterState::Empty { .. } => CrmRenderedState::Empty,
        PresenterState::Success { value, .. } => CrmRenderedState::Content(value.clone()),
        PresenterState::Denied { error } => CrmRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => CrmRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}
fn recovery(error: &UiError, kind: u8) -> CrmRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => CrmRenderedState::Conflict { message, retryable },
        1 => CrmRenderedState::Offline { message, retryable },
        2 => CrmRenderedState::NotReady { message, retryable },
        _ => CrmRenderedState::Failed { message, retryable },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_states_render() {
        assert_eq!(
            render_crm_state(&PresenterState::Loading),
            CrmRenderedState::Loading
        );
        assert_eq!(
            render_crm_state(&PresenterState::Submitting),
            CrmRenderedState::Submitting
        );
    }
    #[test]
    fn view_is_wire_free() {
        for source in [include_str!("view.rs"), include_str!("models.rs")] {
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("erpx_", "crm")));
            assert!(!source.contains(concat!("tonic", "::")));
        }
    }
}
