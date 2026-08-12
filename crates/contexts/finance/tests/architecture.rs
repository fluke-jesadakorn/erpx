use std::{fs, path::Path};

#[test]
fn core_has_no_runtime_framework_or_cross_context_dependencies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    for forbidden in ["sqlx", "tonic", "prost", "dioxus"] {
        assert!(
            !manifest
                .lines()
                .any(|line| line.trim_start().starts_with(forbidden)),
            "Finance core must not depend on {forbidden}"
        );
    }
    let contexts_root = root.parent().unwrap();
    for sibling in fs::read_dir(contexts_root).unwrap() {
        let sibling = sibling.unwrap().path();
        if sibling == root || !sibling.is_dir() {
            continue;
        }
        let Some(name) = sibling.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        assert!(
            !manifest.contains(&format!("../{name}")),
            "Finance core must not import the {name} context core"
        );
    }
}

#[test]
fn reads_are_projection_only_and_inbound_dependencies_are_contract_facts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ports = fs::read_to_string(root.join("src/ports/mod.rs")).unwrap();
    let domain = fs::read_to_string(root.join("src/domain/mod.rs")).unwrap();
    assert!(ports.contains("trait ReadRepository"));
    assert!(ports.contains("QueryConsistency"));
    assert!(!ports.contains("fallback_to_write"));
    for fact in [
        "sales::BillingEligibilityFact",
        "inventory::StockMovementFact",
        "people_payroll::PayrollFinalizationFact",
        "pos::PosSaleFact",
        "pos::PosRefundFact",
    ] {
        assert!(domain.contains(fact), "missing typed inbound fact {fact}");
    }
    assert!(!domain.contains("serde_json"));
}
