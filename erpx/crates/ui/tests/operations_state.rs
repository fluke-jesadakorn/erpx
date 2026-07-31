use erpx_ui::{
    OperationsDisplayModel, OperationsPage, OperationsProjectionMetadata, OperationsScope,
    OperationsScopeKind, OperationsState, UiStateKind,
};

fn metadata() -> OperationsProjectionMetadata {
    OperationsProjectionMetadata::fresh(
        OperationsScope::new("holding_123", "Demo holding", OperationsScopeKind::Holding),
        "2026-07-24T00:00:00Z",
    )
}

#[test]
fn offline_cache_is_optional_but_denied_and_error_never_expose_payloads() {
    let model = OperationsDisplayModel::new(OperationsPage::Overview, metadata());

    let offline_without_cache = OperationsState::Offline {
        data: None,
        metadata: None,
        reason: "backend unavailable".into(),
    };
    assert_eq!(offline_without_cache.kind(), Some(UiStateKind::Offline));
    assert!(
        offline_without_cache
            .kind()
            .expect("offline has an explicit shared state")
            .accessibility()
            .cached_payload_allowed
    );
    assert!(offline_without_cache.data().is_none());
    assert!(offline_without_cache.is_read_only());

    let offline_with_cache = OperationsState::Offline {
        data: Some(model),
        metadata: Some(metadata()),
        reason: "backend unavailable".into(),
    };
    assert_eq!(offline_with_cache.kind(), Some(UiStateKind::Offline));
    assert!(offline_with_cache.data().is_some());
    assert!(offline_with_cache.is_read_only());

    let denied = OperationsState::Denied {
        metadata: None,
        reason: "policy".into(),
    };
    assert_eq!(denied.kind(), Some(UiStateKind::Denied));
    assert!(
        !denied
            .kind()
            .expect("denied has an explicit shared state")
            .accessibility()
            .cached_payload_allowed
    );
    assert!(denied.data().is_none());

    let error = OperationsState::Error {
        reason: "request failed".into(),
    };
    assert_eq!(error.kind(), Some(UiStateKind::Error));
    assert!(
        !error
            .kind()
            .expect("error has an explicit shared state")
            .accessibility()
            .cached_payload_allowed
    );
    assert!(error.data().is_none());
}

#[test]
fn holding_and_branch_scope_metadata_remains_explicit_and_read_only() {
    for (kind, id, label) in [
        (OperationsScopeKind::Holding, "holding-001", "Holding 001"),
        (OperationsScopeKind::Branch, "branch-001", "Branch 001"),
    ] {
        let scope = OperationsScope::new(id, label, kind);
        let metadata = OperationsProjectionMetadata::fresh(scope.clone(), "2026-07-29T00:00:00Z");

        assert_eq!(metadata.scope.kind, kind);
        assert_eq!(metadata.scope.id(), id);
        assert_eq!(metadata.scope.label(), label);
        assert!(metadata.is_fresh());
        assert!(metadata.is_read_only());

        let stale = OperationsProjectionMetadata::stale(scope, "2026-07-28T23:55:00Z");
        assert!(stale.is_stale());
        assert!(stale.is_read_only());
    }
}
