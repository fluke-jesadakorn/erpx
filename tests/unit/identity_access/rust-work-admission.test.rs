use erpx_server_identity_access::public::effective_authority::{
    AuthorizedWorkContext, QueuedWorkBinding, effective_authority_permissions,
};
#[test]
fn serialized_queue_history_requires_every_native_witness_and_is_not_an_authorized_context() {
    let complete = serde_json::json!({"version":1,"holdingId":"h001","companyId":"company","userId":"user","branchId":null,"departmentId":null,"branchSlug":null,"departmentSlug":null,"assignmentId":"grant","assignmentVersion":3,"role":"custom_native","sessionId":"session","selectionRevision":7,"identityVersion":0,"roleRevision":1,"hrAppointmentVersion":null,"emergencyUntil":null,"expiresAt":1800000000000_i64});
    let parsed = QueuedWorkBinding::parse(&complete.to_string()).unwrap();
    assert_eq!(parsed.assignment_version, 3);
    for field in [
        "holdingId",
        "identityVersion",
        "roleRevision",
        "hrAppointmentVersion",
        "emergencyUntil",
        "branchId",
        "departmentSlug",
    ] {
        let mut missing = complete.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            QueuedWorkBinding::parse(&missing.to_string()).is_err(),
            "missing {field} was inferred"
        );
    }
    let mut forged = complete.clone();
    forged["permissions"] = serde_json::json!(["finance.pay"]);
    assert!(QueuedWorkBinding::parse(&forged.to_string()).is_err());
    assert!(QueuedWorkBinding::parse(&" ".repeat(16385)).is_err());
}
use erpx_server_identity_access::public::work_admission::*;
use serde::Deserialize;
#[test]
fn sealed_context_cannot_be_deserialized_or_cloned_and_hr_role_is_not_an_appointment() {
    trait WireBoundary<M> {
        fn marker() {}
    }
    impl<T: ?Sized> WireBoundary<()> for T {}
    impl<T: serde::de::DeserializeOwned> WireBoundary<u8> for T {}
    let _ = <AuthorizedWorkContext as WireBoundary<_>>::marker;
    trait CloneBoundary<M> {
        fn marker() {}
    }
    impl<T: ?Sized> CloneBoundary<()> for T {}
    impl<T: Clone> CloneBoundary<u8> for T {}
    let _ = <AuthorizedWorkContext as CloneBoundary<_>>::marker;
    trait SerializeBoundary<M> {
        fn marker() {}
    }
    impl<T: ?Sized> SerializeBoundary<()> for T {}
    impl<T: serde::Serialize> SerializeBoundary<u8> for T {}
    let _ = <AuthorizedWorkContext as SerializeBoundary<_>>::marker;
    let published = vec![
        "people.manage".into(),
        "finance.read".into(),
        "module.people_payroll.read".into(),
    ];
    assert_eq!(
        effective_authority_permissions("hr_admin", &published, false),
        vec!["finance.read"]
    );
    let actual = effective_authority_permissions(
        "custom_actual_appointment",
        &["finance.read".into()],
        true,
    );
    assert!(actual.iter().any(|p| p == "people.manage"));
    assert!(actual.iter().any(|p| p == "module.people_payroll.read"));
}
#[derive(Deserialize)]
struct Oracle {
    cases: Vec<ModuleCase>,
    destinations: Vec<DestinationCase>,
}
#[derive(Deserialize)]
struct ModuleCase {
    role: String,
    assigned: Vec<String>,
    permissions: Vec<String>,
    beta: bool,
    expected: Vec<String>,
}
#[derive(Deserialize)]
struct DestinationCase {
    destination: String,
    api: bool,
    expected: bool,
}
#[test]
fn module_and_destination_admission_matches_retained_legacy_oracles() {
    let fixture: Oracle = serde_json::from_str(include_str!(
        "../../../fixtures/identity_access/permissions/legacy-work-admission.v1.json"
    ))
    .unwrap();
    assert_eq!(fixture.cases.len(), 1170);
    assert_eq!(fixture.destinations.len(), 378);
    let catalog = work_module_catalog().unwrap();
    for case in fixture.cases {
        assert_eq!(
            catalog.effective_modules(&case.role, &case.assigned, &case.permissions, case.beta),
            case.expected,
            "role {} beta {}",
            case.role,
            case.beta
        );
    }
    for case in fixture.destinations {
        assert_eq!(
            beta_destination_allowed(&case.destination, case.api),
            case.expected,
            "destination {} API {}",
            case.destination,
            case.api
        );
    }
}
#[test]
fn catalogs_and_non_route_destinations_fail_closed() {
    assert!(WorkModuleCatalog::new(vec![]).is_err());
    assert!(
        WorkModuleCatalog::new(vec![BusinessRoleModules {
            id: "r".into(),
            modules: vec![]
        }])
        .is_err()
    );
    for destination in [
        "home\n",
        "beta/a\r",
        "uploads/\0/content",
        "/home",
        "../home",
    ] {
        assert!(!beta_destination_allowed(destination, true));
        assert!(!beta_destination_allowed(destination, false));
    }
}
