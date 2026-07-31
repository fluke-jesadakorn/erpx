use erpx_ui::{
    OperationsDisplayModel, OperationsFieldInput, OperationsPage, OperationsProjectionMetadata,
    OperationsScope, OperationsScopeKind, OperationsState, PlatformOperationsAudience,
    PlatformOperationsRouteRegistry, RouteAuthRequirement, UiStateKind,
};

fn metadata() -> OperationsProjectionMetadata {
    OperationsProjectionMetadata::fresh(
        OperationsScope::new("holding_123", "Demo holding", OperationsScopeKind::Holding),
        "2026-07-24T00:00:00Z",
    )
}

#[test]
fn platform_operations_denial_and_offline_markers_remain_payload_safe() {
    let registry = PlatformOperationsRouteRegistry::new();
    assert!(!registry.definitions().is_empty());
    assert!(registry.definitions().iter().all(|definition| {
        definition.audience == PlatformOperationsAudience::Platform
            && definition.auth == RouteAuthRequirement::Authenticated
    }));

    let denied_marker = UiStateKind::Denied;
    assert_eq!(denied_marker.dom_key(), "denied");
    assert_eq!(denied_marker.authorization_key(), "denied");
    assert_eq!(denied_marker.accessibility().role, "alert");
    assert_eq!(denied_marker.accessibility().aria_live, "assertive");
    assert!(!denied_marker.accessibility().cached_payload_allowed);

    let denied = OperationsState::Denied {
        metadata: None,
        reason: "platform authorization denied".into(),
    };
    assert!(denied.data().is_none());
    assert!(denied.is_read_only());

    let protected_candidate = OperationsDisplayModel::from_untrusted_fields(
        OperationsPage::Overview,
        metadata(),
        [OperationsFieldInput::new(
            "source_reference",
            "access_token=protected",
        )],
    );
    assert!(protected_candidate.fields().is_empty());
    assert_eq!(protected_candidate.redacted_field_count(), 1);

    let offline_marker = UiStateKind::Offline;
    assert_eq!(offline_marker.dom_key(), "offline");
    assert_eq!(offline_marker.authorization_key(), "unknown");
    assert_eq!(offline_marker.accessibility().role, "status");
    assert_eq!(offline_marker.accessibility().aria_live, "polite");
    assert!(offline_marker.accessibility().cached_payload_allowed);

    let offline = OperationsState::Offline {
        data: Some(protected_candidate),
        metadata: Some(metadata()),
        reason: "backend unavailable".into(),
    };
    assert!(offline.is_read_only());
    let cached = offline.data().expect("offline state may retain safe cache");
    assert!(cached.fields().is_empty());
    assert_eq!(cached.redacted_field_count(), 1);
}
