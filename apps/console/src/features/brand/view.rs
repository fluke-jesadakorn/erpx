use erpx_brand::{BrandHistory, BrandProfileView, ResolvedBrandView};
use erpx_ui_foundation::{PresenterState, UiError};

use super::presenter::BrandContent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrandRenderedState {
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
    Profile {
        value: Box<BrandProfileView>,
    },
    Resolved {
        value: Box<ResolvedBrandView>,
    },
    History {
        value: BrandHistory,
    },
    Denied {
        message: String,
    },
    Stale {
        value: Option<BrandContent>,
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

pub fn render_brand_state(state: &PresenterState<BrandContent>) -> BrandRenderedState {
    match state {
        PresenterState::Idle => BrandRenderedState::Idle,
        PresenterState::Loading => BrandRenderedState::Loading {
            message: "Loading brand…",
        },
        PresenterState::Submitting => BrandRenderedState::Submitting {
            message: "Saving brand…",
        },
        PresenterState::Empty { .. } => BrandRenderedState::Empty {
            message: "No Brand profile exists for this scope.",
        },
        PresenterState::Success { value, .. } => success(value.clone()),
        PresenterState::Denied { error } => BrandRenderedState::Denied {
            message: error.message().to_owned(),
        },
        PresenterState::Stale { value, metadata } => BrandRenderedState::Stale {
            value: value.clone(),
            message: "This Brand view may be out of date while projections catch up.",
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, RecoveryKind::Conflict),
        PresenterState::Offline { error } => recovery(error, RecoveryKind::Offline),
        PresenterState::NotReady { error } => recovery(error, RecoveryKind::NotReady),
        PresenterState::Failed { error } => recovery(error, RecoveryKind::Failed),
    }
}

fn success(value: BrandContent) -> BrandRenderedState {
    match value {
        BrandContent::Profile(value) => BrandRenderedState::Profile { value },
        BrandContent::Resolved(value) => BrandRenderedState::Resolved { value },
        BrandContent::History(value) => BrandRenderedState::History { value },
    }
}

#[derive(Debug, Clone, Copy)]
enum RecoveryKind {
    Conflict,
    Offline,
    NotReady,
    Failed,
}

fn recovery(error: &UiError, kind: RecoveryKind) -> BrandRenderedState {
    let message = error.message().to_owned();
    let retryable = error.retryable();
    match kind {
        RecoveryKind::Conflict => BrandRenderedState::Conflict { message, retryable },
        RecoveryKind::Offline => BrandRenderedState::Offline { message, retryable },
        RecoveryKind::NotReady => BrandRenderedState::NotReady { message, retryable },
        RecoveryKind::Failed => BrandRenderedState::Failed { message, retryable },
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
    fn render_covers_progress_empty_stale_and_recovery_states() {
        assert!(matches!(
            render_brand_state(&PresenterState::Loading),
            BrandRenderedState::Loading { .. }
        ));
        assert!(matches!(
            render_brand_state(&PresenterState::Submitting),
            BrandRenderedState::Submitting { .. }
        ));
        assert!(matches!(
            render_brand_state(&PresenterState::Empty {
                metadata: metadata(FreshnessState::Current),
            }),
            BrandRenderedState::Empty { .. }
        ));
        let content = BrandContent::History(BrandHistory {
            entries: Vec::new(),
            next_cursor: None,
        });
        assert!(matches!(
            render_brand_state(&PresenterState::Success {
                value: content.clone(),
                metadata: metadata(FreshnessState::Current),
            }),
            BrandRenderedState::History { .. }
        ));
        assert!(matches!(
            render_brand_state(&PresenterState::Stale {
                value: Some(content),
                metadata: metadata(FreshnessState::Rebuilding),
            }),
            BrandRenderedState::Stale {
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
            let actual = match render_brand_state(&state) {
                BrandRenderedState::Denied { .. } => "denied",
                BrandRenderedState::Conflict { .. } => "conflict",
                BrandRenderedState::Offline { .. } => "offline",
                BrandRenderedState::NotReady { .. } => "not_ready",
                _ => "unexpected",
            };
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn view_has_no_transport_or_write_aggregate_dependency() {
        let source = include_str!("view.rs");
        for forbidden in [
            concat!("erpx_", "proto"),
            concat!("tonic", "::"),
            concat!("prost", "::"),
            concat!("Brand", "Profile>"),
        ] {
            assert!(!source.contains(forbidden));
        }
    }
}
