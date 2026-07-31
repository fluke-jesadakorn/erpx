use dioxus::dioxus_core::{AttributeValue, Mutation};
use dioxus::prelude::*;
use erpx_ui::{RouteState, SafeSessionMetadata, UiState, UiStateKind};

#[component]
fn state_panel_fixture(state: UiStateKind) -> Element {
    rsx! {
        erpx_ui::StatePanel {
            state,
            title: "State".to_owned(),
            message: "The state is rendered from the authoritative boundary.".to_owned(),
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
                    | "aria-labelledby"
                    | "aria-describedby"
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

fn state_panel_fixture_pair() -> Element {
    rsx! {
        erpx_ui::StatePanel {
            state: UiStateKind::Denied,
            title: "First state".to_owned(),
            message: "First message".to_owned(),
            action_label: None,
            on_action: None,
        }
        erpx_ui::StatePanel {
            state: UiStateKind::Denied,
            title: "Second state".to_owned(),
            message: "Second message".to_owned(),
            action_label: None,
            on_action: None,
        }
    }
}

#[component]
fn dialog_fixture() -> Element {
    rsx! {
        erpx_ui::Dialog {
            open: true,
            title: "Accessible dialog".to_owned(),
            on_close: move |_| {},
            p { "Dialog content" }
        }
    }
}

#[component]
fn repeated_field_fixture() -> Element {
    rsx! {
        erpx_ui::TextField {
            label: "Principal".to_owned(),
            name: "principal".to_owned(),
            value: "first".to_owned(),
            placeholder: None,
            error: None,
            help: Some("The server-owned principal reference.".to_owned()),
            required: true,
            oninput: move |_| {},
        }
        erpx_ui::TextField {
            label: "Principal".to_owned(),
            name: "principal".to_owned(),
            value: "second".to_owned(),
            placeholder: None,
            error: Some("The value needs review.".to_owned()),
            help: None,
            required: true,
            oninput: move |_| {},
        }
        erpx_ui::Field {
            label: "Scope".to_owned(),
            name: "scope".to_owned(),
            error: Some("Choose an authorized scope.".to_owned()),
            help: Some("The server validates the selection.".to_owned()),
            required: true,
            input { r#type: "text", value: "holding-001" }
        }
    }
}

fn session() -> SafeSessionMetadata {
    SafeSessionMetadata {
        principal_label: Some("Platform Administrator".to_owned()),
        ..SafeSessionMetadata::default()
    }
}

#[test]
fn g12_final_route_states_keep_denied_offline_cache_and_read_only_postures_distinct() {
    let session = session();
    let states = [
        (
            RouteState::Unauthenticated {
                return_to: "/platform/customers".to_owned(),
            },
            UiStateKind::Denied,
        ),
        (
            RouteState::SessionExpired {
                return_to: "/platform/customers/".to_owned(),
            },
            UiStateKind::Denied,
        ),
        (
            RouteState::SessionRevoked {
                return_to: "/platform/customers".to_owned(),
            },
            UiStateKind::Denied,
        ),
        (
            RouteState::Denied {
                reason: "policy denied".to_owned(),
            },
            UiStateKind::Denied,
        ),
        (
            RouteState::ReadOnly {
                session: session.clone(),
                reason: "billing grace".to_owned(),
            },
            UiStateKind::ReadOnly,
        ),
        (
            RouteState::Offline {
                cached_view_available: false,
                reason: "backend unavailable".to_owned(),
            },
            UiStateKind::Offline,
        ),
        (
            RouteState::Offline {
                cached_view_available: true,
                reason: "backend unavailable".to_owned(),
            },
            UiStateKind::Offline,
        ),
        (
            RouteState::Stale {
                age_seconds: 301,
                reason: "projection lag".to_owned(),
            },
            UiStateKind::Stale,
        ),
        (
            RouteState::Conflict {
                reason: "revision changed".to_owned(),
            },
            UiStateKind::Conflict,
        ),
    ];

    for (route, expected_kind) in states {
        assert_eq!(route.ui_state(), Some(expected_kind));
        let accessibility = expected_kind.accessibility();
        assert!(accessibility.aria_atomic);
        assert!(!accessibility.aria_busy);
        assert_eq!(
            accessibility.cached_payload_allowed,
            matches!(
                expected_kind,
                UiStateKind::Stale
                    | UiStateKind::ReadOnly
                    | UiStateKind::Conflict
                    | UiStateKind::Offline
            )
        );
        assert_eq!(
            accessibility.role,
            if expected_kind == UiStateKind::Denied || expected_kind == UiStateKind::Conflict {
                "alert"
            } else {
                "status"
            }
        );
    }

    let authenticated = RouteState::Authenticated { session };
    assert_eq!(authenticated.ui_state(), None);
    assert!(authenticated.session_metadata().is_some());

    let expired = RouteState::SessionExpired {
        return_to: "/platform/customers/?tab=health".to_owned(),
    };
    assert_eq!(expired.return_to().as_deref(), Some("/platform/customers"));
}

#[test]
fn g12_final_generic_state_machine_preserves_loading_and_ready_boundaries() {
    let states = [
        (UiState::<()>::Loading, UiStateKind::Loading),
        (UiState::<()>::Empty, UiStateKind::Empty),
        (
            UiState::<()>::Denied {
                reason: "policy denied".to_owned(),
            },
            UiStateKind::Denied,
        ),
        (
            UiState::<()>::ReadOnly {
                reason: "read-only context".to_owned(),
            },
            UiStateKind::ReadOnly,
        ),
        (
            UiState::<()>::Offline {
                reason: "backend unavailable".to_owned(),
            },
            UiStateKind::Offline,
        ),
        (
            UiState::<()>::Error {
                reason: "request failed".to_owned(),
            },
            UiStateKind::Error,
        ),
    ];

    for (state, expected_kind) in states {
        assert_eq!(state.kind(), Some(expected_kind));
        assert!(!expected_kind.dom_key().is_empty());
        assert!(!expected_kind.authorization_key().is_empty());
    }

    assert_eq!(
        UiState::<&str>::from_option(None).kind(),
        Some(UiStateKind::Empty)
    );
    let ready = UiState::from_option(Some("safe read model"));
    assert_eq!(ready.kind(), None);
    assert!(matches!(ready, UiState::Ready("safe read model")));
}

#[test]
fn g12_route_state_handoff_rejects_unbounded_or_target_ambiguous_paths() {
    let safe = RouteState::SessionRevoked {
        return_to: "/platform/customers?tab=health".to_owned(),
    };
    assert_eq!(safe.return_to().as_deref(), Some("/platform/customers"));

    let oversized = RouteState::SessionRevoked {
        return_to: format!("/{}", "customer/".repeat(100)),
    };
    let ambiguous = [
        "https://evil.example/platform",
        "/platform\\customers",
        "/platform/%2Fcustomers",
        "/platform/customer:opaque",
        "/platform/customer with spaces",
    ];

    assert_eq!(oversized.return_to().as_deref(), Some("/"));
    for return_to in ambiguous {
        assert_eq!(
            RouteState::SessionRevoked {
                return_to: return_to.to_owned(),
            }
            .return_to()
            .as_deref(),
            Some("/"),
            "unsafe route handoff was not rejected: {return_to}"
        );
    }
}

#[test]
fn g12_final_state_panel_binds_stable_markers_to_shared_accessibility_values() {
    for state in [
        UiStateKind::Loading,
        UiStateKind::Empty,
        UiStateKind::Denied,
        UiStateKind::Stale,
        UiStateKind::ReadOnly,
        UiStateKind::Conflict,
        UiStateKind::Offline,
        UiStateKind::Error,
    ] {
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
        let title_id = rendered_attribute(&attributes, "aria-labelledby");
        let message_id = rendered_attribute(&attributes, "aria-describedby");
        assert!(title_id.starts_with("erpx-state-title-"));
        assert!(message_id.starts_with("erpx-state-message-"));
        assert_ne!(title_id, message_id);
    }
}

#[test]
fn g12_repeated_state_panels_have_unique_accessible_name_targets() {
    let mut dom = VirtualDom::new(state_panel_fixture_pair);
    let edits = dom.rebuild_to_vec().edits;
    let mut labelled_by = Vec::new();
    let mut described_by = Vec::new();
    for mutation in edits {
        if let Mutation::SetAttribute {
            name,
            value: AttributeValue::Text(value),
            ..
        } = mutation
        {
            match name {
                "aria-labelledby" => labelled_by.push(value),
                "aria-describedby" => described_by.push(value),
                _ => {}
            }
        }
    }
    assert_eq!(labelled_by.len(), 2);
    assert_eq!(described_by.len(), 2);
    assert_ne!(labelled_by[0], labelled_by[1]);
    assert_ne!(described_by[0], described_by[1]);
}

#[test]
fn g12_dialog_binds_unique_accessible_name_and_description_targets() {
    let mut dom = VirtualDom::new(dialog_fixture);
    let edits = dom.rebuild_to_vec().edits;
    let mut labelled_by = None;
    let mut described_by = None;
    let mut ids = Vec::new();
    for mutation in edits {
        if let Mutation::SetAttribute {
            name,
            value: AttributeValue::Text(value),
            ..
        } = mutation
        {
            match name {
                "aria-labelledby" => labelled_by = Some(value),
                "aria-describedby" => described_by = Some(value),
                "id" => ids.push(value),
                _ => {}
            }
        }
    }
    let labelled_by = labelled_by.expect("dialog must identify its title");
    let described_by = described_by.expect("dialog must identify its body");
    assert_ne!(labelled_by, described_by);
    assert!(ids.contains(&labelled_by));
    assert!(ids.contains(&described_by));
}

#[test]
fn g12_dialog_owns_keyboard_and_focus_boundaries() {
    let mut dom = VirtualDom::new(dialog_fixture);
    let edits = dom.rebuild_to_vec().edits;
    let attributes = edits
        .iter()
        .filter_map(|mutation| match mutation {
            Mutation::SetAttribute {
                name,
                value: AttributeValue::Text(value),
                ..
            } => Some((*name, value.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(attributes.contains(&("role", "dialog")));
    assert!(attributes.contains(&("aria-modal", "true")));
    assert!(attributes.contains(&("aria-keyshortcuts", "Escape")));
    assert!(attributes.contains(&("data-dialog-focus-trap", "true")));
    assert!(attributes.contains(&("data-dialog-focus-restore", "caller-target-required")));
    assert_eq!(
        attributes
            .iter()
            .filter(|(name, value)| *name == "data-dialog-focus-guard" && *value == "start")
            .count(),
        1
    );
    assert_eq!(
        attributes
            .iter()
            .filter(|(name, value)| *name == "data-dialog-focus-guard" && *value == "end")
            .count(),
        1
    );
    assert_eq!(
        attributes
            .iter()
            .filter(|(name, value)| *name == "aria-label" && *value == "Dialog focus boundary")
            .count(),
        2
    );

    let listeners = edits.iter().filter_map(|mutation| match mutation {
        Mutation::NewEventListener { name, .. } => Some(name.as_str()),
        _ => None,
    });
    let listeners = listeners.collect::<Vec<_>>();
    assert!(listeners.contains(&"click"));
    assert!(listeners.contains(&"keydown"));
    assert!(listeners.contains(&"focus"));
    assert!(listeners.contains(&"mounted"));
}

#[test]
fn g12_repeated_fields_have_unique_accessible_ids_and_descriptions() {
    let mut dom = VirtualDom::new(repeated_field_fixture);
    let edits = dom.rebuild_to_vec().edits;
    let mut input_ids = Vec::new();
    let mut label_for = Vec::new();
    let mut labelled_by = Vec::new();
    let mut described_by = Vec::new();
    for mutation in edits {
        if let Mutation::SetAttribute {
            name,
            value: AttributeValue::Text(value),
            ..
        } = mutation
        {
            match name {
                "id" if value.starts_with("erpx-field-")
                    && !value.ends_with("-label")
                    && !value.ends_with("-error")
                    && !value.ends_with("-help") =>
                {
                    input_ids.push(value)
                }
                "for" => label_for.push(value),
                "aria-labelledby" => labelled_by.push(value),
                "aria-describedby" => described_by.push(value),
                _ => {}
            }
        }
    }
    assert_eq!(input_ids.len(), 2);
    assert_ne!(input_ids[0], input_ids[1]);
    assert!(
        label_for
            .iter()
            .all(|target| target.starts_with("erpx-field-"))
    );
    assert!(
        labelled_by
            .iter()
            .any(|target| target.ends_with("scope-label"))
    );
    assert!(
        described_by
            .iter()
            .any(|target| target.contains("scope-error"))
    );
    assert!(
        described_by
            .iter()
            .any(|target| target.contains("scope-help"))
    );
}
