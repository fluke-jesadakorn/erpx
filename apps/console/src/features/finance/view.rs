use super::presenter::FinanceContent;
use erpx_ui_foundation::{PresenterState, UiError};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinanceRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Content(FinanceContent),
    Denied(String),
    Stale {
        value: Option<FinanceContent>,
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
pub fn render_finance_state(s: &PresenterState<FinanceContent>) -> FinanceRenderedState {
    match s {
        PresenterState::Idle => FinanceRenderedState::Idle,
        PresenterState::Loading => FinanceRenderedState::Loading,
        PresenterState::Submitting => FinanceRenderedState::Submitting,
        PresenterState::Empty { .. } => FinanceRenderedState::Empty,
        PresenterState::Success { value, .. } => FinanceRenderedState::Content(value.clone()),
        PresenterState::Denied { error } => FinanceRenderedState::Denied(error.message().into()),
        PresenterState::Stale { value, metadata } => FinanceRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}
fn recovery(e: &UiError, k: u8) -> FinanceRenderedState {
    let message = e.message().into();
    let retryable = e.retryable();
    match k {
        0 => FinanceRenderedState::Conflict { message, retryable },
        1 => FinanceRenderedState::Offline { message, retryable },
        2 => FinanceRenderedState::NotReady { message, retryable },
        _ => FinanceRenderedState::Failed { message, retryable },
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_states_render() {
        assert_eq!(
            render_finance_state(&PresenterState::Loading),
            FinanceRenderedState::Loading
        );
        assert_eq!(
            render_finance_state(&PresenterState::Submitting),
            FinanceRenderedState::Submitting
        );
    }
    #[test]
    fn view_and_models_are_wire_and_domain_free() {
        for source in [include_str!("view.rs"), include_str!("models.rs")] {
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("erpx_", "finance")));
            assert!(!source.contains(concat!("ton", "ic::")));
        }
    }
}
