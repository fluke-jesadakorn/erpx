#![allow(dead_code)]

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) type AffectedResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const SNAPSHOT_VERSION: u32 = 3;
const SNAPSHOT_FILE: &str = "affected-tests.json";
const CHECK_SNAPSHOT_FILE: &str = "affected-checks.json";
const RELEVANT_ROOT_FILES: [&str; 5] = [
    ".cargo/config.toml",
    ".github/workflows/erpx-targets.yml",
    "Cargo.lock",
    "Cargo.toml",
    "rust-toolchain.toml",
];
const RELEVANT_DIRECTORIES: [&str; 4] = ["apps", "crates", "fixtures", "xtask"];
const HARNESSES: [&str; 19] = [
    "infra_platform",
    "infra_tenant_runtime",
    "infra_finance",
    "infra_marketing",
    "infra_operations",
    "infra_people_support",
    "infra_backup",
    "infra_migration",
    "worker_platform",
    "worker_finance_operations",
    "worker_directory",
    "migrator_products",
    "migrator_cutover",
    "identity_core",
    "identity_acceptance",
    "domain_contracts",
    "ui_domains",
    "ui_kit",
    "ui_platform",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CargoInvocation {
    pub(crate) label: String,
    pub(crate) args: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct AffectedPlan {
    pub(crate) changed_files: Vec<String>,
    pub(crate) rust_files: Vec<String>,
    pub(crate) changed_packages: Vec<String>,
    pub(crate) invocations: Vec<CargoInvocation>,
    pub(crate) harnesses: Vec<String>,
    pub(crate) architecture: bool,
    pub(crate) web_assets_only: bool,
    snapshot: Snapshot,
}

#[derive(Debug)]
pub(crate) struct AffectedCheckPlan {
    pub(crate) changed_files: Vec<String>,
    pub(crate) rust_files: Vec<String>,
    pub(crate) changed_packages: Vec<String>,
    pub(crate) invocations: Vec<CargoInvocation>,
    pub(crate) web_assets_only: bool,
    pub(crate) global: bool,
    snapshot: Snapshot,
}

impl AffectedCheckPlan {
    pub(crate) fn is_empty(&self) -> bool {
        self.changed_files.is_empty()
    }

    pub(crate) fn print(&self) {
        if self.changed_files.is_empty() {
            println!("affected checks: no relevant changes since the last successful quick lane");
            return;
        }
        println!("affected check files ({}):", self.changed_files.len());
        for path in &self.changed_files {
            println!("  {path}");
        }
        if !self.changed_packages.is_empty() {
            println!(
                "affected check packages: {}",
                self.changed_packages.join(", ")
            );
        }
        if !self.rust_files.is_empty() {
            println!(
                "would run: rustfmt --edition 2024 --check --config skip_children=true {}",
                self.rust_files.join(" ")
            );
        }
        if self.web_assets_only {
            println!(
                "affected check lane: Web assets only; Dioxus hot reload is the validation path"
            );
        }
        if self.global {
            println!(
                "affected check lane: global dependency/tooling/contract change; escalating outside the quick path"
            );
        }
        for invocation in &self.invocations {
            println!("would run: cargo {}", invocation.args.join(" "));
        }
    }
}

impl AffectedPlan {
    pub(crate) fn is_empty(&self) -> bool {
        self.changed_files.is_empty()
    }

    pub(crate) fn print(&self) {
        if self.changed_files.is_empty() {
            println!("affected tests: no relevant changes since the last successful lane");
            return;
        }
        println!("affected files ({}):", self.changed_files.len());
        for path in &self.changed_files {
            println!("  {path}");
        }
        if !self.changed_packages.is_empty() {
            println!("affected packages: {}", self.changed_packages.join(", "));
        }
        if !self.rust_files.is_empty() {
            println!(
                "would run: rustfmt --edition 2024 --check --config skip_children=true {}",
                self.rust_files.join(" ")
            );
        }
        if self.web_assets_only {
            println!("affected lane: Web assets only; Dioxus hot reload is the validation path");
        }
        for invocation in &self.invocations {
            println!("would run: cargo {}", invocation.args.join(" "));
        }
        for harness in &self.harnesses {
            println!("would run: cargo xtask test {harness}");
        }
        if self.architecture {
            println!("would run: cargo xtask architecture");
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Snapshot {
    version: u32,
    files: BTreeMap<String, FileState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct FileState {
    sha256: String,
    len: u64,
    modified_ns: u64,
}

#[derive(Debug, Deserialize)]
struct Metadata {
    packages: Vec<MetadataPackage>,
    workspace_members: Vec<String>,
    resolve: Option<MetadataResolve>,
}

#[derive(Debug, Deserialize)]
struct MetadataPackage {
    id: String,
    name: String,
    manifest_path: PathBuf,
    targets: Vec<MetadataTarget>,
}

#[derive(Debug, Deserialize)]
struct MetadataTarget {
    name: String,
    kind: Vec<String>,
    src_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct MetadataResolve {
    nodes: Vec<MetadataNode>,
}

#[derive(Debug, Deserialize)]
struct MetadataNode {
    id: String,
    deps: Vec<MetadataDependency>,
}

#[derive(Debug, Deserialize)]
struct MetadataDependency {
    pkg: String,
}

#[derive(Debug, Clone)]
struct WorkspacePackage {
    id: String,
    name: String,
    root: PathBuf,
    manifest: PathBuf,
    test_targets: BTreeSet<String>,
}

pub(crate) fn build_plan(root: &Path) -> AffectedResult<AffectedPlan> {
    let previous = read_snapshot(root)?;
    let current = capture_snapshot(root, previous.as_ref())?;
    let changed = match previous {
        Some(previous) => diff_snapshots(&previous, &current),
        None => git_worktree_changes(root)?,
    };
    let changed_files = changed.into_iter().collect::<Vec<_>>();
    if changed_files.is_empty() {
        return Ok(AffectedPlan {
            changed_files,
            rust_files: Vec::new(),
            changed_packages: Vec::new(),
            invocations: Vec::new(),
            harnesses: Vec::new(),
            architecture: false,
            web_assets_only: false,
            snapshot: current,
        });
    }

    let metadata = cargo_metadata(root)?;
    let packages = workspace_packages(root, &metadata)?;
    let changed_package_ids = packages_for_paths(&packages, &changed_files);
    let changed_packages = changed_package_ids
        .iter()
        .filter_map(|id| packages.iter().find(|package| &package.id == id))
        .map(|package| package.name.clone())
        .collect::<BTreeSet<_>>();
    let rust_files = changed_files
        .iter()
        .filter(|path| path.ends_with(".rs") && root.join(path).is_file())
        .cloned()
        .collect::<Vec<_>>();
    let web_assets_only = changed_files.iter().all(|path| is_web_asset(path));
    let global = changed_files.iter().any(|path| is_global_change(path));
    let mut invocations = Vec::new();
    let mut architecture = false;

    if global {
        invocations.push(cargo_invocation(
            "affected workspace check",
            [
                "check",
                "--workspace",
                "--exclude",
                "erpx-android-bridge",
                "--lib",
                "--bins",
                "--locked",
                "--offline",
            ],
        ));
        invocations.push(cargo_invocation(
            "affected workspace tests",
            [
                "test",
                "--workspace",
                "--exclude",
                "erpx-android-bridge",
                "--lib",
                "--bins",
                "--locked",
                "--offline",
                "--no-fail-fast",
            ],
        ));
        architecture = true;
    } else if !web_assets_only {
        let mut library_packages = BTreeSet::new();
        let mut changed_bins = BTreeSet::new();
        for package_id in &changed_package_ids {
            let Some(package) = packages.iter().find(|package| &package.id == package_id) else {
                continue;
            };
            if matches!(package.name.as_str(), "xtask" | "erpx-android-bridge") {
                continue;
            }
            let package_paths = changed_files
                .iter()
                .filter(|path| Path::new(path).starts_with(&package.root))
                .collect::<Vec<_>>();
            let package_metadata = metadata
                .packages
                .iter()
                .find(|candidate| candidate.id == package.id)
                .ok_or_else(|| format!("missing Cargo metadata for {}", package.name))?;
            let binary_targets = changed_binary_targets(root, package_metadata, &package_paths)?;
            let has_library = package_metadata.targets.iter().any(|target| {
                target
                    .kind
                    .iter()
                    .any(|kind| kind == "lib" || kind == "proc-macro")
            });
            let library_changed = has_library
                && package_paths.iter().any(|path| {
                    let relative = Path::new(path)
                        .strip_prefix(&package.root)
                        .unwrap_or_else(|_| Path::new(path));
                    path.ends_with("Cargo.toml")
                        || (relative.starts_with("src")
                            && !binary_targets.iter().any(|(_, source)| source == *path))
                });
            if library_changed {
                library_packages.insert(package.name.clone());
            }
            changed_bins.extend(
                binary_targets
                    .into_iter()
                    .map(|(binary, _)| (package.name.clone(), binary)),
            );
        }
        let library_packages = library_packages.into_iter().collect::<Vec<_>>();
        if !library_packages.is_empty() {
            let mut args = vec!["test".into(), "--locked".into(), "--offline".into()];
            append_packages(&mut args, &library_packages);
            args.extend(["--lib".into(), "--no-fail-fast".into()]);
            invocations.push(CargoInvocation {
                label: "affected package library tests".into(),
                args,
            });
        }
        for (package, binary) in changed_bins {
            invocations.push(CargoInvocation {
                label: format!("affected package binary test {package}/{binary}"),
                args: vec![
                    "test".into(),
                    "-p".into(),
                    package,
                    "--bin".into(),
                    binary,
                    "--locked".into(),
                    "--offline".into(),
                    "--no-fail-fast".into(),
                ],
            });
        }
        if changed_packages.contains("xtask") {
            invocations.push(cargo_invocation(
                "affected lightweight xtask tests",
                [
                    "test",
                    "-p",
                    "xtask",
                    "--no-default-features",
                    "--bin",
                    "xtask",
                    "--locked",
                    "--offline",
                ],
            ));
        }

        let reverse_ids = reverse_dependents(&metadata, &changed_package_ids);
        let console_feature_changed = changed_files
            .iter()
            .any(|path| is_console_feature_source(path));
        let console_bff_changed = changed_files.iter().any(|path| is_console_bff_source(path));
        let reverse_packages = reverse_ids
            .difference(&changed_package_ids)
            .filter_map(|id| packages.iter().find(|package| &package.id == id))
            .map(|package| package.name.clone())
            .filter(|name| {
                !matches!(name.as_str(), "xtask" | "erpx-android-bridge")
                    && !(name == "erpx-console" && (console_feature_changed || console_bff_changed))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if !reverse_packages.is_empty() {
            let mut args = vec!["check".into(), "--locked".into(), "--offline".into()];
            append_packages(&mut args, &reverse_packages);
            // Cargo's default `check` target set covers each selected package's
            // library and binaries. Passing both selectors fails when the
            // reverse closure contains a binary-only package.
            invocations.push(CargoInvocation {
                label: "affected reverse-dependency check".into(),
                args,
            });
        }

        for (package, test) in changed_integration_tests(root, &packages, &changed_files) {
            if HARNESSES.contains(&test.as_str()) || package == "erpx-android-bridge" {
                continue;
            }
            invocations.push(CargoInvocation {
                label: format!("affected integration test {package}/{test}"),
                args: vec![
                    "test".into(),
                    "-p".into(),
                    package,
                    "--test".into(),
                    test,
                    "--locked".into(),
                    "--offline".into(),
                    "--no-fail-fast".into(),
                ],
            });
        }

        if console_bff_changed {
            invocations.push(console_consumer_invocation("server"));
        } else if console_feature_changed {
            invocations.push(console_consumer_invocation("web"));
        } else if changed_files.iter().any(|path| is_web_source(path)) {
            invocations.extend(web_console_invocations());
        }
        architecture = changed_files.iter().any(|path| {
            path.ends_with(".rs") || path.ends_with("Cargo.toml") || path == "Cargo.lock"
        });
    }

    let harnesses = if web_assets_only {
        Vec::new()
    } else {
        select_harnesses(&changed_files).into_iter().collect()
    };
    Ok(AffectedPlan {
        harnesses,
        changed_files,
        rust_files,
        changed_packages: changed_packages.into_iter().collect(),
        invocations,
        architecture,
        web_assets_only,
        snapshot: current,
    })
}

pub(crate) fn build_check_plan(root: &Path) -> AffectedResult<AffectedCheckPlan> {
    let check_snapshot = read_snapshot_named(root, CHECK_SNAPSHOT_FILE)?;
    let test_snapshot = if check_snapshot.is_none() {
        read_snapshot(root)?
    } else {
        None
    };
    let previous = select_check_baseline(check_snapshot, test_snapshot);
    let current = capture_snapshot(root, previous.as_ref())?;
    let changed = match previous {
        Some(previous) => diff_snapshots(&previous, &current),
        None => git_worktree_changes(root)?,
    };
    let changed_files = changed.into_iter().collect::<Vec<_>>();
    if changed_files.is_empty() {
        return Ok(AffectedCheckPlan {
            changed_files,
            rust_files: Vec::new(),
            changed_packages: Vec::new(),
            invocations: Vec::new(),
            web_assets_only: false,
            global: false,
            snapshot: current,
        });
    }

    let rust_files = changed_files
        .iter()
        .filter(|path| path.ends_with(".rs") && root.join(path).is_file())
        .cloned()
        .collect::<Vec<_>>();
    let web_assets_only = changed_files.iter().all(|path| is_web_asset(path));
    let global = changed_files.iter().any(|path| is_global_change(path));
    let metadata = cargo_metadata(root)?;
    let packages = workspace_packages(root, &metadata)?;
    let changed_package_ids = packages_for_paths(&packages, &changed_files);
    let changed_packages = changed_package_ids
        .iter()
        .filter_map(|id| packages.iter().find(|package| &package.id == id))
        .map(|package| package.name.clone())
        .collect::<BTreeSet<_>>();
    let mut invocations = Vec::new();

    if global {
        invocations.push(cargo_invocation(
            "affected quick lane workspace escalation",
            [
                "check",
                "--workspace",
                "--exclude",
                "erpx-android-bridge",
                "--lib",
                "--bins",
                "--locked",
                "--offline",
            ],
        ));
    } else if !web_assets_only {
        let integration_tests = changed_integration_tests(root, &packages, &changed_files);
        for (package, test) in &integration_tests {
            if package == "erpx-android-bridge" {
                continue;
            }
            invocations.push(CargoInvocation {
                label: format!("affected integration check {package}/{test}"),
                args: vec![
                    "check".into(),
                    "-p".into(),
                    package.clone(),
                    "--test".into(),
                    test.clone(),
                    "--locked".into(),
                    "--offline".into(),
                ],
            });
        }

        for package_id in &changed_package_ids {
            let Some(package) = packages.iter().find(|package| &package.id == package_id) else {
                continue;
            };
            if package.name == "erpx-android-bridge" {
                continue;
            }
            let package_paths = changed_files
                .iter()
                .filter(|path| Path::new(path).starts_with(&package.root))
                .collect::<Vec<_>>();
            if package_paths.iter().all(|path| {
                let relative = Path::new(path)
                    .strip_prefix(&package.root)
                    .unwrap_or_else(|_| Path::new(path));
                relative.starts_with("tests") || is_web_asset(path)
            }) {
                continue;
            }

            if package.name == "erpx-console" {
                invocations.extend(console_check_invocations(&package_paths));
                continue;
            }
            if package.name == "xtask" {
                invocations.push(cargo_invocation(
                    "affected lightweight xtask check",
                    [
                        "check",
                        "-p",
                        "xtask",
                        "--no-default-features",
                        "--bin",
                        "xtask",
                        "--locked",
                        "--offline",
                    ],
                ));
                continue;
            }

            let manifest_changed = package_paths
                .iter()
                .any(|path| path.ends_with("Cargo.toml"));
            let package_metadata = metadata
                .packages
                .iter()
                .find(|candidate| candidate.id == package.id)
                .ok_or_else(|| format!("missing Cargo metadata for {}", package.name))?;
            if manifest_changed {
                invocations.push(package_check(&package.name, None));
                continue;
            }

            let has_library = package_metadata.targets.iter().any(|target| {
                target
                    .kind
                    .iter()
                    .any(|kind| kind == "lib" || kind == "proc-macro")
            });
            let changed_bins = changed_binary_targets(root, package_metadata, &package_paths)?;
            let library_changed = has_library
                && package_paths.iter().any(|path| {
                    let relative = Path::new(path)
                        .strip_prefix(&package.root)
                        .unwrap_or_else(|_| Path::new(path));
                    relative.starts_with("src")
                        && !changed_bins.iter().any(|(_, source)| source == *path)
                });
            if library_changed {
                invocations.push(package_check(&package.name, Some("--lib")));
            }
            for (binary, _) in changed_bins {
                invocations.push(CargoInvocation {
                    label: format!("affected package binary check {}/{}", package.name, binary),
                    args: vec![
                        "check".into(),
                        "-p".into(),
                        package.name.clone(),
                        "--bin".into(),
                        binary,
                        "--locked".into(),
                        "--offline".into(),
                    ],
                });
            }
            if !has_library
                && invocations.iter().all(|invocation| {
                    !invocation
                        .label
                        .starts_with(&format!("affected package binary check {}/", package.name))
                })
            {
                invocations.push(package_check(&package.name, None));
            }
        }
    }

    Ok(AffectedCheckPlan {
        changed_files,
        rust_files,
        changed_packages: changed_packages.into_iter().collect(),
        invocations,
        web_assets_only,
        global,
        snapshot: current,
    })
}

fn select_check_baseline(
    check_snapshot: Option<Snapshot>,
    test_snapshot: Option<Snapshot>,
) -> Option<Snapshot> {
    check_snapshot.or(test_snapshot)
}

fn console_check_invocations(paths: &[&String]) -> Vec<CargoInvocation> {
    let server_only = paths.iter().all(|path| is_console_server_only_source(path));
    let feature = if server_only { "server" } else { "web" };
    vec![CargoInvocation {
        label: format!("affected Console {feature} check"),
        args: [
            "check",
            "-p",
            "erpx-console",
            "--no-default-features",
            "--features",
            feature,
            "--locked",
            "--offline",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    }]
}

fn is_console_server_only_source(path: &str) -> bool {
    matches!(
        path,
        "apps/console/src/bff.rs" | "apps/console/src/server.rs"
    ) || path.starts_with("apps/console/src/bff/")
        || path.starts_with("apps/console/src/server/")
}

fn package_check(package: &str, target: Option<&str>) -> CargoInvocation {
    let mut args = vec!["check".into(), "-p".into(), package.into()];
    if let Some(target) = target {
        args.push(target.into());
    }
    args.extend(["--locked".into(), "--offline".into()]);
    CargoInvocation {
        label: format!("affected package check {package}"),
        args,
    }
}

fn changed_binary_targets(
    root: &Path,
    package: &MetadataPackage,
    paths: &[&String],
) -> AffectedResult<Vec<(String, String)>> {
    let root = root.canonicalize()?;
    let changed = paths
        .iter()
        .map(|path| root.join(path))
        .collect::<BTreeSet<_>>();
    Ok(package
        .targets
        .iter()
        .filter(|target| target.kind.iter().any(|kind| kind == "bin"))
        .filter_map(|target| {
            let relative = target.src_path.strip_prefix(&root).ok()?;
            let relative = relative.to_string_lossy().into_owned();
            changed
                .contains(&target.src_path)
                .then(|| (target.name.clone(), relative))
        })
        .collect())
}

pub(crate) fn commit_plan(root: &Path, plan: &AffectedPlan) -> AffectedResult {
    write_snapshot(root, &plan.snapshot)
}

pub(crate) fn commit_check_plan(root: &Path, plan: &AffectedCheckPlan) -> AffectedResult {
    write_snapshot_named(root, CHECK_SNAPSHOT_FILE, &plan.snapshot)
}

pub(crate) fn record_verified_snapshot(root: &Path) -> AffectedResult {
    let previous = read_snapshot(root)?;
    let snapshot = capture_snapshot(root, previous.as_ref())?;
    write_snapshot(root, &snapshot)?;
    write_snapshot_named(root, CHECK_SNAPSHOT_FILE, &snapshot)
}

pub(crate) fn validate_workspace(root: &Path) -> AffectedResult {
    let metadata = cargo_metadata_no_deps(root)?;
    let packages = workspace_packages(root, &metadata)?;
    let workspace_manifests = packages
        .iter()
        .map(|package| package.manifest.clone())
        .collect::<BTreeSet<_>>();
    let discovered = discover_manifests(root)?;
    if workspace_manifests != discovered {
        let missing = discovered
            .difference(&workspace_manifests)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        let unknown = workspace_manifests
            .difference(&discovered)
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        return Err(format!(
            "workspace manifest inventory mismatch; missing members: [{}]; unknown members: [{}]",
            missing.join(", "),
            unknown.join(", ")
        )
        .into());
    }
    for package in packages {
        let source = fs::read_to_string(&package.manifest)?;
        for (index, line) in source.lines().enumerate() {
            if line.trim_start().starts_with("erpx-") && line.contains("path =") {
                return Err(format!(
                    "internal dependency must inherit from workspace.dependencies at {}:{}",
                    package.manifest.display(),
                    index + 1
                )
                .into());
            }
        }
        for nested in [package.root.join("Cargo.lock"), package.root.join("target")] {
            if nested.exists() {
                return Err(format!(
                    "nested Cargo state is forbidden in the monorepo: {}",
                    nested.display()
                )
                .into());
            }
        }
    }
    Ok(())
}

fn cargo_invocation<const N: usize>(label: &str, args: [&str; N]) -> CargoInvocation {
    CargoInvocation {
        label: label.into(),
        args: args.into_iter().map(str::to_owned).collect(),
    }
}

fn append_packages(args: &mut Vec<String>, packages: &[String]) {
    for package in packages {
        args.push("-p".into());
        args.push(package.clone());
    }
}

fn web_console_invocations() -> Vec<CargoInvocation> {
    ["web", "server"]
        .into_iter()
        .map(console_consumer_invocation)
        .collect()
}

fn console_consumer_invocation(feature: &str) -> CargoInvocation {
    CargoInvocation {
        label: format!("affected Console {feature} check"),
        args: [
            "check",
            "-p",
            "erpx-console",
            "--no-default-features",
            "--features",
            feature,
            "--locked",
            "--offline",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    }
}

fn is_web_asset(path: &str) -> bool {
    (path.starts_with("apps/console/") || path.starts_with("crates/ui-kit/"))
        && [".css", ".html", ".svg", ".png", ".jpg", ".jpeg", ".webp"]
            .iter()
            .any(|extension| path.ends_with(extension))
}

fn is_web_source(path: &str) -> bool {
    path.starts_with("apps/console/")
        || path.starts_with("crates/ui/")
        || path.starts_with("crates/ui-kit/")
        || path.starts_with("crates/ui-domains/")
        || path.starts_with("crates/ui-facade/")
        || path.starts_with("crates/ui-foundation/")
        || path.starts_with("crates/console-features/")
        || path.starts_with("crates/console-shell/")
        || path.starts_with("crates/console-transport/")
        || path.starts_with("crates/console-bff/")
}

fn is_console_feature_source(path: &str) -> bool {
    path.starts_with("crates/console-features/")
}

fn is_console_bff_source(path: &str) -> bool {
    path.starts_with("crates/console-bff/")
}

fn is_global_change(path: &str) -> bool {
    matches!(
        path,
        ".cargo/config.toml"
            | ".github/workflows/erpx-targets.yml"
            | "Cargo.lock"
            | "Cargo.toml"
            | "rust-toolchain.toml"
    ) || [
        "crates/kernel/",
        "crates/cqrs/",
        "crates/integration-contracts/",
        "crates/migration/",
        "crates/proto/",
        "crates/ui-foundation/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
        || matches!(
            path,
            "crates/ui/src/lib.rs" | "crates/ui/src/routes.rs" | "crates/ui/src/workflows.rs"
        )
}

fn select_harnesses(paths: &[String]) -> BTreeSet<String> {
    let mut selected = BTreeSet::new();
    for path in paths {
        if let Some(stem) = Path::new(path).file_stem().and_then(|stem| stem.to_str())
            && HARNESSES.contains(&stem)
        {
            selected.insert(stem.to_owned());
        }
        let normalized = path.to_ascii_lowercase();
        if normalized.starts_with("crates/domain/") {
            selected.insert("domain_contracts".into());
        }
        if normalized.starts_with("crates/identity/") {
            selected.extend(["identity_core".into(), "identity_acceptance".into()]);
            if normalized.contains("directory") {
                selected.insert("worker_directory".into());
            }
        }
        if let Some(harness) = ui_harness_for_path(&normalized) {
            selected.insert(harness.into());
        }
        if contains_any(
            &normalized,
            &[
                "platform",
                "organization",
                "commercial",
                "brand",
                "identity-access",
            ],
        ) {
            selected.extend(["infra_platform".into(), "worker_platform".into()]);
        }
        if normalized.contains("finance") {
            selected.extend(["infra_finance".into(), "worker_finance_operations".into()]);
        }
        if contains_any(&normalized, &["marketing", "crm"]) {
            selected.insert("infra_marketing".into());
        }
        if normalized.contains("operations") {
            selected.extend([
                "infra_operations".into(),
                "worker_finance_operations".into(),
            ]);
        }
        if contains_any(
            &normalized,
            &["people", "payroll", "support", "legal", "/hr_"],
        ) {
            selected.insert("infra_people_support".into());
        }
        if contains_any(&normalized, &["backup", "pitr", "wal-archive"]) {
            selected.insert("infra_backup".into());
        }
        if contains_any(
            &normalized,
            &[
                "migration",
                "migrator",
                "bootstrap",
                "data-plane",
                "provisioning",
            ],
        ) {
            selected.extend([
                "infra_migration".into(),
                "migrator_cutover".into(),
                "migrator_products".into(),
            ]);
        }
        if normalized.starts_with("crates/contexts/") || normalized.starts_with("crates/adapters/")
        {
            selected.insert("infra_tenant_runtime".into());
        }
    }
    selected
}

fn ui_harness_for_path(path: &str) -> Option<&'static str> {
    if path.starts_with("crates/ui/src/kit/")
        || path.starts_with("crates/ui-kit/")
        || path.ends_with("/ui_kit.rs")
    {
        return Some("ui_kit");
    }
    if contains_any(
        path,
        &[
            "/crm/",
            "/customer-360/",
            "/customer_360/",
            "/finance/",
            "/people-payroll/",
            "/people_payroll/",
            "/legal/",
            "/pos/",
            "crates/ui/src/crm.rs",
            "crates/ui/src/customer360.rs",
            "crates/ui/src/finance.rs",
            "crates/ui/src/hr.rs",
            "crates/ui/src/legal.rs",
            "crates/ui/src/pos.rs",
            "crates/ui/src/pos_commands.rs",
            "/tests/crm.rs",
            "/tests/finance.rs",
            "/tests/hr.rs",
            "/tests/legal.rs",
            "/tests/pos_commands.rs",
        ],
    ) {
        return Some("ui_domains");
    }
    if is_web_source(path) {
        return Some("ui_platform");
    }
    None
}

fn contains_any(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.contains(candidate))
}

fn packages_for_paths(packages: &[WorkspacePackage], paths: &[String]) -> BTreeSet<String> {
    paths
        .iter()
        .filter_map(|path| {
            let path = Path::new(path);
            packages
                .iter()
                .filter(|package| path.starts_with(&package.root))
                .max_by_key(|package| package.root.components().count())
                .map(|package| package.id.clone())
        })
        .collect()
}

fn packages_with_target_kind(
    metadata: &Metadata,
    package_names: &[String],
    target_kind: &str,
) -> Vec<String> {
    package_names
        .iter()
        .filter(|name| {
            metadata.packages.iter().any(|package| {
                package.name.as_str() == name.as_str()
                    && package.targets.iter().any(|target| {
                        target.kind.iter().any(|kind| {
                            kind == target_kind || (target_kind == "lib" && kind == "proc-macro")
                        })
                    })
            })
        })
        .cloned()
        .collect()
}

fn changed_integration_tests(
    root: &Path,
    packages: &[WorkspacePackage],
    paths: &[String],
) -> BTreeSet<(String, String)> {
    paths
        .iter()
        .filter(|path| path.ends_with(".rs") && root.join(path).is_file())
        .filter_map(|path| {
            let path = Path::new(path);
            let package = packages
                .iter()
                .filter(|package| path.starts_with(&package.root))
                .max_by_key(|package| package.root.components().count())?;
            let relative = path.strip_prefix(&package.root).ok()?;
            let mut components = relative.components();
            if components.next()?.as_os_str() != "tests" || components.count() != 1 {
                return None;
            }
            Some((
                package.name.clone(),
                relative
                    .file_stem()?
                    .to_str()
                    .filter(|test| package.test_targets.contains(*test))?
                    .to_owned(),
            ))
        })
        .collect()
}

fn reverse_dependents(metadata: &Metadata, seeds: &BTreeSet<String>) -> BTreeSet<String> {
    let workspace = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut reverse = BTreeMap::<String, BTreeSet<String>>::new();
    if let Some(resolve) = &metadata.resolve {
        for node in &resolve.nodes {
            if !workspace.contains(&node.id) {
                continue;
            }
            for dependency in &node.deps {
                if workspace.contains(&dependency.pkg) {
                    reverse
                        .entry(dependency.pkg.clone())
                        .or_default()
                        .insert(node.id.clone());
                }
            }
        }
    }
    let mut found = seeds.clone();
    let mut pending = seeds.iter().cloned().collect::<VecDeque<_>>();
    while let Some(package) = pending.pop_front() {
        for dependent in reverse.get(&package).into_iter().flatten() {
            if found.insert(dependent.clone()) {
                pending.push_back(dependent.clone());
            }
        }
    }
    found
}

fn cargo_metadata(root: &Path) -> AffectedResult<Metadata> {
    cargo_metadata_with_args(root, &[])
}

fn cargo_metadata_no_deps(root: &Path) -> AffectedResult<Metadata> {
    cargo_metadata_with_args(root, &["--no-deps"])
}

fn cargo_metadata_with_args(root: &Path, extra: &[&str]) -> AffectedResult<Metadata> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--locked", "--offline"])
        .args(extra)
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn workspace_packages(root: &Path, metadata: &Metadata) -> AffectedResult<Vec<WorkspacePackage>> {
    let workspace = metadata
        .workspace_members
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    metadata
        .packages
        .iter()
        .filter(|package| workspace.contains(&package.id))
        .map(|package| {
            let manifest = package.manifest_path.canonicalize()?;
            let package_root = manifest.parent().ok_or("package manifest has no parent")?;
            Ok(WorkspacePackage {
                id: package.id.clone(),
                name: package.name.clone(),
                root: package_root
                    .strip_prefix(root.canonicalize()?)?
                    .to_path_buf(),
                manifest,
                test_targets: package
                    .targets
                    .iter()
                    .filter(|target| target.kind.iter().any(|kind| kind == "test"))
                    .map(|target| target.name.clone())
                    .collect(),
            })
        })
        .collect()
}

fn capture_snapshot(root: &Path, previous: Option<&Snapshot>) -> AffectedResult<Snapshot> {
    let mut files = BTreeMap::new();
    for relative in git_inventory(root)? {
        let path = root.join(&relative);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // `git ls-files --cached` includes tracked paths deleted by an
                // intentional cutover. Their absence is represented by the
                // previous snapshot key not appearing in the current map.
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            continue;
        }
        files.insert(relative.clone(), file_state(&path, &relative, previous)?);
    }
    Ok(Snapshot {
        version: SNAPSHOT_VERSION,
        files,
    })
}

fn git_inventory(root: &Path) -> AffectedResult<BTreeSet<String>> {
    let mut command = Command::new("git");
    command.args([
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
    ]);
    command.args(RELEVANT_ROOT_FILES);
    command.args(RELEVANT_DIRECTORIES);
    let output = command.current_dir(root).output()?;
    if !output.status.success() {
        return Err(format!(
            "git inventory failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let path = String::from_utf8(path.to_vec())?;
            if !is_relevant_path(&path) {
                return Err(format!("git returned an out-of-scope inventory path: {path}").into());
            }
            Ok(path)
        })
        .collect()
}

fn file_state(
    path: &Path,
    relative: &str,
    previous: Option<&Snapshot>,
) -> AffectedResult<FileState> {
    let metadata = fs::metadata(path)?;
    let modified_ns = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos()
        .try_into()
        .unwrap_or(u64::MAX);
    if let Some(state) = previous.and_then(|snapshot| snapshot.files.get(relative))
        && state.len == metadata.len()
        && state.modified_ns == modified_ns
    {
        return Ok(state.clone());
    }
    let bytes = fs::read(path)?;
    Ok(FileState {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        len: metadata.len(),
        modified_ns,
    })
}

fn diff_snapshots(previous: &Snapshot, current: &Snapshot) -> BTreeSet<String> {
    previous
        .files
        .keys()
        .chain(current.files.keys())
        .filter(
            |path| match (previous.files.get(*path), current.files.get(*path)) {
                (Some(previous), Some(current)) => {
                    previous.sha256 != current.sha256 || previous.len != current.len
                }
                (None, None) => false,
                _ => true,
            },
        )
        .cloned()
        .collect()
}

fn git_worktree_changes(root: &Path) -> AffectedResult<BTreeSet<String>> {
    let mut changed = BTreeSet::new();
    for args in [
        vec!["diff", "--name-only", "-z", "HEAD", "--"],
        vec!["ls-files", "--others", "--exclude-standard", "-z"],
    ] {
        let output = Command::new("git").args(args).current_dir(root).output()?;
        if !output.status.success() {
            return Err(format!(
                "git change discovery failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        for path in output.stdout.split(|byte| *byte == 0) {
            if path.is_empty() {
                continue;
            }
            let path = String::from_utf8(path.to_vec())?;
            if is_relevant_path(&path) {
                changed.insert(path);
            }
        }
    }
    Ok(changed)
}

fn is_relevant_path(path: &str) -> bool {
    RELEVANT_ROOT_FILES.contains(&path)
        || RELEVANT_DIRECTORIES
            .iter()
            .any(|directory| path.starts_with(&format!("{directory}/")))
}

fn read_snapshot(root: &Path) -> AffectedResult<Option<Snapshot>> {
    read_snapshot_named(root, SNAPSHOT_FILE)
}

fn read_snapshot_named(root: &Path, name: &str) -> AffectedResult<Option<Snapshot>> {
    let path = snapshot_path_named(root, name);
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("unsafe affected-test snapshot: {}", path.display()).into());
    }
    let snapshot: Snapshot = serde_json::from_slice(&fs::read(path)?)?;
    if snapshot.version != SNAPSHOT_VERSION {
        return Ok(None);
    }
    Ok(Some(snapshot))
}

fn write_snapshot(root: &Path, snapshot: &Snapshot) -> AffectedResult {
    write_snapshot_named(root, SNAPSHOT_FILE, snapshot)
}

fn write_snapshot_named(root: &Path, name: &str, snapshot: &Snapshot) -> AffectedResult {
    let path = snapshot_path_named(root, name);
    let parent = path
        .parent()
        .ok_or("affected-test snapshot has no parent")?;
    fs::create_dir_all(parent)?;
    if path.exists() && fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(format!(
            "refusing symlinked affected-test snapshot: {}",
            path.display()
        )
        .into());
    }
    let temporary = parent.join(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&temporary, serde_json::to_vec_pretty(snapshot)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn snapshot_path(root: &Path) -> PathBuf {
    snapshot_path_named(root, SNAPSHOT_FILE)
}

fn snapshot_path_named(root: &Path, name: &str) -> PathBuf {
    root.join(".runtime/erpx").join(name)
}

fn discover_manifests(root: &Path) -> AffectedResult<BTreeSet<PathBuf>> {
    let mut manifests = BTreeSet::new();
    for directory in ["apps", "crates", "xtask"] {
        let start = root.join(directory);
        let mut pending = vec![start];
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let metadata = fs::symlink_metadata(entry.path())?;
                if metadata.file_type().is_symlink() {
                    continue;
                }
                if metadata.is_dir() {
                    pending.push(entry.path());
                } else if entry.file_name() == "Cargo.toml" {
                    manifests.insert(entry.path().canonicalize()?);
                }
            }
        }
    }
    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(entries: &[(&str, &str)]) -> Snapshot {
        Snapshot {
            version: SNAPSHOT_VERSION,
            files: entries
                .iter()
                .map(|(path, digest)| {
                    (
                        (*path).into(),
                        FileState {
                            sha256: (*digest).into(),
                            len: 0,
                            modified_ns: 0,
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn snapshot_diff_detects_edits_additions_and_deletions() {
        let previous = snapshot(&[("apps/server/src/lib.rs", "a"), ("deleted.rs", "b")]);
        let current = snapshot(&[("apps/server/src/lib.rs", "c"), ("new.rs", "d")]);
        assert_eq!(
            diff_snapshots(&previous, &current),
            ["apps/server/src/lib.rs", "deleted.rs", "new.rs"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
    }

    #[test]
    fn snapshot_diff_ignores_timestamp_only_changes() {
        let previous = snapshot(&[("crates/ui/tests/ui_premium.rs", "same")]);
        let mut current = snapshot(&[("crates/ui/tests/ui_premium.rs", "same")]);
        current
            .files
            .get_mut("crates/ui/tests/ui_premium.rs")
            .expect("fixture file")
            .modified_ns = 42;

        assert!(diff_snapshots(&previous, &current).is_empty());
    }

    #[test]
    fn leaf_reverse_dependency_closure_is_transitive() {
        let metadata = Metadata {
            packages: Vec::new(),
            workspace_members: vec!["leaf".into(), "middle".into(), "app".into()],
            resolve: Some(MetadataResolve {
                nodes: vec![
                    MetadataNode {
                        id: "leaf".into(),
                        deps: Vec::new(),
                    },
                    MetadataNode {
                        id: "middle".into(),
                        deps: vec![MetadataDependency { pkg: "leaf".into() }],
                    },
                    MetadataNode {
                        id: "app".into(),
                        deps: vec![MetadataDependency {
                            pkg: "middle".into(),
                        }],
                    },
                ],
            }),
        };
        assert_eq!(
            reverse_dependents(&metadata, &["leaf".into()].into_iter().collect()),
            ["app", "leaf", "middle"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
    }

    #[test]
    fn package_target_selection_keeps_binary_only_packages_out_of_lib_tests() {
        let metadata = Metadata {
            packages: vec![
                MetadataPackage {
                    id: "library".into(),
                    name: "library".into(),
                    manifest_path: "library/Cargo.toml".into(),
                    targets: vec![MetadataTarget {
                        name: "library".into(),
                        kind: vec!["lib".into()],
                        src_path: "library/src/lib.rs".into(),
                    }],
                },
                MetadataPackage {
                    id: "binary".into(),
                    name: "binary".into(),
                    manifest_path: "binary/Cargo.toml".into(),
                    targets: vec![MetadataTarget {
                        name: "binary".into(),
                        kind: vec!["bin".into()],
                        src_path: "binary/src/main.rs".into(),
                    }],
                },
            ],
            workspace_members: vec!["library".into(), "binary".into()],
            resolve: None,
        };
        let packages = vec!["binary".into(), "library".into()];

        assert_eq!(
            packages_with_target_kind(&metadata, &packages, "lib"),
            vec!["library"]
        );
        assert_eq!(
            packages_with_target_kind(&metadata, &packages, "bin"),
            vec!["binary"]
        );
    }

    #[test]
    fn web_assets_do_not_select_rust_or_platform_targets() {
        assert!(is_web_asset("apps/console/assets/console-surfaces.css"));
        assert!(!is_global_change(
            "apps/console/assets/console-surfaces.css"
        ));
        assert!(
            ["apps/console/assets/console-surfaces.css"]
                .into_iter()
                .all(is_web_asset)
        );
    }

    #[test]
    fn quick_check_snapshot_prefers_its_lane_and_seeds_from_successful_tests() {
        let check = snapshot(&[("apps/console/src/lib.rs", "check")]);
        let tests = snapshot(&[("apps/console/src/lib.rs", "tests")]);
        assert_eq!(
            select_check_baseline(Some(check.clone()), Some(tests.clone())),
            Some(check)
        );
        assert_eq!(
            select_check_baseline(None, Some(tests.clone())),
            Some(tests)
        );
        assert_eq!(select_check_baseline(None, None), None);
    }

    #[test]
    fn console_quick_check_uses_one_feature_combination() {
        let web = "crates/console-features/finance/src/view.rs".to_owned();
        let server = "apps/console/src/bff.rs".to_owned();
        let web_plan = console_check_invocations(&[&web]);
        let server_plan = console_check_invocations(&[&server]);
        assert_eq!(web_plan.len(), 1);
        assert!(web_plan[0].args.contains(&"web".to_owned()));
        assert!(!web_plan[0].args.contains(&"server".to_owned()));
        assert_eq!(server_plan.len(), 1);
        assert!(server_plan[0].args.contains(&"server".to_owned()));
        assert!(!server_plan[0].args.contains(&"web".to_owned()));
    }

    #[test]
    fn shared_contracts_escalate_but_leaf_contexts_do_not() {
        assert!(is_global_change("crates/kernel/src/lib.rs"));
        assert!(is_global_change("crates/proto/proto/erpx.proto"));
        assert!(is_global_change(".github/workflows/erpx-targets.yml"));
        assert!(!is_global_change("crates/contexts/quality/src/lib.rs"));
    }

    #[test]
    fn changed_integration_test_maps_to_only_its_package_and_binary() {
        let root =
            std::env::temp_dir().join(format!("erpx-affected-integration-{}", std::process::id()));
        let test = root.join("crates/example/tests/example_acceptance.rs");
        fs::create_dir_all(test.parent().expect("test parent")).expect("create test directory");
        fs::write(&test, "#[test] fn acceptance() {}\n").expect("write test");
        fs::write(
            root.join("crates/example/tests/acceptance_support.rs"),
            "pub fn support() {}\n",
        )
        .expect("write nested test module");
        let packages = vec![WorkspacePackage {
            id: "example-id".into(),
            name: "erpx-example".into(),
            root: PathBuf::from("crates/example"),
            manifest: root.join("crates/example/Cargo.toml"),
            test_targets: ["example_acceptance".into()].into_iter().collect(),
        }];
        assert_eq!(
            changed_integration_tests(
                &root,
                &packages,
                &[
                    "crates/example/tests/example_acceptance.rs".into(),
                    "crates/example/tests/acceptance_support.rs".into(),
                ]
            ),
            [("erpx-example".into(), "example_acceptance".into())]
                .into_iter()
                .collect()
        );
        fs::remove_dir_all(root).expect("remove affected-test fixture");
    }

    #[test]
    fn domain_paths_select_only_relevant_harness_families() {
        let finance = select_harnesses(&["crates/contexts/finance/src/lib.rs".into()]);
        assert!(finance.contains("infra_finance"));
        assert!(finance.contains("infra_tenant_runtime"));
        assert!(!finance.contains("infra_marketing"));
        let directory = select_harnesses(&["crates/identity/src/directory/sync.rs".into()]);
        assert!(directory.contains("identity_core"));
        assert!(directory.contains("identity_acceptance"));
        assert!(directory.contains("worker_directory"));
    }

    #[test]
    fn ui_paths_select_only_the_owning_harness() {
        assert_eq!(
            ui_harness_for_path("crates/console-features/finance/src/view.rs"),
            Some("ui_domains")
        );
        assert_eq!(
            ui_harness_for_path("crates/console-features/support/src/presenter.rs"),
            Some("ui_platform")
        );
        assert_eq!(
            ui_harness_for_path("crates/ui-kit/src/kit/button.rs"),
            Some("ui_kit")
        );
    }

    #[test]
    fn quick_lane_classifies_assets_features_server_and_global_paths() {
        assert!(is_web_asset("apps/console/assets/console-surfaces.css"));
        assert!(is_console_feature_source(
            "crates/console-features/finance/src/view.rs"
        ));
        assert!(is_console_server_only_source("apps/console/src/server.rs"));
        assert!(is_console_bff_source("crates/console-bff/src/lib.rs"));
        assert!(is_global_change("Cargo.lock"));
        assert!(!is_global_change(
            "crates/console-features/support/src/presenter.rs"
        ));
    }

    #[test]
    fn failed_check_does_not_advance_its_independent_snapshot() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "erpx-affected-check-retention-{}-{nonce}",
            std::process::id()
        ));
        let baseline = snapshot(&[("apps/console/src/lib.rs", "passing")]);
        write_snapshot_named(&root, CHECK_SNAPSHOT_FILE, &baseline).expect("write baseline");

        // A failed runner never calls commit_check_plan; the durable baseline
        // must therefore remain the last successful quick check.
        assert_eq!(
            read_snapshot_named(&root, CHECK_SNAPSHOT_FILE).expect("read baseline"),
            Some(baseline)
        );
        fs::remove_dir_all(root).expect("remove check retention fixture");
    }

    #[test]
    fn snapshot_inventory_excludes_ignored_files() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "erpx-affected-inventory-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("apps/example/src")).expect("create fixture directory");
        fs::write(root.join(".gitignore"), ".DS_Store\n").expect("write ignore file");
        fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("write tracked file");
        fs::write(
            root.join("apps/example/src/lib.rs"),
            "pub fn tracked() {}\n",
        )
        .expect("write tracked source");
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&root)
            .status()
            .expect("initialize git fixture");
        assert!(status.success());
        let status = Command::new("git")
            .args(["add", "Cargo.toml", "apps/example/src/lib.rs"])
            .current_dir(&root)
            .status()
            .expect("add fixture files");
        assert!(status.success());
        fs::write(
            root.join("apps/example/src/untracked.rs"),
            "pub fn untracked() {}\n",
        )
        .expect("write untracked source");
        fs::write(root.join("apps/example/.DS_Store"), "ignored\n").expect("write ignored fixture");

        let inventory = git_inventory(&root).expect("capture git inventory");
        assert!(inventory.contains("Cargo.toml"));
        assert!(inventory.contains("apps/example/src/lib.rs"));
        assert!(inventory.contains("apps/example/src/untracked.rs"));
        assert!(!inventory.contains("apps/example/.DS_Store"));

        fs::remove_dir_all(root).expect("remove inventory fixture");
    }
}
