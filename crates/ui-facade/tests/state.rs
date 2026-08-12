use erpx_ui::{SafeUiText, UiAccessibilityContract, UiStateKind};

#[test]
fn every_non_ready_state_has_a_target_neutral_accessibility_contract() {
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
        let contract: UiAccessibilityContract = state.accessibility();
        assert_eq!(
            contract.role,
            if matches!(
                state,
                UiStateKind::Denied | UiStateKind::Conflict | UiStateKind::Error
            ) {
                "alert"
            } else {
                "status"
            }
        );
        assert!(contract.aria_atomic);
        assert_eq!(
            contract.aria_live,
            if contract.role == "alert" {
                "assertive"
            } else {
                "polite"
            }
        );
        assert_eq!(contract.aria_busy, matches!(state, UiStateKind::Loading));
    }
}

#[test]
fn state_markers_are_stable_for_endpoint_backed_qa() {
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
        assert!(!state.dom_key().is_empty());
        assert!(!state.authorization_key().is_empty());
        assert_eq!(
            state.accessibility().cached_payload_allowed,
            matches!(
                state,
                UiStateKind::Stale
                    | UiStateKind::ReadOnly
                    | UiStateKind::Conflict
                    | UiStateKind::Offline
            )
        );
    }

    assert_eq!(UiStateKind::Denied.dom_key(), "denied");
    assert_eq!(UiStateKind::Denied.authorization_key(), "denied");
    assert_eq!(UiStateKind::Error.authorization_key(), "unknown");
    assert_eq!(UiStateKind::Loading.authorization_key(), "pending");
}

#[test]
fn authorized_state_copy_never_renders_credential_bearing_diagnostics() {
    let diagnostics = [
        "access_token=secret",
        "refresh_token=secret",
        "database_url=postgres://user:secret@example.test/db",
        "Authorization: Bearer secret",
        "-----BEGIN PRIVATE KEY-----",
    ];

    for diagnostic in diagnostics {
        let safe = SafeUiText::new(diagnostic);
        assert_eq!(safe.as_str(), "[redacted]");
        assert!(!format!("{safe:?}").contains("secret"));
    }
}
