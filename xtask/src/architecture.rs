use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

pub(crate) type ArchitectureResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

pub(crate) fn scan(root: &Path) -> ArchitectureResult {
    crate::affected::validate_workspace(root)?;
    let context_root = root.join("crates/contexts");
    if context_root.is_dir() {
        for entry in fs::read_dir(&context_root)? {
            let path = entry?.path();
            if !path.is_dir() {
                continue;
            }
            let manifest = path.join("Cargo.toml");
            if manifest.is_file() {
                reject_manifest_dependencies(
                    &manifest,
                    &[
                        "sqlx",
                        "tonic",
                        "prost",
                        "dioxus",
                        "erpx-domain",
                        "erpx-infrastructure",
                    ],
                )?;
                reject_context_dependency_edges(&manifest)?;
            }
            reject_rust_imports(
                &path,
                &["sqlx::", "tonic::", "prost::", "dioxus::", "erpx_domain::"],
            )?;
        }
    }

    let ui_foundation = root.join("crates/ui-foundation/src");
    if ui_foundation.is_dir() {
        reject_rust_imports(
            &ui_foundation,
            &["erpx_proto::", "tonic::", "prost::", "erpx_domain::"],
        )?;
    }

    let web_session_manifest = root.join("crates/web-session/Cargo.toml");
    if web_session_manifest.is_file() {
        reject_manifest_dependencies(
            &web_session_manifest,
            &[
                "sqlx",
                "tonic",
                "prost",
                "dioxus",
                "axum",
                "erpx-infrastructure",
            ],
        )?;
        reject_rust_imports(
            &root.join("crates/web-session/src"),
            &[
                "sqlx::",
                "tonic::",
                "prost::",
                "dioxus::",
                "axum::",
                "erpx_infrastructure::",
            ],
        )?;
    }

    // A feature gateway is a Console transport adapter and may translate the
    // wire contract. Presenters and views remain transport/domain agnostic.
    let console_features = root.join("apps/console/src/features");
    if console_features.is_dir() {
        for file in rust_files(&console_features)? {
            let boundary_file = file
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| matches!(name, "presenter.rs" | "view.rs"));
            if boundary_file {
                reject_rust_imports(
                    &file,
                    &["erpx_proto::", "tonic::", "prost::", "erpx_domain::"],
                )?;
            }
        }
    }

    // Keep the public Console entry point as a composition shell. Wire and
    // domain mapping belongs behind transport/features modules.
    let console_root = root.join("apps/console/src/lib.rs");
    if console_root.is_file() {
        reject_rust_imports(
            &console_root,
            &["erpx_proto::", "tonic::", "prost::", "erpx_domain::"],
        )?;
    }

    let console_bff = root.join("apps/console/src/bff");
    if console_bff.is_dir() {
        reject_rust_imports(
            &console_bff,
            &["sqlx::", "erpx_infrastructure::", "erpx_web_session_sqlx::"],
        )?;
    }

    let server_contexts = root.join("apps/server/src/contexts");
    if server_contexts.is_dir() {
        reject_rust_imports(&server_contexts, &["sqlx::", "PgPool", "PgConnection"])?;
    }

    // The Server root is transport/composition. SQL belongs in persistence
    // adapters; test-only acceptance queries remain below the test-module
    // boundary and are intentionally excluded from this production scan.
    let server_root = root.join("apps/server/src/lib.rs");
    if server_root.is_file() {
        reject_server_root_sql(&server_root)?;
    }

    let server_manifest = root.join("apps/server/Cargo.toml");
    if server_manifest.is_file() {
        reject_manifest_dependencies(&server_manifest, &["erpx-worker"])?;
    }
    let server_sources = root.join("apps/server/src");
    if server_sources.is_dir() {
        reject_rust_imports(&server_sources, &["erpx_worker::"])?;
    }

    enforce_ui_kit_boundary(root)?;

    println!("CQRS/DDD and ERPX UI kit architecture boundaries passed");
    Ok(())
}

fn enforce_ui_kit_boundary(root: &Path) -> ArchitectureResult {
    let ui_kit = root.join("crates/ui/src/kit");
    let scan_roots = [root.join("crates/ui/src"), root.join("apps/console/src")];
    for scan_root in scan_roots {
        if !scan_root.is_dir() {
            continue;
        }
        for file in rust_files(&scan_root)? {
            if file.starts_with(&ui_kit) {
                continue;
            }
            let source = fs::read_to_string(&file)?;
            for (index, line) in source.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue;
                }
                if line.contains("dioxus_primitives") {
                    return Err(format!(
                        "dioxus_primitives may only be imported by erpx_ui::kit: {}:{}",
                        file.display(),
                        index + 1
                    )
                    .into());
                }
                for tag in [
                    "button", "input", "select", "textarea", "a", "details", "table",
                ] {
                    if contains_rsx_tag(line, tag) {
                        return Err(format!(
                            "raw interactive <{tag}> markup is outside erpx_ui::kit: {}:{}",
                            file.display(),
                            index + 1
                        )
                        .into());
                    }
                }
            }
        }
    }
    Ok(())
}

fn contains_rsx_tag(line: &str, tag: &str) -> bool {
    let needle = format!("{tag} {{");
    let mut remainder = line;
    while let Some(index) = remainder.find(&needle) {
        let prefix = remainder[..index].trim_end();
        if prefix.is_empty()
            || prefix.ends_with('{')
            || prefix.ends_with('}')
            || prefix.ends_with(',')
            || prefix.ends_with('"')
        {
            return true;
        }
        remainder = &remainder[index + needle.len()..];
    }
    false
}

fn reject_manifest_dependencies(path: &Path, forbidden: &[&str]) -> ArchitectureResult {
    let source = fs::read_to_string(path)?;
    for line in source.lines() {
        let dependency = line.split_once('=').map_or(line, |(name, _)| name).trim();
        let dependency = dependency.strip_suffix(".workspace").unwrap_or(dependency);
        if forbidden.contains(&dependency) {
            return Err(format!(
                "forbidden context-core dependency {dependency} in {}",
                path.display()
            )
            .into());
        }
    }
    Ok(())
}

fn reject_context_dependency_edges(path: &Path) -> ArchitectureResult {
    const ALLOWED_ERPX_DEPENDENCIES: &[&str] =
        &["erpx-kernel", "erpx-cqrs", "erpx-integration-contracts"];
    let source = fs::read_to_string(path)?;
    for line in source.lines() {
        let Some((name, _)) = line.split_once('=') else {
            continue;
        };
        let dependency = name.trim();
        let dependency = dependency.strip_suffix(".workspace").unwrap_or(dependency);
        if dependency.starts_with("erpx-") && !ALLOWED_ERPX_DEPENDENCIES.contains(&dependency) {
            return Err(format!(
                "forbidden cross-context dependency {dependency} in {}",
                path.display()
            )
            .into());
        }
    }
    Ok(())
}

fn reject_rust_imports(path: &Path, forbidden: &[&str]) -> ArchitectureResult {
    let files = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        rust_files(path)?
    };
    for file in files {
        let source = fs::read_to_string(&file)?;
        for (index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with('#') {
                continue;
            }
            if forbidden.iter().any(|token| line.contains(token)) {
                return Err(format!(
                    "forbidden architecture import in {}:{}",
                    file.display(),
                    index + 1
                )
                .into());
            }
        }
    }
    Ok(())
}

fn reject_server_root_sql(path: &Path) -> ArchitectureResult {
    const TEST_BOUNDARY: &str = "#[cfg(test)]\nmod v1_service_authority_tests";
    let source = fs::read_to_string(path)?;
    let production = source.split_once(TEST_BOUNDARY).map_or_else(
        || {
            Err(format!(
                "server composition test boundary is missing from {}",
                path.display()
            ))
        },
        |(production, _)| Ok(production),
    )?;
    for (index, line) in production.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        if ["sqlx::query", "sqlx::query_as", "sqlx::query_scalar"]
            .iter()
            .any(|token| line.contains(token))
        {
            return Err(format!(
                "direct SQL is forbidden in Server composition root {}:{}",
                path.display(),
                index + 1
            )
            .into());
        }
    }
    Ok(())
}

fn rust_files(path: &Path) -> ArchitectureResult<Vec<PathBuf>> {
    let mut pending = vec![path.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_root_rejects_wire_imports() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "erpx-architecture-console-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create architecture fixture");
        let source = root.join("lib.rs");
        fs::write(&source, "use erpx_proto::platform::Client;\n")
            .expect("write architecture fixture");
        let error = reject_rust_imports(&source, &["erpx_proto::"])
            .expect_err("wire import must be rejected");
        assert!(error.to_string().contains("forbidden architecture import"));
        fs::remove_dir_all(root).expect("remove architecture fixture");
    }

    #[test]
    fn server_root_rejects_production_sql_but_allows_acceptance_queries() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "erpx-architecture-server-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create architecture fixture");
        let source = root.join("lib.rs");
        fs::write(
            &source,
            "fn compose() { sqlx::query(\"SELECT 1\"); }\n\
             #[cfg(test)]\nmod v1_service_authority_tests {\n\
             fn acceptance() { sqlx::query(\"SELECT 1\"); }\n}\n",
        )
        .expect("write architecture fixture");
        let error = reject_server_root_sql(&source).expect_err("composition SQL must be rejected");
        assert!(error.to_string().contains("direct SQL is forbidden"));

        fs::write(
            &source,
            "fn compose() {}\n#[cfg(test)]\nmod v1_service_authority_tests {\n\
             fn acceptance() { sqlx::query(\"SELECT 1\"); }\n}\n",
        )
        .expect("write architecture fixture");
        reject_server_root_sql(&source).expect("test-only SQL remains valid");
        fs::remove_dir_all(root).expect("remove architecture fixture");
    }

    #[test]
    fn ui_tag_scan_distinguishes_rsx_from_rust_match_expressions() {
        assert!(contains_rsx_tag(
            "button { onclick: move |_| {} }",
            "button"
        ));
        assert!(contains_rsx_tag(
            "label { \"Name\" input { value: x } }",
            "input"
        ));
        assert!(!contains_rsx_tag("let value = match input {", "input"));
        assert!(!contains_rsx_tag("fn input() {", "input"));
    }
}
