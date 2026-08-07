use std::{
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant, SystemTime},
};

use fs2::FileExt;

#[path = "../affected.rs"]
mod affected;
#[path = "../cache.rs"]
mod cache;

type TaskResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const POSTGRES_DATA: &str = "/Users/fluke/erpx-postgres-data";
const POSTGRES_CTL: &str = "/opt/homebrew/opt/postgresql@18/bin/pg_ctl";

#[derive(Clone, Copy)]
struct ServiceRecord {
    name: &'static str,
    expected_command: &'static str,
}

const SERVICES: [ServiceRecord; 57] = [
    ServiceRecord {
        name: "server",
        expected_command: "erpx-server",
    },
    ServiceRecord {
        name: "ucs-lab",
        expected_command: "erpx-ucs-lab",
    },
    ServiceRecord {
        name: "support-worker",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "commercial-catalog-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "wave5-control-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "quality-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "identity-access-principals-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "identity-access-roles-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "organization-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "commercial-subscriptions-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "commercial-entitlements-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "marketing-lead-candidates-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "crm-leads-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "crm-opportunities-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "procurement-purchase-orders-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "inventory-stock-items-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "assets-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "maintenance-work-orders-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "maintenance-asset-facts-worker",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "sales-quotes-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "sales-orders-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "people-payroll-employees-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "people-payroll-leave-requests-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "people-payroll-expense-claims-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "people-payroll-payroll-runs-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "pos-sales-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "pos-refunds-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "finance-charts-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "finance-periods-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "finance-journals-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "finance-subledger-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "finance-trial-balance-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "finance-bank-transactions-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "finance-statutory-tax-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "legal-contracts-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "legal-documents-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "legal-access-history-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "support-tickets-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "support-incidents-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "support-sessions-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "support-capabilities-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "operations-exceptions-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "operations-source-signals-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "operations-work-queue-policies-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "operations-source-ingestion-worker",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "customer360-control-customers-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "customer360-tenant-activity-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "branch-provisioning-worker",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "tenant-projection-status-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "marketing-public-abuse-cleanup-worker",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "marketing-delivery-worker",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "marketing-journey-integration-worker",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "directory-supervisor",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "tenant-backup-worker",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "backup-retention-worker",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "pitr-controller",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "web",
        expected_command: "dx serve",
    },
];

#[derive(Clone, Copy, Debug)]
struct Harness {
    name: &'static str,
    package: &'static str,
}

const HARNESSES: [Harness; 18] = [
    Harness {
        name: "infra_platform",
        package: "erpx-infrastructure",
    },
    Harness {
        name: "infra_tenant_runtime",
        package: "erpx-infrastructure",
    },
    Harness {
        name: "infra_finance",
        package: "erpx-infrastructure",
    },
    Harness {
        name: "infra_marketing",
        package: "erpx-infrastructure",
    },
    Harness {
        name: "infra_operations",
        package: "erpx-infrastructure",
    },
    Harness {
        name: "infra_people_support",
        package: "erpx-infrastructure",
    },
    Harness {
        name: "infra_backup",
        package: "erpx-infrastructure",
    },
    Harness {
        name: "infra_migration",
        package: "erpx-infrastructure",
    },
    Harness {
        name: "worker_platform",
        package: "erpx-worker",
    },
    Harness {
        name: "worker_finance_operations",
        package: "erpx-worker",
    },
    Harness {
        name: "worker_directory",
        package: "erpx-worker",
    },
    Harness {
        name: "migrator_products",
        package: "erpx-migrator",
    },
    Harness {
        name: "migrator_cutover",
        package: "erpx-migrator",
    },
    Harness {
        name: "identity_core",
        package: "erpx-identity",
    },
    Harness {
        name: "identity_acceptance",
        package: "erpx-identity",
    },
    Harness {
        name: "domain_contracts",
        package: "erpx-domain",
    },
    Harness {
        name: "ui_domains",
        package: "erpx-ui",
    },
    Harness {
        name: "ui_platform",
        package: "erpx-ui",
    },
];

struct CommandGuard {
    _file: File,
}

impl Drop for CommandGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self._file);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Route<'a> {
    Help,
    Status,
    Stop,
    CheckFast,
    TestFast,
    TestAffected {
        dry_run: bool,
    },
    Architecture,
    CacheStatus,
    CachePrune {
        dry_run: bool,
        incremental: bool,
    },
    CacheUsage,
    Harness {
        name: &'a str,
        filter: Option<&'a str>,
    },
    TestUsage,
    Heavy,
}

impl CommandGuard {
    fn acquire(root: &Path, command: &str) -> TaskResult<Self> {
        let runtime = root.join(".runtime/erpx");
        fs::create_dir_all(&runtime)?;
        let path = runtime.join("cargo-command.lock");
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)?;
        if let Err(error) = FileExt::try_lock_exclusive(&file) {
            let owner =
                fs::read_to_string(&path).unwrap_or_else(|_| "owner metadata unavailable".into());
            return Err(format!(
                "another ERPX Cargo command is active ({error}): {}",
                owner.trim()
            )
            .into());
        }
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(file, "pid={}", std::process::id())?;
        writeln!(file, "command={command}")?;
        file.flush()?;
        Ok(Self { _file: file })
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> TaskResult {
    let root = workspace_root()?;
    let args = env::args().skip(1).collect::<Vec<_>>();
    match route(&args) {
        Route::Help => {
            print_help();
            Ok(())
        }
        Route::Status => status(&root),
        Route::Stop => stop(&root),
        Route::CheckFast => guarded_cargo(
            &root,
            "cargo check-fast",
            &[
                "check",
                "--workspace",
                "--exclude",
                "erpx-android-bridge",
                "--lib",
                "--bins",
                "--locked",
                "--offline",
            ],
        ),
        Route::TestFast => guarded_cargo(
            &root,
            "cargo test-fast",
            &[
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
        ),
        Route::TestAffected { dry_run } => run_affected(&root, dry_run),
        Route::Architecture => architecture_scan(&root),
        Route::CacheStatus => cache::status(&root),
        Route::CachePrune {
            dry_run,
            incremental,
        } => {
            let _guard = CommandGuard::acquire(&root, "cargo xtask cache prune")?;
            if incremental {
                cache::prune_incremental(&root, dry_run)
            } else {
                cache::prune(&root, dry_run, &[])
            }
            .map(|_| ())
        }
        Route::CacheUsage => {
            Err("usage: cargo xtask cache status|prune [--dry-run] [--incremental]".into())
        }
        Route::Harness { name, filter } => run_harness(&root, name, filter),
        Route::TestUsage => Err("usage: cargo xtask test <harness> [filter]".into()),
        Route::Heavy => run_heavy(&root, &args),
    }
}

fn route(args: &[String]) -> Route<'_> {
    match args {
        [] => Route::Help,
        [arg] if arg == "help" || arg == "--help" => Route::Help,
        [command] if command == "status" => Route::Status,
        [command] if command == "stop" => Route::Stop,
        [command] if command == "check-fast" => Route::CheckFast,
        [command] if command == "test-fast" => Route::TestFast,
        [command] if command == "test-affected" => Route::TestAffected { dry_run: false },
        [command, flag] if command == "test-affected" && flag == "--dry-run" => {
            Route::TestAffected { dry_run: true }
        }
        [command] if command == "architecture" => Route::Architecture,
        [command, subcommand] if command == "cache" && subcommand == "status" => Route::CacheStatus,
        [command, subcommand] if command == "cache" && subcommand == "prune" => Route::CachePrune {
            dry_run: false,
            incremental: false,
        },
        [command, subcommand, flag]
            if command == "cache" && subcommand == "prune" && flag == "--dry-run" =>
        {
            Route::CachePrune {
                dry_run: true,
                incremental: false,
            }
        }
        [command, subcommand, flag]
            if command == "cache" && subcommand == "prune" && flag == "--incremental" =>
        {
            Route::CachePrune {
                dry_run: false,
                incremental: true,
            }
        }
        [command, subcommand, first, second]
            if command == "cache"
                && subcommand == "prune"
                && ((first == "--dry-run" && second == "--incremental")
                    || (first == "--incremental" && second == "--dry-run")) =>
        {
            Route::CachePrune {
                dry_run: true,
                incremental: true,
            }
        }
        [command, ..] if command == "cache" => Route::CacheUsage,
        [command, harness] if command == "test" => Route::Harness {
            name: harness,
            filter: None,
        },
        [command, harness, filter] if command == "test" => Route::Harness {
            name: harness,
            filter: Some(filter),
        },
        [command, ..] if command == "test" => Route::TestUsage,
        _ => Route::Heavy,
    }
}

fn workspace_root() -> TaskResult<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask has no workspace parent".into())
}

fn print_help() {
    println!(
        "ERPX development tasks\n\n\
         cargo xtask dev --detach [--fresh]\n\
         cargo xtask status\n\
         cargo xtask stop\n\
         cargo xtask test <harness> [filter]\n\
         cargo xtask test-affected [--dry-run]\n\
         cargo check-fast\n\
         cargo test-fast\n\
         cargo xtask architecture\n\
         cargo xtask cache status\n\
         cargo xtask cache prune [--dry-run] [--incremental]\n\
         cargo xtask verify\n\
         cargo xtask verify-platforms\n\n\
         integration harnesses:\n  {}",
        HARNESSES
            .iter()
            .map(|harness| harness.name)
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

fn architecture_scan(root: &Path) -> TaskResult {
    affected::validate_workspace(root)?;
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

    let server_manifest = root.join("apps/server/Cargo.toml");
    if server_manifest.is_file() {
        reject_manifest_dependencies(&server_manifest, &["erpx-worker"])?;
    }
    let server_sources = root.join("apps/server/src");
    if server_sources.is_dir() {
        reject_rust_imports(&server_sources, &["erpx_worker::"])?;
    }

    println!("CQRS/DDD architecture boundaries passed");
    Ok(())
}

fn reject_manifest_dependencies(path: &Path, forbidden: &[&str]) -> TaskResult {
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

fn reject_context_dependency_edges(path: &Path) -> TaskResult {
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

fn reject_rust_imports(path: &Path, forbidden: &[&str]) -> TaskResult {
    let files = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        rust_files(path)?
    };
    for file in files {
        let source = fs::read_to_string(&file)?;
        for (index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("#") {
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

fn rust_files(path: &Path) -> TaskResult<Vec<PathBuf>> {
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

fn guarded_cargo(root: &Path, label: &str, args: &[&str]) -> TaskResult {
    let _guard = CommandGuard::acquire(root, label)?;
    let target = root.join("target/debug");
    cache_preflight(root, std::slice::from_ref(&target));
    let started = SystemTime::now();
    let status = cargo_command(root).args(args).status()?;
    cache_postflight(root, started, &[target]);
    require_success(status, label)
}

fn harness_named(name: &str) -> TaskResult<&'static Harness> {
    HARNESSES
        .iter()
        .find(|harness| harness.name == name)
        .ok_or_else(|| {
            format!(
                "unknown integration harness {name}; expected one of: {}",
                HARNESSES
                    .iter()
                    .map(|harness| harness.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
            .into()
        })
}

fn run_harness(root: &Path, name: &str, filter: Option<&str>) -> TaskResult {
    let label = format!("cargo xtask test {name}");
    let _guard = CommandGuard::acquire(root, &label)?;
    let target = root.join("target/debug");
    cache_preflight(root, std::slice::from_ref(&target));
    let started = SystemTime::now();
    let result = execute_harness(root, name, filter);
    cache_postflight(root, started, &[target]);
    result
}

fn execute_harness(root: &Path, name: &str, filter: Option<&str>) -> TaskResult {
    let harness = harness_named(name)?;
    let label = format!("cargo xtask test {name}");
    let mut command = cargo_command(root);
    command.args([
        "test",
        "--locked",
        "--offline",
        "-p",
        harness.package,
        "--test",
        name,
    ]);
    if let Some(filter) = filter {
        command.arg(filter);
    }
    require_success(command.status()?, &label)
}

fn run_affected(root: &Path, dry_run: bool) -> TaskResult {
    let plan = affected::build_plan(root)?;
    plan.print();
    if dry_run {
        return Ok(());
    }
    let _guard = CommandGuard::acquire(root, "cargo xtask test-affected")?;
    let target = root.join("target/debug");
    cache_preflight(root, std::slice::from_ref(&target));
    let started = SystemTime::now();
    if !plan.rust_files.is_empty() {
        let status = Command::new("rustfmt")
            .args([
                "--edition",
                "2024",
                "--check",
                "--config",
                "skip_children=true",
            ])
            .args(&plan.rust_files)
            .current_dir(root)
            .status()?;
        require_success(status, "affected Rust formatting")?;
    }
    for invocation in &plan.invocations {
        println!("affected lane: {}", invocation.label);
        let status = cargo_command(root).args(&invocation.args).status()?;
        require_success(status, &invocation.label)?;
    }
    for harness in &plan.harnesses {
        execute_harness(root, harness, None)?;
    }
    if plan.architecture {
        architecture_scan(root)?;
    }
    affected::commit_plan(root, &plan)?;
    cache_postflight(root, started, &[target]);
    if plan.is_empty() {
        println!("affected-test baseline recorded");
    } else {
        println!("affected tests passed; snapshot advanced");
    }
    Ok(())
}

fn run_heavy(root: &Path, args: &[String]) -> TaskResult {
    let label = format!("cargo xtask {}", args.join(" "));
    let _guard = CommandGuard::acquire(root, &label)?;
    let target = root.join("target/debug");
    cache_preflight(root, std::slice::from_ref(&target));
    let started = SystemTime::now();
    let status = cargo_command(root)
        .args([
            "run",
            "--locked",
            "--offline",
            "-p",
            "xtask",
            "--no-default-features",
            "--features",
            "heavy",
            "--bin",
            "xtask-heavy",
            "--",
        ])
        .args(args)
        .status()?;
    cache_postflight(root, started, &[target]);
    require_success(status, &label)
}

fn cache_preflight(root: &Path, pins: &[PathBuf]) {
    if let Err(error) = cache::mark_used(root, pins) {
        eprintln!("warning: could not update Cargo cache access time: {error}");
    }
    cache::auto_prune(root, pins);
}

fn cache_postflight(root: &Path, started: SystemTime, pins: &[PathBuf]) {
    if let Err(error) = cache::mark_modified_since(root, started) {
        eprintln!("warning: could not update Cargo cache access time: {error}");
    }
    if let Err(error) = cache::mark_used(root, pins) {
        eprintln!("warning: could not update Cargo cache access time: {error}");
    }
    cache::auto_prune(root, pins);
}

fn cargo_command(root: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(root);
    // Cargo sets package-scoped variables for the outer `cargo run` target.
    // They describe xtask, not packages in the nested workspace invocation.
    // Build scripts can opt into these variables with rerun-if-env-changed;
    // inheriting even one of them makes fingerprints alternate with direct
    // Cargo runs and can rebuild the dependency graph on every lane switch.
    // Preserve user configuration such as CARGO_HOME and CARGO_TARGET_DIR.
    for key in env::vars_os().map(|(key, _)| key).filter(|key| {
        let key = key.to_string_lossy();
        key.starts_with("CARGO_PKG_")
            || matches!(
                key.as_ref(),
                "CARGO_BIN_NAME"
                    | "CARGO_CRATE_NAME"
                    | "CARGO_MANIFEST_DIR"
                    | "CARGO_MANIFEST_PATH"
                    | "CARGO_PRIMARY_PACKAGE"
                    | "CARGO_SBOM_PATH"
            )
    }) {
        command.env_remove(key);
    }
    command
}

fn require_success(status: ExitStatus, label: &str) -> TaskResult {
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed with {status}").into())
    }
}

fn runtime_dir(root: &Path) -> PathBuf {
    root.join(".runtime/erpx")
}

fn status(root: &Path) -> TaskResult {
    let runtime = runtime_dir(root);
    for service in SERVICES {
        let pid = read_pid(&runtime.join(format!("{}.pid", service.name)))?;
        match pid {
            Some(pid) if recorded_process_matches(service, pid) => {
                println!("{}: running (pid {pid})", service.name);
            }
            Some(pid) => println!("{}: stopped (stale pid {pid})", service.name),
            None => println!("{}: stopped", service.name),
        }
    }
    println!(
        "infra-postgres: {} (127.0.0.1:55432, {POSTGRES_DATA})",
        if postgres_running() {
            "running"
        } else {
            "stopped"
        }
    );
    println!(
        "infra-minio: {} (https://127.0.0.1:9000, console 9001, /Users/fluke/erpx-minio-data)",
        if minio_running(root) {
            "running"
        } else {
            "stopped"
        }
    );
    println!(
        "infra-logical-topology: {} (one PostgreSQL cluster, one MinIO instance, one UCS process)",
        if runtime.join("infra/local-topology.json").is_file()
            && runtime.join("ucs-lab/authorities.json").is_file()
        {
            "ready"
        } else {
            "unconfigured"
        }
    );
    Ok(())
}

fn stop(root: &Path) -> TaskResult {
    let runtime = runtime_dir(root);
    let mut stopping = Vec::new();
    for service in SERVICES.into_iter().rev() {
        let pid_path = runtime.join(format!("{}.pid", service.name));
        if let Some(pid) = read_pid(&pid_path)?
            && recorded_process_matches(service, pid)
        {
            send_signal(pid, "-TERM")?;
            stopping.push((service.name, pid));
        }
        remove_pid_file(&pid_path)?;
    }
    wait_for_stopped(&stopping, Duration::from_secs(10))?;
    stop_infrastructure(root)
}

fn stop_infrastructure(root: &Path) -> TaskResult {
    validate_fixed_root(Path::new(POSTGRES_DATA))?;
    let minio_pid_path = runtime_dir(root).join("infra/minio.pid");
    if let Some(pid) = read_pid(&minio_pid_path)?
        && process_command(pid).is_some_and(|command| command.contains("minio server"))
    {
        send_signal(pid, "-TERM")?;
        wait_for_stopped(&[("infra-minio", pid)], Duration::from_secs(10))?;
    }
    remove_pid_file(&minio_pid_path)?;
    if postgres_running() {
        require_success(
            Command::new(POSTGRES_CTL)
                .args(["-D", POSTGRES_DATA, "-w", "-t", "60", "-m", "fast", "stop"])
                .status()?,
            "dedicated PostgreSQL stop",
        )?;
    }
    Ok(())
}

fn read_pid(path: &Path) -> TaskResult<Option<u32>> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(None);
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!("unsafe PID record: {}", path.display()).into());
    }
    Ok(fs::read_to_string(path)?.trim().parse().ok())
}

fn remove_pid_file(path: &Path) -> TaskResult {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn recorded_process_matches(service: ServiceRecord, pid: u32) -> bool {
    process_command(pid).is_some_and(|command| process_command_matches(service, &command))
}

fn process_command_matches(service: ServiceRecord, command: &str) -> bool {
    command.contains(service.expected_command)
}

fn process_command(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn send_signal(pid: u32, signal: &str) -> TaskResult {
    let status = Command::new("kill")
        .args([signal, &pid.to_string()])
        .status()?;
    if status.success() || !process_exists(pid) {
        Ok(())
    } else {
        Err(format!("failed to signal ERPX process {pid}").into())
    }
}

fn wait_for_stopped(processes: &[(&str, u32)], timeout: Duration) -> TaskResult {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if processes.iter().all(|(_, pid)| !process_exists(*pid)) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let alive = processes
        .iter()
        .filter(|(_, pid)| process_exists(*pid))
        .map(|(name, pid)| format!("{name} ({pid})"))
        .collect::<Vec<_>>();
    if alive.is_empty() {
        Ok(())
    } else {
        Err(format!("timed out stopping ERPX processes: {}", alive.join(", ")).into())
    }
}

fn process_exists(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn postgres_running() -> bool {
    let Ok(contents) = fs::read_to_string(Path::new(POSTGRES_DATA).join("postmaster.pid")) else {
        return false;
    };
    let Some(pid) = contents.lines().next().and_then(|value| value.parse().ok()) else {
        return false;
    };
    process_command(pid)
        .is_some_and(|command| command.contains("postgres") && command.contains(POSTGRES_DATA))
}

fn minio_running(root: &Path) -> bool {
    read_pid(&runtime_dir(root).join("infra/minio.pid"))
        .ok()
        .flatten()
        .and_then(process_command)
        .is_some_and(|command| command.contains("minio server"))
}

fn validate_fixed_root(path: &Path) -> TaskResult {
    if !path.is_absolute() || path.parent().is_none() || path == Path::new("/") {
        return Err(format!("unsafe ERPX infrastructure root: {}", path.display()).into());
    }
    if path.exists() && path.is_symlink() {
        return Err(format!("symlinked ERPX infrastructure root: {}", path.display()).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, time::SystemTime};

    use super::*;

    fn temporary_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        env::temp_dir().join(format!("erpx-xtask-{name}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn harness_map_is_complete_and_unique() {
        let names = HARNESSES
            .iter()
            .map(|harness| harness.name)
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), 18);
        assert_eq!(names.len(), HARNESSES.len());
    }

    #[test]
    fn lightweight_service_catalog_covers_the_direct_v1_runtime() {
        let names = SERVICES
            .iter()
            .map(|service| service.name)
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), SERVICES.len());
        for required in [
            "commercial-catalog-projector",
            "quality-projector",
            "identity-access-principals-projector",
            "maintenance-asset-facts-worker",
            "people-payroll-payroll-runs-projector",
            "tenant-projection-status-projector",
        ] {
            assert!(names.contains(required));
        }
    }

    #[test]
    fn unknown_harness_is_rejected_before_cargo_dispatch() {
        let error = harness_named("not_a_harness").expect_err("unknown harness must fail");
        assert!(error.to_string().contains("unknown integration harness"));
        assert!(error.to_string().contains("domain_contracts"));
    }

    #[test]
    fn lightweight_and_heavy_commands_route_without_ambiguity() {
        assert_eq!(route(&["status".into()]), Route::Status);
        assert_eq!(route(&["test-fast".into()]), Route::TestFast);
        assert_eq!(
            route(&["test-affected".into(), "--dry-run".into()]),
            Route::TestAffected { dry_run: true }
        );
        assert_eq!(route(&["architecture".into()]), Route::Architecture);
        assert_eq!(
            route(&["cache".into(), "status".into()]),
            Route::CacheStatus
        );
        assert_eq!(
            route(&["cache".into(), "prune".into(), "--dry-run".into()]),
            Route::CachePrune {
                dry_run: true,
                incremental: false
            }
        );
        assert_eq!(
            route(&[
                "cache".into(),
                "prune".into(),
                "--incremental".into(),
                "--dry-run".into()
            ]),
            Route::CachePrune {
                dry_run: true,
                incremental: true
            }
        );
        assert_eq!(route(&["cache".into()]), Route::CacheUsage);
        assert_eq!(
            route(&["test".into(), "domain_contracts".into(), "posting".into()]),
            Route::Harness {
                name: "domain_contracts",
                filter: Some("posting")
            }
        );
        assert_eq!(route(&["test".into()]), Route::TestUsage);
        assert_eq!(route(&["dev".into(), "--detach".into()]), Route::Heavy);
    }

    #[test]
    fn default_verifier_and_ci_do_not_compile_deferred_platforms() {
        let heavy = include_str!("../main.rs");
        let default_verify = heavy
            .split_once("async fn verify(root: &Path)")
            .expect("default verifier")
            .1
            .split_once("fn verify_platforms(root: &Path)")
            .expect("manual platform verifier")
            .0;
        let workflow = include_str!("../../../.github/workflows/erpx-targets.yml");
        for forbidden in [
            "\"desktop\"",
            "\"mobile\"",
            "aarch64-apple-ios",
            "aarch64-linux-android",
        ] {
            assert!(
                !default_verify.contains(forbidden),
                "default verify contains {forbidden}"
            );
            assert!(!workflow.contains(forbidden), "CI contains {forbidden}");
        }
    }

    #[test]
    fn command_guard_rejects_contention_and_recovers_after_drop() {
        let root = temporary_root("lock");
        let first = CommandGuard::acquire(&root, "first").expect("first lock");
        assert!(CommandGuard::acquire(&root, "second").is_err());
        drop(first);
        let recovered = CommandGuard::acquire(&root, "recovered").expect("recovered lock");
        drop(recovered);
        fs::remove_dir_all(root).expect("remove temporary lock root");
    }

    #[test]
    fn command_guard_recovers_stale_owner_metadata() {
        let root = temporary_root("stale-lock");
        let runtime = root.join(".runtime/erpx");
        fs::create_dir_all(&runtime).expect("create runtime directory");
        let path = runtime.join("cargo-command.lock");
        fs::write(&path, "pid=999999\ncommand=stale\n").expect("write stale owner");

        let guard = CommandGuard::acquire(&root, "replacement").expect("recover stale lock");
        let owner = fs::read_to_string(&path).expect("read recovered owner");
        assert!(owner.contains(&format!("pid={}", std::process::id())));
        assert!(owner.contains("command=replacement"));

        drop(guard);
        fs::remove_dir_all(root).expect("remove temporary lock root");
    }

    #[test]
    fn process_matching_uses_the_recorded_service_contract() {
        let service = ServiceRecord {
            name: "server",
            expected_command: "erpx-server",
        };
        assert!(process_command_matches(
            service,
            "/workspace/target/debug/erpx-server"
        ));
        assert!(!process_command_matches(
            service,
            "/workspace/target/debug/other-server"
        ));
        let names = SERVICES
            .iter()
            .map(|service| service.name)
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), SERVICES.len());
        assert!(
            SERVICES
                .iter()
                .filter(|service| service.name.ends_with("-projector"))
                .all(|service| process_command_matches(
                    *service,
                    "/workspace/target/debug/erpx-worker run"
                ))
        );
    }
}
