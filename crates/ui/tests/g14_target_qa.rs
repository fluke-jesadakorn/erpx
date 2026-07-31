use dioxus::dioxus_core::{AttributeValue, Mutation};
use dioxus::prelude::*;
use erpx_ui::{
    ClientTarget, ExpiryTimestamp, LoginRequest, PlatformRoute, RouteState, SafeSessionMetadata,
    SessionCommandError, SessionId, SessionSnapshot, SessionState, StoredSession,
    SupportAccessMetadata, TargetSecureSessionAdapter,
};

#[component]
fn dialog_fixture() -> Element {
    rsx! {
        erpx_ui::Dialog {
            open: true,
            title: "G14 accessibility dialog".to_owned(),
            on_close: move |_| {},
            p { "Target-neutral dialog content" }
        }
    }
}

#[test]
fn g14_dialog_accessibility_contract_is_rendered_by_the_shared_ui() {
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
            .filter(|(name, value)| *name == "data-dialog-focus-guard"
                && matches!(*value, "start" | "end"))
            .count(),
        2
    );
    assert!(edits.iter().any(|mutation| {
        matches!(
            mutation,
            Mutation::NewEventListener { name, .. } if name == "keydown"
        )
    }));
}

#[test]
fn g14_deep_link_restoration_is_canonical_and_same_origin() {
    let cases = [
        ("/platform", "/platform"),
        ("#/platform/customers", "/platform/customers"),
        (
            "/#/platform/customers/cus_123/health?tab=summary",
            "/platform/customers/cus_123/health",
        ),
    ];

    for (input, expected) in cases {
        let route = PlatformRoute::parse(input).expect("allow-listed deep link must parse");
        assert_eq!(route.path(), expected);
        for target in [
            ClientTarget::Web,
            ClientTarget::Desktop,
            ClientTarget::Android,
            ClientTarget::Ios,
        ] {
            assert!(!target.label().is_empty());
            let restored = RouteState::SessionExpired {
                return_to: input.to_owned(),
            };
            assert_eq!(
                restored.return_to().as_deref(),
                Some(expected),
                "route restoration must be target-neutral for {}",
                target.label()
            );
        }
    }

    for unsafe_input in [
        "https://outside.example/platform",
        "/#/platform/customers?access_token=should-not-persist",
        "/#/platform/customers?refresh_token=should-not-persist",
        "/#/platform/customers?id_token=should-not-persist",
        "/#/platform/customers?client_secret=should-not-persist",
        "/#/platform/customers?authorization=Bearer-should-not-persist",
        "/#/platform/customers#password=should-not-persist",
        "/platform/../customers",
        "/platform/%2Fcustomers",
    ] {
        assert_eq!(
            RouteState::SessionRevoked {
                return_to: unsafe_input.to_owned(),
            }
            .return_to()
            .as_deref(),
            Some("/"),
            "unsafe deep link must restore only to home: {unsafe_input}"
        );
        assert!(
            PlatformRoute::parse(unsafe_input).is_err(),
            "credential-bearing or unsafe deep link must not enter the route registry: {unsafe_input}"
        );
    }
}

#[test]
fn g14_high_risk_access_stays_backend_mfa_gated_across_the_full_session_lifecycle() {
    let snapshot = SessionSnapshot {
        session_id: SessionId::new("session-g14-mfa").expect("session ID"),
        principal_ref: "principal-g14".to_owned(),
        access_expires_at: ExpiryTimestamp::new("access-expiry").expect("access expiry"),
        refresh_expires_at: ExpiryTimestamp::new("refresh-expiry").expect("refresh expiry"),
        metadata: SafeSessionMetadata::default(),
    };
    let states = [
        (
            SessionState::SignedOut {
                return_to: "/platform".to_owned(),
            },
            "denied",
        ),
        (
            SessionState::Restoring {
                return_to: "/platform".to_owned(),
            },
            "denied",
        ),
        (
            SessionState::Authenticating {
                return_to: "/platform".to_owned(),
            },
            "denied",
        ),
        (
            SessionState::Authenticated(snapshot.clone()),
            "backend-mfa-required",
        ),
        (SessionState::Refreshing(snapshot.clone()), "denied"),
        (SessionState::Revoking(snapshot.clone()), "denied"),
        (
            SessionState::Expired {
                return_to: "/platform".to_owned(),
                reason: "expired".to_owned(),
            },
            "denied",
        ),
        (
            SessionState::Revoked {
                return_to: "/platform".to_owned(),
            },
            "denied",
        ),
        (
            SessionState::Offline {
                last_known: Some(snapshot.clone()),
                reason: "offline".to_owned(),
            },
            "denied",
        ),
        (
            SessionState::Error {
                return_to: "/platform".to_owned(),
                message: "error".to_owned(),
            },
            "denied",
        ),
    ];

    for (state, expected_marker) in states {
        assert_eq!(state.high_risk_mfa_marker(), expected_marker);
        assert!(!state.may_render_high_risk_access());
        assert!(!state.high_risk_mfa_marker().contains("local"));
    }
}

#[test]
fn g14_session_restore_and_in_flight_operations_never_render_as_authenticated_routes() {
    let snapshot = SessionSnapshot {
        session_id: SessionId::new("session-in-flight").expect("session ID"),
        principal_ref: "principal-g14".to_owned(),
        access_expires_at: ExpiryTimestamp::new("access-expiry").expect("access expiry"),
        refresh_expires_at: ExpiryTimestamp::new("refresh-expiry").expect("refresh expiry"),
        metadata: SafeSessionMetadata::default(),
    };

    let states = [
        (
            SessionState::Restoring {
                return_to: "/platform".to_owned(),
            },
            erpx_ui::UiStateKind::Denied,
        ),
        (
            SessionState::Refreshing(snapshot.clone()),
            erpx_ui::UiStateKind::Offline,
        ),
        (
            SessionState::Revoking(snapshot),
            erpx_ui::UiStateKind::Offline,
        ),
    ];

    for (state, expected_kind) in states {
        assert_eq!(state.route_state().ui_state(), Some(expected_kind));
        assert!(state.route_state().session_metadata().is_none());
        assert!(!state.may_render_high_risk_access());
    }
}

#[test]
fn g14_secret_and_mfa_material_stays_out_of_display_and_local_persistence_contracts() {
    let login = LoginRequest::new(
        "principal=access_token=secret",
        "password-secret",
        "target-g14",
    )
    .expect("QA login input must be valid");
    let stored = StoredSession::new(
        "principal=access_token=secret",
        "session-id=secret",
        "access-secret",
        "refresh-secret",
        "access-expiry",
        "refresh-expiry",
    )
    .expect("QA stored session must be valid");
    let snapshot = SessionSnapshot {
        session_id: SessionId::new("session-id=secret").expect("session ID"),
        principal_ref: "principal=access_token=secret".to_owned(),
        access_expires_at: ExpiryTimestamp::new("access-expiry").expect("access expiry"),
        refresh_expires_at: ExpiryTimestamp::new("refresh-expiry").expect("refresh expiry"),
        metadata: SafeSessionMetadata::default(),
    };
    let state = SessionState::Authenticated(snapshot);

    for debug in [
        format!("{login:?}"),
        format!("{stored:?}"),
        format!("{state:?}"),
        format!(
            "{:?}",
            RouteState::ReadOnly {
                session: SafeSessionMetadata {
                    principal_label: Some("password=secret".to_owned()),
                    support: Some(SupportAccessMetadata {
                        ticket_label: "ticket=secret".to_owned(),
                        expires_at: "refresh_token=secret".to_owned(),
                        mode_label: "authorization: Bearer secret".to_owned(),
                    }),
                    ..SafeSessionMetadata::default()
                },
                reason: "database_url=postgres://secret".to_owned(),
            }
        ),
        format!(
            "{:?}",
            SessionCommandError::InvalidSession("mfa_code=secret".to_owned())
        ),
    ] {
        for secret in [
            "password-secret",
            "access-secret",
            "refresh-secret",
            "principal=access_token=secret",
            "session-id=secret",
            "password=secret",
            "ticket=secret",
            "refresh_token=secret",
            "authorization: Bearer secret",
            "database_url=postgres://secret",
        ] {
            assert!(
                !debug.contains(secret),
                "secret reached Debug output: {secret}"
            );
        }
        assert!(!debug.contains("mfa_code"));
    }
    assert_eq!(state.high_risk_mfa_marker(), "backend-mfa-required");
    assert!(!state.may_render_high_risk_access());

    let descriptor = TargetSecureSessionAdapter::descriptor();
    assert!(!descriptor.local_fallback_allowed());
    assert!(!descriptor.status_copy().as_str().contains("localStorage"));
    assert!(!descriptor.status_copy().as_str().contains("sessionStorage"));
}
