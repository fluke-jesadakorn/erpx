use erpx_ui_foundation::{PresenterState, UiError};

use super::presenter::PeoplePayrollContent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeoplePayrollRenderedState {
    Idle,
    Loading,
    Submitting,
    Empty,
    Content(PeoplePayrollContent),
    Denied(String),
    Stale {
        value: Option<PeoplePayrollContent>,
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

pub fn render_people_payroll_state(
    state: &PresenterState<PeoplePayrollContent>,
) -> PeoplePayrollRenderedState {
    match state {
        PresenterState::Idle => PeoplePayrollRenderedState::Idle,
        PresenterState::Loading => PeoplePayrollRenderedState::Loading,
        PresenterState::Submitting => PeoplePayrollRenderedState::Submitting,
        PresenterState::Empty { .. } => PeoplePayrollRenderedState::Empty,
        PresenterState::Success { value, .. } => PeoplePayrollRenderedState::Content(value.clone()),
        PresenterState::Denied { error } => {
            PeoplePayrollRenderedState::Denied(error.message().into())
        }
        PresenterState::Stale { value, metadata } => PeoplePayrollRenderedState::Stale {
            value: value.clone(),
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, 0),
        PresenterState::Offline { error } => recovery(error, 1),
        PresenterState::NotReady { error } => recovery(error, 2),
        PresenterState::Failed { error } => recovery(error, 3),
    }
}
fn recovery(error: &UiError, kind: u8) -> PeoplePayrollRenderedState {
    let message = error.message().into();
    let retryable = error.retryable();
    match kind {
        0 => PeoplePayrollRenderedState::Conflict { message, retryable },
        1 => PeoplePayrollRenderedState::Offline { message, retryable },
        2 => PeoplePayrollRenderedState::NotReady { message, retryable },
        _ => PeoplePayrollRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_ui_foundation::{PresenterState, UiErrorKind};
    #[test]
    fn view_renders_every_recovery_state() {
        for (kind, discriminator) in [
            (UiErrorKind::Denied, 0),
            (UiErrorKind::Conflict, 1),
            (UiErrorKind::Offline, 2),
            (UiErrorKind::NotReady, 3),
        ] {
            let error = UiError::bounded(kind, "x", "x", true, None);
            let state = match discriminator {
                0 => PresenterState::Denied { error },
                1 => PresenterState::Conflict { error },
                2 => PresenterState::Offline { error },
                _ => PresenterState::NotReady { error },
            };
            assert!(!matches!(
                render_people_payroll_state(&state),
                PeoplePayrollRenderedState::Failed { .. }
            ));
        }
    }
    #[test]
    fn presenter_view_and_models_are_wire_and_aggregate_free() {
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_people", "_payroll")));
            assert!(!source.contains(concat!("erpx_", "proto")));
            assert!(!source.contains(concat!("tonic", "::")));
        }
    }
}
