use super::presenter::CommercialContent;
use erpx_commercial::{CatalogPlanView, EntitlementView, Page, SubscriptionView};
use erpx_ui_foundation::{PresenterState, UiError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommercialRenderedState {
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
    CatalogPlan {
        value: CatalogPlanView,
    },
    CatalogPlans {
        value: Page<CatalogPlanView>,
    },
    Subscription {
        value: SubscriptionView,
    },
    Subscriptions {
        value: Page<SubscriptionView>,
    },
    Entitlement {
        value: EntitlementView,
    },
    Entitlements {
        value: Page<EntitlementView>,
    },
    Denied {
        message: String,
    },
    Stale {
        value: Option<CommercialContent>,
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

pub fn render_commercial_state(
    state: &PresenterState<CommercialContent>,
) -> CommercialRenderedState {
    match state {
        PresenterState::Idle => CommercialRenderedState::Idle,
        PresenterState::Loading => CommercialRenderedState::Loading {
            message: "Loading commercial data…",
        },
        PresenterState::Submitting => CommercialRenderedState::Submitting {
            message: "Saving commercial change…",
        },
        PresenterState::Empty { .. } => CommercialRenderedState::Empty {
            message: "No commercial records match this view.",
        },
        PresenterState::Success { value, .. } => success(value.clone()),
        PresenterState::Denied { error } => CommercialRenderedState::Denied {
            message: error.message().to_owned(),
        },
        PresenterState::Stale { value, metadata } => CommercialRenderedState::Stale {
            value: value.clone(),
            message: "This view may be out of date while Commercial catches up.",
            lag_millis: metadata.projection.lag_millis,
        },
        PresenterState::Conflict { error } => recovery(error, Recovery::Conflict),
        PresenterState::Offline { error } => recovery(error, Recovery::Offline),
        PresenterState::NotReady { error } => recovery(error, Recovery::NotReady),
        PresenterState::Failed { error } => recovery(error, Recovery::Failed),
    }
}
fn success(value: CommercialContent) -> CommercialRenderedState {
    match value {
        CommercialContent::CatalogPlan(value) => CommercialRenderedState::CatalogPlan { value },
        CommercialContent::CatalogPlans(value) => CommercialRenderedState::CatalogPlans { value },
        CommercialContent::Subscription(value) => CommercialRenderedState::Subscription { value },
        CommercialContent::Subscriptions(value) => CommercialRenderedState::Subscriptions { value },
        CommercialContent::Entitlement(value) => CommercialRenderedState::Entitlement { value },
        CommercialContent::Entitlements(value) => CommercialRenderedState::Entitlements { value },
    }
}
#[derive(Debug, Clone, Copy)]
enum Recovery {
    Conflict,
    Offline,
    NotReady,
    Failed,
}
fn recovery(error: &UiError, kind: Recovery) -> CommercialRenderedState {
    let message = error.message().to_owned();
    let retryable = error.retryable();
    match kind {
        Recovery::Conflict => CommercialRenderedState::Conflict { message, retryable },
        Recovery::Offline => CommercialRenderedState::Offline { message, retryable },
        Recovery::NotReady => CommercialRenderedState::NotReady { message, retryable },
        Recovery::Failed => CommercialRenderedState::Failed { message, retryable },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_cqrs::{FreshnessState, ProjectionMetadata, QueryConsistency, QueryMetadata};
    use erpx_kernel::CorrelationId;
    use erpx_ui_foundation::UiErrorKind;
    use uuid::Uuid;
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
    fn render_covers_progress_empty_stale_and_recovery() {
        assert!(matches!(
            render_commercial_state(&PresenterState::Loading),
            CommercialRenderedState::Loading { .. }
        ));
        assert!(matches!(
            render_commercial_state(&PresenterState::Submitting),
            CommercialRenderedState::Submitting { .. }
        ));
        assert!(matches!(
            render_commercial_state(&PresenterState::Empty {
                metadata: metadata(FreshnessState::Current)
            }),
            CommercialRenderedState::Empty { .. }
        ));
        let content = CommercialContent::Subscriptions(Page {
            records: vec![],
            next_cursor: None,
        });
        assert!(matches!(
            render_commercial_state(&PresenterState::Stale {
                value: Some(content),
                metadata: metadata(FreshnessState::Stale)
            }),
            CommercialRenderedState::Stale {
                lag_millis: Some(13),
                ..
            }
        ));
        for (kind, name) in [
            (UiErrorKind::Denied, "denied"),
            (UiErrorKind::Conflict, "conflict"),
            (UiErrorKind::Offline, "offline"),
            (UiErrorKind::NotReady, "not_ready"),
        ] {
            let error = UiError::bounded(kind, name, name, true, None);
            let state = match kind {
                UiErrorKind::Denied => PresenterState::Denied { error },
                UiErrorKind::Conflict => PresenterState::Conflict { error },
                UiErrorKind::Offline => PresenterState::Offline { error },
                UiErrorKind::NotReady => PresenterState::NotReady { error },
                _ => unreachable!(),
            };
            let rendered = render_commercial_state(&state);
            let actual = match rendered {
                CommercialRenderedState::Denied { .. } => "denied",
                CommercialRenderedState::Conflict { .. } => "conflict",
                CommercialRenderedState::Offline { .. } => "offline",
                CommercialRenderedState::NotReady { .. } => "not_ready",
                _ => "unexpected",
            };
            assert_eq!(actual, name);
        }
    }
    #[test]
    fn presenter_and_view_sources_do_not_cross_transport_or_domain_boundaries() {
        for (owner, source) in [
            ("presenter", include_str!("presenter.rs")),
            ("view", include_str!("view.rs")),
        ] {
            for forbidden in [
                concat!("erpx_", "proto"),
                concat!("tonic", "::"),
                concat!("prost", "::"),
                concat!("erpx_", "domain"),
                concat!("Commercial", "Aggregate"),
                concat!("verification", "_token"),
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{owner} must not expose {forbidden}"
                );
            }
        }
    }
}
