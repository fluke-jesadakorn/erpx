use erpx_cqrs::{FreshnessState, QueryMetadata, QueryResult};
use serde::{Deserialize, Serialize};

use crate::{UiError, UiErrorKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresenterState<T> {
    Idle,
    Loading,
    Submitting,
    Empty {
        metadata: QueryMetadata,
    },
    Success {
        value: T,
        metadata: QueryMetadata,
    },
    Denied {
        error: UiError,
    },
    Stale {
        value: Option<T>,
        metadata: QueryMetadata,
    },
    Conflict {
        error: UiError,
    },
    Offline {
        error: UiError,
    },
    NotReady {
        error: UiError,
    },
    Failed {
        error: UiError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresenterEvent<T> {
    LoadStarted,
    SubmitStarted,
    QueryResolved(QueryResult<Option<T>>),
    Failed(UiError),
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presenter<T> {
    state: PresenterState<T>,
}

impl<T> Default for Presenter<T> {
    fn default() -> Self {
        Self {
            state: PresenterState::Idle,
        }
    }
}

impl<T> Presenter<T> {
    pub fn state(&self) -> &PresenterState<T> {
        &self.state
    }

    pub fn transition(&mut self, event: PresenterEvent<T>) {
        self.state = match event {
            PresenterEvent::LoadStarted => PresenterState::Loading,
            PresenterEvent::SubmitStarted => PresenterState::Submitting,
            PresenterEvent::QueryResolved(result) => resolved(result),
            PresenterEvent::Failed(error) => failed(error),
            PresenterEvent::Reset => PresenterState::Idle,
        };
    }

    pub fn into_state(self) -> PresenterState<T> {
        self.state
    }
}

fn resolved<T>(result: QueryResult<Option<T>>) -> PresenterState<T> {
    let QueryResult { value, metadata } = result;
    match metadata.projection.freshness {
        FreshnessState::Current => match value {
            Some(value) => PresenterState::Success { value, metadata },
            None => PresenterState::Empty { metadata },
        },
        FreshnessState::Stale | FreshnessState::Rebuilding | FreshnessState::Unavailable => {
            PresenterState::Stale { value, metadata }
        }
    }
}

fn failed<T>(error: UiError) -> PresenterState<T> {
    match error.kind() {
        UiErrorKind::Denied => PresenterState::Denied { error },
        UiErrorKind::Conflict => PresenterState::Conflict { error },
        UiErrorKind::Offline => PresenterState::Offline { error },
        UiErrorKind::NotReady => PresenterState::NotReady { error },
        UiErrorKind::Invalid | UiErrorKind::NotFound | UiErrorKind::Failed => {
            PresenterState::Failed { error }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_cqrs::{ProjectionMetadata, QueryConsistency, QueryMetadata, QueryResult};
    use erpx_kernel::CorrelationId;

    fn correlation_id(value: u128) -> CorrelationId {
        CorrelationId::try_from_uuid(
            format!("{value:032x}")
                .parse()
                .expect("test UUID must parse"),
        )
        .unwrap()
    }

    fn result<T>(value: Option<T>, freshness: FreshnessState) -> QueryResult<Option<T>> {
        QueryResult {
            value,
            metadata: QueryMetadata {
                correlation_id: correlation_id(1),
                projection: ProjectionMetadata {
                    checkpoint: None,
                    freshness,
                    lag_millis: Some(5),
                },
                consistency: QueryConsistency::Eventual,
            },
        }
    }

    #[test]
    fn presenter_covers_progress_empty_success_and_stale_states() {
        let mut presenter = Presenter::<String>::default();
        presenter.transition(PresenterEvent::LoadStarted);
        assert!(matches!(presenter.state(), PresenterState::Loading));
        presenter.transition(PresenterEvent::SubmitStarted);
        assert!(matches!(presenter.state(), PresenterState::Submitting));
        presenter.transition(PresenterEvent::QueryResolved(result(
            None,
            FreshnessState::Current,
        )));
        assert!(matches!(presenter.state(), PresenterState::Empty { .. }));
        presenter.transition(PresenterEvent::QueryResolved(result(
            Some("inspection".into()),
            FreshnessState::Current,
        )));
        assert!(matches!(presenter.state(), PresenterState::Success { .. }));
        presenter.transition(PresenterEvent::QueryResolved(result(
            Some("cached inspection".into()),
            FreshnessState::Rebuilding,
        )));
        assert!(matches!(presenter.state(), PresenterState::Stale { .. }));
    }

    #[test]
    fn presenter_maps_bounded_errors_to_explicit_recovery_states() {
        let cases = [
            (UiErrorKind::Denied, "denied"),
            (UiErrorKind::Conflict, "conflict"),
            (UiErrorKind::Offline, "offline"),
            (UiErrorKind::NotReady, "not-ready"),
        ];
        for (kind, expected) in cases {
            let mut presenter = Presenter::<()>::default();
            presenter.transition(PresenterEvent::Failed(UiError::bounded(
                kind, "failure", "failed", true, None,
            )));
            let observed = match presenter.state() {
                PresenterState::Denied { .. } => "denied",
                PresenterState::Conflict { .. } => "conflict",
                PresenterState::Offline { .. } => "offline",
                PresenterState::NotReady { .. } => "not-ready",
                _ => "other",
            };
            assert_eq!(observed, expected);
        }
    }
}
