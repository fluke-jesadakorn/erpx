use std::{fs, path::Path};

#[test]
fn support_core_has_no_framework_or_foreign_context_dependencies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    for forbidden in [
        "sqlx",
        "tonic",
        "prost",
        "dioxus",
        "erpx-domain",
        "erpx-application",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "forbidden dependency: {forbidden}"
        );
    }
    let mut source = String::new();
    for file in [
        "src/lib.rs",
        "src/domain/mod.rs",
        "src/application/mod.rs",
        "src/ports/mod.rs",
        "src/integration/mod.rs",
    ] {
        source.push_str(&fs::read_to_string(root.join(file)).unwrap());
    }
    for forbidden in [
        "use sqlx",
        "use tonic",
        "use prost",
        "use dioxus",
        "erpx_domain",
        "erpx_application",
        "erpx_identity::",
        "erpx_commercial::",
        "erpx_customer_authority::",
    ] {
        assert!(
            !source.contains(forbidden),
            "forbidden core import: {forbidden}"
        );
    }
}

#[test]
fn query_handler_has_no_write_repository_field_or_fallback() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(root.join("src/application/mod.rs")).unwrap();
    let start = source.find("pub struct SupportQueryHandler").unwrap();
    let query_source = &source[start..];
    assert!(!query_source.contains("WriteRepository"));
    assert!(!query_source.contains("load_ticket"));
    assert!(!query_source.contains("load_incident"));
    assert!(!query_source.contains("load_session"));
    assert!(!query_source.contains("load_capability"));
}
