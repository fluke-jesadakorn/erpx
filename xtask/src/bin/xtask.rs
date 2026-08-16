use std::{
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant, SystemTime},
};

use fs2::FileExt;

#[path = "../affected.rs"]
mod affected;
#[path = "../architecture.rs"]
mod architecture;
#[path = "../cache.rs"]
mod cache;
#[path = "../wasm.rs"]
mod wasm;
#[path = "../web_toolchain.rs"]
mod web_toolchain;

type TaskResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const POSTGRES_DATA: &str = "/Users/fluke/erpx-postgres-data";
const POSTGRES_CTL: &str = "/opt/homebrew/opt/postgresql@18/bin/pg_ctl";
const WAL_SPOOL: &str = "/Users/fluke/erpx-wal-spool";
const MINIO_PITR_CHAIN: &str =
    "/Users/fluke/erpx-minio-data/erpx-platform-pitr/platform/erpx-local-pg18/chains/local-chain";
const PITR_LEASE_FILE: &str = "pitr/rehearsal-lease.json";

#[derive(Clone, Copy)]
struct ServiceRecord {
    name: &'static str,
    expected_command: &'static str,
}

const SERVICES: [ServiceRecord; 99] = [
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
        name: "agent-tenant-operations-worker",
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
        name: "quality-lifecycle-records-projector",
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
        name: "identity-access-governance-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "organization-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "organization-sharing-projector",
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
        name: "commercial-lifecycle-records-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "marketing-lead-candidates-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "marketing-campaign-budgets-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "marketing-business-records-projector",
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
        name: "crm-business-records-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "procurement-purchase-orders-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "procurement-business-records-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "inventory-stock-items-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "inventory-lifecycle-records-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "customer-authority-business-records-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "customer360-owned-records-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "support-platform-cases-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "people-payroll-time-records-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "legal-cases-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "shared-signatures-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "manufacturing-orders-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "planning-runs-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "ecommerce-listings-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "ecommerce-journeys-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "talent-candidate-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "talent-workforce-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "learning-enrollments-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "benefits-enrollments-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "field-service-orders-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "projects-projects-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "projects-hierarchy-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "collaboration-threads-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "collaboration-stream-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "collaboration-inbox-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "item-directory-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "assets-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "asset-lifecycle-records-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "maintenance-work-orders-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "maintenance-lifecycle-records-projector",
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
        name: "sales-revenue-arrangements-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "sales-lifecycle-records-projector",
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
        name: "pos-lifecycle-records-projector",
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
        name: "finance-customer-invoices-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "finance-revenue-schedules-projector",
        expected_command: "erpx-worker",
    },
    ServiceRecord {
        name: "finance-business-records-projector",
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
        name: "legal-matters-projector",
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
        name: "operations-lifecycle-records-projector",
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
        name: "marketing-approval-decisions-worker",
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

const HARNESSES: [Harness; 19] = [
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
        name: "ui_kit",
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
    CheckAffected {
        dry_run: bool,
    },
    TestAffected {
        dry_run: bool,
    },
    Architecture,
    CacheStatus,
    CachePrune {
        dry_run: bool,
        incremental: bool,
    },
    CachePruneDebug {
        dry_run: bool,
    },
    WasmSize {
        profile: wasm::WasmProfile,
    },
    WebToolchain,
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
        Route::CheckAffected { dry_run } => run_affected_check(&root, dry_run),
        Route::TestAffected { dry_run } => run_affected(&root, dry_run),
        Route::Architecture => architecture::scan(&root),
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
        Route::CachePruneDebug { dry_run } => {
            let _guard = CommandGuard::acquire(&root, "cargo xtask cache prune-debug")?;
            cache::prune_workspace_debug(&root, dry_run)
        }
        Route::CacheUsage => {
            Err("usage: cargo xtask cache status|prune [--dry-run] [--incremental]|prune-debug [--dry-run]".into())
        }
        Route::WasmSize { profile } => {
            wasm::compress_and_measure(&root, profile).map(|_| ())
        }
        Route::WebToolchain => web_toolchain::ensure(&root).map(|_| ()),
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
        [command] if command == "check-affected" => Route::CheckAffected { dry_run: false },
        [command, flag] if command == "check-affected" && flag == "--dry-run" => {
            Route::CheckAffected { dry_run: true }
        }
        [command] if command == "test-affected" => Route::TestAffected { dry_run: false },
        [command, flag] if command == "test-affected" && flag == "--dry-run" => {
            Route::TestAffected { dry_run: true }
        }
        [command] if command == "architecture" => Route::Architecture,
        [command] if command == "web-toolchain" => Route::WebToolchain,
        [command, flag] if command == "wasm-size" && flag == "--dev" => Route::WasmSize {
            profile: wasm::WasmProfile::Dev,
        },
        [command, flag] if command == "wasm-size" && flag == "--release" => Route::WasmSize {
            profile: wasm::WasmProfile::Release,
        },
        [command, subcommand] if command == "cache" && subcommand == "status" => Route::CacheStatus,
        [command, subcommand] if command == "cache" && subcommand == "prune-debug" => {
            Route::CachePruneDebug { dry_run: false }
        }
        [command, subcommand, flag]
            if command == "cache" && subcommand == "prune-debug" && flag == "--dry-run" =>
        {
            Route::CachePruneDebug { dry_run: true }
        }
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
         cargo xtask dev --detach [--fresh] [--no-hot-patch] [--web-debug]\n\
         cargo xtask status\n\
         cargo xtask stop\n\
         cargo xtask test <harness> [filter]\n\
         cargo xtask check-affected [--dry-run]\n\
         cargo xtask test-affected [--dry-run]\n\
         cargo check-fast\n\
         cargo test-fast\n\
         cargo xtask architecture\n\
         cargo xtask wasm-size --dev|--release\n\
         cargo xtask web-toolchain\n\
         cargo xtask cache status\n\
         cargo xtask cache prune [--dry-run] [--incremental]\n\
         cargo xtask cache prune-debug [--dry-run]\n\
         cargo xtask verify\n\
         cargo xtask verify-web-production\n\
         cargo xtask verify-platforms\n\n\
         integration harnesses:\n  {}",
        HARNESSES
            .iter()
            .map(|harness| harness.name)
            .collect::<Vec<_>>()
            .join("\n  ")
    );
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
        architecture::scan(root)?;
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

fn run_affected_check(root: &Path, dry_run: bool) -> TaskResult {
    let total_started = Instant::now();
    let selector_started = Instant::now();
    let plan = affected::build_check_plan(root)?;
    let selector_elapsed = selector_started.elapsed();
    plan.print();
    if dry_run {
        print_check_timings(
            selector_elapsed,
            Duration::ZERO,
            Duration::ZERO,
            total_started.elapsed(),
        );
        return Ok(());
    }

    let _guard = CommandGuard::acquire(root, "cargo xtask check-affected")?;
    let target = root.join("target/debug");
    cache_preflight(root, std::slice::from_ref(&target));
    let cache_started = SystemTime::now();

    let rustfmt_started = Instant::now();
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
    let rustfmt_elapsed = rustfmt_started.elapsed();

    let cargo_started = Instant::now();
    for invocation in &plan.invocations {
        println!("affected check lane: {}", invocation.label);
        let status = cargo_command(root).args(&invocation.args).status()?;
        require_success(status, &invocation.label)?;
    }
    let cargo_elapsed = cargo_started.elapsed();

    affected::commit_check_plan(root, &plan)?;
    cache_postflight(root, cache_started, &[target]);
    if plan.is_empty() {
        println!("affected-check baseline recorded");
    } else {
        println!("affected checks passed; check snapshot advanced");
    }
    print_check_timings(
        selector_elapsed,
        rustfmt_elapsed,
        cargo_elapsed,
        total_started.elapsed(),
    );
    Ok(())
}

fn print_check_timings(selector: Duration, rustfmt: Duration, cargo: Duration, total: Duration) {
    println!(
        "affected check timings: selector={:.2}s rustfmt={:.2}s cargo={:.2}s total={:.2}s",
        selector.as_secs_f64(),
        rustfmt.as_secs_f64(),
        cargo.as_secs_f64(),
        total.as_secs_f64()
    );
}

fn run_heavy(root: &Path, args: &[String]) -> TaskResult {
    let label = format!("cargo xtask {}", args.join(" "));
    let _guard = CommandGuard::acquire(root, &label)?;
    let workspace_target = root.join("target/debug");
    let heavy_target = runtime_dir(root).join("heavy-xtask-target");
    let pins = [workspace_target, heavy_target.clone()];
    cache_preflight(root, &pins);
    let started = SystemTime::now();
    let status = cargo_command(root)
        .args(["run", "--locked", "--offline", "--target-dir"])
        .arg(&heavy_target)
        .args([
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
    cache_postflight(root, started, &pins);
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
    let toolchain = web_toolchain::status(root)?;
    println!(
        "web-toolchain: {} (version={}, standard-contract=v{}, path={})",
        if toolchain.ready {
            "ready"
        } else {
            "missing-or-stale"
        },
        toolchain
            .dx_version
            .as_deref()
            .map(str::trim)
            .unwrap_or("missing"),
        toolchain.contract_version,
        toolchain.dx_path.display(),
    );
    for service in SERVICES {
        let pid = read_pid(&runtime.join(format!("{}.pid", service.name)))?;
        match pid {
            Some(pid) if recorded_process_matches(service, pid) => {
                if service.name == "web" {
                    let command = process_command(pid).unwrap_or_default();
                    let (mode, debug) = web_process_mode(&command);
                    println!("web: running (pid {pid}, monolithic, {mode}, {debug})");
                } else {
                    println!("{}: running (pid {pid})", service.name);
                }
            }
            Some(pid) => println!("{}: stopped (stale pid {pid})", service.name),
            None => println!("{}: stopped", service.name),
        }
    }
    for profile in [wasm::WasmProfile::Dev, wasm::WasmProfile::Release] {
        if let Some(report) = wasm::latest_report(root, profile)? {
            println!(
                "web-wasm-{}: monolithic={}, raw={}, Brotli={}",
                profile.label(),
                report.base_path,
                wasm::display_bytes(report.base_raw_bytes),
                wasm::display_bytes(report.base_brotli_bytes),
            );
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
    let (spool_segments, spool_bytes) = flat_private_file_summary(Path::new(WAL_SPOOL))?;
    let (minio_objects, minio_bytes) = minio_pitr_summary()?;
    let lease = runtime.join(PITR_LEASE_FILE);
    if lease.is_file() {
        let metadata = fs::symlink_metadata(&lease)?;
        if metadata.file_type().is_symlink() || metadata.len() > 4 * 1024 {
            return Err("unsafe local PITR rehearsal lease".into());
        }
        let value: serde_json::Value = serde_json::from_slice(&fs::read(lease)?)?;
        let expires_at = value
            .get("expires_at")
            .and_then(serde_json::Value::as_str)
            .ok_or("invalid local PITR rehearsal lease")?;
        println!(
            "local-pitr: rehearsal-active (expires {expires_at}, spool {spool_segments} segments/{}, MinIO {minio_objects} objects/{})",
            display_bytes(spool_bytes),
            display_bytes(minio_bytes),
        );
    } else {
        println!(
            "local-pitr: disabled (spool {spool_segments} segments/{}, MinIO {minio_objects} objects/{})",
            display_bytes(spool_bytes),
            display_bytes(minio_bytes),
        );
    }
    Ok(())
}

fn web_process_mode(command: &str) -> (&'static str, &'static str) {
    let mode = if command.contains("--hot-patch false") {
        "stable"
    } else if command.contains("--hot-patch") {
        "hot-patch"
    } else {
        "stable"
    };
    let debug = if command.contains("--debug-symbols true") {
        "debug-on"
    } else {
        "debug-off"
    };
    (mode, debug)
}

fn flat_private_file_summary(path: &Path) -> TaskResult<(u64, u64)> {
    validate_fixed_root(path)?;
    if !path.exists() {
        return Ok((0, 0));
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err("unsafe local PITR spool".into());
    }
    let mut count = 0_u64;
    let mut bytes = 0_u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err("unsafe local PITR spool entry".into());
        }
        count = count.saturating_add(1);
        bytes = bytes
            .checked_add(metadata.len())
            .ok_or("local PITR spool size overflow")?;
    }
    Ok((count, bytes))
}

fn tree_bytes(path: &Path) -> TaskResult<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err("local PITR MinIO data contains a symlink".into());
    }
    if metadata.file_type().is_file() {
        return Ok(metadata.len());
    }
    if !metadata.file_type().is_dir() {
        return Err("local PITR MinIO data contains an unsupported entry".into());
    }
    let mut bytes = 0_u64;
    for entry in fs::read_dir(path)? {
        bytes = bytes
            .checked_add(tree_bytes(&entry?.path())?)
            .ok_or("local PITR MinIO size overflow")?;
    }
    Ok(bytes)
}

fn minio_pitr_summary() -> TaskResult<(u64, u64)> {
    let chain = Path::new(MINIO_PITR_CHAIN);
    validate_fixed_root(chain)?;
    let mut objects = 0_u64;
    for kind in ["wal", "base"] {
        let path = chain.join(kind);
        if !path.exists() {
            continue;
        }
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                return Err("local PITR MinIO prefix contains a symlink".into());
            }
            objects = objects.saturating_add(1);
        }
    }
    Ok((objects, tree_bytes(chain)?))
}

fn display_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else {
        format!("{:.2} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
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
        assert_eq!(names.len(), 19);
        assert_eq!(names.len(), HARNESSES.len());
    }

    #[test]
    fn lightweight_service_catalog_covers_the_generation_two_runtime() {
        let names = SERVICES
            .iter()
            .map(|service| service.name)
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), SERVICES.len());
        for required in [
            "agent-tenant-operations-worker",
            "commercial-catalog-projector",
            "quality-projector",
            "identity-access-principals-projector",
            "maintenance-asset-facts-worker",
            "people-payroll-payroll-runs-projector",
            "projects-projects-projector",
            "projects-hierarchy-projector",
            "collaboration-threads-projector",
            "collaboration-stream-projector",
            "collaboration-inbox-projector",
            "item-directory-projector",
            "manufacturing-orders-projector",
            "planning-runs-projector",
            "ecommerce-listings-projector",
            "ecommerce-journeys-projector",
            "talent-candidate-projector",
            "talent-workforce-projector",
            "learning-enrollments-projector",
            "benefits-enrollments-projector",
            "field-service-orders-projector",
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
            route(&["check-affected".into(), "--dry-run".into()]),
            Route::CheckAffected { dry_run: true }
        );
        assert_eq!(
            route(&["test-affected".into(), "--dry-run".into()]),
            Route::TestAffected { dry_run: true }
        );
        assert_eq!(route(&["architecture".into()]), Route::Architecture);
        assert_eq!(route(&["web-toolchain".into()]), Route::WebToolchain);
        assert_eq!(
            route(&["wasm-size".into(), "--dev".into()]),
            Route::WasmSize {
                profile: wasm::WasmProfile::Dev
            }
        );
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
            route(&["cache".into(), "prune-debug".into(), "--dry-run".into()]),
            Route::CachePruneDebug { dry_run: true }
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

    #[test]
    fn web_status_reports_launch_mode() {
        assert_eq!(
            web_process_mode("dx serve --hot-patch --debug-symbols false"),
            ("hot-patch", "debug-off")
        );
        assert_eq!(
            web_process_mode("dx serve --hot-patch false --debug-symbols true"),
            ("stable", "debug-on")
        );
    }
}
