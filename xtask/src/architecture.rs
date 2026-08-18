use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

pub(crate) type ArchitectureResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const DOMAIN_FORBIDDEN_DEPENDENCIES: &[&str] = &[
    "sqlx",
    "tonic",
    "prost",
    "prost-types",
    "dioxus",
    "axum",
    "http",
    "hyper",
    "reqwest",
    "tower",
    "tower-http",
    "aws-sdk-s3",
    "stripe",
    "ldap3",
    "erpx-domain",
    "erpx-infrastructure",
];

const DOMAIN_FORBIDDEN_IMPORTS: &[&str] = &[
    "sqlx::",
    "tonic::",
    "prost::",
    "dioxus::",
    "axum::",
    "http::",
    "hyper::",
    "reqwest::",
    "tower::",
    "aws_sdk_",
    "stripe::",
    "ldap3::",
    "ollama_rs::",
    "async_openai::",
    "erpx_cqrs::",
    "erpx_domain::",
    "erpx_infrastructure::",
];

const CONTEXT_FORBIDDEN_IMPORTS: &[&str] = &[
    "sqlx::",
    "tonic::",
    "prost::",
    "dioxus::",
    "axum::",
    "http::",
    "hyper::",
    "reqwest::",
    "tower::",
    "aws_sdk_",
    "stripe::",
    "ldap3::",
    "ollama_rs::",
    "async_openai::",
    "erpx_domain::",
    "erpx_infrastructure::",
];

pub(crate) fn scan(root: &Path) -> ArchitectureResult {
    crate::affected::validate_workspace(root)?;
    enforce_foundation_dependencies(root)?;
    let context_root = root.join("crates/contexts");
    if context_root.is_dir() {
        let mut violations = Vec::new();
        for entry in fs::read_dir(&context_root)? {
            let path = entry?.path();
            if !path.is_dir() {
                continue;
            }
            let manifest = path.join("Cargo.toml");
            if manifest.is_file() {
                collect_violation(
                    reject_manifest_dependencies(&manifest, DOMAIN_FORBIDDEN_DEPENDENCIES),
                    &mut violations,
                );
                collect_violation(reject_context_dependency_edges(&manifest), &mut violations);
            }
            collect_violation(
                reject_rust_imports(&path, CONTEXT_FORBIDDEN_IMPORTS),
                &mut violations,
            );
            collect_violation(enforce_context_layer_dependencies(&path), &mut violations);
        }
        violations.sort();
        violations.dedup();
        if !violations.is_empty() {
            return Err(format!(
                "context architecture violations:\n- {}",
                violations.join("\n- ")
            )
            .into());
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
    // wire contract. Presenters and views remain transport/domain agnostic,
    // and feature packages must stay as independent leaves.
    let console_features = root.join("crates/console-features");
    if console_features.is_dir() {
        for entry in fs::read_dir(&console_features)? {
            let feature = entry?.path();
            if !feature.is_dir() {
                continue;
            }
            let manifest = feature.join("Cargo.toml");
            if manifest.is_file() {
                reject_manifest_dependency_prefix(&manifest, "erpx-console-feature-")?;
            }
            for file in rust_files(&feature.join("src"))? {
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
    }

    let console_shell_manifest = root.join("crates/console-shell/Cargo.toml");
    if console_shell_manifest.is_file() {
        reject_manifest_dependencies_except_prefix(
            &console_shell_manifest,
            "erpx-",
            &["erpx-ui-core", "erpx-ui-kit"],
        )?;
        reject_rust_imports(
            &root.join("crates/console-shell/src"),
            &[
                "erpx_proto::",
                "tonic::",
                "prost::",
                "erpx_domain::",
                "erpx_cqrs::",
            ],
        )?;
    }

    let console_transport_manifest = root.join("crates/console-transport/Cargo.toml");
    if console_transport_manifest.is_file() {
        reject_manifest_dependency_prefix(&console_transport_manifest, "erpx-console-feature-")?;
        reject_manifest_dependencies(
            &console_transport_manifest,
            &[
                "erpx-console-shell",
                "erpx-ui",
                "erpx-ui-core",
                "erpx-ui-kit",
            ],
        )?;
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

    let console_manifest = root.join("apps/console/Cargo.toml");
    if console_manifest.is_file() {
        reject_manifest_dependencies(&console_manifest, &["erpx-ui"])?;
    }

    let console_bff = root.join("crates/console-bff/src");
    if console_bff.is_dir() {
        reject_rust_imports(
            &console_bff,
            &["sqlx::", "erpx_infrastructure::", "erpx_web_session_sqlx::"],
        )?;
    }
    let console_bff_manifest = root.join("crates/console-bff/Cargo.toml");
    if console_bff_manifest.is_file() {
        reject_manifest_dependencies(
            &console_bff_manifest,
            &["sqlx", "erpx-infrastructure", "erpx-web-session-sqlx"],
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
    scan_native_boundaries(root)?;

    println!("DDD/Clean/Hexagonal/CQRS and ERPX UI kit architecture boundaries passed");
    Ok(())
}

/// Enforce the Web/macOS renderer boundary independently so platform release
/// checks can run it without repeating every domain architecture rule.
pub(crate) fn scan_native_boundaries(root: &Path) -> ArchitectureResult {
    let mut violations = Vec::new();
    for source_root in [root.join("apps"), root.join("crates")] {
        if !source_root.is_dir() {
            continue;
        }
        for file in rust_files(&source_root)? {
            let source = fs::read_to_string(&file)?;
            let web_adapter = is_web_adapter(&file, root);
            for (index, line) in source.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") {
                    continue;
                }
                if line.contains("document::eval") {
                    violations.push(format!(
                        "document::eval is forbidden in first-party code: {}:{}",
                        file.display(),
                        index + 1
                    ));
                }
                if is_javascript_bridge_constant(trimmed) {
                    violations.push(format!(
                        "JavaScript bridge constants are forbidden: {}:{}",
                        file.display(),
                        index + 1
                    ));
                }
                if line.contains("web_sys") && !web_adapter {
                    violations.push(format!(
                        "web_sys is restricted to the Console Web adapter: {}:{}",
                        file.display(),
                        index + 1
                    ));
                }
            }
        }
    }

    for manifest in manifest_files(root)? {
        let source = fs::read_to_string(&manifest)?;
        let lower = source.to_ascii_lowercase();
        for forbidden in [
            "dioxus/desktop",
            "dioxus/mobile",
            "dioxus-desktop",
            "dioxus-mobile",
            "wry",
            "webview",
            "webkit",
        ] {
            if lower.lines().any(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with('#') && contains_dependency_token(trimmed, forbidden)
            }) {
                violations.push(format!(
                    "WebView renderer dependency `{forbidden}` is forbidden: {}",
                    manifest.display()
                ));
            }
        }
    }

    violations.sort();
    violations.dedup();
    if violations.is_empty() {
        println!("Dioxus Web/macOS native renderer boundaries passed");
        Ok(())
    } else {
        Err(format!(
            "Web/macOS native architecture violations:\n- {}",
            violations.join("\n- ")
        )
        .into())
    }
}

fn is_javascript_bridge_constant(line: &str) -> bool {
    let declaration = line
        .strip_prefix("pub ")
        .unwrap_or(line)
        .strip_prefix("const ")
        .or_else(|| line.strip_prefix("static "));
    let Some(declaration) = declaration else {
        return false;
    };
    let name = declaration
        .split(|character: char| character == ':' || character.is_whitespace())
        .next()
        .unwrap_or_default();
    name.ends_with("_SCRIPT") || name.contains("JAVASCRIPT") || name.contains("JS_BRIDGE")
}

fn is_web_adapter(file: &Path, root: &Path) -> bool {
    let Ok(relative) = file.strip_prefix(root.join("apps/console/src")) else {
        return false;
    };
    let components = relative
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    // Console platform/transport modules are adapters; their typed Web
    // implementations are target-gated beside the native implementation.
    // Composition, UI, and feature modules do not get this exemption.
    components
        .first()
        .is_some_and(|component| matches!(*component, "platform" | "platform.rs" | "transport"))
        || components.contains(&"web")
        || relative.file_stem().is_some_and(|stem| stem == "web")
}

fn contains_dependency_token(line: &str, token: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower
        .split(|character: char| !(character.is_ascii_alphanumeric() || "-_/".contains(character)))
        .any(|word| word == token)
}

fn manifest_files(root: &Path) -> ArchitectureResult<Vec<PathBuf>> {
    let mut pending = vec![root.join("apps"), root.join("crates")];
    let mut manifests = Vec::new();
    while let Some(directory) = pending.pop() {
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|name| name == "Cargo.toml") {
                manifests.push(path);
            }
        }
    }
    manifests.sort();
    Ok(manifests)
}

fn enforce_foundation_dependencies(root: &Path) -> ArchitectureResult {
    let kernel_manifest = root.join("crates/kernel/Cargo.toml");
    if kernel_manifest.is_file() {
        reject_manifest_dependencies(&kernel_manifest, DOMAIN_FORBIDDEN_DEPENDENCIES)?;
        reject_manifest_dependency_prefix(&kernel_manifest, "erpx-")?;
        reject_rust_imports(&root.join("crates/kernel/src"), DOMAIN_FORBIDDEN_IMPORTS)?;
    }

    let contracts_manifest = root.join("crates/integration-contracts/Cargo.toml");
    if contracts_manifest.is_file() {
        reject_manifest_dependencies(&contracts_manifest, DOMAIN_FORBIDDEN_DEPENDENCIES)?;
        reject_manifest_dependencies_except_prefix(&contracts_manifest, "erpx-", &["erpx-kernel"])?;
        reject_rust_imports(
            &root.join("crates/integration-contracts/src"),
            &[
                "sqlx::",
                "tonic::",
                "prost::",
                "dioxus::",
                "axum::",
                "http::",
                "hyper::",
                "reqwest::",
                "erpx_cqrs::",
                "erpx_proto::",
                "erpx_domain::",
                "erpx_infrastructure::",
            ],
        )?;
    }
    Ok(())
}

fn enforce_context_layer_dependencies(context: &Path) -> ArchitectureResult {
    // Domain is the innermost ring. It may use its own types, the minimal
    // kernel, and language-level libraries, but never application/runtime or
    // adapter technology.
    let mut violations = Vec::new();
    for domain in [context.join("src/domain"), context.join("src/domain.rs")] {
        if domain.exists() {
            collect_violation(
                reject_rust_imports(&domain, DOMAIN_FORBIDDEN_IMPORTS),
                &mut violations,
            );
        }
    }

    // Consumer-owned ports describe capabilities. Implementations belong in
    // adapters and must not leak database, transport, or provider SDK types.
    for ports in [context.join("src/ports"), context.join("src/ports.rs")] {
        if ports.exists() {
            collect_violation(
                reject_rust_imports(&ports, CONTEXT_FORBIDDEN_IMPORTS),
                &mut violations,
            );
        }
    }

    // Context integration modules map versioned facts to internal values; the
    // network/database transport remains an outer adapter.
    for integration in [
        context.join("src/integration"),
        context.join("src/integration.rs"),
    ] {
        if integration.exists() {
            collect_violation(
                reject_rust_imports(&integration, DOMAIN_FORBIDDEN_IMPORTS),
                &mut violations,
            );
        }
    }
    if !violations.is_empty() {
        return Err(violations.join("\n").into());
    }
    Ok(())
}

fn collect_violation(result: ArchitectureResult, violations: &mut Vec<String>) {
    if let Err(error) = result {
        violations.push(error.to_string());
    }
}

fn enforce_ui_kit_boundary(root: &Path) -> ArchitectureResult {
    let ui_kit = root.join("crates/ui-kit/src");
    let scan_roots = [
        root.join("crates/ui/src"),
        root.join("crates/ui-kit/src"),
        root.join("crates/ui-domains"),
        root.join("apps/console/src"),
        root.join("crates/console-features"),
    ];
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
                if !file.starts_with(root.join("crates/ui-kit"))
                    && line.contains("erpx_ui_core::kit")
                {
                    return Err(format!(
                        "Console code must import shared controls through erpx_ui_kit: {}:{}",
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

fn reject_manifest_dependency_prefix(path: &Path, forbidden_prefix: &str) -> ArchitectureResult {
    let source = fs::read_to_string(path)?;
    for line in source.lines() {
        let Some((name, _)) = line.split_once('=') else {
            continue;
        };
        let dependency = name.trim();
        let dependency = dependency.strip_suffix(".workspace").unwrap_or(dependency);
        if dependency.starts_with(forbidden_prefix) {
            return Err(format!("forbidden dependency {dependency} in {}", path.display()).into());
        }
    }
    Ok(())
}

fn reject_manifest_dependencies_except_prefix(
    path: &Path,
    prefix: &str,
    allowed: &[&str],
) -> ArchitectureResult {
    let source = fs::read_to_string(path)?;
    for line in source.lines() {
        let Some((name, _)) = line.split_once('=') else {
            continue;
        };
        let dependency = name.trim();
        let dependency = dependency.strip_suffix(".workspace").unwrap_or(dependency);
        if dependency.starts_with(prefix) && !allowed.contains(&dependency) {
            return Err(format!(
                "forbidden internal dependency {dependency} in {}",
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
    let mut violations = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file)?;
        for (index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with('#') {
                continue;
            }
            if forbidden.iter().any(|token| line.contains(token)) {
                violations.push(format!(
                    "forbidden architecture import in {}:{}",
                    file.display(),
                    index + 1
                ));
            }
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n").into())
    }
}

fn reject_server_root_sql(path: &Path) -> ArchitectureResult {
    const TEST_BOUNDARY: &str = "#[cfg(test)]\nmod target_service_registration_tests";
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
    fn javascript_bridge_constant_names_are_rejected() {
        assert!(is_javascript_bridge_constant(
            "const AUTH_HTTP_SCRIPT: &str = r#\"fetch()\"#;"
        ));
        assert!(is_javascript_bridge_constant(
            "pub const JS_BRIDGE_SOURCE: &str = \"\";"
        ));
        assert!(!is_javascript_bridge_constant(
            "const MAX_RESPONSE_BYTES: usize = 1024;"
        ));
    }

    #[test]
    fn web_sys_is_limited_to_named_web_adapters() {
        let root = Path::new("/workspace");
        assert!(is_web_adapter(
            Path::new("/workspace/apps/console/src/platform/web.rs"),
            root
        ));
        assert!(is_web_adapter(
            Path::new("/workspace/apps/console/src/platform/web/history.rs"),
            root
        ));
        assert!(is_web_adapter(
            Path::new("/workspace/apps/console/src/platform/http.rs"),
            root
        ));
        assert!(is_web_adapter(
            Path::new("/workspace/apps/console/src/transport/mod.rs"),
            root
        ));
        assert!(!is_web_adapter(
            Path::new("/workspace/apps/console/src/composition.rs"),
            root
        ));
    }

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
        fs::write(&source, concat!("use erpx_proto::", "platform::Client;\n"))
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
             #[cfg(test)]\nmod target_service_registration_tests {\n\
             fn acceptance() { sqlx::query(\"SELECT 1\"); }\n}\n",
        )
        .expect("write architecture fixture");
        let error = reject_server_root_sql(&source).expect_err("composition SQL must be rejected");
        assert!(error.to_string().contains("direct SQL is forbidden"));

        fs::write(
            &source,
            "fn compose() {}\n#[cfg(test)]\nmod target_service_registration_tests {\n\
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

    #[test]
    fn feature_manifests_reject_cross_feature_dependencies() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "erpx-architecture-feature-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create architecture fixture");
        let manifest = root.join("Cargo.toml");
        fs::write(
            &manifest,
            "[dependencies]\nerpx-console-feature-finance.workspace = true\n",
        )
        .expect("write architecture fixture");
        let error = reject_manifest_dependency_prefix(&manifest, "erpx-console-feature-")
            .expect_err("cross-feature edge must be rejected");
        assert!(error.to_string().contains("forbidden dependency"));
        fs::remove_dir_all(root).expect("remove architecture fixture");
    }

    #[test]
    fn domain_layer_rejects_runtime_and_provider_dependencies() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "erpx-architecture-domain-{}-{nonce}",
            std::process::id()
        ));
        let domain = root.join("src/domain");
        fs::create_dir_all(&domain).expect("create domain fixture");
        fs::write(domain.join("mod.rs"), "use reqwest::Client;\n").expect("write domain fixture");
        let error = enforce_context_layer_dependencies(&root)
            .expect_err("provider SDK must remain outside the domain");
        assert!(error.to_string().contains("forbidden architecture import"));
        fs::remove_dir_all(root).expect("remove architecture fixture");
    }

    #[test]
    fn flat_domain_file_is_scanned_and_all_imports_are_reported() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "erpx-architecture-flat-domain-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).expect("create domain fixture");
        fs::write(
            root.join("src/domain.rs"),
            "use erpx_cqrs::FreshnessState;\nuse sqlx::PgPool;\n",
        )
        .expect("write flat domain fixture");
        let error = enforce_context_layer_dependencies(&root)
            .expect_err("flat domain runtime imports must be rejected");
        let message = error.to_string();
        assert_eq!(message.matches("forbidden architecture import").count(), 2);
        assert!(message.contains("domain.rs:1"));
        assert!(message.contains("domain.rs:2"));
        fs::remove_dir_all(root).expect("remove architecture fixture");
    }

    #[test]
    fn context_manifests_reject_other_context_cores() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "erpx-architecture-context-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create context fixture");
        let manifest = root.join("Cargo.toml");
        fs::write(
            &manifest,
            "[dependencies]\nerpx-kernel.workspace = true\nerpx-sales.workspace = true\n",
        )
        .expect("write context fixture");
        let error = reject_context_dependency_edges(&manifest)
            .expect_err("a context core must not depend on another context core");
        assert!(
            error
                .to_string()
                .contains("forbidden cross-context dependency")
        );
        fs::remove_dir_all(root).expect("remove architecture fixture");
    }

    #[test]
    fn integration_contracts_only_accept_the_kernel_as_an_internal_dependency() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "erpx-architecture-contract-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create contract fixture");
        let manifest = root.join("Cargo.toml");
        fs::write(
            &manifest,
            "[dependencies]\nerpx-kernel.workspace = true\nerpx-finance.workspace = true\n",
        )
        .expect("write contract fixture");
        let error =
            reject_manifest_dependencies_except_prefix(&manifest, "erpx-", &["erpx-kernel"])
                .expect_err("integration contract must point inward only to the kernel");
        assert!(error.to_string().contains("forbidden internal dependency"));

        fs::write(
            &manifest,
            "[dependencies]\nerpx-kernel.workspace = true\nserde.workspace = true\n",
        )
        .expect("write allowed contract fixture");
        reject_manifest_dependencies_except_prefix(&manifest, "erpx-", &["erpx-kernel"])
            .expect("kernel-only internal dependency is valid");
        fs::remove_dir_all(root).expect("remove architecture fixture");
    }
}
