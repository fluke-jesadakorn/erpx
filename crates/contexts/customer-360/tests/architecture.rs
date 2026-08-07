use std::{fs, path::Path};

#[test]
fn customer_360_core_has_no_framework_or_foreign_core_dependencies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    for forbidden in [
        "sqlx",
        "tonic",
        "prost",
        "dioxus",
        "erpx-domain",
        "erpx-application",
        "erpx-customer-authority",
        "erpx-commercial",
        "erpx-crm",
        "erpx-sales",
        "erpx-support",
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
        "erpx_customer_authority::",
        "erpx_commercial::",
        "erpx_crm::",
        "erpx_sales::",
        "erpx_support::",
        "serde_json",
    ] {
        assert!(
            !source.contains(forbidden),
            "forbidden core import or generic payload surface: {forbidden}"
        );
    }
}

#[test]
fn query_handler_can_only_reach_the_read_repository() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read_to_string(root.join("src/application/mod.rs")).unwrap();
    let start = source.find("pub struct Customer360QueryHandler").unwrap();
    let end = source
        .find("pub struct Customer360ProjectionHandler")
        .unwrap();
    let query_source = &source[start..end];
    assert!(query_source.contains("ReadRepository"));
    assert!(!query_source.contains("WriteRepository"));
    assert!(!query_source.contains("CommandHandler"));
    assert!(!query_source.contains("CommandEnvelope"));
}
