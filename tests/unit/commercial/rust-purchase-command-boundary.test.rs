use erpx_server_commercial::public::purchase_command_boundary::{
    PurchaseCommandBoundary, PurchaseCommandScope,
};
use erpx_server_commercial::public::purchases::{CapabilityPurpose, PackageEnforcementMode};
use erpx_server_shared::public::{CompanyId, FailureKind};

fn scope(capability: &str) -> PurchaseCommandScope {
    PurchaseCommandScope {
        company_id: CompanyId::new("company").unwrap(),
        capability: capability.into(),
        purpose: CapabilityPurpose::NewWork,
    }
}

#[test]
fn disabled_mode_builds_no_boundary_and_retained_modes_keep_the_owner_key() {
    assert!(
        PurchaseCommandBoundary::new(
            PackageEnforcementMode::Disabled,
            "commercial-command-001",
            scope("legal.contracts"),
        )
        .unwrap()
        .is_none()
    );

    let shadow = PurchaseCommandBoundary::new(
        PackageEnforcementMode::Shadow,
        "commercial-command-002",
        scope("legal.contracts"),
    )
    .unwrap()
    .expect("shadow boundary");
    assert_eq!(shadow.key(), "commercial-command-002");
    assert_eq!(shadow.mode(), PackageEnforcementMode::Shadow);

    let enforced = PurchaseCommandBoundary::new(
        PackageEnforcementMode::Enforced,
        "commercial-command-003",
        scope("legal.contracts"),
    )
    .unwrap()
    .expect("enforced boundary");
    assert_eq!(enforced.key(), "commercial-command-003");
    assert_eq!(enforced.mode(), PackageEnforcementMode::Enforced);
}

#[test]
fn unbounded_keys_and_capabilities_are_rejected_before_any_sql() {
    for (key, capability) in [
        ("", "legal.contracts"),
        ("commercial-command-001", ""),
    ] {
        let error = PurchaseCommandBoundary::new(
            PackageEnforcementMode::Enforced,
            key,
            scope(capability),
        )
        .unwrap_err();
        assert_eq!(error.code, "COMMERCIAL_PURCHASE_COMMAND_REQUIRED");
        assert_eq!(error.kind, FailureKind::InvalidInput);
    }

    let oversized = "k".repeat(201);
    let error = PurchaseCommandBoundary::new(
        PackageEnforcementMode::Shadow,
        oversized,
        scope("legal.contracts"),
    )
    .unwrap_err();
    assert_eq!(error.code, "COMMERCIAL_PURCHASE_COMMAND_REQUIRED");

    let oversized_capability = "c".repeat(129);
    let error = PurchaseCommandBoundary::new(
        PackageEnforcementMode::Shadow,
        "commercial-command-001",
        scope(&oversized_capability),
    )
    .unwrap_err();
    assert_eq!(error.code, "COMMERCIAL_PURCHASE_COMMAND_REQUIRED");
}
