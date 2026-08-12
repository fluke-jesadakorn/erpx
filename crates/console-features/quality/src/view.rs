use erpx_quality::{InspectionList, InspectionView};
use erpx_ui_foundation::{PresenterState, UiError};

use super::presenter::QualityContent;

/// Target-neutral render result. A Dioxus component maps these variants to
/// elements; keeping the state model here makes every recovery path testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualityRenderedState {
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
    Inspection {
        value: InspectionView,
    },
    Inspections {
        value: InspectionList,
    },
    Denied {
        message: String,
    },
    Stale {
        value: Option<QualityContent>,
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

pub fn render_quality_state(state: &PresenterState<QualityContent>) -> QualityRenderedState {
    match state {
        PresenterState::Idle => QualityRenderedState::Idle,
        PresenterState::Loading => QualityRenderedState::Loading {
            message: "Loading quality inspections…",
        },
        PresenterState::Submitting => QualityRenderedState::Submitting {
            message: "Saving quality inspection…",
        },
        PresenterState::Empty { .. } => QualityRenderedState::Empty {
            message: "No quality inspections match this view.",
        },
        PresenterState::Success { value, .. } => success(value.clone()),
        PresenterState::Denied { error } => QualityRenderedState::Denied {
            message: error.message().to_owned(),
        },
        PresenterState::Stale { value, metadata } => QualityRenderedState::Stale {
            value: value.clone(),
            message: "This view may be out of date while Quality catches up.",
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, RecoveryKind::Conflict),
        PresenterState::Offline { error } => recovery(error, RecoveryKind::Offline),
        PresenterState::NotReady { error } => recovery(error, RecoveryKind::NotReady),
        PresenterState::Failed { error } => recovery(error, RecoveryKind::Failed),
    }
}

fn success(value: QualityContent) -> QualityRenderedState {
    match value {
        QualityContent::Inspection(value) => QualityRenderedState::Inspection { value },
        QualityContent::Inspections(value) => QualityRenderedState::Inspections { value },
    }
}

#[derive(Debug, Clone, Copy)]
enum RecoveryKind {
    Conflict,
    Offline,
    NotReady,
    Failed,
}

fn recovery(error: &UiError, kind: RecoveryKind) -> QualityRenderedState {
    let message = error.message().to_owned();
    let retryable = error.retryable();
    match kind {
        RecoveryKind::Conflict => QualityRenderedState::Conflict { message, retryable },
        RecoveryKind::Offline => QualityRenderedState::Offline { message, retryable },
        RecoveryKind::NotReady => QualityRenderedState::NotReady { message, retryable },
        RecoveryKind::Failed => QualityRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    use erpx_cqrs::{FreshnessState, ProjectionMetadata, QueryConsistency, QueryMetadata};
    use erpx_kernel::CorrelationId;
    use erpx_ui_foundation::{UiError, UiErrorKind};
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
    fn render_covers_progress_empty_and_all_recovery_states() {
        assert!(matches!(
            render_quality_state(&PresenterState::Loading),
            QualityRenderedState::Loading { .. }
        ));
        assert!(matches!(
            render_quality_state(&PresenterState::Submitting),
            QualityRenderedState::Submitting { .. }
        ));
        assert!(matches!(
            render_quality_state(&PresenterState::Empty {
                metadata: metadata(FreshnessState::Current),
            }),
            QualityRenderedState::Empty { .. }
        ));

        let content = QualityContent::Inspections(InspectionList {
            inspections: Vec::new(),
            next_cursor: None,
        });
        assert!(matches!(
            render_quality_state(&PresenterState::Success {
                value: content.clone(),
                metadata: metadata(FreshnessState::Current),
            }),
            QualityRenderedState::Inspections { .. }
        ));
        assert!(matches!(
            render_quality_state(&PresenterState::Stale {
                value: Some(content),
                metadata: metadata(FreshnessState::Stale),
            }),
            QualityRenderedState::Stale {
                lag_millis: Some(13),
                ..
            }
        ));

        for (kind, state) in [
            (UiErrorKind::Denied, "denied"),
            (UiErrorKind::Conflict, "conflict"),
            (UiErrorKind::Offline, "offline"),
            (UiErrorKind::NotReady, "not_ready"),
        ] {
            let error = UiError::bounded(kind, state, state, true, None);
            let presenter_state = match kind {
                UiErrorKind::Denied => PresenterState::Denied { error },
                UiErrorKind::Conflict => PresenterState::Conflict { error },
                UiErrorKind::Offline => PresenterState::Offline { error },
                UiErrorKind::NotReady => PresenterState::NotReady { error },
                _ => unreachable!(),
            };
            let rendered = render_quality_state(&presenter_state);
            let actual = match rendered {
                QualityRenderedState::Denied { .. } => "denied",
                QualityRenderedState::Conflict { .. } => "conflict",
                QualityRenderedState::Offline { .. } => "offline",
                QualityRenderedState::NotReady { .. } => "not_ready",
                _ => "unexpected",
            };
            assert_eq!(actual, state);
        }
    }

    #[test]
    fn presenter_and_view_sources_do_not_cross_transport_or_domain_boundaries() {
        let sources = [
            ("presenter", include_str!("presenter.rs")),
            ("view", include_str!("view.rs")),
        ];
        for (owner, source) in sources {
            for forbidden in [
                concat!("erpx_", "proto"),
                concat!("tonic", "::"),
                concat!("prost", "::"),
                concat!("erpx_", "domain"),
                concat!("Quality", "Inspection"),
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{owner} must not import or expose {forbidden}"
                );
            }
        }
    }
}
