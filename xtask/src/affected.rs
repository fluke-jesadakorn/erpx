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

const SNAPSHOT_VERSION: u32 = 2;
const SNAPSHOT_FILE: &str = "affected-tests.json";
const RELEVANT_ROOT_FILES: [&str; 5] = [
    ".cargo/config.toml",
    ".github/workflows/erpx-targets.yml",
    "Cargo.lock",
    "Cargo.toml",
    "rust-toolchain.toml",
];
const RELEVANT_DIRECTORIES: [&str; 4] = ["apps", "crates", "fixtures", "xtask"];
const HARNESSES: [&str; 18] = [
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
        let regular_packages = changed_packages
            .iter()
            .filter(|name| !matches!(name.as_str(), "xtask" | "erpx-android-bridge"))
            .cloned()
            .collect::<Vec<_>>();
        if !regular_packages.is_empty() {
            let mut args = vec!["test".into(), "--locked".into(), "--offline".into()];
            append_packages(&mut args, &regular_packages);
            args.extend(["--lib".into(), "--bins".into(), "--no-fail-fast".into()]);
            invocations.push(CargoInvocation {
                label: "affected package tests".into(),
                args,
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
        let reverse_packages = reverse_ids
            .difference(&changed_package_ids)
            .filter_map(|id| packages.iter().find(|package| &package.id == id))
            .map(|package| package.name.clone())
            .filter(|name| !matches!(name.as_str(), "xtask" | "erpx-android-bridge"))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if !reverse_packages.is_empty() {
            let mut args = vec!["check".into(), "--locked".into(), "--offline".into()];
            append_packages(&mut args, &reverse_packages);
            args.extend(["--lib".into(), "--bins".into()]);
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

        if changed_files.iter().any(|path| is_web_source(path)) {
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

pub(crate) fn commit_plan(root: &Path, plan: &AffectedPlan) -> AffectedResult {
    write_snapshot(root, &plan.snapshot)
}

pub(crate) fn record_verified_snapshot(root: &Path) -> AffectedResult {
    let previous = read_snapshot(root)?;
    let snapshot = capture_snapshot(root, previous.as_ref())?;
    write_snapshot(root, &snapshot)
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
        .map(|feature| CargoInvocation {
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
        })
        .collect()
}

fn is_web_asset(path: &str) -> bool {
    path.starts_with("apps/console/")
        && [".css", ".html", ".svg", ".png", ".jpg", ".jpeg", ".webp"]
            .iter()
            .any(|extension| path.ends_with(extension))
}

fn is_web_source(path: &str) -> bool {
    path.starts_with("apps/console/")
        || path.starts_with("crates/ui/")
        || path.starts_with("crates/ui-foundation/")
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
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
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
        if is_web_source(path) {
            selected.extend(["ui_domains".into(), "ui_platform".into()]);
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
                relative.file_stem()?.to_str()?.to_owned(),
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
            })
        })
        .collect()
}

fn capture_snapshot(root: &Path, previous: Option<&Snapshot>) -> AffectedResult<Snapshot> {
    let mut files = BTreeMap::new();
    for relative in RELEVANT_ROOT_FILES {
        let path = root.join(relative);
        if path.is_file() {
            files.insert(relative.into(), file_state(&path, relative, previous)?);
        }
    }
    for directory in RELEVANT_DIRECTORIES {
        collect_files(root, &root.join(directory), previous, &mut files)?;
    }
    Ok(Snapshot {
        version: SNAPSHOT_VERSION,
        files,
    })
}

fn collect_files(
    root: &Path,
    directory: &Path,
    previous: Option<&Snapshot>,
    files: &mut BTreeMap<String, FileState>,
) -> AffectedResult {
    if !directory.exists() {
        return Ok(());
    }
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                let relative = entry.path().strip_prefix(root)?.to_path_buf();
                let relative_string = relative.to_string_lossy().into_owned();
                files.insert(
                    relative_string.clone(),
                    file_state(&entry.path(), &relative_string, previous)?,
                );
            }
        }
    }
    Ok(())
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
        .filter(|path| previous.files.get(*path) != current.files.get(*path))
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
    let path = snapshot_path(root);
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
    let path = snapshot_path(root);
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
    let temporary = parent.join(format!(".{SNAPSHOT_FILE}.{}.tmp", std::process::id()));
    fs::write(&temporary, serde_json::to_vec_pretty(snapshot)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn snapshot_path(root: &Path) -> PathBuf {
    root.join(".runtime/erpx").join(SNAPSHOT_FILE)
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
    fn web_assets_do_not_select_rust_or_platform_targets() {
        assert!(is_web_asset("apps/console/assets/main.css"));
        assert!(!is_global_change("apps/console/assets/main.css"));
        assert!(
            ["apps/console/assets/main.css"]
                .into_iter()
                .all(is_web_asset)
        );
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
        let packages = vec![WorkspacePackage {
            id: "example-id".into(),
            name: "erpx-example".into(),
            root: PathBuf::from("crates/example"),
            manifest: root.join("crates/example/Cargo.toml"),
        }];
        assert_eq!(
            changed_integration_tests(
                &root,
                &packages,
                &["crates/example/tests/example_acceptance.rs".into()]
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
}
