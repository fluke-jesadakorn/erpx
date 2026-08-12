use erpx_ui::{
    FINANCE_ROUTES, FinanceDisplayField, FinanceDisplayModel, FinanceDisplayValueError,
    FinanceLabel, FinanceOpaqueId, FinancePageState, FinancePageStateKind,
    FinanceProjectionMetadata, FinanceRoute, FinanceRouteId, FinanceRouteRegistry, FinanceScalar,
    FinanceScope, FinanceSurface, Locale, MAX_FINANCE_DISPLAY_VALUE_BYTES,
    MAX_FINANCE_IDENTIFIER_BYTES, ProjectionFreshness, RouteAuthRequirement, UiStateKind,
};

#[test]
fn finance_registry_parses_only_stable_routes_and_redacts_deep_links() {
    let registry = FinanceRouteRegistry::new();
    let expected = [
        (FinanceRouteId::Finance, "/finance"),
        (FinanceRouteId::Chart, "/finance/chart"),
        (FinanceRouteId::Journals, "/finance/journals"),
        (FinanceRouteId::Subledgers, "/finance/subledgers"),
        (FinanceRouteId::AccountsPayable, "/finance/ap"),
        (FinanceRouteId::AccountsReceivable, "/finance/ar"),
        (FinanceRouteId::Cash, "/finance/cash"),
        (FinanceRouteId::Banking, "/finance/banking"),
        (FinanceRouteId::Close, "/finance/close"),
        (FinanceRouteId::Budgets, "/finance/budgets"),
    ];

    assert_eq!(registry.definitions(), FINANCE_ROUTES);
    assert_eq!(registry.definitions().len(), expected.len());
    for (definition, (id, path)) in registry.definitions().iter().zip(expected) {
        assert_eq!(definition.id, id);
        assert_eq!(definition.template, path);
        assert_eq!(definition.auth, RouteAuthRequirement::Authenticated);
        assert_ne!(
            definition.label_for(Locale::En),
            definition.label_for(Locale::Th)
        );
        assert_eq!(registry.resolve_id(path), Ok(id));
        assert_eq!(registry.resolve(path).expect("route").path(), path);
    }

    assert_eq!(
        FinanceRoute::parse("//finance/chart?access_token=never-store"),
        Err(erpx_ui::FinanceRouteParseError {
            normalized_path: "/".to_string(),
        })
    );
    assert!(FinanceRoute::parse("/finance/unknown").is_err());
}

#[test]
fn finance_identifiers_and_scalar_display_values_are_bounded_and_redacted() {
    assert!(FinanceOpaqueId::new("journal-opaque-1").is_ok());
    assert_eq!(
        FinanceOpaqueId::new("a".repeat(MAX_FINANCE_IDENTIFIER_BYTES + 1)),
        Err(erpx_ui::FinanceIdentifierError::TooLong)
    );
    assert_eq!(
        FinanceOpaqueId::new("database_url=postgresql-redacted"),
        Err(erpx_ui::FinanceIdentifierError::SensitiveMarker)
    );
    assert_eq!(
        FinanceScalar::text("{\"secret\":true}"),
        Err(FinanceDisplayValueError::StructuredValue)
    );
    assert_eq!(
        FinanceScalar::text("secret=do-not-display"),
        Err(FinanceDisplayValueError::SensitiveMarker)
    );
    assert_eq!(
        FinanceScalar::text("a".repeat(MAX_FINANCE_DISPLAY_VALUE_BYTES + 1)),
        Err(FinanceDisplayValueError::TooLong)
    );
}

#[test]
fn finance_labels_and_models_are_bilingual_and_scalar_only() {
    let label = FinanceLabel::new("Journal", "สมุดรายวัน");
    assert_eq!(label.text(Locale::En), "Journal");
    assert_eq!(label.text(Locale::Th), "สมุดรายวัน");
    assert_eq!(FinanceSurface::Banking.label().text(Locale::En), "Banking");
    assert_eq!(FinanceSurface::Banking.label().text(Locale::Th), "ธนาคาร");

    let field = FinanceDisplayField::new(
        "status",
        label,
        FinanceScalar::text("posted").expect("safe scalar"),
    )
    .expect("safe field");
    let model = FinanceDisplayModel::new(
        FinanceSurface::Journals,
        FinanceOpaqueId::new("journal-opaque-1").expect("safe id"),
        vec![field],
    )
    .expect("safe model");
    assert_eq!(model.surface(), FinanceSurface::Journals);
    assert_eq!(model.id().as_str(), "journal-opaque-1");
    assert_eq!(model.fields().len(), 1);
    assert_eq!(model.fields()[0].value().text_value(), Some("posted"));
    assert_eq!(FinanceScalar::integer(7).text_value(), None);
    assert_eq!(FinanceScalar::boolean(true).text_value(), None);
}

#[test]
fn finance_page_states_cover_all_explicit_transitions_and_remain_read_only() {
    let metadata = FinanceProjectionMetadata::fresh("2026-07-26T00:00:00Z");
    assert_eq!(metadata.freshness, ProjectionFreshness::Fresh);
    assert!(metadata.is_read_only());
    assert!(!metadata.is_stale());

    let states = [
        FinancePageState::<()>::Loading {
            metadata: Some(metadata.clone()),
        },
        FinancePageState::<()>::Empty {
            metadata: metadata.clone(),
        },
        FinancePageState::<()>::Ready {
            data: (),
            metadata: metadata.clone(),
        },
        FinancePageState::<()>::Denied {
            metadata: None,
            reason: "Not available".into(),
        },
        FinancePageState::<()>::Stale {
            data: (),
            metadata: FinanceProjectionMetadata::stale("2026-07-25T00:00:00Z"),
            age_seconds: 86_400,
            reason: "Projection is delayed".into(),
        },
        FinancePageState::<()>::ReadOnly {
            data: (),
            metadata: metadata.clone(),
            reason: "Read-only projection".into(),
        },
        FinancePageState::<()>::Conflict {
            data: Some(()),
            metadata: Some(metadata.clone()),
            reason: "Review required".into(),
        },
        FinancePageState::<()>::Offline {
            data: None,
            metadata: Some(metadata),
            reason: "Connection unavailable".into(),
        },
        FinancePageState::<()>::Error {
            reason: "Request failed".into(),
        },
    ];
    let expected = [
        FinancePageStateKind::Loading,
        FinancePageStateKind::Empty,
        FinancePageStateKind::Ready,
        FinancePageStateKind::Denied,
        FinancePageStateKind::Stale,
        FinancePageStateKind::ReadOnly,
        FinancePageStateKind::Conflict,
        FinancePageStateKind::Offline,
        FinancePageStateKind::Error,
    ];

    for (state, expected_kind) in states.iter().zip(expected) {
        assert_eq!(state.state_kind(), expected_kind);
        assert_eq!(state.kind(), expected_kind.shared_kind());
        assert!(!state.can_mutate());
    }
    assert_eq!(
        FinancePageState::<()>::Error {
            reason: "database_url=redacted".into(),
        }
        .state_label(Locale::En),
        Some("Error")
    );
    assert_eq!(
        FinancePageStateKind::Ready.shared_kind(),
        None::<UiStateKind>
    );
}

#[test]
fn finance_scope_and_high_risk_mfa_state_fail_closed() {
    let scope = FinanceScope::new("holding-1", Some("company-1")).expect("scope");
    assert_eq!(scope.holding_id().as_str(), "holding-1");
    assert_eq!(scope.company_id().expect("company").as_str(), "company-1");

    let metadata = FinanceProjectionMetadata::fresh("2026-07-29T00:00:00Z").with_scope(scope);
    assert!(metadata.scope().is_some());
    let scoped_ready = FinancePageState::Ready {
        data: "scoped read model",
        metadata: metadata.clone(),
    };
    assert_eq!(scoped_ready.data(), Some(&"scoped read model"));
    assert!(scoped_ready.can_render_data());
    assert!(!scoped_ready.can_mutate());

    let unscoped_ready = FinancePageState::Ready {
        data: "must not render",
        metadata: FinanceProjectionMetadata::fresh("2026-07-29T00:00:00Z"),
    };
    assert!(
        unscoped_ready
            .metadata()
            .is_some_and(|value| value.scope().is_none())
    );
    assert!(unscoped_ready.data().is_none());
    assert!(!unscoped_ready.can_render_data());
    assert!(!unscoped_ready.can_mutate());

    let unscoped_cached = FinancePageState::Offline {
        data: Some("cached but unbound"),
        metadata: Some(FinanceProjectionMetadata::fresh("2026-07-29T00:00:00Z")),
        reason: "backend unavailable".into(),
    };
    assert!(unscoped_cached.data().is_none());
    assert!(!unscoped_cached.can_render_data());

    let state = FinancePageState::<()>::MfaRequired {
        metadata: Some(metadata),
        reason: "fresh MFA is required".into(),
    };
    assert_eq!(state.state_kind(), FinancePageStateKind::MfaRequired);
    assert_eq!(state.kind(), Some(UiStateKind::Denied));
    assert!(state.is_read_only());
    assert!(!state.can_render_data());
    assert_eq!(state.state_label(Locale::En), Some("MFA required"));
}
