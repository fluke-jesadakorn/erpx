use erpx_customer_authority::{CustomerAuthorityView, CustomerList};
use erpx_ui_foundation::{PresenterState, UiError};

use super::presenter::CustomerAuthorityContent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomerAuthorityRenderedState {
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
    Customer {
        value: Box<CustomerAuthorityView>,
    },
    Customers {
        value: CustomerList,
    },
    Denied {
        message: String,
    },
    Stale {
        value: Option<CustomerAuthorityContent>,
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

pub fn render_customer_authority_state(
    state: &PresenterState<CustomerAuthorityContent>,
) -> CustomerAuthorityRenderedState {
    match state {
        PresenterState::Idle => CustomerAuthorityRenderedState::Idle,
        PresenterState::Loading => CustomerAuthorityRenderedState::Loading {
            message: "Loading customer authority…",
        },
        PresenterState::Submitting => CustomerAuthorityRenderedState::Submitting {
            message: "Saving customer authority…",
        },
        PresenterState::Empty { .. } => CustomerAuthorityRenderedState::Empty {
            message: "No customers match this authority view.",
        },
        PresenterState::Success { value, .. } => success(value.clone()),
        PresenterState::Denied { error } => CustomerAuthorityRenderedState::Denied {
            message: error.message().to_owned(),
        },
        PresenterState::Stale { value, metadata } => CustomerAuthorityRenderedState::Stale {
            value: value.clone(),
            message: "Customer authority may be out of date while projection catches up.",
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, RecoveryKind::Conflict),
        PresenterState::Offline { error } => recovery(error, RecoveryKind::Offline),
        PresenterState::NotReady { error } => recovery(error, RecoveryKind::NotReady),
        PresenterState::Failed { error } => recovery(error, RecoveryKind::Failed),
    }
}

fn success(value: CustomerAuthorityContent) -> CustomerAuthorityRenderedState {
    match value {
        CustomerAuthorityContent::Customer(value) => {
            CustomerAuthorityRenderedState::Customer { value }
        }
        CustomerAuthorityContent::Customers(value) => {
            CustomerAuthorityRenderedState::Customers { value }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RecoveryKind {
    Conflict,
    Offline,
    NotReady,
    Failed,
}

fn recovery(error: &UiError, kind: RecoveryKind) -> CustomerAuthorityRenderedState {
    let message = error.message().to_owned();
    let retryable = error.retryable();
    match kind {
        RecoveryKind::Conflict => CustomerAuthorityRenderedState::Conflict { message, retryable },
        RecoveryKind::Offline => CustomerAuthorityRenderedState::Offline { message, retryable },
        RecoveryKind::NotReady => CustomerAuthorityRenderedState::NotReady { message, retryable },
        RecoveryKind::Failed => CustomerAuthorityRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    use erpx_cqrs::{FreshnessState, ProjectionMetadata, QueryConsistency, QueryMetadata};
    use erpx_kernel::CorrelationId;
    use erpx_ui_foundation::{PresenterState, UiErrorKind};
    use uuid::Uuid;

    use super::*;

    fn metadata(freshness: FreshnessState) -> QueryMetadata {
        QueryMetadata {
            correlation_id: CorrelationId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            projection: ProjectionMetadata {
                checkpoint: None,
                freshness,
                lag_millis: Some(13),
            },
            consistency: QueryConsistency::Eventual,
        }
    }

    #[test]
    fn render_covers_progress_success_stale_and_recovery_states() {
        assert!(matches!(
            render_customer_authority_state(&PresenterState::Loading),
            CustomerAuthorityRenderedState::Loading { .. }
        ));
        assert!(matches!(
            render_customer_authority_state(&PresenterState::Submitting),
            CustomerAuthorityRenderedState::Submitting { .. }
        ));
        assert!(matches!(
            render_customer_authority_state(&PresenterState::Empty {
                metadata: metadata(FreshnessState::Current),
            }),
            CustomerAuthorityRenderedState::Empty { .. }
        ));
        let content = CustomerAuthorityContent::Customers(CustomerList {
            customers: Vec::new(),
            next_cursor: None,
        });
        assert!(matches!(
            render_customer_authority_state(&PresenterState::Success {
                value: content.clone(),
                metadata: metadata(FreshnessState::Current),
            }),
            CustomerAuthorityRenderedState::Customers { .. }
        ));
        assert!(matches!(
            render_customer_authority_state(&PresenterState::Stale {
                value: Some(content),
                metadata: metadata(FreshnessState::Rebuilding),
            }),
            CustomerAuthorityRenderedState::Stale {
                lag_millis: Some(13),
                ..
            }
        ));
        for (kind, expected) in [
            (UiErrorKind::Denied, "denied"),
            (UiErrorKind::Conflict, "conflict"),
            (UiErrorKind::Offline, "offline"),
            (UiErrorKind::NotReady, "not_ready"),
        ] {
            let error = UiError::bounded(kind, expected, expected, true, None);
            let state = match kind {
                UiErrorKind::Denied => PresenterState::Denied { error },
                UiErrorKind::Conflict => PresenterState::Conflict { error },
                UiErrorKind::Offline => PresenterState::Offline { error },
                UiErrorKind::NotReady => PresenterState::NotReady { error },
                _ => unreachable!(),
            };
            let actual = match render_customer_authority_state(&state) {
                CustomerAuthorityRenderedState::Denied { .. } => "denied",
                CustomerAuthorityRenderedState::Conflict { .. } => "conflict",
                CustomerAuthorityRenderedState::Offline { .. } => "offline",
                CustomerAuthorityRenderedState::NotReady { .. } => "not_ready",
                _ => "unexpected",
            };
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn presenter_and_view_do_not_import_transport_or_write_aggregate() {
        for (owner, source) in [
            ("presenter", include_str!("presenter.rs")),
            ("view", include_str!("view.rs")),
        ] {
            for forbidden in [
                concat!("erpx_", "proto"),
                concat!("tonic", "::"),
                concat!("prost", "::"),
                concat!("Customer", "Account"),
                concat!("Customer", "Identity"),
            ] {
                assert!(!source.contains(forbidden), "{owner} exposes {forbidden}");
            }
        }
    }
}
