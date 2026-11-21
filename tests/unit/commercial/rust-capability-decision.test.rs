use erpx_server_commercial::public::capability_decision::*;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CompanyPackageGrant, CompanyPackageHistoryRight, CompanyPurchaseFacts,
    PackageDecisionCode, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};

fn now(value: i64) -> TimestampMillis {
    TimestampMillis::new(value)
}

fn grant(state: &str, starts_at: i64, ends_at: Option<i64>, version: i64) -> CompanyPackageGrant {
    CompanyPackageGrant {
        company_id: "company".into(),
        package_id: "legal".into(),
        package_version: 1,
        state: state.into(),
        starts_at,
        ends_at,
        version,
        reason: "Reviewed package grant".into(),
        evidence: "PRIVATE_EVIDENCE".into(),
    }
}

fn work_grant(package_id: &str) -> CompanyPackageGrant {
    CompanyPackageGrant {
        company_id: "company".into(),
        package_id: package_id.into(),
        package_version: 1,
        state: "active".into(),
        starts_at: 0,
        ends_at: None,
        version: 1,
        reason: "Reviewed package grant".into(),
        evidence: "PRIVATE_EVIDENCE".into(),
    }
}

fn history(package_id: &str, retained_at: i64) -> CompanyPackageHistoryRight {
    CompanyPackageHistoryRight {
        company_id: "company".into(),
        package_id: package_id.into(),
        package_version: 1,
        source_grant_version: 1,
        source_event_id: "event".into(),
        retained_at,
    }
}

fn facts(
    grants: Vec<CompanyPackageGrant>,
    history: Vec<CompanyPackageHistoryRight>,
) -> CompanyPurchaseFacts {
    CompanyPurchaseFacts { grants, history }
}

fn scope(capability: &str, purpose: CapabilityPurpose) -> CapabilityScope {
    CapabilityScope {
        company_id: "company".into(),
        capability: capability.into(),
        purpose,
    }
}

fn gate(allowed: bool, revision: &str) -> CapabilityGate {
    CapabilityGate {
        allowed,
        revision: revision.into(),
    }
}

#[test]
fn purchase_settlement_history_expiry_and_inactive_codes_are_retained() {
    let catalogue = current_package_catalogue().unwrap();
    let current = facts(vec![grant("active", 0, Some(3000), 1)], Vec::new());
    let check = require_purchased_capability(
        &catalogue,
        &current,
        &scope("legal.contracts", CapabilityPurpose::NewWork),
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap();
    assert!(check.allowed_by_mode());
    assert_eq!(check.decision().unwrap().code, PackageDecisionCode::Granted);
    assert!(check.revision.as_ref().unwrap().len() == 64);

    // Expiry behaves as draining for new work, and refuses with its code.
    let error = require_purchased_capability(
        &catalogue,
        &current,
        &scope("legal.contracts", CapabilityPurpose::NewWork),
        PackageEnforcementMode::Enforced,
        now(3000),
    )
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(error.code, "PACKAGE_DRAINING");

    // Settlement and history retain the expired/current period.
    for purpose in [
        CapabilityPurpose::SettleExisting,
        CapabilityPurpose::History,
    ] {
        let check = require_purchased_capability(
            &catalogue,
            &current,
            &scope("legal.contracts", purpose),
            PackageEnforcementMode::Enforced,
            now(3000),
        )
        .unwrap();
        assert_eq!(check.decision().unwrap().code, PackageDecisionCode::Granted);
    }

    let draining = facts(vec![grant("draining", 0, Some(3000), 2)], Vec::new());
    let check = purchased_access(
        &catalogue,
        &draining,
        "company",
        "legal.contracts",
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap();
    assert!(!check.new_work);
    assert!(check.settle_existing);
    assert!(check.history);
    assert_eq!(check.codes.new_work, PackageDecisionCode::PackageDraining);

    let inactive = facts(vec![grant("inactive", 0, Some(3000), 3)], Vec::new());
    let check = purchased_access(
        &catalogue,
        &inactive,
        "company",
        "legal.contracts",
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap();
    assert!(!check.new_work);
    assert!(!check.settle_existing);
    assert!(!check.history);
    assert_eq!(check.codes.new_work, PackageDecisionCode::PackageInactive);

    // A retained history right restores history work without a current grant.
    let retained = facts(Vec::new(), vec![history("legal", 500)]);
    let check = purchased_access(
        &catalogue,
        &retained,
        "company",
        "legal.contracts",
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap();
    assert!(!check.new_work);
    assert!(!check.settle_existing);
    assert!(check.history);
    assert_eq!(check.codes.history, PackageDecisionCode::Granted);
}

#[test]
fn dependencies_and_missing_grants_never_fabricate_entitlement() {
    let catalogue = current_package_catalogue().unwrap();
    let addon_only = facts(vec![work_grant("work-social")], Vec::new());
    let check = purchased_access(
        &catalogue,
        &addon_only,
        "company",
        "work.social",
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap();
    assert!(!check.new_work);
    assert_eq!(
        check.codes.new_work,
        PackageDecisionCode::DependencyRequired
    );

    let with_parent = facts(
        vec![work_grant("work"), work_grant("work-social")],
        Vec::new(),
    );
    let check = purchased_access(
        &catalogue,
        &with_parent,
        "company",
        "work.social",
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap();
    assert!(check.new_work);
    assert_eq!(check.codes.new_work, PackageDecisionCode::Granted);

    let empty = facts(Vec::new(), Vec::new());
    let check = purchased_access(
        &catalogue,
        &empty,
        "company",
        "legal.contracts",
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap();
    assert!(!check.new_work && !check.settle_existing && !check.history);
    assert_eq!(check.codes.new_work, PackageDecisionCode::PackageRequired);
}

#[test]
fn disabled_shadow_and_enforced_modes_keep_the_retained_contract() {
    let catalogue = current_package_catalogue().unwrap();
    let empty = facts(Vec::new(), Vec::new());
    let capabilities = vec!["legal.contracts".to_owned(), "legal.documents".to_owned()];

    let disabled = purchased_accesses(
        &catalogue,
        &empty,
        "company",
        &capabilities,
        PackageEnforcementMode::Disabled,
        now(1000),
    )
    .unwrap();
    for access in disabled.values() {
        assert!(access.new_work && access.settle_existing && access.history);
        assert_eq!(access.revision, "disabled");
        assert_eq!(access.codes.new_work, PackageDecisionCode::Granted);
        assert_eq!(access.codes.settle_existing, PackageDecisionCode::Granted);
        assert_eq!(access.codes.history, PackageDecisionCode::Granted);
    }
    assert_eq!(disabled.len(), 2);

    let disabled_check = check_purchased_capability(
        &catalogue,
        &empty,
        &scope("legal.contracts", CapabilityPurpose::NewWork),
        PackageEnforcementMode::Disabled,
        now(1000),
    )
    .unwrap();
    assert!(disabled_check.decision().is_none());
    assert!(disabled_check.allowed_by_mode());
    assert!(disabled_check.revision.is_none());

    let shadow = purchased_access(
        &catalogue,
        &empty,
        "company",
        "legal.contracts",
        PackageEnforcementMode::Shadow,
        now(1000),
    )
    .unwrap();
    assert!(shadow.new_work && shadow.settle_existing && shadow.history);
    assert_eq!(shadow.revision.len(), 64);
    assert_eq!(shadow.codes.new_work, PackageDecisionCode::PackageRequired);

    let shadow_check = require_purchased_capability(
        &catalogue,
        &empty,
        &scope("legal.contracts", CapabilityPurpose::NewWork),
        PackageEnforcementMode::Shadow,
        now(1000),
    )
    .unwrap();
    assert!(shadow_check.allowed_by_mode());
    assert!(!shadow_check.decision().unwrap().allowed);

    let error = require_purchased_capability(
        &catalogue,
        &empty,
        &scope("legal.contracts", CapabilityPurpose::NewWork),
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap_err();
    assert_eq!(error.code, "PACKAGE_REQUIRED");
}

#[test]
fn effective_decision_precedence_uses_owner_gates_and_retained_codes() {
    let catalogue = current_package_catalogue().unwrap();
    let current = facts(vec![grant("active", 0, Some(3000), 1)], Vec::new());
    let release = gate(true, "release-1");
    let authority = gate(true, "authority-1");
    let record = gate(true, "record-1");
    let observation = decide_effective_capability(
        &catalogue,
        &current,
        "company",
        "legal.contracts",
        CapabilityPurpose::NewWork,
        now(1000),
        &release,
        &authority,
        &record,
    )
    .unwrap();
    assert!(observation.decision.allowed);
    assert_eq!(observation.decision.code.as_code(), "GRANTED");
    assert_eq!(observation.decision.revisions.release, "release-1");
    assert_eq!(observation.source_revision.len(), 64);
    assert_eq!(observation.revision.len(), 64);

    let cases = [
        (
            gate(false, "release-1"),
            gate(true, "authority-1"),
            gate(true, "record-1"),
            "RELEASE_NOT_READY",
        ),
        (
            gate(true, ""),
            gate(true, "authority-1"),
            gate(true, "record-1"),
            "RELEASE_NOT_READY",
        ),
        (
            gate(true, "release-1"),
            gate(false, "authority-1"),
            gate(true, "record-1"),
            "AUTHORITY_REQUIRED",
        ),
        (
            gate(true, "release-1"),
            gate(true, ""),
            gate(true, "record-1"),
            "AUTHORITY_REQUIRED",
        ),
        (
            gate(true, "release-1"),
            gate(true, "authority-1"),
            gate(false, "record-1"),
            "RECORD_DENIED",
        ),
    ];
    for (release, authority, record, expected) in cases {
        let observation = decide_effective_capability(
            &catalogue,
            &current,
            "company",
            "legal.contracts",
            CapabilityPurpose::NewWork,
            now(1000),
            &release,
            &authority,
            &record,
        )
        .unwrap();
        assert!(!observation.decision.allowed);
        assert_eq!(observation.decision.code.as_code(), expected);
    }

    // Purchase denial precedes every other gate.
    let empty = facts(Vec::new(), Vec::new());
    let observation = decide_effective_capability(
        &catalogue,
        &empty,
        "company",
        "legal.contracts",
        CapabilityPurpose::NewWork,
        now(1000),
        &gate(false, ""),
        &gate(false, ""),
        &gate(false, ""),
    )
    .unwrap();
    assert_eq!(observation.decision.code.as_code(), "PACKAGE_REQUIRED");
}

#[test]
fn revision_is_the_sha256_of_the_exact_retained_canonical_source() {
    let grants = vec![
        grant("active", 0, Some(3000), 1),
        CompanyPackageGrant {
            company_id: "company".into(),
            package_id: "support".into(),
            package_version: 1,
            state: "draining".into(),
            starts_at: 10,
            ends_at: Some(2000),
            version: 2,
            reason: "Reviewed".into(),
            evidence: "PRIVATE_EVIDENCE".into(),
        },
        CompanyPackageGrant {
            company_id: "foreign".into(),
            package_id: "assets".into(),
            package_version: 1,
            state: "active".into(),
            starts_at: 0,
            ends_at: None,
            version: 1,
            reason: "Reviewed".into(),
            evidence: "PRIVATE_EVIDENCE".into(),
        },
    ];
    let history = vec![history("legal", 500)];
    let source = capability_revision_source("company", &grants, &history, now(1000)).unwrap();
    assert_eq!(
        source,
        r#"{"companyId":"company","grants":[["legal",1,1,"active",0,3000,"current"],["support",1,2,"draining",10,2000,"current"]],"history":[["legal",1,1,"event",500,"retained"]]}"#
    );
    // Pinned Node 26.11.1 JSON.stringify / localeCompare / SHA-256 oracle.
    assert_eq!(
        capability_revision("company", &grants, &history, now(1000)).unwrap(),
        "2dd27c92df0f8177f3071568a317ba3c32b83e4e284fe9b054ccf9484a43c950"
    );
    assert_eq!(
        capability_revision("company", &[], &[], now(1000)).unwrap(),
        "39ed07fbbd040d61afcab922e85fc1a9eba49b14b70593a1e4727c96cb63f8e1"
    );

    // Row order never changes the source; phase boundaries do.
    let mut reordered = grants.clone();
    reordered.reverse();
    assert_eq!(
        capability_revision_source("company", &reordered, &history, now(1000)).unwrap(),
        source
    );
    assert_ne!(
        capability_revision_source("company", &grants, &history, now(3000)).unwrap(),
        source
    );
}

#[test]
fn malformed_or_foreign_facts_fail_closed() {
    let catalogue = current_package_catalogue().unwrap();
    let foreign = facts(
        vec![CompanyPackageGrant {
            company_id: "foreign".into(),
            ..grant("active", 0, None, 1)
        }],
        Vec::new(),
    );
    let error = check_purchased_capability(
        &catalogue,
        &foreign,
        &scope("legal.contracts", CapabilityPurpose::NewWork),
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap_err();
    assert_eq!(error.code, "COMMERCIAL_PURCHASE_FACTS_INVALID");

    for invalid in [
        grant("unknown", 0, None, 1),
        grant("active", -1, None, 1),
        grant("active", 0, Some(0), 1),
        grant("active", 0, None, 0),
    ] {
        let facts = facts(vec![invalid], Vec::new());
        assert_eq!(
            check_purchased_capability(
                &catalogue,
                &facts,
                &scope("legal.contracts", CapabilityPurpose::NewWork),
                PackageEnforcementMode::Enforced,
                now(1000),
            )
            .unwrap_err()
            .code,
            "COMMERCIAL_PURCHASE_FACTS_INVALID"
        );
    }

    let duplicate = facts(
        vec![grant("active", 0, None, 1), grant("active", 0, None, 2)],
        Vec::new(),
    );
    assert_eq!(
        purchased_access(
            &catalogue,
            &duplicate,
            "company",
            "legal.contracts",
            PackageEnforcementMode::Enforced,
            now(1000),
        )
        .unwrap_err()
        .code,
        "COMMERCIAL_PURCHASE_FACTS_INVALID"
    );

    let mut bad_history = history("legal", 500);
    bad_history.source_event_id.clear();
    assert_eq!(
        purchased_access(
            &catalogue,
            &facts(Vec::new(), vec![bad_history]),
            "company",
            "legal.contracts",
            PackageEnforcementMode::Enforced,
            now(1000),
        )
        .unwrap_err()
        .code,
        "COMMERCIAL_PURCHASE_FACTS_INVALID"
    );

    let error: BusinessError = purchased_accesses(
        &catalogue,
        &facts(Vec::new(), Vec::new()),
        "company",
        &[],
        PackageEnforcementMode::Enforced,
        now(1000),
    )
    .unwrap_err();
    assert_eq!(error.code, "COMMERCIAL_CAPABILITIES_REQUIRED");
}
