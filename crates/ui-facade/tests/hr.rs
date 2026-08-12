use erpx_ui::{
    HrEmployeeDisplayModel, HrEmployeeId, HrEmployeeState, HrEmployeeStatus, HrPageState,
    HrPageStateKind, HrProjectionMetadata, HrRoute, HrRouteId, HrRouteRegistry, HrScope, Locale,
    RouteAuthRequirement, UiStateKind,
};

#[test]
fn public_hr_contract_exposes_only_the_allowlisted_routes() {
    let registry = HrRouteRegistry::new();
    let expected = [
        (HrRouteId::Hr, "/hr"),
        (HrRouteId::Employees, "/hr/employees"),
        (HrRouteId::Leave, "/hr/leave"),
        (HrRouteId::Expenses, "/hr/expenses"),
        (HrRouteId::Payroll, "/hr/payroll"),
        (HrRouteId::Reassignments, "/hr/reassignments"),
    ];
    assert_eq!(registry.definitions().len(), expected.len());
    for (definition, (id, path)) in registry.definitions().iter().zip(expected) {
        assert_eq!(definition.id, id);
        assert_eq!(definition.template, path);
        assert_eq!(definition.auth, RouteAuthRequirement::Authenticated);
        assert_ne!(
            definition.label_for(Locale::En),
            definition.label_for(Locale::Th)
        );
        assert_eq!(registry.resolve(path).map(HrRoute::path), Ok(path));
    }
}

#[test]
fn public_hr_state_contract_cannot_authorize_mutations() {
    let employee = HrEmployeeDisplayModel::new(
        HrEmployeeId::new("employee-opaque").expect("opaque id"),
        "EMP-001",
        "Somchai",
        HrEmployeeStatus::Active,
        "2026-01-01",
        None,
    )
    .expect("safe employee");
    let metadata = HrProjectionMetadata::fresh(
        HrScope::new(
            "holding-1",
            Some("company-1".into()),
            Some("branch-1".into()),
        )
        .expect("safe scope"),
        "2026-07-25T00:00:00Z",
    );
    let state = HrEmployeeState::Ready {
        data: employee,
        metadata,
    };
    assert_eq!(state.kind(), None);
    assert_eq!(state.state_kind(), HrPageStateKind::Ready);
    assert!(state.data().is_some());
    assert!(!state.can_mutate());
    assert!(state.is_read_only());
    assert_eq!(HrEmployeeStatus::Active.label(Locale::En), "Active");
    assert_ne!(
        HrEmployeeStatus::Active.label(Locale::En),
        HrEmployeeStatus::Active.label(Locale::Th)
    );
    assert_eq!(
        HrPageState::<()>::Error {
            reason: "safe".into()
        }
        .kind(),
        Some(UiStateKind::Error)
    );
}
