use dioxus::dioxus_core::{AttributeValue, Mutation};
use dioxus::prelude::*;
use erpx_ui::{
    OperationsDisplayModel, OperationsFieldInput, OperationsPage, OperationsProjectionMetadata,
    OperationsScope, OperationsScopeKind, OperationsState, PlatformOperationsAudience,
    PlatformOperationsRouteRegistry, RouteAuthRequirement, UiStateKind,
};

#[component]
fn state_panel_fixture(state: UiStateKind) -> Element {
    rsx! {
        erpx_ui::StatePanel {
            state,
            title: "State".to_string(),
            message: "The state is rendered from the authoritative boundary.".to_string(),
            action_label: None,
            on_action: None,
        }
    }
}

fn rendered_state_panel_attributes(state: UiStateKind) -> Vec<(&'static str, String)> {
    let mut dom =
        VirtualDom::new_with_props(state_panel_fixture, state_panel_fixtureProps { state });
    dom.rebuild_to_vec()
        .edits
        .into_iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetAttribute {
                name,
                value: AttributeValue::Text(value),
                ..
            } if matches!(
                name,
                "data-ui-state"
                    | "data-ui-authorization"
                    | "data-ui-cached-payload"
                    | "role"
                    | "aria-live"
                    | "aria-atomic"
                    | "aria-busy"
            ) =>
            {
                Some((name, value))
            }
            _ => None,
        })
        .collect()
}

fn rendered_attribute<'a>(attributes: &'a [(&'static str, String)], name: &'static str) -> &'a str {
    attributes
        .iter()
        .find_map(|(attribute, value)| (*attribute == name).then_some(value.as_str()))
        .unwrap_or_else(|| panic!("rendered StatePanel is missing {name}"))
}

fn metadata() -> OperationsProjectionMetadata {
    OperationsProjectionMetadata::fresh(
        OperationsScope::new("holding_123", "Demo holding", OperationsScopeKind::Holding),
        "2026-07-24T00:00:00Z",
    )
}

fn safe_model() -> OperationsDisplayModel {
    OperationsDisplayModel::from_untrusted_fields(
        OperationsPage::Overview,
        metadata(),
        [
            OperationsFieldInput::new("exception-status", "escalated"),
            OperationsFieldInput::new("source_reference", "access_token=protected"),
        ],
    )
}

#[test]
fn g12_platform_operations_state_contract_remains_fail_closed_and_read_only() {
    let registry = PlatformOperationsRouteRegistry::new();
    assert!(!registry.definitions().is_empty());
    assert!(registry.definitions().iter().all(|definition| {
        definition.audience == PlatformOperationsAudience::Platform
            && definition.auth == RouteAuthRequirement::Authenticated
    }));

    let cached_model = safe_model();
    assert_eq!(cached_model.fields().len(), 1);
    assert_eq!(cached_model.redacted_field_count(), 1);
    assert_eq!(cached_model.fields()[0].value(), "escalated");

    let states = [
        OperationsState::Loading {
            metadata: Some(metadata()),
        },
        OperationsState::Empty {
            metadata: metadata(),
        },
        OperationsState::Denied {
            metadata: None,
            reason: "authorization denied".into(),
        },
        OperationsState::Stale {
            data: cached_model.clone(),
            metadata: OperationsProjectionMetadata::stale(
                OperationsScope::new("holding_123", "Demo holding", OperationsScopeKind::Holding),
                "2026-07-23T00:00:00Z",
            ),
            age_seconds: 301,
            reason: "projection lag".into(),
        },
        OperationsState::ReadOnly {
            data: cached_model.clone(),
            metadata: metadata(),
            reason: "read-only context".into(),
        },
        OperationsState::Conflict {
            data: Some(cached_model.clone()),
            metadata: Some(metadata()),
            reason: "concurrent revision".into(),
        },
        OperationsState::Offline {
            data: Some(cached_model),
            metadata: Some(metadata()),
            reason: "backend unavailable".into(),
        },
        OperationsState::Error {
            reason: "request failed".into(),
        },
    ];
    let expected = [
        (
            UiStateKind::Loading,
            "loading",
            "pending",
            "status",
            "polite",
        ),
        (
            UiStateKind::Empty,
            "empty",
            "authorized",
            "status",
            "polite",
        ),
        (
            UiStateKind::Denied,
            "denied",
            "denied",
            "alert",
            "assertive",
        ),
        (
            UiStateKind::Stale,
            "stale",
            "authorized",
            "status",
            "polite",
        ),
        (
            UiStateKind::ReadOnly,
            "read-only",
            "read-only",
            "status",
            "polite",
        ),
        (
            UiStateKind::Conflict,
            "conflict",
            "authorized",
            "alert",
            "assertive",
        ),
        (
            UiStateKind::Offline,
            "offline",
            "unknown",
            "status",
            "polite",
        ),
        (UiStateKind::Error, "error", "unknown", "alert", "assertive"),
    ];

    for (state, (expected_kind, dom_key, authorization_key, role, aria_live)) in
        states.into_iter().zip(expected)
    {
        let kind = state
            .kind()
            .expect("every non-ready state has a shared kind");
        let contract = kind.accessibility();
        assert_eq!(kind, expected_kind);
        assert_eq!(kind.dom_key(), dom_key);
        assert_eq!(kind.authorization_key(), authorization_key);
        assert_eq!(contract.role, role);
        assert_eq!(contract.aria_live, aria_live);
        assert!(contract.aria_atomic);
        assert_eq!(contract.aria_busy, kind == UiStateKind::Loading);
        assert!(state.is_read_only());
        assert_eq!(state.data().is_some(), contract.cached_payload_allowed);
    }

    let offline_without_cache = OperationsState::Offline {
        data: None,
        metadata: None,
        reason: "backend unavailable".into(),
    };
    assert_eq!(offline_without_cache.kind(), Some(UiStateKind::Offline));
    assert!(offline_without_cache.is_read_only());
    assert!(offline_without_cache.data().is_none());
    assert!(UiStateKind::Offline.accessibility().cached_payload_allowed);
}

#[test]
fn g12_rendered_state_markers_follow_the_authoritative_ui_state_contract() {
    let states = [
        UiStateKind::Loading,
        UiStateKind::Empty,
        UiStateKind::Denied,
        UiStateKind::Stale,
        UiStateKind::ReadOnly,
        UiStateKind::Conflict,
        UiStateKind::Offline,
        UiStateKind::Error,
    ];

    for state in states {
        let accessibility = state.accessibility();
        let attributes = rendered_state_panel_attributes(state);

        assert_eq!(
            rendered_attribute(&attributes, "data-ui-state"),
            state.dom_key()
        );
        assert_eq!(
            rendered_attribute(&attributes, "data-ui-authorization"),
            state.authorization_key()
        );
        assert_eq!(
            rendered_attribute(&attributes, "data-ui-cached-payload"),
            if accessibility.cached_payload_allowed {
                "allowed"
            } else {
                "hidden"
            }
        );
        assert_eq!(rendered_attribute(&attributes, "role"), accessibility.role);
        assert_eq!(
            rendered_attribute(&attributes, "aria-live"),
            accessibility.aria_live
        );
        assert_eq!(
            rendered_attribute(&attributes, "aria-atomic"),
            accessibility.aria_atomic.to_string()
        );
        assert_eq!(
            rendered_attribute(&attributes, "aria-busy"),
            accessibility.aria_busy.to_string()
        );
    }
}
