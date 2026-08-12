use erpx_ui_foundation::{PresenterState, UiError};

use super::model::IdentityAccessContent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityAccessRenderedState {
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
    Content(IdentityAccessContent),
    Denied {
        message: String,
    },
    Stale {
        value: Option<IdentityAccessContent>,
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

pub fn render_identity_access_state(
    state: &PresenterState<IdentityAccessContent>,
) -> IdentityAccessRenderedState {
    match state {
        PresenterState::Idle => IdentityAccessRenderedState::Idle,
        PresenterState::Loading => IdentityAccessRenderedState::Loading {
            message: "Loading identity and access data…",
        },
        PresenterState::Submitting => IdentityAccessRenderedState::Submitting {
            message: "Saving identity and access changes…",
        },
        PresenterState::Empty { .. } => IdentityAccessRenderedState::Empty {
            message: "No identity or role records match this view.",
        },
        PresenterState::Success { value, .. } => {
            IdentityAccessRenderedState::Content(value.clone())
        }
        PresenterState::Denied { error } => IdentityAccessRenderedState::Denied {
            message: error.message().into(),
        },
        PresenterState::Stale { value, metadata } => IdentityAccessRenderedState::Stale {
            value: value.clone(),
            message: "Access data may be stale while projections catch up.",
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, RecoveryKind::Conflict),
        PresenterState::Offline { error } => recovery(error, RecoveryKind::Offline),
        PresenterState::NotReady { error } => recovery(error, RecoveryKind::NotReady),
        PresenterState::Failed { error } => recovery(error, RecoveryKind::Failed),
    }
}

#[derive(Debug, Clone, Copy)]
enum RecoveryKind {
    Conflict,
    Offline,
    NotReady,
    Failed,
}

fn recovery(error: &UiError, kind: RecoveryKind) -> IdentityAccessRenderedState {
    let message = error.message().to_owned();
    let retryable = error.retryable();
    match kind {
        RecoveryKind::Conflict => IdentityAccessRenderedState::Conflict { message, retryable },
        RecoveryKind::Offline => IdentityAccessRenderedState::Offline { message, retryable },
        RecoveryKind::NotReady => IdentityAccessRenderedState::NotReady { message, retryable },
        RecoveryKind::Failed => IdentityAccessRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    use erpx_cqrs::{FreshnessState, ProjectionMetadata, QueryConsistency, QueryMetadata};
    use erpx_kernel::CorrelationId;
    use erpx_ui_foundation::UiErrorKind;
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
    fn render_covers_all_required_states() {
        assert!(matches!(
            render_identity_access_state(&PresenterState::Loading),
            IdentityAccessRenderedState::Loading { .. }
        ));
        assert!(matches!(
            render_identity_access_state(&PresenterState::Submitting),
            IdentityAccessRenderedState::Submitting { .. }
        ));
        assert!(matches!(
            render_identity_access_state(&PresenterState::Empty {
                metadata: metadata(FreshnessState::Current)
            }),
            IdentityAccessRenderedState::Empty { .. }
        ));
        let content = IdentityAccessContent::Roles {
            values: Vec::new(),
            next_cursor: None,
        };
        assert!(matches!(
            render_identity_access_state(&PresenterState::Success {
                value: content.clone(),
                metadata: metadata(FreshnessState::Current)
            }),
            IdentityAccessRenderedState::Content(_)
        ));
        assert!(matches!(
            render_identity_access_state(&PresenterState::Stale {
                value: Some(content),
                metadata: metadata(FreshnessState::Stale)
            }),
            IdentityAccessRenderedState::Stale {
                lag_millis: Some(13),
                ..
            }
        ));
        for kind in [
            UiErrorKind::Denied,
            UiErrorKind::Conflict,
            UiErrorKind::Offline,
            UiErrorKind::NotReady,
        ] {
            let error = UiError::bounded(kind, "test", "test", true, None);
            let state = match kind {
                UiErrorKind::Denied => PresenterState::Denied { error },
                UiErrorKind::Conflict => PresenterState::Conflict { error },
                UiErrorKind::Offline => PresenterState::Offline { error },
                UiErrorKind::NotReady => PresenterState::NotReady { error },
                _ => unreachable!(),
            };
            assert!(!matches!(
                render_identity_access_state(&state),
                IdentityAccessRenderedState::Failed { .. }
            ));
        }
    }

    #[test]
    fn presenter_and_view_do_not_cross_transport_or_domain_boundaries() {
        for source in [include_str!("presenter.rs"), include_str!("view.rs")] {
            for forbidden in [
                concat!("erpx_", "proto"),
                concat!("tonic", "::"),
                concat!("prost", "::"),
                concat!("erpx_", "domain"),
                concat!("erpx_identity_", "access"),
            ] {
                assert!(!source.contains(forbidden));
            }
        }
    }
}
