#[path = "../../../scripts/testing/rust-architecture.rs"]
mod architecture;

#[test]
fn implemented_rust_owners_keep_portable_layers_and_public_facades() {
    let packages = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let mut checked = 0;
    let mut issues = Vec::new();
    for entry in std::fs::read_dir(packages).unwrap() {
        let path = entry.unwrap().path();
        if path.join("Cargo.toml").is_file()
            && path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("server-")
        {
            checked += 1;
            issues.extend(architecture::inspect_crate(&path).unwrap());
        }
    }
    assert!(
        checked >= 3,
        "expected implemented shared/runtime/legal owner crates"
    );
    assert!(issues.is_empty(), "{}", issues.join("\n"));
}

#[test]
fn rejects_aliased_nested_and_qualified_effects_in_portable_core() {
    for source in [
        "use std::{fs as storage};",
        "use sqlx::PgPool as Store;",
        "fn x(){std::env::var(\"SECRET\");}",
        "use tokio::{net::TcpStream};",
        "fn x(){std::time::SystemTime::now();}",
    ] {
        assert!(
            !architecture::inspect_source(source, "application/commands.rs")
                .unwrap()
                .is_empty(),
            "accepted {source}"
        );
    }
    assert!(
        architecture::inspect_source(
            "use std::{future::Future, pin::Pin}; use crate::domain::Amount;",
            "application/ports.rs"
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn rejects_layer_backdoors_and_private_cross_owner_imports() {
    for (source, path) in [
        ("use crate::adapters::store;", "application/command.rs"),
        (
            "use super::super::adapters::store;",
            "application/command.rs",
        ),
        ("use crate::application::Service;", "domain/model.rs"),
        ("use crate::public::runtime;", "domain/model.rs"),
        (
            "use erpx_server_finance::adapters::sql;",
            "adapters/query.rs",
        ),
    ] {
        assert!(
            !architecture::inspect_source(source, path)
                .unwrap()
                .is_empty(),
            "accepted {source}"
        );
    }
    assert!(
        architecture::inspect_source(
            "use erpx_server_shared::public::BusinessError;",
            "application/command.rs"
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn rejects_unexamined_core_macros_and_source_path_injection() {
    for source in [
        "const ROOT:&str=env!(\"HOME\");",
        "include!(\"hidden.rs\");",
        "const DATA:&str=include_str!(\"config\");",
        "custom_effect!();",
        "macro_rules! hidden {()=>{std::fs::read(\"x\")}}",
        "#[path=\"../adapters/io.rs\"] mod hidden;",
    ] {
        assert!(
            !architecture::inspect_source(source, "domain/model.rs")
                .unwrap()
                .is_empty(),
            "accepted {source}"
        );
    }
}

#[test]
fn contract_metadata_is_fully_examined_and_cannot_hide_effects_or_other_attributes() {
    for attr in [
        "derive(ts_rs::TS, utoipa::ToSchema)",
        "derive(ts_rs::TS)",
        "derive(utoipa::ToSchema)",
        "ts(optional)",
        "ts(type = \"number\")",
        "schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64)",
    ] {
        let source =
            format!("#[cfg_attr(feature = \"contracts\", {attr})] struct Item {{ value: i64 }}");
        assert!(
            architecture::inspect_source(&source, "domain/model.rs")
                .unwrap()
                .is_empty(),
            "rejected {source}"
        );
    }
    for attr in [
        "feature = \"other\", derive(ts_rs::TS)",
        "any(feature = \"contracts\"), derive(ts_rs::TS)",
        "all(feature = \"contracts\"), derive(ts_rs::TS)",
        "not(feature = \"contracts\"), derive(ts_rs::TS)",
        "feature = \"contracts\", derive(ts_rs::TS, unknown::Effect)",
        "feature = \"contracts\", derive(ts_rs::TS, ts_rs::TS)",
        "feature = \"contracts\", derive(ts_rs::TS), path = \"../adapters/io.rs\"",
        "feature = \"contracts\", cfg_attr(feature = \"other\", path = \"x\")",
        "feature = \"contracts\", ts(optional, optional)",
        "feature = \"contracts\", ts(type = \"number\", optional)",
        "feature = \"contracts\", ts(type = std::fs::read(\"x\"))",
        "feature = \"contracts\", ts(type = \"number\"), allow(dead_code)",
        "feature = \"contracts\", schema(value_type = std::fs::File, minimum = 1, maximum = 2)",
        "feature = \"contracts\", schema(value_type = i64, minimum = std::fs::read(\"x\"), maximum = 2)",
        "feature = \"contracts\", schema(value_type = i64, minimum = 1, maximum = 2, example = std::fs::read(\"x\"))",
        "feature = \"contracts\", schema(value_type = i64, minimum = 1, maximum = 2, maximum = 3)",
        "feature = \"contracts\", schema(value_type = i64, minimum = 1, maximum = include!(\"x\"))",
        "feature = \"contracts\", schema(value_type = i64, minimum = 1, maximum = 2) trailing",
    ] {
        let source = format!("#[cfg_attr({attr})] struct Item {{ value: i64 }}");
        let result = architecture::inspect_source(&source, "domain/model.rs");
        assert!(
            result.is_err() || !result.unwrap().is_empty(),
            "accepted {source}"
        );
    }
    let source =
        "#[cfg_attr(feature = \"contracts\", derive(ts_rs::TS))] fn f() { std::fs::read(\"x\"); }";
    assert!(
        architecture::inspect_source(source, "application/check.rs")
            .unwrap()
            .iter()
            .any(|issue| issue.starts_with("portable-core-effect:std::fs"))
    );
    let source = "#[cfg_attr(feature = \"contracts\", derive(ts_rs::TS))] fn f() { include!(\"hidden.rs\"); }";
    assert!(
        architecture::inspect_source(source, "application/check.rs")
            .unwrap()
            .iter()
            .any(|issue| issue == "portable-core-macro-not-approved:include")
    );
}

#[test]
fn approved_macros_do_not_hide_effectful_arguments() {
    for source in [
        "fn x(){format!(\"{:?}\",std::fs::read(\"x\"));}",
        "fn x(){vec![std::env::var(\"SECRET\");2];}",
        "fn x(){matches!(value, Some(_) if std::fs::read(\"x\").is_ok());}",
    ] {
        assert!(
            !architecture::inspect_source(source, "domain/model.rs")
                .unwrap()
                .is_empty(),
            "accepted {source}"
        );
    }
    assert!(
        architecture::inspect_source(
            "fn x(){matches!(value, Some(_) | None); vec![1;2]; format!(\"{}\",1);}",
            "domain/model.rs"
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn crate_roots_export_only_the_named_public_facade() {
    assert!(
        architecture::inspect_source(
            "mod domain; mod application; mod adapters; pub mod public;",
            "lib.rs"
        )
        .unwrap()
        .is_empty()
    );
    for source in [
        "pub mod domain; pub mod public;",
        "pub use adapters::store; pub mod public;",
        "mod domain;",
    ] {
        assert!(
            !architecture::inspect_source(source, "lib.rs")
                .unwrap()
                .is_empty(),
            "accepted {source}"
        );
    }
}
