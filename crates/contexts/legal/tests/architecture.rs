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
            "Legal core must not depend on {forbidden}",
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
            "Legal core must not import the {name} context core",
        );
    }
}

#[test]
fn read_port_is_projection_only_and_access_history_has_no_update_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ports = fs::read_to_string(root.join("src/ports/mod.rs")).unwrap();
    let domain = fs::read_to_string(root.join("src/domain/mod.rs")).unwrap();
    assert!(ports.contains("trait ReadRepository"));
    assert!(ports.contains("QueryConsistency"));
    assert!(!ports.contains("fallback_to_write"));
    assert!(!ports.contains("update_access"));
    assert!(!ports.contains("delete_access"));
    assert!(domain.contains("pub struct AccessEntry"));
    assert!(!domain.contains("fn mutate_access"));
}
