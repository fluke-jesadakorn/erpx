use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::IpAddr,
    os::unix::{
        fs::{OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use erpx_application::{RoleLifecycleService, SystemClock};
use erpx_domain::{
    access::{RiskTier, RoleAssignment, Scope},
    tenant_administration::CustomerTenantBinding,
};
use erpx_identity::{SqlxIdentityStore, apply_identity_migration};
use erpx_infrastructure::{
    ControlPlanePools, SqlxControlPlane, SqlxTenantAdministrationStore, SqlxTenantStoreRegistry,
    TenantStoreRegistration, tenant_holding_scope_id,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, Executor, PgConnection, PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;

mod affected;
mod dev_access;
mod directory_lab;
mod infra;

const RESET_CONFIRMATION: &str = "RESET_ERPX_DEV";
const ROOT_PASSWORD_RESET_CONFIRMATION: &str = "RESET_ERPX_PLATFORM_ROOT";
const LEGACY_DATABASE: &str = "folio_db";
const LEGACY_BUCKET: &str = "folio-storage";
const ERPX_BUCKET: &str = "erpx-demo";
const WORKER_ID: &str = "1db95d2e-6aa6-5de7-81b2-e650a66be777";
const TENANT_ID: &str = "demo-tenant";
const HOLDING_REF: &str = "demo-holding";
const RESTRICTED_TENANT_ID: &str = "demo-tenant-restricted";
const RESTRICTED_HOLDING_REF: &str = "beacon-holding";
const FIXTURE_VERSION: &str = "erpx-demo-v4";
const DEV_STATE_VERSION: u32 = 1;
const DEV_STATE_FILE: &str = "dev-state.json";
const FRESH_RUNTIME_MANIFEST_FILE: &str = "cutover/fresh-state.json";
const DEV_WORKER_POLL_INTERVAL_MS: &str = "5000";
const DEVELOPMENT_TAILSCALE_IP_ENV: &str = "ERPX_DEV_TAILSCALE_IP";
const DATABASES: [&str; 6] = [
    "erpx_control_write",
    "erpx_control_read",
    "erpx_tenant_write",
    "erpx_tenant_read",
    "erpx_restricted_tenant_write",
    "erpx_restricted_tenant_read",
];
const LEGACY_SCHEMAS: [&str; 11] = [
    "ai",
    "auth",
    "chat",
    "finance",
    "folio",
    "hook",
    "hr",
    "inventory",
    "law",
    "perm",
    "platform",
];
const PRESERVED_SCHEMAS: [&str; 2] = ["n8n", "public"];
const DEMO_CONTEXTS: [(&str, &str, &str, i64); 22] = [
    ("platform", "Platform control plane", "ready", 1),
    ("organization", "Holding, company, and branch", "ready", 3),
    (
        "identity",
        "Native identities and expiring roles",
        "ready",
        4,
    ),
    ("customers", "Customer 360", "ready", 6),
    ("support", "Support and incidents", "ready", 5),
    ("billing", "Billing and entitlements", "ready", 4),
    ("operations", "Operations", "ready", 8),
    ("finance", "Finance and accounting", "ready", 12),
    ("sales", "Sales", "ready", 7),
    ("scm", "Supply chain", "ready", 6),
    ("marketing", "Marketing and CRM", "ready", 5),
    ("pos", "Point of sale", "ready", 8),
    ("assets", "Assets", "ready", 5),
    ("maintenance", "Maintenance", "ready", 4),
    ("hr", "HR and payroll", "ready", 6),
    ("legal", "Legal and documents", "ready", 5),
    ("quality", "Quality", "ready", 4),
    ("directory", "LDAP, AD DS, and UCS", "disabled", 0),
    ("payments_provider", "Payment provider", "unconfigured", 0),
    ("banking_provider", "Banking provider", "unconfigured", 0),
    (
        "marketing_provider",
        "Marketing provider",
        "unconfigured",
        0,
    ),
    ("object_storage", "ERPX object namespace", "ready", 1),
];
const DEMO_IDENTITIES: [(&str, &str, i64); 4] = [
    ("local:platform-root", "platform_root", 89),
    ("local:demo-operations", "operations_demo", 45),
    ("local:demo-finance", "finance_demo", 30),
    ("local:demo-sales", "sales_demo", 60),
];
const PLATFORM_PERMISSIONS: &[&str] = &[
    "commercial.entitlement.activate",
    "commercial.entitlement.allocate",
    "commercial.billing.read",
    "diagnostics:read",
    "support.read",
    "operations.read",
    "finance.read",
    "sales.read",
    "procurement.read",
    "inventory.read",
    "assets.read",
    "maintenance.read",
    "quality.read",
    "marketing-crm.read",
    "hr-payroll.read",
    "point-of-sale.read",
    "legal.read",
    "platform.customer.access_audit.read",
    "platform.customer.read",
    "platform.brand.lock",
    "platform.brand.manage",
    "platform.brand.publish",
    "platform.brand.read",
    "platform.directory.manage",
    "platform.directory.read",
    "platform.outbox.replay",
    "platform.projection.rebuild",
    "platform.tenant.backup",
    "platform.tenant.provision",
    "platform.tenant.read",
    "platform.tenant.recovery",
    "platform.tenant.suspend",
    "tenant.projection.execute",
    "support.incident.create",
    "support.incident.open",
    "support.incident.read",
    "support.incident.timeline",
    "support.incident.transition",
    "support.incident.broadcast",
    "support.incident.update",
    "support.capability.issue",
    "support.capability.revoke",
    "support.session.approve",
    "support.session.request",
    "support.session.revoke",
    "support.ticket.note",
    "support.ticket.open",
    "support.ticket.reply",
    "support.ticket.transition",
];

type TaskResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

const STANDARD_WORKERS: [&str; 50] = [
    "support-worker",
    "commercial-catalog-projector",
    "wave5-control-projector",
    "quality-projector",
    "identity-access-principals-projector",
    "identity-access-roles-projector",
    "organization-projector",
    "commercial-subscriptions-projector",
    "commercial-entitlements-projector",
    "marketing-lead-candidates-projector",
    "crm-leads-projector",
    "crm-opportunities-projector",
    "procurement-purchase-orders-projector",
    "inventory-stock-items-projector",
    "assets-projector",
    "maintenance-work-orders-projector",
    "maintenance-asset-facts-worker",
    "sales-quotes-projector",
    "sales-orders-projector",
    "people-payroll-employees-projector",
    "people-payroll-leave-requests-projector",
    "people-payroll-expense-claims-projector",
    "people-payroll-payroll-runs-projector",
    "pos-sales-projector",
    "pos-refunds-projector",
    "finance-charts-projector",
    "finance-periods-projector",
    "finance-journals-projector",
    "finance-subledger-projector",
    "finance-trial-balance-projector",
    "finance-bank-transactions-projector",
    "finance-statutory-tax-projector",
    "legal-contracts-projector",
    "legal-documents-projector",
    "legal-access-history-projector",
    "support-tickets-projector",
    "support-incidents-projector",
    "support-sessions-projector",
    "support-capabilities-projector",
    "operations-exceptions-projector",
    "operations-source-signals-projector",
    "operations-work-queue-policies-projector",
    "operations-source-ingestion-worker",
    "customer360-control-customers-projector",
    "customer360-tenant-activity-projector",
    "branch-provisioning-worker",
    "tenant-projection-status-projector",
    "marketing-public-abuse-cleanup-worker",
    "marketing-delivery-worker",
    "marketing-journey-integration-worker",
];
const DEDICATED_WORKERS: [&str; 54] = [
    "support-worker",
    "commercial-catalog-projector",
    "wave5-control-projector",
    "quality-projector",
    "identity-access-principals-projector",
    "identity-access-roles-projector",
    "organization-projector",
    "commercial-subscriptions-projector",
    "commercial-entitlements-projector",
    "marketing-lead-candidates-projector",
    "crm-leads-projector",
    "crm-opportunities-projector",
    "procurement-purchase-orders-projector",
    "inventory-stock-items-projector",
    "assets-projector",
    "maintenance-work-orders-projector",
    "maintenance-asset-facts-worker",
    "sales-quotes-projector",
    "sales-orders-projector",
    "people-payroll-employees-projector",
    "people-payroll-leave-requests-projector",
    "people-payroll-expense-claims-projector",
    "people-payroll-payroll-runs-projector",
    "pos-sales-projector",
    "pos-refunds-projector",
    "finance-charts-projector",
    "finance-periods-projector",
    "finance-journals-projector",
    "finance-subledger-projector",
    "finance-trial-balance-projector",
    "finance-bank-transactions-projector",
    "finance-statutory-tax-projector",
    "legal-contracts-projector",
    "legal-documents-projector",
    "legal-access-history-projector",
    "support-tickets-projector",
    "support-incidents-projector",
    "support-sessions-projector",
    "support-capabilities-projector",
    "operations-exceptions-projector",
    "operations-source-signals-projector",
    "operations-work-queue-policies-projector",
    "operations-source-ingestion-worker",
    "customer360-control-customers-projector",
    "customer360-tenant-activity-projector",
    "branch-provisioning-worker",
    "tenant-projection-status-projector",
    "marketing-public-abuse-cleanup-worker",
    "marketing-delivery-worker",
    "marketing-journey-integration-worker",
    "directory-supervisor",
    "tenant-backup-worker",
    "backup-retention-worker",
    "pitr-controller",
];
const UCS_LABS: [&str; 1] = ["ucs-lab"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DevMode {
    Warm,
    Fresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkerSpec {
    name: &'static str,
    handler: &'static str,
    worker_id: &'static str,
    projection_kind: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DevState {
    version: u32,
    server_binary_sha256: String,
    worker_binary_sha256: String,
    ucs_binary_sha256: String,
    server_environment_sha256: String,
    worker_environment_sha256: String,
    ucs_environment_sha256: String,
    web_environment_sha256: String,
    development_access_enabled: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RuntimeSnapshot {
    server: bool,
    all_workers: bool,
    any_workers: bool,
    all_ucs_labs: bool,
    any_ucs_labs: bool,
    web: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DevRestartPlan {
    server: bool,
    workers: bool,
    ucs_labs: bool,
    stop_disabled_ucs_labs: bool,
    web: bool,
}

#[derive(Debug, Clone)]
struct DevConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    legacy_database: String,
    minio_endpoint: String,
    minio_root: Option<PathBuf>,
    postgres_ca_certificate_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct FreshRuntimeManifest {
    version: u32,
    state: String,
    prepared_at: String,
    installation_generation: Uuid,
    installation_prefix: String,
    tenant_id: String,
    holding_ref: String,
    control_write_database: String,
    control_read_database: String,
    tenant_write_database: String,
    tenant_read_database: String,
    object_namespace: String,
    v1_service_authority: String,
    n8n_config_sha256: String,
    rollback: serde_json::Value,
}

impl FreshRuntimeManifest {
    fn load(root: &Path) -> TaskResult<Option<Self>> {
        let path = runtime_dir(root).join(FRESH_RUNTIME_MANIFEST_FILE);
        if !path.exists() {
            return Ok(None);
        }
        for component in [
            root.join(".runtime"),
            runtime_dir(root),
            runtime_dir(root).join("cutover"),
        ] {
            let metadata = fs::symlink_metadata(&component)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("fresh-state runtime manifest path is unsafe".into());
            }
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 128 * 1024 {
            return Err("fresh-state runtime manifest is not a bounded regular file".into());
        }
        let manifest: Self = serde_json::from_slice(&fs::read(&path)?)
            .map_err(|_| "fresh-state runtime manifest is invalid")?;
        manifest.validate()?;
        Ok(Some(manifest))
    }

    fn validate(&self) -> TaskResult {
        let valid_prefix = (2..=16).contains(&self.installation_prefix.len())
            && self.installation_prefix.as_bytes()[0].is_ascii_lowercase()
            && self
                .installation_prefix
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        let databases = self.database_names();
        let database_names_valid = databases.iter().all(|database| {
            (1..=63).contains(&database.len())
                && database.as_bytes()[0].is_ascii_lowercase()
                && database
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        });
        let unique_databases = databases.into_iter().collect::<BTreeSet<_>>().len() == 4;
        let tenant_id = Uuid::parse_str(&self.tenant_id)
            .ok()
            .filter(|value| !value.is_nil());
        let holding_ref = Uuid::parse_str(&self.holding_ref)
            .ok()
            .filter(|value| !value.is_nil());
        let rollback_valid = self.rollback.get("retained_resource_authority")
            == Some(&serde_json::Value::String(
                "offline_non_authoritative".into(),
            ))
            && self.rollback.get("write_policy")
                == Some(&serde_json::Value::String("isolated_no_merge".into()))
            && self.rollback.get("n8n_config_sha256")
                == Some(&serde_json::Value::String(self.n8n_config_sha256.clone()));
        let expected_object_namespace = format!(
            "{}-tenant-objects",
            self.installation_prefix.replace('_', "-")
        );
        let tenant_write_token = self
            .tenant_write_database
            .strip_prefix(&format!("{}_", self.installation_prefix))
            .and_then(|value| value.strip_suffix("_tenant_write"));
        let tenant_read_token = self
            .tenant_read_database
            .strip_prefix(&format!("{}_", self.installation_prefix))
            .and_then(|value| value.strip_suffix("_tenant_read"));
        let tenant_token_valid = tenant_write_token == tenant_read_token
            && tenant_write_token.is_some_and(|token| {
                token.len() == 12 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
        let names_match_prefix = self.control_write_database
            == format!("{}_control_write", self.installation_prefix)
            && self.control_read_database == format!("{}_control_read", self.installation_prefix)
            && tenant_token_valid;
        let n8n_hash_valid = self.n8n_config_sha256.len() == 64
            && self
                .n8n_config_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
        if self.version != 1
            || self.state != "prepared"
            || self.v1_service_authority != "typed"
            || self.installation_generation.is_nil()
            || !valid_prefix
            || !database_names_valid
            || !unique_databases
            || tenant_id.is_none()
            || holding_ref.is_none()
            || chrono::DateTime::parse_from_rfc3339(&self.prepared_at).is_err()
            || self.object_namespace != expected_object_namespace
            || !names_match_prefix
            || !n8n_hash_valid
            || !rollback_valid
        {
            return Err("fresh-state runtime manifest failed closed validation".into());
        }
        Ok(())
    }

    fn database_names(&self) -> [&str; 4] {
        [
            &self.control_write_database,
            &self.control_read_database,
            &self.tenant_write_database,
            &self.tenant_read_database,
        ]
    }

    async fn require_complete_receipt(&self, root: &Path, config: &DevConfig) -> TaskResult {
        let n8n_path = root.join(".runtime/n8n/.n8n/config");
        for component in [
            root.join(".runtime"),
            root.join(".runtime/n8n"),
            root.join(".runtime/n8n/.n8n"),
        ] {
            let metadata = fs::symlink_metadata(component)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("fresh-state runtime refused an unsafe n8n path".into());
            }
        }
        let metadata = fs::symlink_metadata(&n8n_path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > 1024 * 1024
            || required_file_sha256(&n8n_path)? != self.n8n_config_sha256
        {
            return Err("fresh-state runtime refused a changed n8n configuration".into());
        }
        let pool = pool(&config.database_url(&self.control_write_database)).await?;
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                SELECT 1
                FROM erpx_control.platform_bootstrap_receipts receipt
                JOIN erpx_control.tenant_registry registry
                  ON registry.tenant_id=receipt.tenant_id
                WHERE receipt.installation_prefix=$1
                  AND receipt.tenant_id=$2
                  AND receipt.holding_ref=$3
                  AND receipt.state='complete'
                  AND registry.holding_ref=$3
                  AND registry.write_database_key=$4
                  AND registry.read_database_key=$5
                  AND registry.storage_namespace=$6
                  AND registry.state='active'
            )",
        )
        .bind(&self.installation_prefix)
        .bind(&self.tenant_id)
        .bind(&self.holding_ref)
        .bind(&self.tenant_write_database)
        .bind(&self.tenant_read_database)
        .bind(&self.object_namespace)
        .fetch_one(&pool)
        .await?;
        pool.close().await;
        if !valid {
            return Err(
                "fresh-state runtime requires a matching complete receipt and active tenant".into(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DevelopmentAccessRuntimeTarget {
    control_write_database: String,
    control_read_database: String,
    controller_target_tenant: String,
    fresh_state: bool,
}

fn development_access_target_from_manifest(
    manifest: Option<&FreshRuntimeManifest>,
) -> DevelopmentAccessRuntimeTarget {
    manifest.map_or_else(
        || DevelopmentAccessRuntimeTarget {
            control_write_database: DATABASES[0].into(),
            control_read_database: DATABASES[1].into(),
            controller_target_tenant: erpx_identity::DEVELOPMENT_CONTROLLER_TARGET_TENANT.into(),
            fresh_state: false,
        },
        |manifest| DevelopmentAccessRuntimeTarget {
            control_write_database: manifest.control_write_database.clone(),
            control_read_database: manifest.control_read_database.clone(),
            controller_target_tenant: manifest.tenant_id.clone(),
            fresh_state: true,
        },
    )
}

async fn development_access_runtime_target(
    root: &Path,
    config: &DevConfig,
) -> TaskResult<DevelopmentAccessRuntimeTarget> {
    let manifest = FreshRuntimeManifest::load(root)?;
    if let Some(manifest) = manifest.as_ref() {
        manifest.require_complete_receipt(root, config).await?;
    }
    Ok(development_access_target_from_manifest(manifest.as_ref()))
}

async fn reconcile_development_access_for_runtime(
    root: &Path,
    config: &DevConfig,
    target: &DevelopmentAccessRuntimeTarget,
) -> TaskResult {
    if target.fresh_state {
        dev_access::reconcile_runtime_catalog(
            root,
            config,
            &target.control_write_database,
            &target.control_read_database,
            &target.controller_target_tenant,
        )
        .await
    } else {
        dev_access::reconcile(root, config).await
    }
}

impl DevConfig {
    fn load(root: &Path) -> TaskResult<Self> {
        let mut values = read_dotenv(&root.join(".env.local"))?;
        for key in [
            "POSTGRES_HOST",
            "POSTGRES_PORT",
            "POSTGRES_USER",
            "POSTGRES_PASSWORD",
            "POSTGRES_DB",
            "ERPX_DEV_LEGACY_MINIO_ENDPOINT",
            "ERPX_DEV_LEGACY_MINIO_ROOT",
            "ERPX_POSTGRES_CA_CERTIFICATE_PATH",
        ] {
            if let Ok(value) = env::var(key) {
                values.insert(key.to_owned(), value);
            }
        }
        let host = value_or(&values, "POSTGRES_HOST", "127.0.0.1");
        let port = value_or(&values, "POSTGRES_PORT", "5432")
            .parse()
            .map_err(|_| "POSTGRES_PORT must be a TCP port")?;
        let user = required_value(&values, "POSTGRES_USER")?;
        let password = required_value(&values, "POSTGRES_PASSWORD")?;
        let legacy_database = value_or(&values, "POSTGRES_DB", LEGACY_DATABASE);
        let minio_endpoint = value_or(
            &values,
            "ERPX_DEV_LEGACY_MINIO_ENDPOINT",
            "http://127.0.0.1:9000",
        );
        let minio_root = values
            .get("ERPX_DEV_LEGACY_MINIO_ROOT")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        let postgres_ca_certificate_path = values
            .get("ERPX_POSTGRES_CA_CERTIFICATE_PATH")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        Ok(Self {
            host,
            port,
            user,
            password,
            legacy_database,
            minio_endpoint,
            minio_root,
            postgres_ca_certificate_path,
        })
    }

    fn validate_local(&self) -> TaskResult {
        if !is_loopback_host(&self.host) {
            return Err("reset-dev refuses a non-loopback PostgreSQL host".into());
        }
        let minio_host = endpoint_host(&self.minio_endpoint)
            .ok_or("reset-dev could not parse the MinIO endpoint")?;
        if !is_loopback_host(minio_host) {
            return Err("reset-dev refuses a non-loopback MinIO endpoint".into());
        }
        if self.legacy_database != LEGACY_DATABASE
            && !(self.port == 55_432 && self.legacy_database == DATABASES[0])
        {
            return Err("reset-dev refuses an unexpected legacy database".into());
        }
        Ok(())
    }

    fn database_url(&self, database: &str) -> String {
        let tls = self.postgres_ca_certificate_path.as_ref().map_or_else(
            || "sslmode=disable".to_owned(),
            |path| format!("sslmode=verify-full&sslrootcert={}", path.display()),
        );
        format!(
            "postgres://{}:{}@{}:{}/{}?{tls}",
            percent_encode(&self.user),
            percent_encode(&self.password),
            self.host,
            self.port,
            database
        )
    }

    fn database_template(&self) -> String {
        let tls = self.postgres_ca_certificate_path.as_ref().map_or_else(
            || "sslmode=disable".to_owned(),
            |path| format!("sslmode=verify-full&sslrootcert={}", path.display()),
        );
        format!(
            "postgres://{}:{}@{}:{}/{{database}}?{tls}",
            percent_encode(&self.user),
            percent_encode(&self.password),
            self.host,
            self.port
        )
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> TaskResult {
    let root = workspace_root()?;
    let args = env::args().skip(1).collect::<Vec<_>>();
    if let Some(mode) = parse_dev_mode(&args) {
        let config = DevConfig::load(&root)?;
        return dev(&root, &config, mode).await;
    }
    match args.as_slice() {
        [command, subcommand] if command == "infra" && subcommand == "init" => infra::init(&root),
        [command, subcommand] if command == "directory-lab" && subcommand == "init" => {
            directory_lab::init(&root)
        }
        [command, subcommand] if command == "directory-lab" && subcommand == "start" => {
            directory_lab::start(&root)
        }
        [command, subcommand] if command == "directory-lab" && subcommand == "status" => {
            directory_lab::status(&root)
        }
        [command, subcommand] if command == "directory-lab" && subcommand == "configure" => {
            let config = DevConfig::load(&root)?;
            directory_lab::configure(&root, &config.database_url(DATABASES[0])).await
        }
        [command, subcommand] if command == "directory-lab" && subcommand == "verify-deny-wins" => {
            let config = DevConfig::load(&root)?;
            directory_lab::verify_deny_wins(&root, &config.database_url(DATABASES[0])).await
        }
        [command, subcommand] if command == "directory-lab" && subcommand == "stop" => {
            directory_lab::stop(&root)
        }
        [command, subcommand, action] if command == "directory-lab" && subcommand == "ucs-real" => {
            directory_lab::ucs_real(action)
        }
        [command, subcommand] if command == "dev-access" && subcommand == "init" => {
            require_development_access_requested(&root)?;
            let config = DevConfig::load(&root)?;
            config.validate_local()?;
            let target = development_access_runtime_target(&root, &config).await?;
            if !target.fresh_state {
                initialize_databases(&config, false).await?;
            }
            dev_access::init(&root, &config)?;
            reconcile_development_access_for_runtime(&root, &config, &target).await
        }
        [command, subcommand] if command == "dev-access" && subcommand == "reconcile" => {
            require_development_access_requested(&root)?;
            let config = DevConfig::load(&root)?;
            config.validate_local()?;
            let target = development_access_runtime_target(&root, &config).await?;
            reconcile_development_access_for_runtime(&root, &config, &target).await
        }
        [command, subcommand] if command == "dev-access" && subcommand == "status" => {
            require_development_access_requested(&root)?;
            let config = DevConfig::load(&root)?;
            config.validate_local()?;
            let target = development_access_runtime_target(&root, &config).await?;
            dev_access::status(&root, &config, &target.control_write_database).await
        }
        [command, subcommand] if command == "dev-access" && subcommand == "credentials" => {
            require_development_access_requested(&root)?;
            dev_access::credentials(&root, None)
        }
        [command, subcommand, tenant_flag, tenant]
            if command == "dev-access"
                && subcommand == "credentials"
                && tenant_flag == "--tenant" =>
        {
            require_development_access_requested(&root)?;
            dev_access::credentials(&root, Some(tenant))
        }
        [command, subcommand, confirm_flag, confirmation]
            if command == "dev-access" && subcommand == "rotate" && confirm_flag == "--confirm" =>
        {
            require_development_access_requested(&root)?;
            let config = DevConfig::load(&root)?;
            config.validate_local()?;
            let target = development_access_runtime_target(&root, &config).await?;
            dev_access::rotate_controller(
                &root,
                &config,
                &target.control_write_database,
                confirmation,
            )
            .await
        }
        [command, subcommand, confirm_flag, confirmation]
            if command == "dev-access" && subcommand == "reset" && confirm_flag == "--confirm" =>
        {
            require_development_access_requested(&root)?;
            let config = DevConfig::load(&root)?;
            config.validate_local()?;
            let target = development_access_runtime_target(&root, &config).await?;
            dev_access::reset(&root, &config, &target.control_write_database, confirmation).await
        }
        [command, subcommand] if command == "infra" && subcommand == "start" => {
            infra::start(&root).await
        }
        [command, subcommand] if command == "infra" && subcommand == "status" => {
            infra::status(&root)
        }
        [command, subcommand] if command == "infra" && subcommand == "stop" => infra::stop(&root),
        [command, subcommand] if command == "infra" && subcommand == "activate-cutover" => {
            infra::activate_cutover_config(&root)
        }
        [command] if command == "migrate-all-erpx-databases" => {
            let source = DevConfig::load(&root)?;
            source.validate_local()?;
            stop(&root)?;
            infra::migrate_all_erpx_databases(
                &root,
                &source.host,
                source.port,
                &source.user,
                &source.password,
            )
            .await
        }
        [command, subcommand] if command == "pitr" && subcommand == "rehearsal" => {
            infra::pitr_rehearsal(&root).await
        }
        [command, subcommand] if command == "pitr" && subcommand == "controller" => {
            infra::pitr_controller(&root).await
        }
        [command, subcommand, confirm_flag, confirmation]
            if command == "pitr" && subcommand == "promote" && confirm_flag == "--confirm" =>
        {
            infra::pitr_promote(&root, confirmation).await
        }
        [command] if command == "status" => status(&root),
        [command] if command == "stop" => stop(&root),
        [command] if command == "seed-demo" => {
            let config = DevConfig::load(&root)?;
            initialize_databases(&config, false).await?;
            seed_demo(&root, &config).await?;
            if development_access_requested(&root)? {
                dev_access::reconcile(&root, &config).await?;
            }
            Ok(())
        }
        [command, confirm_flag, confirmation]
            if command == "reset-platform-root" && confirm_flag == "--confirm" =>
        {
            let config = DevConfig::load(&root)?;
            reset_platform_root_password(&config, confirmation).await
        }
        [command, confirm_flag, confirmation]
            if command == "reset-dev" && confirm_flag == "--confirm" =>
        {
            let config = DevConfig::load(&root)?;
            reset_dev(&root, &config, confirmation).await
        }
        [command] if command == "verify" => verify(&root).await,
        [command] if command == "verify-platforms" => verify_platforms(&root),
        _ => {
            println!(
                "ERPX development tasks\n\n\
                 cargo xtask dev --detach\n\
                 cargo xtask dev --detach --fresh\n\
                 cargo xtask status\n\
                 cargo xtask stop\n\
                 cargo xtask infra init|start|status|stop\n\
                 cargo xtask directory-lab init|start|configure|verify-deny-wins|status|stop\n\
                 cargo xtask directory-lab ucs-real check|seed|verify\n\
                 cargo xtask dev-access init|reconcile|status|credentials [--tenant TENANT]\n\
                 cargo xtask dev-access rotate|reset --confirm ...\n\
                 cargo xtask migrate-all-erpx-databases\n\
                 cargo xtask pitr rehearsal\n\
                 cargo xtask pitr promote --confirm PROMOTE_ERPX_PITR\n\
                 cargo xtask reset-dev --confirm {RESET_CONFIRMATION}\n\
                 cargo xtask reset-platform-root --confirm {ROOT_PASSWORD_RESET_CONFIRMATION}\n\
                 cargo xtask seed-demo\n\
                 cargo xtask verify
                 cargo xtask verify-platforms"
            );
            Ok(())
        }
    }
}

fn parse_dev_mode(args: &[String]) -> Option<DevMode> {
    match args {
        [command, flag] if command == "dev" && flag == "--detach" => Some(DevMode::Warm),
        [command, flag, fresh] if command == "dev" && flag == "--detach" && fresh == "--fresh" => {
            Some(DevMode::Fresh)
        }
        _ => None,
    }
}

fn workspace_root() -> TaskResult<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask has no workspace parent".into())
}

async fn initialize_databases(config: &DevConfig, recreate: bool) -> TaskResult {
    config.validate_local()?;
    let admin_url = config.database_url("postgres");
    initialize_databases_with_admin(config, recreate, &admin_url, None).await
}

async fn initialize_databases_with_admin(
    config: &DevConfig,
    recreate: bool,
    admin_url: &str,
    owner_role: Option<&str>,
) -> TaskResult {
    let mut admin = PgConnection::connect(admin_url).await?;
    for database in DATABASES {
        if recreate {
            sqlx::query(
                "SELECT pg_terminate_backend(pid)
                 FROM pg_stat_activity
                 WHERE datname = $1 AND pid <> pg_backend_pid()",
            )
            .bind(database)
            .execute(&mut admin)
            .await?;
            admin
                .execute(format!("DROP DATABASE IF EXISTS \"{database}\"").as_str())
                .await?;
        }
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)")
                .bind(database)
                .fetch_one(&mut admin)
                .await?;
        if !exists {
            let statement = owner_role.map_or_else(
                || format!("CREATE DATABASE \"{database}\""),
                |owner| format!("CREATE DATABASE \"{database}\" OWNER \"{owner}\""),
            );
            admin.execute(statement.as_str()).await?;
        }
    }
    drop(admin);
    migrate_all(config).await
}

async fn migrate_all(config: &DevConfig) -> TaskResult {
    let control_write = pool(&config.database_url(DATABASES[0])).await?;
    let control_read = pool(&config.database_url(DATABASES[1])).await?;
    let tenant_write = pool(&config.database_url(DATABASES[2])).await?;
    let tenant_read = pool(&config.database_url(DATABASES[3])).await?;
    let restricted_tenant_write = pool(&config.database_url(DATABASES[4])).await?;
    let restricted_tenant_read = pool(&config.database_url(DATABASES[5])).await?;
    erpx_migration::migrate_control_plane(&control_write).await?;
    erpx_migration::migrate_control_read(&control_read).await?;
    erpx_migration::migrate_tenant_write(&tenant_write).await?;
    erpx_migration::migrate_tenant_read(&tenant_read).await?;
    erpx_migration::migrate_tenant_write(&restricted_tenant_write).await?;
    erpx_migration::migrate_tenant_read(&restricted_tenant_read).await?;
    apply_identity_migration(&control_write).await?;
    let control = SqlxControlPlane::new(ControlPlanePools {
        write: control_write,
        read: control_read,
    });
    control.require_erpx_command_owner().await?;
    tenant_write.close().await;
    tenant_read.close().await;
    restricted_tenant_write.close().await;
    restricted_tenant_read.close().await;
    Ok(())
}

async fn pool(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new().max_connections(4).connect(url).await
}

async fn reset_dev(root: &Path, config: &DevConfig, confirmation: &str) -> TaskResult {
    config.validate_local()?;
    if config.port == 55_432 {
        return Err("reset-dev refuses the dedicated cutover cluster".into());
    }
    let legacy_url = config.database_url(&config.legacy_database);
    let legacy = pool(&legacy_url).await?;
    let schemas = user_schemas(&legacy).await?;
    validate_schemas(&schemas)?;
    let buckets = inspect_buckets(config.minio_root.as_deref())?;
    validate_buckets(&buckets)?;

    println!("ERPX development reset targets:");
    println!(
        "  PostgreSQL: {}:{} database {}",
        config.host, config.port, config.legacy_database
    );
    println!("  Drop schemas: {}", LEGACY_SCHEMAS.join(", "));
    println!("  Preserve schemas: {}", PRESERVED_SCHEMAS.join(", "));
    println!("  Recreate databases: {}", DATABASES.join(", "));
    println!("  MinIO endpoint: {}", config.minio_endpoint);
    println!("  Delete bucket: {LEGACY_BUCKET}");
    println!("  Preserve/create bucket: {ERPX_BUCKET}");
    println!("  Preserve runtime: .runtime/n8n");
    if confirmation != RESET_CONFIRMATION {
        return Err(format!(
            "reset-dev requires --confirm {RESET_CONFIRMATION}; no changes were made"
        )
        .into());
    }

    let n8n_config = root.join(".runtime/n8n/.n8n/config");
    let n8n_hash_before = sha256_file_if_present(&n8n_config)?;
    for schema in LEGACY_SCHEMAS {
        legacy
            .execute(format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE").as_str())
            .await?;
    }
    legacy.close().await;
    remove_legacy_bucket(config.minio_root.as_deref())?;
    cleanup_legacy_runtime(root)?;
    initialize_databases(config, true).await?;
    let n8n_hash_after = sha256_file_if_present(&n8n_config)?;
    if n8n_hash_before != n8n_hash_after {
        return Err("n8n encryption configuration changed during reset".into());
    }
    println!("Reset complete; n8n state and encryption configuration were preserved.");
    Ok(())
}

async fn user_schemas(pool: &PgPool) -> Result<BTreeSet<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT schema_name
         FROM information_schema.schemata
         WHERE schema_name <> 'information_schema'
           AND schema_name NOT LIKE 'pg_%'
         ORDER BY schema_name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("schema_name"))
        .collect())
}

fn validate_schemas(schemas: &BTreeSet<String>) -> TaskResult {
    let allowed = LEGACY_SCHEMAS
        .iter()
        .chain(PRESERVED_SCHEMAS.iter())
        .copied()
        .collect::<BTreeSet<_>>();
    let unknown = schemas
        .iter()
        .filter(|schema| !allowed.contains(schema.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(format!("reset-dev refuses unknown schemas: {}", unknown.join(", ")).into());
    }
    Ok(())
}

fn inspect_buckets(root: Option<&Path>) -> TaskResult<BTreeSet<String>> {
    let Some(root) = root else {
        return Ok(BTreeSet::new());
    };
    if !root.exists() {
        return Ok(BTreeSet::new());
    }
    let mut buckets = BTreeSet::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name != ".minio.sys" {
                buckets.insert(name);
            }
        }
    }
    Ok(buckets)
}

fn validate_buckets(buckets: &BTreeSet<String>) -> TaskResult {
    let unknown = buckets
        .iter()
        .filter(|bucket| bucket.as_str() != LEGACY_BUCKET && bucket.as_str() != ERPX_BUCKET)
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(format!("reset-dev refuses unknown buckets: {}", unknown.join(", ")).into());
    }
    Ok(())
}

fn remove_legacy_bucket(root: Option<&Path>) -> TaskResult {
    let Some(root) = root else {
        println!("  MinIO data root not configured; endpoint cleanup was not required.");
        return Ok(());
    };
    let canonical_root = fs::canonicalize(root)?;
    if canonical_root.parent().is_none() || canonical_root == Path::new("/") {
        return Err("reset-dev refuses a broad MinIO data root".into());
    }
    let legacy = canonical_root.join(LEGACY_BUCKET);
    if legacy.is_symlink() {
        return Err("reset-dev refuses a symlinked legacy bucket".into());
    }
    if legacy.exists() {
        fs::remove_dir_all(&legacy)?;
    }
    fs::create_dir_all(canonical_root.join(ERPX_BUCKET))?;
    Ok(())
}

fn cleanup_legacy_runtime(root: &Path) -> TaskResult {
    let runtime = root.join(".runtime");
    for name in ["backups", "legacy", "logs"] {
        let target = runtime.join(name);
        if target.exists() {
            fs::remove_dir_all(target)?;
        }
    }
    Ok(())
}

fn demo_seed_receipt_key() -> String {
    format!("seed-demo:{FIXTURE_VERSION}")
}

async fn demo_fixture_current(root: &Path, config: &DevConfig) -> TaskResult<bool> {
    let write = pool(&config.database_url(DATABASES[0])).await?;
    let receipt_matches: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM erpx_control.demo_seed_commands
             WHERE idempotency_key=$1 AND fixture_version=$2 AND content_sha256=$3
         )",
    )
    .bind(demo_seed_receipt_key())
    .bind(FIXTURE_VERSION)
    .bind(fixture_digest())
    .fetch_one(&write)
    .await?;
    if !receipt_matches {
        write.close().await;
        return Ok(false);
    }

    let tenant_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.tenant_registry
         WHERE (tenant_id=$1 AND holding_ref=$2 AND write_database_key=$3
                AND read_database_key=$4 AND storage_namespace=$5 AND state='active')
            OR (tenant_id=$6 AND holding_ref=$7 AND write_database_key=$8
                AND read_database_key=$9 AND storage_namespace=$10 AND state='active')",
    )
    .bind(TENANT_ID)
    .bind(HOLDING_REF)
    .bind(DATABASES[2])
    .bind(DATABASES[3])
    .bind(ERPX_BUCKET)
    .bind(RESTRICTED_TENANT_ID)
    .bind(RESTRICTED_HOLDING_REF)
    .bind(DATABASES[4])
    .bind(DATABASES[5])
    .bind("erpx-demo-restricted")
    .fetch_one(&write)
    .await?;

    let configured_tenant_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.tenant_summary_projection
         WHERE tenant_id = ANY($1)
           AND configuration_state='configured'
           AND customer_id IS NOT NULL
           AND root_holding_id IS NOT NULL
           AND cardinality(enabled_modules) > 0",
    )
    .bind([TENANT_ID, RESTRICTED_TENANT_ID])
    .fetch_one(&write)
    .await?;

    let context_keys = DEMO_CONTEXTS
        .iter()
        .map(|(key, _, _, _)| *key)
        .collect::<Vec<_>>();
    let context_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.demo_contexts
         WHERE tenant_id=$1 AND holding_ref=$2 AND context_key = ANY($3)",
    )
    .bind(TENANT_ID)
    .bind(HOLDING_REF)
    .bind(&context_keys)
    .fetch_one(&write)
    .await?;

    let mut principal_refs = DEMO_IDENTITIES
        .iter()
        .map(|(principal_ref, _, _)| *principal_ref)
        .collect::<Vec<_>>();
    principal_refs.push("local:native-demo");
    let principal_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_identity.principals
         WHERE principal_ref = ANY($1) AND status='active'",
    )
    .bind(&principal_refs)
    .fetch_one(&write)
    .await?;

    let assignment_ids = DEMO_IDENTITIES
        .iter()
        .map(|(principal_ref, role_key, _)| {
            Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("erpx:demo:{FIXTURE_VERSION}:role:{principal_ref}:{role_key}").as_bytes(),
            )
        })
        .collect::<Vec<_>>();
    let assignment_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM erpx_control.role_assignments WHERE id = ANY($1)")
            .bind(&assignment_ids)
            .fetch_one(&write)
            .await?;
    write.close().await;

    let native_secret = runtime_dir(root).join("secrets/native-demo-password.key");
    Ok(tenant_count == 2
        && configured_tenant_count == 2
        && context_count == i64::try_from(DEMO_CONTEXTS.len())?
        && principal_count == i64::try_from(principal_refs.len())?
        && assignment_count == i64::try_from(assignment_ids.len())?
        && native_secret.is_file())
}

async fn seed_demo(root: &Path, config: &DevConfig) -> TaskResult {
    let write = pool(&config.database_url(DATABASES[0])).await?;
    let read = pool(&config.database_url(DATABASES[1])).await?;
    let control = std::sync::Arc::new(SqlxControlPlane::new(ControlPlanePools {
        write: write.clone(),
        read,
    }));
    control.require_erpx_command_owner().await?;

    let registry = SqlxTenantStoreRegistry::new(control.clone());
    if registry.get(TENANT_ID).await?.is_none() {
        registry
            .register(TenantStoreRegistration {
                tenant_id: TENANT_ID.into(),
                holding_ref: HOLDING_REF.into(),
                write_database_key: DATABASES[2].into(),
                read_database_key: DATABASES[3].into(),
                storage_namespace: ERPX_BUCKET.into(),
                credential_key_ref: "local_shared".into(),
                backup_chain_ref: "local-development".into(),
                state: "active".into(),
                registry_version: 1,
            })
            .await?;
    }
    if registry.get(RESTRICTED_TENANT_ID).await?.is_none() {
        registry
            .register(TenantStoreRegistration {
                tenant_id: RESTRICTED_TENANT_ID.into(),
                holding_ref: RESTRICTED_HOLDING_REF.into(),
                write_database_key: DATABASES[4].into(),
                read_database_key: DATABASES[5].into(),
                storage_namespace: "erpx-demo-restricted".into(),
                credential_key_ref: "local_shared".into(),
                backup_chain_ref: "local-development".into(),
                state: "active".into(),
                registry_version: 1,
            })
            .await?;
    }
    sqlx::query(
        "UPDATE erpx_control.tenant_registry
         SET credential_key_ref='local_shared',
             updated_at=now()
         WHERE tenant_id IN ($1,$2)",
    )
    .bind(TENANT_ID)
    .bind(RESTRICTED_TENANT_ID)
    .execute(&write)
    .await?;

    configure_demo_tenants(&control).await?;

    let identity = SqlxIdentityStore::new(write.clone());
    ensure_native_demo_principal(root, &write, &identity).await?;
    let principal_ref = "local:platform-root";
    let principal_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM erpx_identity.principals WHERE principal_ref = $1
         )",
    )
    .bind(principal_ref)
    .fetch_one(&write)
    .await?;
    if !principal_exists {
        let one_time_password = format!(
            "ERPX-{}-{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        identity
            .create_principal_with_password(principal_ref, &one_time_password, Utc::now())
            .await?;
        let recovery = identity
            .issue_recovery_codes(principal_ref, 1, Utc::now())
            .await?;
        let recovery_code = recovery.code(0).ok_or("recovery code was not issued")?;
        println!("Platform Root bootstrap credentials (shown once):");
        println!("  principal: {principal_ref}");
        println!("  one-time password: {one_time_password}");
        println!("  one-time recovery code: {recovery_code}");
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
             VALUES ($1, $2, NULL, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(principal_ref)
        .bind("platform.break_glass.bootstrap")
        .bind("Create local break-glass Platform Root; secret material stored only as hashes")
        .bind(Uuid::new_v4())
        .bind(serde_json::json!({
            "principal_ref": principal_ref,
            "password_hash_only": true,
            "recovery_hash_only": true,
            "passkey_or_totp_required_for_high_risk": true
        }))
        .execute(&write)
        .await?;
    }
    seed_demo_identities_and_roles(&write, &identity, &control).await?;

    let fixture_digest = fixture_digest();
    let already_seeded: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM erpx_control.demo_seed_commands
             WHERE idempotency_key = $1 AND content_sha256 = $2
         )",
    )
    .bind(demo_seed_receipt_key())
    .bind(&fixture_digest)
    .fetch_one(&write)
    .await?;
    if !already_seeded {
        let mut transaction = write.begin().await?;
        for (key, display_name, state, sample_count) in DEMO_CONTEXTS {
            sqlx::query(
                "INSERT INTO erpx_control.demo_contexts
                    (context_key, display_name, tenant_id, holding_ref, state,
                     sample_count, source_version, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, 1, $7)
                 ON CONFLICT (context_key) DO UPDATE
                 SET display_name = EXCLUDED.display_name,
                     state = EXCLUDED.state,
                     sample_count = EXCLUDED.sample_count,
                     source_version = erpx_control.demo_contexts.source_version + 1,
                     updated_at = EXCLUDED.updated_at",
            )
            .bind(key)
            .bind(display_name)
            .bind(TENANT_ID)
            .bind(HOLDING_REF)
            .bind(state)
            .bind(sample_count)
            .bind(Utc::now())
            .execute(&mut *transaction)
            .await?;
            let event_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("erpx:demo:{FIXTURE_VERSION}:{key}").as_bytes(),
            );
            sqlx::query(
                "INSERT INTO erpx_control.outbox_events (
                    id, event_type, event_version, aggregate_type, aggregate_id,
                    aggregate_version, tenant_scope, actor, correlation_id,
                    idempotency_key, payload, occurred_at
                 )
                 VALUES ($1, 'erpx.demo.context.seeded.v1', 1, 'demo_context', $1,
                         1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(event_id)
            .bind(serde_json::json!({"tenant_id":TENANT_ID,"holding_ref":HOLDING_REF}))
            .bind(serde_json::json!({"kind":"system","subject":"xtask:seed-demo"}))
            .bind(Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                b"erpx:demo:correlation:v1",
            ))
            .bind(format!("seed-demo:{FIXTURE_VERSION}:{key}"))
            .bind(serde_json::json!({
                "context": key,
                "state": state,
                "sample_count": sample_count
            }))
            .bind(Utc::now())
            .execute(&mut *transaction)
            .await?;
        }
        sqlx::query(
            "INSERT INTO erpx_control.demo_seed_commands
                (idempotency_key, fixture_version, content_sha256, completed_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(demo_seed_receipt_key())
        .bind(FIXTURE_VERSION)
        .bind(&fixture_digest)
        .bind(Utc::now())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
    }
    let context_count: i64 = sqlx::query_scalar("SELECT count(*) FROM erpx_control.demo_contexts")
        .fetch_one(&write)
        .await?;
    let connector_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM erpx_identity.directory_connectors")
            .fetch_one(&write)
            .await?;
    println!(
        "Demo fixture ready: {context_count} contexts, {connector_count} persisted directory connectors."
    );
    write.close().await;
    Ok(())
}

async fn configure_demo_tenants(control: &std::sync::Arc<SqlxControlPlane>) -> TaskResult {
    const ALL_MODULES: &[&str] = &[
        "operations",
        "finance",
        "sales",
        "procurement",
        "inventory",
        "assets",
        "maintenance",
        "quality",
        "marketing-crm",
        "hr-payroll",
        "point-of-sale",
        "legal",
    ];
    const RESTRICTED_MODULES: &[&str] = &[
        "operations",
        "finance",
        "sales",
        "inventory",
        "hr-payroll",
        "point-of-sale",
    ];
    let tenants = [
        (
            TENANT_ID,
            HOLDING_REF,
            "DEMO-ATLAS",
            "Atlas Holdings",
            "Atlas Holdings Co., Ltd.",
            ALL_MODULES,
            100_i32,
            24_i32,
            2_i32,
            4_i32,
        ),
        (
            RESTRICTED_TENANT_ID,
            RESTRICTED_HOLDING_REF,
            "DEMO-BEACON",
            "Beacon Services",
            "Beacon Services Co., Ltd.",
            RESTRICTED_MODULES,
            40_i32,
            12_i32,
            2_i32,
            4_i32,
        ),
    ];
    let administration = SqlxTenantAdministrationStore::new(control.clone());

    for (
        tenant_id,
        holding_ref,
        account_code,
        trading_name,
        legal_name,
        modules,
        purchased_seats,
        used_seats,
        company_count,
        branch_count,
    ) in tenants
    {
        let customer_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:demo:customer:v1:{tenant_id}").as_bytes(),
        );
        let root_holding_id = tenant_holding_scope_id(holding_ref);
        let payload = serde_json::json!({
            "account": {
                "id": customer_id,
                "account_code": account_code,
                "trading_name": trading_name,
                "legal_name": legal_name,
                "industry": "Business services",
                "country": "TH",
                "status": "Active",
                "billing_state": "Active",
                "renewal_at": "2099-12-31T00:00:00Z",
                "lifetime_value_minor": 0
            },
            "health": {
                "customer_account_id": customer_id,
                "state": "Healthy",
                "projection_lag_seconds": 0,
                "worker_failures": 0,
                "backup_status": "verified",
                "last_activity_at": null
            },
            "holding_count": 1,
            "company_count": company_count,
            "branch_count": branch_count,
            "active_user_count": used_seats,
            "expiring_role_count": 0,
            "open_ticket_count": 0,
            "module_count": modules.len()
        });
        for database in [&control.pools().write, &control.pools().read] {
            sqlx::query(
                "INSERT INTO erpx_control.customers
                    (id, account_code, trading_name, legal_name, country,
                     status, billing_state, payload, updated_at)
                 VALUES ($1,$2,$3,$4,'TH','active','active',$5,now())
                 ON CONFLICT (id) DO UPDATE SET
                    account_code=EXCLUDED.account_code,
                    trading_name=EXCLUDED.trading_name,
                    legal_name=EXCLUDED.legal_name,
                    country=EXCLUDED.country,
                    status=EXCLUDED.status,
                    billing_state=EXCLUDED.billing_state,
                    payload=EXCLUDED.payload,
                    updated_at=EXCLUDED.updated_at",
            )
            .bind(customer_id)
            .bind(account_code)
            .bind(trading_name)
            .bind(legal_name)
            .bind(&payload)
            .execute(database)
            .await?;
        }
        sqlx::query(
            "INSERT INTO erpx_control.customer_scope_bindings
                (customer_id, holding_id, company_id, branch_id)
             VALUES ($1,$2,NULL,NULL)
             ON CONFLICT DO NOTHING",
        )
        .bind(customer_id)
        .bind(root_holding_id)
        .execute(&control.pools().write)
        .await?;

        if administration
            .customer_tenant_binding(tenant_id)
            .await?
            .is_none()
        {
            let binding =
                CustomerTenantBinding::new(customer_id, tenant_id, root_holding_id, holding_ref)?;
            administration
                .bind_customer_tenant(&binding, "xtask:seed-demo")
                .await?;
        }

        sqlx::query(
            "UPDATE erpx_control.tenant_summary_projection
             SET subscription_state='active',
                 plan_key='demo-enterprise',
                 enabled_modules=$2,
                 purchased_seats=$3,
                 used_seats=$4,
                 company_count=$5,
                 branch_count=$6,
                 renewal_at='2099-12-31T00:00:00Z'::timestamptz,
                 overdue=FALSE,
                 projection_freshness='current',
                 storage_state='ready',
                 backup_state='verified',
                 integration_health='healthy',
                 health='healthy',
                 projected_at=now()
             WHERE tenant_id=$1",
        )
        .bind(tenant_id)
        .bind(modules)
        .bind(purchased_seats)
        .bind(used_seats)
        .bind(company_count)
        .bind(branch_count)
        .execute(&control.pools().write)
        .await?;
    }
    Ok(())
}

async fn reset_platform_root_password(config: &DevConfig, confirmation: &str) -> TaskResult {
    if confirmation != ROOT_PASSWORD_RESET_CONFIRMATION {
        return Err(format!(
            "reset-platform-root requires the literal confirmation {ROOT_PASSWORD_RESET_CONFIRMATION}"
        )
        .into());
    }
    config.validate_local()?;
    if config.port != 55_432 || config.legacy_database != DATABASES[0] {
        return Err(
            "reset-platform-root is limited to the dedicated ERPX PostgreSQL cluster".into(),
        );
    }
    let write = pool(&config.database_url(DATABASES[0])).await?;
    let identity = SqlxIdentityStore::new(write.clone());
    let new_password = format!(
        "ERPX-ROOT-{}-{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let revoked_sessions = identity
        .rotate_platform_root_password(&new_password, Utc::now())
        .await?;
    let principal_ref = "local:platform-root";
    sqlx::query(
        "INSERT INTO erpx_control.audit_events
            (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
         VALUES ($1, $2, NULL, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind(principal_ref)
    .bind("platform.break_glass.password_rotated")
    .bind("Local development break-glass Platform Root password rotation")
    .bind(Uuid::new_v4())
    .bind(serde_json::json!({
        "principal_ref": principal_ref,
        "password_hash_only": true,
        "revoked_sessions": revoked_sessions
    }))
    .execute(&write)
    .await?;
    println!("Platform Root password rotated (shown once):");
    println!("  principal: {principal_ref}");
    println!("  password: {new_password}");
    println!("  revoked sessions: {revoked_sessions}");
    write.close().await;
    Ok(())
}

async fn ensure_native_demo_principal(
    root: &Path,
    write: &PgPool,
    identity: &SqlxIdentityStore,
) -> TaskResult {
    let secret_root = runtime_dir(root).join("secrets");
    fs::create_dir_all(&secret_root)?;
    fs::set_permissions(&secret_root, fs::Permissions::from_mode(0o700))?;
    let password_component = secret_file(&secret_root.join("native-demo-password.key"))?;
    let principal_ref = "local:native-demo";
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM erpx_identity.principals WHERE principal_ref=$1
         )",
    )
    .bind(principal_ref)
    .fetch_one(write)
    .await?;
    if !exists {
        identity
            .create_principal_with_password(
                principal_ref,
                &format!("ERPX-NATIVE-{password_component}"),
                Utc::now(),
            )
            .await?;
    }
    Ok(())
}

async fn seed_demo_identities_and_roles(
    write: &PgPool,
    identity: &SqlxIdentityStore,
    control: &std::sync::Arc<SqlxControlPlane>,
) -> TaskResult {
    let now = Utc::now();
    for (principal_ref, _, _) in DEMO_IDENTITIES.iter().copied() {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_identity.principals WHERE principal_ref = $1
             )",
        )
        .bind(principal_ref)
        .fetch_one(write)
        .await?;
        if !exists {
            let discarded_bootstrap_secret = format!("ERPX-DEMO-{}", Uuid::new_v4().simple());
            identity
                .create_principal_with_password(principal_ref, &discarded_bootstrap_secret, now)
                .await?;
        }
    }

    let lifecycle = RoleLifecycleService::new(
        control.clone(),
        std::sync::Arc::new(SystemClock),
        control.clone(),
    );
    let platform_permissions = PLATFORM_PERMISSIONS
        .iter()
        .copied()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    for (principal_ref, role_key, lifetime_days) in DEMO_IDENTITIES.iter().copied() {
        let assignment_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:demo:{FIXTURE_VERSION}:role:{principal_ref}:{role_key}").as_bytes(),
        );
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_control.role_assignments WHERE id = $1
             )",
        )
        .bind(assignment_id)
        .fetch_one(write)
        .await?;
        if exists {
            continue;
        }
        let platform_root = role_key == "platform_root";
        lifecycle
            .grant(
                RoleAssignment {
                    id: assignment_id,
                    principal_ref: principal_ref.into(),
                    role_key: role_key.into(),
                    permissions: if platform_root {
                        platform_permissions.clone()
                    } else {
                        vec![
                            "platform.tenant.read".into(),
                            "platform.customer.read".into(),
                        ]
                    },
                    functional_domain: if platform_root {
                        "platform".into()
                    } else {
                        role_key.trim_end_matches("_demo").into()
                    },
                    authority_level: if platform_root { 255 } else { 32 },
                    approval_limit_minor: if platform_root { u64::MAX } else { 0 },
                    scope: Scope::Platform,
                    valid_from: now,
                    valid_until: now + chrono::Duration::days(lifetime_days),
                    timezone: "Asia/Bangkok".into(),
                    risk_tier: if platform_root {
                        RiskTier::PlatformRoot
                    } else {
                        RiskTier::Standard
                    },
                    grant_reason: "Idempotent ERPX development fixture".into(),
                    ticket_reference: Some("local:cutover".into()),
                    grantor: "bootstrap:system".into(),
                    approver: platform_root.then(|| "bootstrap:independent-approver".into()),
                    renewal_reviewer: None,
                    parent_assignment_id: None,
                    assignment_version: 1,
                },
                None,
            )
            .await?;
    }
    Ok(())
}

fn fixture_digest() -> String {
    fixture_digest_for(&DEMO_CONTEXTS, &DEMO_IDENTITIES, PLATFORM_PERMISSIONS)
}

fn fixture_digest_for(
    contexts: &[(&str, &str, &str, i64)],
    identities: &[(&str, &str, i64)],
    permissions: &[&str],
) -> String {
    let mut digest = Sha256::new();
    for value in [
        FIXTURE_VERSION,
        TENANT_ID,
        HOLDING_REF,
        RESTRICTED_TENANT_ID,
        RESTRICTED_HOLDING_REF,
        ERPX_BUCKET,
        "erpx-demo-restricted",
        "local-development-disabled",
        "local-development",
        "Asia/Bangkok",
        "local:cutover",
        "bootstrap:system",
        "bootstrap:independent-approver",
    ]
    .into_iter()
    .chain(DATABASES)
    {
        update_digest(&mut digest, value);
    }
    for context in contexts {
        update_digest(&mut digest, &format!("{context:?}"));
    }
    for identity in identities {
        update_digest(&mut digest, &format!("{identity:?}"));
    }
    for permission in permissions {
        update_digest(&mut digest, permission);
    }
    format!("{:x}", digest.finalize())
}

fn update_digest(digest: &mut Sha256, value: &str) {
    digest.update(value.len().to_le_bytes());
    digest.update(value.as_bytes());
}

fn development_access_requested(root: &Path) -> TaskResult<bool> {
    let values = read_dotenv(&root.join(".env.local"))?;
    let flag = env::var("ERPX_DEV_PERSONA_MODE")
        .ok()
        .or_else(|| values.get("ERPX_DEV_PERSONA_MODE").cloned());
    Ok(flag.as_deref() == Some("enabled"))
}

fn require_development_access_requested(root: &Path) -> TaskResult {
    if development_access_requested(root)? {
        Ok(())
    } else {
        Err("development access is disabled; set ERPX_DEV_PERSONA_MODE=enabled explicitly".into())
    }
}

async fn dev(root: &Path, config: &DevConfig, mode: DevMode) -> TaskResult {
    let total_started = Instant::now();
    config.validate_local()?;
    let fresh_runtime = FreshRuntimeManifest::load(root)?;
    let development_access_enabled = development_access_requested(root)?;
    let dedicated = config.port == 55_432;
    let runtime = runtime_dir(root);
    fs::create_dir_all(&runtime)?;

    if mode == DevMode::Fresh {
        let phase = Instant::now();
        stop(root)?;
        println!(
            "dev processes: stopped for fresh start ({:?})",
            phase.elapsed()
        );
    } else {
        prune_stale_pid_files(root)?;
    }

    let phase = Instant::now();
    if dedicated {
        if mode == DevMode::Warm && infra::is_running(root) {
            println!("dev infrastructure: reused ({:?})", phase.elapsed());
        } else {
            infra::start(root).await?;
            println!("dev infrastructure: started ({:?})", phase.elapsed());
        }
    } else {
        println!(
            "dev infrastructure: external host reused ({:?})",
            phase.elapsed()
        );
    }
    if development_access_enabled {
        dev_access::init(root, config)?;
    }
    if let Some(manifest) = fresh_runtime.as_ref() {
        if dedicated {
            infra::ensure_runtime_postgres_hba()?;
        }
        manifest.require_complete_receipt(root, config).await?;
        println!(
            "dev cutover: verified fresh receipt for installation {}",
            manifest.installation_generation
        );
    }

    let phase = Instant::now();
    let mut build_arguments = vec![
        "build",
        "--offline",
        "-p",
        "erpx-server",
        "-p",
        "erpx-worker",
        "-p",
        "erpx-ucs-lab",
    ];
    if dedicated {
        build_arguments.extend(["-p", "erpx-wal-archive"]);
    }
    run_command(root, "cargo", build_arguments)?;
    println!("dev binaries: built ({:?})", phase.elapsed());

    let phase = Instant::now();
    if fresh_runtime.is_some() {
        println!("dev schema: using bootstrap-migrated fresh databases");
    } else if dedicated {
        let admin_url = infra::admin_runtime_database_url(root)?;
        initialize_databases_with_admin(config, false, &admin_url, Some("erpx_app")).await?;
        infra::reconcile_logical_profiles(root).await?;
    } else {
        initialize_databases(config, false).await?;
    }
    println!("dev schema: reconciled ({:?})", phase.elapsed());

    let demo_current = fresh_runtime.is_some()
        || (mode == DevMode::Warm && demo_fixture_current(root, config).await?);
    let access_current = !development_access_enabled
        || (mode == DevMode::Warm
            && match fresh_runtime.as_ref() {
                Some(manifest) => {
                    dev_access::runtime_catalog_current(
                        root,
                        config,
                        &manifest.control_write_database,
                        &manifest.tenant_id,
                    )
                    .await?
                }
                None => dev_access::fixtures_current(root, config).await?,
            });
    if mode == DevMode::Warm && (!demo_current || !access_current) {
        stop_service_group(root, &["server"])?;
        stop_service_group(root, &DEDICATED_WORKERS)?;
        stop_service_group(root, &UCS_LABS)?;
        println!("dev runtime services: paused for fixture reconciliation");
    }

    let phase = Instant::now();
    if !demo_current {
        seed_demo(root, config).await?;
        println!("dev demo fixture: reconciled ({:?})", phase.elapsed());
    } else {
        println!("dev demo fixture: reused ({:?})", phase.elapsed());
    }
    if development_access_enabled {
        let phase = Instant::now();
        if !access_current {
            if let Some(manifest) = fresh_runtime.as_ref() {
                dev_access::reconcile_runtime_catalog(
                    root,
                    config,
                    &manifest.control_write_database,
                    &manifest.control_read_database,
                    &manifest.tenant_id,
                )
                .await?;
            } else {
                dev_access::reconcile(root, config).await?;
            }
            println!("dev access fixture: reconciled ({:?})", phase.elapsed());
        } else {
            println!("dev access fixture: reused ({:?})", phase.elapsed());
        }
    }

    let phase = Instant::now();
    let runtime_values = read_dotenv(&root.join(".env.local"))?;
    let configured_value = |key: &str| {
        env::var(key)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                runtime_values
                    .get(key)
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
            })
    };
    let tailnet_ip = configured_development_tailnet_ip(root)?;
    if tailnet_ip.is_some() && !development_access_enabled {
        return Err(
            "ERPX_DEV_TAILSCALE_IP requires ERPX_DEV_PERSONA_MODE=enabled for dev-only access"
                .into(),
        );
    }
    let advertised_host = tailnet_ip
        .map(|address| address.to_string())
        .unwrap_or_else(|| "localhost".to_owned());
    let auth_http_origin = tailnet_ip.map_or_else(
        || {
            configured_value("ERPX_CONSOLE_AUTH_HTTP_ORIGIN")
                .unwrap_or_else(|| "http://localhost:50052".to_owned())
        },
        |address| format!("http://{address}:50052"),
    );
    let grpc_endpoint = tailnet_ip.map_or_else(
        || {
            configured_value("ERPX_CONSOLE_GRPC_ENDPOINT")
                .unwrap_or_else(|| "http://127.0.0.1:50051".to_owned())
        },
        |address| format!("http://{address}:50051"),
    );
    let mut common = dev_common_environment(
        root,
        config,
        dedicated,
        development_access_enabled,
        fresh_runtime.as_ref(),
    )?;
    let mut allowed_origins = configured_value("ERPX_HTTP_ALLOWED_ORIGINS");
    if let Some(address) = tailnet_ip {
        let origin = format!("http://{address}:3004");
        let values = allowed_origins.get_or_insert_with(String::new);
        if !values.split(',').any(|value| value.trim() == origin) {
            if !values.is_empty() {
                values.push(',');
            }
            values.push_str(&origin);
        }
    }
    if let Some(allowed_origins) = allowed_origins {
        common.push(("ERPX_HTTP_ALLOWED_ORIGINS", allowed_origins));
    }
    let mut server_environment = common.clone();
    server_environment.push((
        "ERPX_SUPPORT_CAPABILITY_ESCROW_KEY",
        secret_file(&runtime.join("support-capability-escrow.key"))?,
    ));
    let mut web_environment = vec![
        ("ERPX_DEV_WEB_BIND_ADDR", "127.0.0.1".to_owned()),
        ("ERPX_CONSOLE_API_BASE", auth_http_origin.clone()),
        ("ERPX_CONSOLE_AUTH_HTTP_ORIGIN", auth_http_origin),
        ("ERPX_CONSOLE_GRPC_ENDPOINT", grpc_endpoint),
        ("ERPX_CONSOLE_V1_SERVICE_AUTHORITY", "typed".into()),
        ("ERPX_WEB_BFF_ENABLED", "1".into()),
        ("ERPX_WEB_SSR_ENABLED", "1".into()),
        ("ERPX_WEB_SESSION_KEY_REFERENCE", "local-dev-v1".into()),
        (
            "ERPX_WEB_SESSION_KEY",
            STANDARD.encode(Sha256::digest(
                secret_file(&runtime.join("web-session.key"))?.as_bytes(),
            )),
        ),
    ];
    let control_write_database_url = common
        .iter()
        .find_map(|(key, value)| {
            (*key == "ERPX_CONTROL_WRITE_DATABASE_URL").then_some(value.clone())
        })
        .ok_or("control write database URL is unavailable for Web BFF")?;
    web_environment.push((
        "ERPX_CONTROL_WRITE_DATABASE_URL",
        control_write_database_url,
    ));
    if development_access_enabled {
        web_environment.push(("ERPX_CONSOLE_DEV_PERSONA_ENABLED", "1".into()));
    }
    let desired_state = DevState {
        version: DEV_STATE_VERSION,
        server_binary_sha256: required_file_sha256(&root.join("target/debug/erpx-server"))?,
        worker_binary_sha256: required_file_sha256(&root.join("target/debug/erpx-worker"))?,
        ucs_binary_sha256: required_file_sha256(&root.join("target/debug/erpx-ucs-lab"))?,
        server_environment_sha256: environment_sha256(&server_environment),
        worker_environment_sha256: worker_environment_sha256(&common, dedicated),
        ucs_environment_sha256: if development_access_enabled {
            ucs_environment_sha256(root)?
        } else {
            "disabled".into()
        },
        web_environment_sha256: environment_sha256(&web_environment),
        development_access_enabled,
    };
    let previous_state = read_dev_state(root);
    let snapshot = runtime_snapshot(root, dedicated);
    let restart_plan = plan_dev_restart(mode, previous_state.as_ref(), &desired_state, snapshot);
    println!(
        "dev fingerprints/restart plan: ready ({:?})",
        phase.elapsed()
    );

    let phase = Instant::now();
    if let Some(manifest) = fresh_runtime.as_ref() {
        let workers_started_at = Utc::now();
        apply_worker_plan(
            root,
            &runtime,
            restart_plan.workers,
            snapshot.any_workers,
            &common,
            dedicated,
        )?;
        wait_for_fresh_worker_barrier(root, &common, manifest, workers_started_at, dedicated)
            .await?;
        apply_server_plan(
            root,
            &runtime,
            restart_plan.server,
            snapshot.server,
            &server_environment,
        )?;
    } else {
        apply_server_plan(
            root,
            &runtime,
            restart_plan.server,
            snapshot.server,
            &server_environment,
        )?;
        apply_worker_plan(
            root,
            &runtime,
            restart_plan.workers,
            snapshot.any_workers,
            &common,
            dedicated,
        )?;
    }
    apply_ucs_plan(
        root,
        restart_plan,
        snapshot.any_ucs_labs,
        development_access_enabled,
    )?;
    apply_web_plan(
        root,
        &runtime,
        restart_plan.web,
        snapshot.web,
        web_environment,
    )?;
    println!("dev runtime services: applied ({:?})", phase.elapsed());

    let phase = Instant::now();
    wait_for_http("http://127.0.0.1:50052/health", 90).await?;
    wait_for_http("http://127.0.0.1:50052/ready", 90).await?;
    wait_for_http("http://127.0.0.1:3004", 180).await?;
    if let Some(manifest) = fresh_runtime.as_ref() {
        require_fresh_command_projection_smoke(&common, manifest).await?;
    }
    if development_access_enabled {
        wait_for_ucs_authorities(root, 30).await?;
    }
    require_runtime_services(root, dedicated, development_access_enabled)?;
    write_dev_state(root, &desired_state)?;
    println!("dev health/state: ready ({:?})", phase.elapsed());
    println!(
        "ERPX is running at http://{advertised_host}:3004 (dev ensure {:?})",
        total_started.elapsed()
    );
    Ok(())
}

fn dev_common_environment(
    root: &Path,
    config: &DevConfig,
    dedicated: bool,
    development_access_enabled: bool,
    fresh_runtime: Option<&FreshRuntimeManifest>,
) -> TaskResult<Vec<(&'static str, String)>> {
    let runtime = runtime_dir(root);
    let access_key = secret_file(&runtime.join("access-token.key"))?;
    let totp_key = secret_file(&runtime.join("totp.key"))?;
    let control_write_database = fresh_runtime
        .map(|manifest| manifest.control_write_database.as_str())
        .unwrap_or(DATABASES[0]);
    let control_read_database = fresh_runtime
        .map(|manifest| manifest.control_read_database.as_str())
        .unwrap_or(DATABASES[1]);
    let mut common = vec![
        (
            "ERPX_CONTROL_WRITE_DATABASE_URL",
            config.database_url(control_write_database),
        ),
        (
            "ERPX_CONTROL_READ_DATABASE_URL",
            config.database_url(control_read_database),
        ),
        (
            "ERPX_TENANT_ADMIN_DATABASE_URL",
            if dedicated {
                infra::admin_runtime_database_url(root)?
            } else {
                config.database_url("postgres")
            },
        ),
        (
            "ERPX_TENANT_DATABASE_URL_TEMPLATE",
            config.database_template(),
        ),
        ("ERPX_ACCESS_TOKEN_SIGNING_KEY", access_key),
        ("ERPX_IDENTITY_TOTP_KEY", totp_key),
        (
            "ERPX_DIRECTORY_RUNTIME",
            if dedicated { "enabled" } else { "disabled" }.into(),
        ),
        (
            "ERPX_MARKETING_CALLBACK_SECRET_RESOLVER",
            "unconfigured".into(),
        ),
        ("ERPX_WEBAUTHN_PROVIDER", "webauthn-rs".into()),
        ("ERPX_WEBAUTHN_REQUIRED", "true".into()),
        ("ERPX_WEBAUTHN_RP_ID", "localhost".into()),
        ("ERPX_WEBAUTHN_ORIGINS", "http://localhost:3004".into()),
        (
            "ERPX_DEVELOPMENT_PROFILE",
            if fresh_runtime.is_some() {
                "local-fresh-state"
            } else {
                "local"
            }
            .into(),
        ),
        // Development exercises the same direct-v1 typed authority as a
        // fresh-state cutover. Retained legacy binaries/resources remain the
        // rollback path and are never co-authoritative in this runtime.
        ("ERPX_V1_SERVICE_AUTHORITY", "typed".into()),
    ];
    if development_access_enabled {
        let target_tenant = fresh_runtime.map_or(
            erpx_identity::DEVELOPMENT_CONTROLLER_TARGET_TENANT,
            |manifest| manifest.tenant_id.as_str(),
        );
        common.extend([
            ("ERPX_DEV_PERSONA_MODE", "enabled".into()),
            ("ERPX_DEVELOPMENT_CLUSTER", "local-ucs-lab".into()),
            (
                "ERPX_DEV_CONTROLLER_ROLE_PROFILE",
                "restricted_tenant_full_admin".into(),
            ),
            ("ERPX_DEV_CONTROLLER_TARGET_TENANT", target_tenant.into()),
            ("ERPX_DEV_CONTROLLER_MFA_BYPASS", "enabled".into()),
            ("ERPX_DEV_CONTROLLER_SOD_BYPASS", "enabled".into()),
            (
                "ERPX_DEV_CONTROLLER_PASSWORD_FILE",
                dev_access::controller_password_path(root)
                    .to_string_lossy()
                    .into_owned(),
            ),
        ]);
    }
    if dedicated {
        common.extend(infra::dedicated_common_environment(root)?);
    }
    // Dedicated infrastructure contributes credential-bearing defaults for
    // the legacy development databases. Replace only the two control URLs;
    // tenant routing continues to resolve database keys from the fresh
    // registry through the unchanged template.
    common.retain(|(key, _)| {
        *key != "ERPX_CONTROL_WRITE_DATABASE_URL" && *key != "ERPX_CONTROL_READ_DATABASE_URL"
    });
    common.extend([
        (
            "ERPX_CONTROL_WRITE_DATABASE_URL",
            config.database_url(control_write_database),
        ),
        (
            "ERPX_CONTROL_READ_DATABASE_URL",
            config.database_url(control_read_database),
        ),
    ]);
    if fresh_runtime.is_some() {
        common.push((
            "ERPX_FRESH_STATE_MANIFEST",
            runtime_dir(root)
                .join(FRESH_RUNTIME_MANIFEST_FILE)
                .display()
                .to_string(),
        ));
    }
    Ok(common)
}

fn worker_specs(dedicated: bool) -> Vec<WorkerSpec> {
    let mut workers = vec![
        WorkerSpec {
            name: "support-worker",
            handler: "support_workflow",
            worker_id: WORKER_ID,
            projection_kind: None,
        },
        WorkerSpec {
            name: "commercial-catalog-projector",
            handler: "commercial_catalog_projection",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b144",
            projection_kind: None,
        },
        WorkerSpec {
            name: "wave5-control-projector",
            handler: "wave5_control_projections",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b145",
            projection_kind: None,
        },
    ];
    workers.extend(
        [
            ("quality-projector", "quality", "b146"),
            (
                "identity-access-principals-projector",
                "identity_access_principals",
                "b147",
            ),
            (
                "identity-access-roles-projector",
                "identity_access_roles",
                "b148",
            ),
            ("organization-projector", "organization", "b149"),
            (
                "commercial-subscriptions-projector",
                "commercial_subscriptions",
                "b14a",
            ),
            (
                "commercial-entitlements-projector",
                "commercial_entitlements",
                "b14b",
            ),
            (
                "marketing-lead-candidates-projector",
                "marketing_lead_candidates",
                "b14c",
            ),
            ("crm-leads-projector", "crm_leads", "b14d"),
            ("crm-opportunities-projector", "crm_opportunities", "b14e"),
            (
                "procurement-purchase-orders-projector",
                "procurement_purchase_orders",
                "b14f",
            ),
            (
                "inventory-stock-items-projector",
                "inventory_stock_items",
                "b150",
            ),
            ("assets-projector", "assets_cqrs", "b151"),
            (
                "maintenance-work-orders-projector",
                "maintenance_work_orders",
                "b152",
            ),
            (
                "maintenance-asset-facts-worker",
                "maintenance_asset_facts",
                "b153",
            ),
            ("sales-quotes-projector", "sales_quotes", "b154"),
            ("sales-orders-projector", "sales_orders", "b155"),
            (
                "people-payroll-employees-projector",
                "people_payroll_employees",
                "b156",
            ),
            (
                "people-payroll-leave-requests-projector",
                "people_payroll_leave_requests",
                "b157",
            ),
            (
                "people-payroll-expense-claims-projector",
                "people_payroll_expense_claims",
                "b158",
            ),
            (
                "people-payroll-payroll-runs-projector",
                "people_payroll_payroll_runs",
                "b159",
            ),
            ("pos-sales-projector", "pos_sales", "b15a"),
            ("pos-refunds-projector", "pos_refunds", "b15b"),
        ]
        .into_iter()
        .map(|(name, projection_kind, suffix)| WorkerSpec {
            name,
            handler: "tenant_projection_scheduler",
            worker_id: match suffix {
                "b146" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b146",
                "b147" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b147",
                "b148" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b148",
                "b149" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b149",
                "b14a" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b14a",
                "b14b" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b14b",
                "b14c" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b14c",
                "b14d" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b14d",
                "b14e" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b14e",
                "b14f" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b14f",
                "b150" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b150",
                "b151" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b151",
                "b152" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b152",
                "b153" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b153",
                "b154" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b154",
                "b155" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b155",
                "b156" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b156",
                "b157" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b157",
                "b158" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b158",
                "b159" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b159",
                "b15a" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b15a",
                "b15b" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b15b",
                _ => unreachable!("frozen direct-v1 projection worker suffix"),
            },
            projection_kind: Some(projection_kind),
        }),
    );
    workers.extend(
        [
            ("finance-charts-projector", "finance_charts", "b130"),
            ("finance-periods-projector", "finance_periods", "b131"),
            ("finance-journals-projector", "finance_journals", "b132"),
            ("finance-subledger-projector", "finance_subledger", "b133"),
            (
                "finance-trial-balance-projector",
                "finance_trial_balance",
                "b134",
            ),
            (
                "finance-bank-transactions-projector",
                "finance_bank_transactions",
                "b135",
            ),
            (
                "finance-statutory-tax-projector",
                "finance_statutory_tax",
                "b136",
            ),
            ("legal-contracts-projector", "legal_contracts", "b137"),
            ("legal-documents-projector", "legal_documents", "b138"),
            (
                "legal-access-history-projector",
                "legal_access_history",
                "b139",
            ),
            ("support-tickets-projector", "support_tickets", "b13a"),
            ("support-incidents-projector", "support_incidents", "b13b"),
            ("support-sessions-projector", "support_sessions", "b13c"),
            (
                "support-capabilities-projector",
                "support_capabilities",
                "b13d",
            ),
            (
                "operations-exceptions-projector",
                "operations_exceptions",
                "b13e",
            ),
            (
                "operations-source-signals-projector",
                "operations_source_signals",
                "b13f",
            ),
            (
                "operations-work-queue-policies-projector",
                "operations_work_queue_policies",
                "b140",
            ),
        ]
        .into_iter()
        .map(|(name, projection_kind, suffix)| WorkerSpec {
            name,
            handler: "tenant_projection_scheduler",
            worker_id: match suffix {
                "b130" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b130",
                "b131" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b131",
                "b132" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b132",
                "b133" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b133",
                "b134" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b134",
                "b135" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b135",
                "b136" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b136",
                "b137" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b137",
                "b138" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b138",
                "b139" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b139",
                "b13a" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b13a",
                "b13b" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b13b",
                "b13c" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b13c",
                "b13d" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b13d",
                "b13e" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b13e",
                "b13f" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b13f",
                "b140" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b140",
                _ => unreachable!("frozen typed projection worker suffix"),
            },
            projection_kind: Some(projection_kind),
        }),
    );
    workers.extend([
        WorkerSpec {
            name: "operations-source-ingestion-worker",
            handler: "operations_source_ingestion",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b143",
            projection_kind: None,
        },
        WorkerSpec {
            name: "customer360-control-customers-projector",
            handler: "customer360_control_customers_projection",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b141",
            projection_kind: None,
        },
        WorkerSpec {
            name: "customer360-tenant-activity-projector",
            handler: "customer360_tenant_activity_projection",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b142",
            projection_kind: None,
        },
        WorkerSpec {
            name: "branch-provisioning-worker",
            handler: "branch_provisioning",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b15c",
            projection_kind: None,
        },
        WorkerSpec {
            name: "tenant-projection-status-projector",
            handler: "tenant_projection_status_projection",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b15d",
            projection_kind: None,
        },
        WorkerSpec {
            name: "marketing-public-abuse-cleanup-worker",
            handler: "marketing_public_abuse_cleanup",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b15e",
            projection_kind: None,
        },
        WorkerSpec {
            name: "marketing-delivery-worker",
            handler: "marketing_delivery",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b15f",
            projection_kind: None,
        },
        WorkerSpec {
            name: "marketing-journey-integration-worker",
            handler: "marketing_journey_integration",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b160",
            projection_kind: None,
        },
    ]);
    if dedicated {
        workers.extend([
            WorkerSpec {
                name: "directory-supervisor",
                handler: "directory_supervisor",
                worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f554a1",
                projection_kind: None,
            },
            WorkerSpec {
                name: "tenant-backup-worker",
                handler: "tenant_backup",
                worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f554a2",
                projection_kind: None,
            },
            WorkerSpec {
                name: "backup-retention-worker",
                handler: "tenant_backup_retention",
                worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f554a3",
                projection_kind: None,
            },
            WorkerSpec {
                name: "pitr-controller",
                handler: "cluster_pitr_controller",
                worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f554a4",
                projection_kind: None,
            },
        ]);
    }
    workers
}

fn worker_environment_sha256(common: &[(&str, String)], dedicated: bool) -> String {
    let mut digest = Sha256::new();
    update_digest(&mut digest, &environment_sha256(common));
    for spec in worker_specs(dedicated) {
        update_digest(&mut digest, spec.name);
        update_digest(&mut digest, spec.handler);
        update_digest(&mut digest, spec.worker_id);
        update_digest(&mut digest, spec.projection_kind.unwrap_or_default());
        update_digest(&mut digest, DEV_WORKER_POLL_INTERVAL_MS);
    }
    format!("{:x}", digest.finalize())
}

fn environment_sha256(environment: &[(&str, String)]) -> String {
    let values = environment
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut digest = Sha256::new();
    for (key, value) in values {
        update_digest(&mut digest, &key);
        update_digest(&mut digest, &value);
    }
    format!("{:x}", digest.finalize())
}

fn ucs_environment_sha256(root: &Path) -> TaskResult<String> {
    let runtime = runtime_dir(root);
    let mut digest = Sha256::new();
    for value in ["platform", TENANT_ID, RESTRICTED_TENANT_ID] {
        update_digest(&mut digest, value);
    }
    for path in [
        runtime.join("ucs-lab/tls/ucs.crt"),
        runtime.join("ucs-lab/tls/ucs.key"),
        dev_access::ucs_authorities_path(root),
        runtime.join("secrets/ucs-shared.password"),
    ] {
        update_digest(&mut digest, &path.to_string_lossy());
        update_digest(&mut digest, &required_file_sha256(&path)?);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn required_file_sha256(path: &Path) -> TaskResult<String> {
    sha256_file_if_present(path)?
        .ok_or_else(|| format!("required file is missing: {}", path.display()).into())
}

fn parse_dev_state(contents: &[u8]) -> Option<DevState> {
    serde_json::from_slice::<DevState>(contents)
        .ok()
        .filter(|state| state.version == DEV_STATE_VERSION)
}

fn read_dev_state(root: &Path) -> Option<DevState> {
    let path = runtime_dir(root).join(DEV_STATE_FILE);
    match fs::read(&path) {
        Ok(contents) => {
            let state = parse_dev_state(&contents);
            if state.is_none() {
                println!("dev state: invalid; using conservative restart plan");
            }
            state
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            println!("dev state: unreadable ({error}); using conservative restart plan");
            None
        }
    }
}

fn write_dev_state(root: &Path, state: &DevState) -> TaskResult {
    let runtime = runtime_dir(root);
    let destination = runtime.join(DEV_STATE_FILE);
    let temporary = runtime.join(format!(".{DEV_STATE_FILE}.{}.tmp", Uuid::new_v4().simple()));
    let result = (|| -> TaskResult {
        let mut options = OpenOptions::new();
        options.create_new(true).write(true).mode(0o600);
        let mut file = options.open(&temporary)?;
        serde_json::to_writer_pretty(&mut file, state)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, &destination)?;
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o600))?;
        Ok(())
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn runtime_snapshot(root: &Path, dedicated: bool) -> RuntimeSnapshot {
    let expected_workers = if dedicated {
        DEDICATED_WORKERS.as_slice()
    } else {
        STANDARD_WORKERS.as_slice()
    };
    RuntimeSnapshot {
        server: service_running(root, "server"),
        all_workers: expected_workers
            .iter()
            .all(|name| service_running(root, name)),
        any_workers: DEDICATED_WORKERS
            .iter()
            .any(|name| service_running(root, name)),
        all_ucs_labs: UCS_LABS.iter().all(|name| service_running(root, name)),
        any_ucs_labs: UCS_LABS.iter().any(|name| service_running(root, name)),
        web: service_running(root, "web"),
    }
}

fn plan_dev_restart(
    mode: DevMode,
    previous: Option<&DevState>,
    desired: &DevState,
    runtime: RuntimeSnapshot,
) -> DevRestartPlan {
    let fresh = mode == DevMode::Fresh;
    let server_changed = previous.is_none_or(|state| {
        state.server_binary_sha256 != desired.server_binary_sha256
            || state.server_environment_sha256 != desired.server_environment_sha256
    });
    let workers_changed = previous.is_none_or(|state| {
        state.worker_binary_sha256 != desired.worker_binary_sha256
            || state.worker_environment_sha256 != desired.worker_environment_sha256
    });
    let ucs_changed = previous.is_none_or(|state| {
        state.ucs_binary_sha256 != desired.ucs_binary_sha256
            || state.ucs_environment_sha256 != desired.ucs_environment_sha256
            || state.development_access_enabled != desired.development_access_enabled
    });
    let web_changed =
        previous.is_none_or(|state| state.web_environment_sha256 != desired.web_environment_sha256);
    DevRestartPlan {
        server: fresh || !runtime.server || server_changed,
        workers: fresh || !runtime.all_workers || workers_changed,
        ucs_labs: desired.development_access_enabled
            && (fresh || !runtime.all_ucs_labs || ucs_changed),
        stop_disabled_ucs_labs: !desired.development_access_enabled && runtime.any_ucs_labs,
        web: fresh || !runtime.web || web_changed,
    }
}

fn apply_server_plan(
    root: &Path,
    runtime: &Path,
    restart: bool,
    was_running: bool,
    environment: &[(&str, String)],
) -> TaskResult {
    if !restart {
        println!("dev server: reused");
        return Ok(());
    }
    stop_service_group(root, &["server"])?;
    spawn_service(
        root,
        runtime,
        "server",
        root.join("target/debug/erpx-server"),
        std::iter::empty::<&str>(),
        environment.to_vec(),
    )?;
    println!(
        "dev server: {}",
        if was_running { "restarted" } else { "started" }
    );
    Ok(())
}

fn apply_worker_plan(
    root: &Path,
    runtime: &Path,
    restart: bool,
    any_were_running: bool,
    common: &[(&str, String)],
    dedicated: bool,
) -> TaskResult {
    if dedicated {
        infra::validate_wal_spool()?;
    }
    if !restart {
        println!("dev workers: reused");
        return Ok(());
    }
    stop_service_group(root, &DEDICATED_WORKERS)?;
    for spec in worker_specs(dedicated) {
        let mut environment = common.to_vec();
        environment.extend([
            ("ERPX_WORKER_HANDLER", spec.handler.into()),
            ("ERPX_WORKER_ID", spec.worker_id.into()),
            ("ERPX_WORKER_ACTOR_REF", "local:platform-root".into()),
            (
                "ERPX_WORKER_POLL_INTERVAL_MS",
                DEV_WORKER_POLL_INTERVAL_MS.into(),
            ),
        ]);
        if let Some(kind) = spec.projection_kind {
            environment.extend([
                ("ERPX_TENANT_PROJECTION_KIND", kind.into()),
                (
                    "ERPX_TENANT_PROJECTION_CLAIM_KEY_PREFIX",
                    format!("tenant-projection-{kind}"),
                ),
            ]);
        }
        spawn_service(
            root,
            runtime,
            spec.name,
            root.join("target/debug/erpx-worker"),
            ["run"],
            environment,
        )?;
    }
    println!(
        "dev workers: {}",
        if any_were_running {
            "restarted"
        } else {
            "started"
        }
    );
    Ok(())
}

async fn wait_for_fresh_worker_barrier(
    root: &Path,
    common: &[(&str, String)],
    manifest: &FreshRuntimeManifest,
    workers_started_at: chrono::DateTime<Utc>,
    dedicated: bool,
) -> TaskResult {
    let control_write_url = common
        .iter()
        .find_map(|(key, value)| (*key == "ERPX_CONTROL_WRITE_DATABASE_URL").then_some(value))
        .ok_or("fresh worker barrier is missing the control-write database")?;
    let control_read_url = common
        .iter()
        .find_map(|(key, value)| (*key == "ERPX_CONTROL_READ_DATABASE_URL").then_some(value))
        .ok_or("fresh worker barrier is missing the control-read database")?;
    let write = PgPoolOptions::new()
        .max_connections(2)
        .connect(control_write_url)
        .await
        .map_err(|_| "fresh worker barrier could not connect to control-write")?;
    let read = PgPoolOptions::new()
        .max_connections(2)
        .connect(control_read_url)
        .await
        .map_err(|_| "fresh worker barrier could not connect to control-read")?;
    let expected_ids = worker_specs(dedicated)
        .into_iter()
        .map(|spec| Uuid::parse_str(spec.worker_id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "fresh worker catalog contains an invalid worker ID")?;
    let job_prefix = format!("fresh-bootstrap:{}:%", manifest.installation_generation);
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let missing_processes = worker_specs(dedicated)
            .into_iter()
            .filter(|spec| !service_running(root, spec.name))
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        if !missing_processes.is_empty() {
            return Err(format!(
                "fresh worker barrier observed exited processes: {}",
                missing_processes.join(", ")
            )
            .into());
        }

        let ready_heartbeats: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.worker_heartbeats
             WHERE worker_id = ANY($1) AND status='ready' AND last_seen_at >= $2",
        )
        .bind(&expected_ids)
        .bind(workers_started_at)
        .fetch_one(&write)
        .await
        .map_err(|_| "fresh worker heartbeat barrier could not be evaluated")?;

        let jobs: (i64, i64, i64, i64, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
            "SELECT count(*) AS total,
                    count(*) FILTER (WHERE state='succeeded') AS succeeded,
                    count(*) FILTER (WHERE state IN ('retryable','dead_letter')) AS failed,
                    count(*) FILTER (WHERE state='claimed' AND lease_until <= now()) AS expired,
                    max(updated_at) AS newest_updated_at
             FROM erpx_control.tenant_projection_jobs
             WHERE tenant_id=$1 AND holding_ref=$2 AND job_key LIKE $3",
        )
        .bind(&manifest.tenant_id)
        .bind(&manifest.holding_ref)
        .bind(&job_prefix)
        .fetch_one(&write)
        .await
        .map_err(|_| "fresh initial projection barrier could not be evaluated")?;
        if jobs.0 != 22 {
            return Err("fresh initial projection job inventory is incomplete".into());
        }
        if jobs.2 != 0 || jobs.3 != 0 {
            return Err("fresh initial projection jobs failed closed".into());
        }

        let status_checkpoint: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT source_updated_at
             FROM erpx_control.tenant_projection_status_checkpoints
             WHERE projection_name='tenant_projection_status'",
        )
        .fetch_optional(&read)
        .await
        .map_err(|_| "fresh projection-status checkpoint could not be evaluated")?
        .flatten();
        let status_current = jobs
            .4
            .is_some_and(|newest| status_checkpoint.is_some_and(|checkpoint| checkpoint >= newest));
        if ready_heartbeats == expected_ids.len() as i64 && jobs.1 == jobs.0 && status_current {
            write.close().await;
            read.close().await;
            println!(
                "dev fresh worker barrier: {} heartbeats and {} initial projections ready",
                ready_heartbeats, jobs.1
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("fresh worker/checkpoint barrier timed out before Server startup".into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn require_fresh_command_projection_smoke(
    common: &[(&str, String)],
    manifest: &FreshRuntimeManifest,
) -> TaskResult {
    let control_write_url = common
        .iter()
        .find_map(|(key, value)| (*key == "ERPX_CONTROL_WRITE_DATABASE_URL").then_some(value))
        .ok_or("fresh smoke is missing the control-write database")?;
    let control_read_url = common
        .iter()
        .find_map(|(key, value)| (*key == "ERPX_CONTROL_READ_DATABASE_URL").then_some(value))
        .ok_or("fresh smoke is missing the control-read database")?;
    let write = PgPoolOptions::new()
        .max_connections(1)
        .connect(control_write_url)
        .await
        .map_err(|_| "fresh command smoke could not connect to control-write")?;
    let read = PgPoolOptions::new()
        .max_connections(1)
        .connect(control_read_url)
        .await
        .map_err(|_| "fresh command smoke could not connect to control-read")?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let write_evidence: (i64, i64, i64, i64, Option<i64>) = sqlx::query_as(
            "SELECT
                (SELECT count(*) FROM erpx_cqrs_write.command_receipts
                 WHERE context_name='brand' AND status='completed'),
                (SELECT count(*) FROM erpx_cqrs_write.audit_log
                 WHERE context_name='brand'),
                (SELECT count(*) FROM erpx_cqrs_write.outbox_events
                 WHERE context_name='brand' AND publish_error IS NULL),
                (SELECT count(DISTINCT command_id) FROM erpx_cqrs_write.outbox_events
                 WHERE context_name='brand' AND publish_error IS NULL),
                (SELECT max(global_sequence) FROM erpx_cqrs_write.outbox_events
                 WHERE context_name='brand')",
        )
        .fetch_one(&write)
        .await
        .map_err(|_| "fresh command-side smoke evidence could not be read")?;
        if write_evidence.0 != 2
            || write_evidence.1 != 2
            || write_evidence.2 != 3
            || write_evidence.3 != 2
        {
            return Err("fresh neutral-brand command atomicity evidence is invalid".into());
        }
        let read_evidence: (i64, Option<i64>, i64) = sqlx::query_as(
            "SELECT
                (SELECT count(*) FROM erpx_brand_read.profile_projection profile
                 JOIN erpx_cqrs_read.projection_generations generation
                   ON generation.projection_name=profile.projection_name
                  AND generation.generation=profile.generation
                 WHERE generation.status='active' AND profile.scope_key='platform'
                   AND profile.published_revision IS NOT NULL),
                (SELECT max(global_sequence)
                 FROM erpx_cqrs_read.projection_checkpoints
                 WHERE projection_name='brand.profiles'),
                (SELECT count(*) FROM erpx_cqrs_read.projection_receipts
                 WHERE projection_name='brand.profiles')",
        )
        .fetch_one(&read)
        .await
        .map_err(|_| "fresh read-side smoke evidence could not be read")?;
        let projection_current = write_evidence.4.is_some_and(|target| {
            read_evidence
                .1
                .is_some_and(|checkpoint| checkpoint >= target)
        });
        if read_evidence.0 == 1 && read_evidence.2 == 2 && projection_current {
            write.close().await;
            read.close().await;
            println!(
                "dev fresh smoke: typed neutral-brand command -> outbox -> projection -> query checkpoint ready for installation {}",
                manifest.installation_generation
            );
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(
                "fresh command-to-query smoke timed out without write-side fallback".into(),
            );
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn apply_ucs_plan(
    root: &Path,
    plan: DevRestartPlan,
    any_were_running: bool,
    enabled: bool,
) -> TaskResult {
    if plan.stop_disabled_ucs_labs {
        stop_service_group(root, &UCS_LABS)?;
        println!("dev UCS labs: stopped (development access disabled)");
        return Ok(());
    }
    if !enabled {
        println!("dev UCS labs: disabled");
        return Ok(());
    }
    if !plan.ucs_labs {
        println!("dev UCS labs: reused");
        return Ok(());
    }
    stop_service_group(root, &UCS_LABS)?;
    dev_access::start_ucs_lab(root)?;
    println!(
        "dev UCS labs: {}",
        if any_were_running {
            "restarted"
        } else {
            "started"
        }
    );
    Ok(())
}

fn apply_web_plan(
    root: &Path,
    runtime: &Path,
    restart: bool,
    was_running: bool,
    environment: Vec<(&str, String)>,
) -> TaskResult {
    if !restart {
        println!("dev web: reused (Dioxus hot reload remains active)");
        return Ok(());
    }
    let bind_addr = environment
        .iter()
        .find_map(|(key, value)| (*key == "ERPX_DEV_WEB_BIND_ADDR").then_some(value.clone()))
        .unwrap_or_else(|| "127.0.0.1".to_owned());
    stop_service_group(root, &["web"])?;
    spawn_service(
        root,
        runtime,
        "web",
        PathBuf::from("dx"),
        vec![
            "serve".to_owned(),
            "--web".to_owned(),
            "--fullstack".to_owned(),
            "true".to_owned(),
            "--no-default-features".to_owned(),
            "--offline".to_owned(),
            "--debug-symbols".to_owned(),
            "false".to_owned(),
            "-p".to_owned(),
            "erpx-console".to_owned(),
            "--port".to_owned(),
            "3004".to_owned(),
            "--addr".to_owned(),
            bind_addr,
            "--open".to_owned(),
            "false".to_owned(),
        ],
        environment,
    )?;
    println!(
        "dev web: {}",
        if was_running { "restarted" } else { "started" }
    );
    Ok(())
}

fn service_running(root: &Path, name: &str) -> bool {
    read_pid(&runtime_dir(root).join(format!("{name}.pid")))
        .ok()
        .flatten()
        .is_some_and(|pid| recorded_process_matches(name, pid))
}

fn prune_stale_pid_files(root: &Path) -> TaskResult {
    let runtime = runtime_dir(root);
    for name in ["server", "web"]
        .into_iter()
        .chain(DEDICATED_WORKERS)
        .chain(UCS_LABS)
    {
        let path = runtime.join(format!("{name}.pid"));
        if path.exists() && !service_running(root, name) {
            fs::remove_file(path)?;
            println!("dev {name}: removed stale PID record");
        }
    }
    Ok(())
}

fn stop_service_group(root: &Path, names: &[&str]) -> TaskResult {
    let runtime = runtime_dir(root);
    let mut stopped = Vec::new();
    for name in names {
        let pid_path = runtime.join(format!("{name}.pid"));
        if let Some(pid) = read_pid(&pid_path)?
            && recorded_process_matches(name, pid)
        {
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
            stopped.push((*name, pid));
        }
        if pid_path.exists() {
            fs::remove_file(pid_path)?;
        }
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while !stopped.is_empty() && Instant::now() < deadline {
        stopped.retain(|(_, pid)| process_exists(*pid));
        if !stopped.is_empty() {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    if !stopped.is_empty() {
        return Err(format!(
            "timed out stopping ERPX service processes: {}",
            stopped
                .iter()
                .map(|(name, pid)| format!("{name} ({pid})"))
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into());
    }
    Ok(())
}

fn process_exists(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn require_runtime_services(
    root: &Path,
    dedicated: bool,
    development_access_enabled: bool,
) -> TaskResult {
    let worker_names = if dedicated {
        DEDICATED_WORKERS.as_slice()
    } else {
        STANDARD_WORKERS.as_slice()
    };
    let mut required = vec!["server", "web"];
    required.extend_from_slice(worker_names);
    if development_access_enabled {
        required.extend_from_slice(&UCS_LABS);
    }
    let missing = required
        .into_iter()
        .filter(|name| !service_running(root, name))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "ERPX development services exited before health handoff: {}",
            missing.join(", ")
        )
        .into());
    }
    Ok(())
}

fn spawn_service<I, S>(
    root: &Path,
    runtime: &Path,
    name: &str,
    program: PathBuf,
    args: I,
    environment: Vec<(&str, String)>,
) -> TaskResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(runtime.join(format!("{name}.log")))?;
    let error = log.try_clone()?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(root)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(error));
    for (key, value) in environment {
        command.env(key, value);
    }
    let child = command.spawn()?;
    fs::write(runtime.join(format!("{name}.pid")), child.id().to_string())?;
    Ok(())
}

fn status(root: &Path) -> TaskResult {
    let runtime = runtime_dir(root);
    for name in ["server", "ucs-lab"]
        .into_iter()
        .chain(DEDICATED_WORKERS)
        .chain(["web"])
    {
        let pid = read_pid(&runtime.join(format!("{name}.pid")))?;
        match pid {
            Some(pid) if recorded_process_matches(name, pid) => {
                println!("{name}: running (pid {pid})");
            }
            Some(pid) => println!("{name}: stopped (stale pid {pid})"),
            None => println!("{name}: stopped"),
        }
    }
    infra::status(root)?;
    Ok(())
}

fn stop(root: &Path) -> TaskResult {
    let runtime = runtime_dir(root);
    for name in ["web", "ucs-lab"]
        .into_iter()
        .chain(DEDICATED_WORKERS)
        .chain(["worker", "server"])
    {
        let pid_path = runtime.join(format!("{name}.pid"));
        if let Some(pid) = read_pid(&pid_path)? {
            if recorded_process_matches(name, pid) {
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
            if pid_path.exists() {
                fs::remove_file(pid_path)?;
            }
        }
    }
    infra::stop(root)
}

fn read_pid(path: &Path) -> TaskResult<Option<u32>> {
    if !path.exists() {
        return Ok(None);
    }
    let value = fs::read_to_string(path)?;
    Ok(value.trim().parse().ok())
}

fn recorded_process_matches(name: &str, pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let command = String::from_utf8_lossy(&output.stdout);
    recorded_command_matches(name, &command)
}

fn recorded_command_matches(name: &str, command: &str) -> bool {
    match name {
        "server" => command.contains("erpx-server"),
        "ucs-lab" => command.contains("erpx-ucs-lab"),
        "worker" => command.contains("erpx-worker"),
        "web" => command.contains("dx serve"),
        _ if DEDICATED_WORKERS.contains(&name) => command.contains("erpx-worker"),
        _ => false,
    }
}

async fn verify(root: &Path) -> TaskResult {
    let config = DevConfig::load(root)?;
    for (program, args) in [
        ("cargo", vec!["fmt", "--all", "--check"]),
        (
            "cargo",
            vec![
                "clippy",
                "--workspace",
                "--exclude",
                "erpx-android-bridge",
                "--all-targets",
                "--locked",
                "--offline",
                "--",
                "-D",
                "warnings",
            ],
        ),
        (
            "cargo",
            vec![
                "test",
                "--workspace",
                "--exclude",
                "erpx-android-bridge",
                "--locked",
                "--offline",
                "--no-fail-fast",
            ],
        ),
        (
            "cargo",
            vec![
                "check",
                "-p",
                "erpx-console",
                "--no-default-features",
                "--features",
                "web",
                "--locked",
                "--offline",
            ],
        ),
        (
            "cargo",
            vec![
                "check",
                "-p",
                "erpx-console",
                "--no-default-features",
                "--features",
                "server",
                "--locked",
                "--offline",
            ],
        ),
        (
            "dx",
            vec![
                "build",
                "--web",
                "--fullstack",
                "true",
                "--no-default-features",
                "--offline",
                "-p",
                "erpx-console",
                "--debug-symbols",
                "false",
            ],
        ),
    ] {
        run_command(root, program, args)?;
    }
    verify_local_sqlx(root, &config).await?;
    repository_scan(root)?;
    for url in [
        "http://127.0.0.1:50052/health",
        "http://127.0.0.1:50052/ready",
        "http://127.0.0.1:3004",
    ] {
        wait_for_http(url, 5).await?;
    }
    verify_web_ssr_and_bff().await?;
    affected::record_verified_snapshot(root)?;
    println!("ERPX Web verification completed.");
    Ok(())
}

fn verify_platforms(root: &Path) -> TaskResult {
    for args in [
        vec![
            "check",
            "-p",
            "erpx-console",
            "--no-default-features",
            "--features",
            "desktop",
            "--locked",
            "--offline",
        ],
        vec![
            "check",
            "-p",
            "erpx-console",
            "--no-default-features",
            "--features",
            "mobile",
            "--locked",
            "--offline",
        ],
        vec![
            "check",
            "-p",
            "erpx-console",
            "--target",
            "aarch64-apple-ios",
            "--no-default-features",
            "--features",
            "mobile",
            "--locked",
            "--offline",
        ],
        vec![
            "check",
            "-p",
            "erpx-console",
            "--target",
            "aarch64-linux-android",
            "--no-default-features",
            "--features",
            "mobile",
            "--locked",
            "--offline",
        ],
        vec![
            "check",
            "-p",
            "erpx-android-bridge",
            "--locked",
            "--offline",
        ],
        vec![
            "check",
            "-p",
            "erpx-android-bridge",
            "--target",
            "aarch64-linux-android",
            "--locked",
            "--offline",
        ],
    ] {
        run_command(root, "cargo", args)?;
    }
    println!("Deferred Desktop/Mobile platform checks completed.");
    Ok(())
}

async fn verify_web_ssr_and_bff() -> TaskResult {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let redirect_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let protected = redirect_client
        .get("http://127.0.0.1:3004/platform/tenants?ignored=1")
        .send()
        .await?;
    if protected.status() != reqwest::StatusCode::TEMPORARY_REDIRECT
        || protected
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            != Some("/login?return_to=/platform/tenants")
        || protected
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok())
            != Some("private, no-store")
    {
        return Err("anonymous protected page did not redirect to canonical login".into());
    }
    let legacy_login = redirect_client
        .get("http://127.0.0.1:3004/auth/sign-in")
        .send()
        .await?;
    if legacy_login.status() != reqwest::StatusCode::TEMPORARY_REDIRECT
        || legacy_login
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            != Some("/login")
    {
        return Err("legacy sign-in path did not redirect to /login".into());
    }
    let sign_in = client
        .get("http://127.0.0.1:3004/login")
        .header(reqwest::header::ACCEPT_LANGUAGE, "th")
        .send()
        .await?;
    if !sign_in.status().is_success() {
        return Err(format!("SSR sign-in returned {}", sign_in.status()).into());
    }
    let sign_in_html = sign_in.text().await?;
    if !sign_in_html.contains("เข้าสู่ระบบ · ERPX Platform")
        || sign_in_html.contains("<main class=\"locale-bootstrap-surface\"")
    {
        return Err("SSR sign-in did not render the localized route content".into());
    }
    let normalized_html = sign_in_html.to_ascii_lowercase();
    for forbidden in [
        "access_token",
        "refresh_token",
        "bearer ",
        "localhost:50052",
        "127.0.0.1:50051",
    ] {
        if normalized_html.contains(forbidden) {
            return Err(format!("SSR sign-in exposed forbidden Web material {forbidden}").into());
        }
    }

    let session = client
        .get("http://127.0.0.1:3004/api/bff/v1/session")
        .send()
        .await?;
    if !session.status().is_success() {
        return Err(format!("BFF session snapshot returned {}", session.status()).into());
    }
    if session
        .headers()
        .get(reqwest::header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        != Some("private, no-store")
    {
        return Err("BFF session snapshot is missing private, no-store".into());
    }
    let snapshot = session.text().await?;
    let normalized = snapshot.to_ascii_lowercase();
    for forbidden in ["access_token", "refresh_token", "password", "bearer"] {
        if normalized.contains(forbidden) {
            return Err(format!("BFF session snapshot exposed forbidden field {forbidden}").into());
        }
    }
    let value: serde_json::Value = serde_json::from_str(&snapshot)?;
    if value
        .get("authenticated")
        .and_then(serde_json::Value::as_bool)
        != Some(false)
    {
        return Err("anonymous BFF session snapshot was not signed out".into());
    }

    let upgrade = client
        .post("http://127.0.0.1:3004/auth/bff/upgrade")
        .send()
        .await?;
    if upgrade.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Err(format!(
            "anonymous legacy-session upgrade returned {}, expected 401",
            upgrade.status()
        )
        .into());
    }

    let spoofed_grpc = client
        .post("http://127.0.0.1:3004/api/bff/v1/grpc/erpx.platform.v1.FinanceQueryService/ListFinanceCharts")
        .header(reqwest::header::ORIGIN, "http://127.0.0.1:3004")
        .header(reqwest::header::CONTENT_TYPE, "application/grpc-web+proto")
        .header("x-erpx-access-token", "browser-spoof")
        .header("x-erpx-actor", "browser-spoof")
        .header("x-erpx-session", Uuid::new_v4().to_string())
        .body(Vec::<u8>::new())
        .send()
        .await?;
    if spoofed_grpc.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Err(format!(
            "anonymous spoofed gRPC request returned {}, expected 401",
            spoofed_grpc.status()
        )
        .into());
    }

    let csrf_login = client
        .post("http://127.0.0.1:3004/api/bff/v1/auth/password")
        .json(&serde_json::json!({
            "principal_ref": "local:probe",
            "password": "not-a-real-password",
            "device_label": "runtime probe",
        }))
        .send()
        .await?;
    if csrf_login.status() != reqwest::StatusCode::BAD_REQUEST {
        return Err(format!(
            "origin-less login mutation returned {}, expected 400",
            csrf_login.status()
        )
        .into());
    }
    Ok(())
}

async fn verify_local_sqlx(root: &Path, config: &DevConfig) -> TaskResult {
    config.validate_local()?;
    let stamp = Uuid::new_v4().simple().to_string();
    let database_keys = [
        format!("erpx_verify_cw_{stamp}"),
        format!("erpx_verify_cr_{stamp}"),
        format!("erpx_verify_tw_{stamp}"),
        format!("erpx_verify_tr_{stamp}"),
        format!("erpx_verify_dir_{stamp}"),
        format!("erpx_verify_com_{stamp}"),
        format!("erpx_verify_mkt_{stamp}"),
        format!("erpx_verify_sup_{stamp}"),
    ];
    let physical_write_key = format!("erpx_verify_ptw_{stamp}");
    let physical_read_key = format!("erpx_verify_ptr_{stamp}");
    let organization_write_key = format!("erpx_verify_otw_{stamp}");
    let organization_read_key = format!("erpx_verify_otr_{stamp}");
    let readiness_write_key = format!("erpx_rdy_w_{stamp}");
    let readiness_read_key = format!("erpx_rdy_r_{stamp}");
    let (admin_url, verification_database_template) = if config.port == 55_432 {
        (
            infra::admin_runtime_database_url(root)?,
            infra::admin_runtime_database_template(root)?,
        )
    } else {
        (config.database_url("postgres"), config.database_template())
    };
    create_disposable_databases(config, &admin_url, &database_keys).await?;

    // The ignored SQLx acceptance suite deliberately performs DDL, creates
    // temporary roles, and seeds rows beneath FORCE RLS to test the
    // application boundary. Keep that harness on the dedicated admin role;
    // the detached ERPX runtime continues to use the least-privilege app role.
    let verification_url =
        |database: &str| verification_database_template.replace("{database}", database);
    let control_write_url = verification_url(&database_keys[0]);
    let control_read_url = verification_url(&database_keys[1]);
    let tenant_write_url = verification_url(&database_keys[2]);
    let tenant_read_url = verification_url(&database_keys[3]);
    let directory_url = verification_url(&database_keys[4]);
    let commercial_url = verification_url(&database_keys[5]);
    let marketing_url = verification_url(&database_keys[6]);
    let support_url = verification_url(&database_keys[7]);
    let test_environment = vec![
        ("ERPX_TEST_CONTROL_WRITE_DATABASE_URL", control_write_url),
        ("ERPX_TEST_CONTROL_READ_DATABASE_URL", control_read_url),
        (
            "ERPX_TEST_TENANT_WRITE_DATABASE_URL",
            tenant_write_url.clone(),
        ),
        (
            "ERPX_TEST_TENANT_READ_DATABASE_URL",
            tenant_read_url.clone(),
        ),
        (
            "ERPX_TEST_DIRECTORY_RBAC_DATABASE_URL",
            directory_url.clone(),
        ),
        ("ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL", directory_url),
        ("ERPX_TEST_COMMERCIAL_DATABASE_URL", commercial_url),
        ("ERPX_TEST_MARKETING_SCHEDULER_DATABASE_URL", marketing_url),
        ("ERPX_SUPPORT_TEST_DATABASE_URL", support_url),
        ("ERPX_TEST_ADMIN_DATABASE_URL", admin_url.clone()),
        ("ERPX_G13_DISPOSABLE_ADMIN_DATABASE_URL", admin_url.clone()),
        (
            "ERPX_TEST_STRIPE_SEMANTIC_ADMIN_DATABASE_URL",
            admin_url.clone(),
        ),
        (
            "ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE",
            verification_database_template,
        ),
        (
            "ERPX_TEST_PHYSICAL_TENANT_WRITE_KEY",
            physical_write_key.clone(),
        ),
        (
            "ERPX_TEST_PHYSICAL_TENANT_READ_KEY",
            physical_read_key.clone(),
        ),
        (
            "ERPX_TEST_ORGANIZATION_TENANT_WRITE_KEY",
            organization_write_key.clone(),
        ),
        (
            "ERPX_TEST_ORGANIZATION_TENANT_READ_KEY",
            organization_read_key.clone(),
        ),
        ("ERPX_TEST_READINESS_SUFFIX", stamp.clone()),
        (
            "ERPX_TEST_G9_INVENTORY_WRITE_DATABASE_URL",
            tenant_write_url.clone(),
        ),
        (
            "ERPX_TEST_G9_INVENTORY_READ_DATABASE_URL",
            tenant_read_url.clone(),
        ),
        (
            "ERPX_TEST_G9_LOGISTICS_WRITE_DATABASE_URL",
            tenant_write_url.clone(),
        ),
        (
            "ERPX_TEST_G9_LOGISTICS_READ_DATABASE_URL",
            tenant_read_url.clone(),
        ),
        (
            "ERPX_TEST_G9_PROCUREMENT_WRITE_DATABASE_URL",
            tenant_write_url.clone(),
        ),
        (
            "ERPX_TEST_G9_PROCUREMENT_READ_DATABASE_URL",
            tenant_read_url.clone(),
        ),
        ("ERPX_TEST_G9_SALES_WRITE_DATABASE_URL", tenant_write_url),
        ("ERPX_TEST_G9_SALES_READ_DATABASE_URL", tenant_read_url),
        (
            "ERPX_TEST_CALLBACK_SECRET",
            "local-verification-callback-secret".into(),
        ),
    ];
    let test_result = run_command_with_env(
        root,
        "cargo",
        [
            "test",
            "--workspace",
            "--exclude",
            "erpx-android-bridge",
            "--locked",
            "--offline",
            "--no-fail-fast",
            "--",
            "--ignored",
            "--test-threads=1",
            "--skip",
            "real_postgres_minio_backup_restore_and_cleanup_are_authoritative",
            "--skip",
            "real_postgres_physical_pitr_capture_restore_and_cleanup_are_authoritative",
            "--skip",
            "g14_external_certification_preflight_fails_closed_when_not_ready",
        ],
        &test_environment,
    );

    let mut cleanup_keys = database_keys.into_iter().collect::<Vec<_>>();
    cleanup_keys.extend([
        physical_write_key,
        physical_read_key,
        organization_write_key,
        organization_read_key,
        readiness_write_key,
        readiness_read_key,
    ]);
    let cleanup_result = drop_disposable_databases(&admin_url, &cleanup_keys).await;
    test_result?;
    cleanup_result?;
    println!(
        "Local SQLx verification passed; credentialed MinIO/PITR and LDAP/AD/UCS certification gates remain explicitly excluded."
    );
    Ok(())
}

async fn create_disposable_databases(
    config: &DevConfig,
    admin_url: &str,
    database_keys: &[String],
) -> TaskResult {
    let mut admin = PgConnection::connect(admin_url).await?;
    for database_key in database_keys {
        validate_disposable_database_key(database_key)?;
        admin
            .execute(
                format!(
                    "CREATE DATABASE {} OWNER {}",
                    quote_postgres_identifier(database_key),
                    quote_postgres_identifier(&config.user),
                )
                .as_str(),
            )
            .await?;
    }
    Ok(())
}

async fn drop_disposable_databases(admin_url: &str, database_keys: &[String]) -> TaskResult {
    let mut admin = PgConnection::connect(admin_url).await?;
    for database_key in database_keys {
        validate_disposable_database_key(database_key)?;
        admin
            .execute(format!("DROP DATABASE IF EXISTS \"{database_key}\" WITH (FORCE)").as_str())
            .await?;
    }
    Ok(())
}

fn quote_postgres_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn validate_disposable_database_key(database_key: &str) -> TaskResult {
    if !(database_key.starts_with("erpx_verify_") || database_key.starts_with("erpx_rdy_"))
        || !database_key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(format!("refusing unsafe disposable database key: {database_key}").into());
    }
    Ok(())
}

fn repository_scan(root: &Path) -> TaskResult {
    for forbidden in ["package.json", "bun.lock", "next.config.ts", "node_modules"] {
        if root.join(forbidden).exists() {
            return Err(format!("forbidden cutover artifact remains: {forbidden}").into());
        }
    }
    let output = Command::new("rg")
        .args([
            "-n",
            "-i",
            "--hidden",
            "-g",
            "!.git/**",
            "-g",
            "!target/**",
            "-g",
            "!.runtime/**",
            "folio|ERPX_G13_SOURCE_|legacy_bridge|FolioSourceExtractor",
            "apps",
            "crates",
            "fixtures",
            "adr",
        ])
        .current_dir(root)
        .output()?;
    if output.status.success() {
        return Err(format!(
            "forbidden legacy references remain:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
        .into());
    }
    Ok(())
}

fn run_command<I, S>(root: &Path, program: &str, args: I) -> TaskResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .status()?;
    if !status.success() {
        return Err(format!("{program} command failed with {status}").into());
    }
    Ok(())
}

fn run_command_with_env<I, S>(
    root: &Path,
    program: &str,
    args: I,
    environment: &[(&str, String)],
) -> TaskResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new(program);
    command.args(args).current_dir(root);
    for (key, value) in environment {
        command.env(key, value);
    }
    let status = command.status()?;
    if !status.success() {
        return Err(format!("{program} command failed with {status}").into());
    }
    Ok(())
}

async fn wait_for_http(url: &str, seconds: u64) -> TaskResult {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;
    for _ in 0..seconds {
        if let Ok(response) = client.get(url).send().await
            && response.status().is_success()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(format!("timed out waiting for {url}").into())
}

async fn wait_for_ucs_authorities(root: &Path, seconds: u64) -> TaskResult {
    let certificate = runtime_dir(root).join("ucs-lab/tls/ucs.crt");
    for _ in 0..seconds {
        let mut ready = true;
        for authority in ["platform", TENANT_ID, RESTRICTED_TENANT_ID] {
            let url = format!("https://127.0.0.1:15443/authorities/{authority}/healthz");
            if !Command::new("/usr/bin/curl")
                .args([
                    "--silent",
                    "--fail",
                    "--connect-timeout",
                    "1",
                    "--max-time",
                    "3",
                ])
                .arg("--cacert")
                .arg(&certificate)
                .arg(url)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
            {
                ready = false;
                break;
            }
        }
        if ready {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err("timed out waiting for all logical UCS authorities on the shared listener".into())
}

fn runtime_dir(root: &Path) -> PathBuf {
    root.join(".runtime/erpx")
}

fn secret_file(path: &Path) -> TaskResult<String> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(format!("refusing unsafe secret file {}", path.display()).into());
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        return Ok(fs::read_to_string(path)?.trim().to_owned());
    }
    let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut options = OpenOptions::new();
    options.create_new(true).write(true).mode(0o600);
    let mut file = options.open(path)?;
    file.write_all(secret.as_bytes())?;
    Ok(secret)
}

fn sha256_file_if_present(path: &Path) -> TaskResult<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(Some(format!("{:x}", digest.finalize())))
}

fn read_dotenv(path: &Path) -> TaskResult<BTreeMap<String, String>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let mut values = BTreeMap::new();
    for line in fs::read_to_string(path)?.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        values.insert(
            key.trim().to_owned(),
            value.trim().trim_matches(['"', '\'']).to_owned(),
        );
    }
    Ok(values)
}

fn configured_development_tailnet_ip(root: &Path) -> TaskResult<Option<IpAddr>> {
    let values = read_dotenv(&root.join(".env.local"))?;
    let configured = env::var(DEVELOPMENT_TAILSCALE_IP_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            values
                .get(DEVELOPMENT_TAILSCALE_IP_ENV)
                .filter(|value| !value.trim().is_empty())
                .cloned()
        });
    configured
        .map(|value| {
            parse_development_tailnet_ip(&value).ok_or_else(|| {
                "ERPX_DEV_TAILSCALE_IP must be an IPv4 address in 100.64.0.0/10".into()
            })
        })
        .transpose()
}

fn parse_development_tailnet_ip(value: &str) -> Option<IpAddr> {
    let address = IpAddr::from_str(value.trim()).ok()?;
    match address {
        IpAddr::V4(address) => {
            let [first, second, _, _] = address.octets();
            (first == 100 && (64..=127).contains(&second)).then_some(IpAddr::V4(address))
        }
        IpAddr::V6(_) => None,
    }
}

fn value_or(values: &BTreeMap<String, String>, key: &str, default: &str) -> String {
    values
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

fn required_value(values: &BTreeMap<String, String>, key: &str) -> TaskResult<String> {
    values
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| format!("{key} is required for local ERPX development").into())
}

fn endpoint_host(endpoint: &str) -> Option<&str> {
    let without_scheme = endpoint.split_once("://")?.1;
    let authority = without_scheme.split('/').next()?;
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, value)| value);
    if host_port.starts_with('[') {
        return host_port
            .strip_prefix('[')
            .and_then(|value| value.split_once(']'))
            .map(|(host, _)| host);
    }
    Some(host_port.split(':').next().unwrap_or(host_port))
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || IpAddr::from_str(host).is_ok_and(|address| address.is_loopback())
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn sample_dev_state() -> DevState {
        DevState {
            version: DEV_STATE_VERSION,
            server_binary_sha256: "server".into(),
            worker_binary_sha256: "worker".into(),
            ucs_binary_sha256: "ucs".into(),
            server_environment_sha256: "server-env".into(),
            worker_environment_sha256: "worker-env".into(),
            ucs_environment_sha256: "ucs-env".into(),
            web_environment_sha256: "web-env".into(),
            development_access_enabled: true,
        }
    }

    fn healthy_runtime() -> RuntimeSnapshot {
        RuntimeSnapshot {
            server: true,
            all_workers: true,
            any_workers: true,
            all_ucs_labs: true,
            any_ucs_labs: true,
            web: true,
        }
    }

    fn sample_fresh_runtime_manifest() -> FreshRuntimeManifest {
        let n8n_config_sha256 = "ab".repeat(32);
        FreshRuntimeManifest {
            version: 1,
            state: "prepared".into(),
            prepared_at: "2026-08-05T01:02:03Z".into(),
            installation_generation: Uuid::parse_str("4fbf88d8-ea38-51b6-93f6-8cdb6c7585ec")
                .expect("generation"),
            installation_prefix: "next".into(),
            tenant_id: "a5c86431-c0b8-58ea-bb5f-5af2f940918d".into(),
            holding_ref: "ad093a73-d3d0-5532-8a03-2f2a29ca0f3c".into(),
            control_write_database: "next_control_write".into(),
            control_read_database: "next_control_read".into(),
            tenant_write_database: "next_1234567890ab_tenant_write".into(),
            tenant_read_database: "next_1234567890ab_tenant_read".into(),
            object_namespace: "next-tenant-objects".into(),
            v1_service_authority: "typed".into(),
            rollback: serde_json::json!({
                "n8n_config_sha256": n8n_config_sha256.clone(),
                "retained_resource_authority": "offline_non_authoritative",
                "write_policy": "isolated_no_merge"
            }),
            n8n_config_sha256,
        }
    }

    #[test]
    fn fresh_runtime_manifest_requires_exact_typed_no_merge_contract() {
        let valid = sample_fresh_runtime_manifest();
        assert!(valid.validate().is_ok());

        let mut legacy = valid.clone();
        legacy.v1_service_authority = "legacy".into();
        assert!(legacy.validate().is_err());

        let mut mismatched_database = valid.clone();
        mismatched_database.tenant_read_database = "retained_tenant_read".into();
        assert!(mismatched_database.validate().is_err());

        let mut merge_policy = valid;
        merge_policy.rollback["write_policy"] = serde_json::json!("merge");
        assert!(merge_policy.validate().is_err());
    }

    #[test]
    fn development_access_targets_the_active_runtime_database() {
        let retained = development_access_target_from_manifest(None);
        assert_eq!(retained.control_write_database, DATABASES[0]);
        assert_eq!(retained.control_read_database, DATABASES[1]);
        assert_eq!(
            retained.controller_target_tenant,
            erpx_identity::DEVELOPMENT_CONTROLLER_TARGET_TENANT
        );
        assert!(!retained.fresh_state);

        let manifest = sample_fresh_runtime_manifest();
        let fresh = development_access_target_from_manifest(Some(&manifest));
        assert_eq!(
            fresh.control_write_database,
            manifest.control_write_database
        );
        assert_eq!(fresh.control_read_database, manifest.control_read_database);
        assert_eq!(fresh.controller_target_tenant, manifest.tenant_id);
        assert!(fresh.fresh_state);
    }

    #[test]
    fn development_starts_the_complete_direct_v1_worker_catalog() {
        let standard = worker_specs(false);
        assert_eq!(standard.len(), 50);
        assert_eq!(worker_specs(true).len(), 54);
        assert_eq!(
            standard.iter().map(|spec| spec.name).collect::<Vec<_>>(),
            STANDARD_WORKERS
        );
        assert_eq!(
            worker_specs(true)
                .iter()
                .map(|spec| spec.name)
                .collect::<Vec<_>>(),
            DEDICATED_WORKERS
        );
        let projection_kinds = standard
            .iter()
            .filter_map(|spec| spec.projection_kind)
            .collect::<BTreeSet<_>>();
        assert_eq!(projection_kinds.len(), 39);
        assert!(projection_kinds.contains("quality"));
        assert!(projection_kinds.contains("identity_access_principals"));
        assert!(projection_kinds.contains("commercial_entitlements"));
        assert!(projection_kinds.contains("marketing_lead_candidates"));
        assert!(projection_kinds.contains("maintenance_asset_facts"));
        assert!(projection_kinds.contains("people_payroll_payroll_runs"));
        assert!(projection_kinds.contains("pos_refunds"));
        assert!(projection_kinds.contains("finance_charts"));
        assert!(projection_kinds.contains("legal_access_history"));
        assert!(projection_kinds.contains("support_capabilities"));
        assert!(projection_kinds.contains("operations_exceptions"));
        assert!(projection_kinds.contains("operations_source_signals"));
        assert!(projection_kinds.contains("operations_work_queue_policies"));
        assert!(standard.iter().any(|spec| {
            spec.handler == "operations_source_ingestion" && spec.projection_kind.is_none()
        }));
        assert!(standard.iter().any(|spec| {
            spec.handler == "customer360_control_customers_projection"
                && spec.projection_kind.is_none()
        }));
        assert!(standard.iter().any(|spec| {
            spec.handler == "customer360_tenant_activity_projection"
                && spec.projection_kind.is_none()
        }));
        assert!(
            standard
                .iter()
                .filter(|spec| spec.projection_kind.is_some())
                .all(|spec| spec.handler == "tenant_projection_scheduler")
        );
        for legacy in [
            "asset_maintenance",
            "legacy_legal_documents",
            "operations",
            "finance_banking",
            "finance_chart",
            "legacy_finance_subledger",
            "finance_close",
            "finance_thai_tax",
            "scm",
            "pos",
            "hr",
            "sales",
            "marketing",
        ] {
            assert!(!projection_kinds.contains(legacy));
        }
        assert_eq!(
            standard
                .iter()
                .map(|spec| spec.name)
                .collect::<BTreeSet<_>>()
                .len(),
            standard.len()
        );
        assert_eq!(
            standard
                .iter()
                .map(|spec| spec.worker_id)
                .collect::<BTreeSet<_>>()
                .len(),
            standard.len()
        );
        assert!(
            worker_specs(true)
                .iter()
                .all(|spec| recorded_command_matches(spec.name, "/runtime/erpx-worker run"))
        );
    }

    #[test]
    fn development_cli_modes_are_exact() {
        assert_eq!(
            parse_dev_mode(&args(&["dev", "--detach"])),
            Some(DevMode::Warm)
        );
        assert_eq!(
            parse_dev_mode(&args(&["dev", "--detach", "--fresh"])),
            Some(DevMode::Fresh)
        );
        assert_eq!(parse_dev_mode(&args(&["dev"])), None);
        assert_eq!(parse_dev_mode(&args(&["dev", "--detach", "--reset"])), None);
    }

    #[test]
    fn valid_dev_state_round_trips_and_wrong_versions_fail_closed() {
        let state = sample_dev_state();
        let encoded = serde_json::to_vec(&state).expect("serialize dev state");
        assert_eq!(parse_dev_state(&encoded), Some(state.clone()));
        let mut stale = state;
        stale.version += 1;
        let encoded = serde_json::to_vec(&stale).expect("serialize stale dev state");
        assert_eq!(parse_dev_state(&encoded), None);
        assert_eq!(parse_dev_state(b"not-json"), None);
    }

    #[test]
    fn unchanged_healthy_runtime_is_reused() {
        let state = sample_dev_state();
        assert_eq!(
            plan_dev_restart(DevMode::Warm, Some(&state), &state, healthy_runtime()),
            DevRestartPlan::default()
        );
    }

    #[test]
    fn changed_binary_or_environment_restarts_only_its_service_group() {
        let previous = sample_dev_state();
        let mut desired = previous.clone();
        desired.server_binary_sha256 = "new-server".into();
        assert_eq!(
            plan_dev_restart(DevMode::Warm, Some(&previous), &desired, healthy_runtime()),
            DevRestartPlan {
                server: true,
                ..DevRestartPlan::default()
            }
        );

        let mut desired = previous.clone();
        desired.worker_environment_sha256 = "new-worker-env".into();
        assert_eq!(
            plan_dev_restart(DevMode::Warm, Some(&previous), &desired, healthy_runtime()),
            DevRestartPlan {
                workers: true,
                ..DevRestartPlan::default()
            }
        );

        let mut desired = previous.clone();
        desired.web_environment_sha256 = "new-web-env".into();
        assert_eq!(
            plan_dev_restart(DevMode::Warm, Some(&previous), &desired, healthy_runtime()),
            DevRestartPlan {
                web: true,
                ..DevRestartPlan::default()
            }
        );
    }

    #[test]
    fn stale_or_missing_services_are_started_conservatively() {
        let state = sample_dev_state();
        let runtime = RuntimeSnapshot {
            server: false,
            all_workers: false,
            any_workers: false,
            all_ucs_labs: false,
            any_ucs_labs: false,
            web: false,
        };
        assert_eq!(
            plan_dev_restart(DevMode::Warm, Some(&state), &state, runtime),
            DevRestartPlan {
                server: true,
                workers: true,
                ucs_labs: true,
                web: true,
                ..DevRestartPlan::default()
            }
        );
    }

    #[test]
    fn disabling_development_access_stops_existing_ucs_labs() {
        let previous = sample_dev_state();
        let mut desired = previous.clone();
        desired.development_access_enabled = false;
        desired.ucs_environment_sha256 = "disabled".into();
        assert_eq!(
            plan_dev_restart(DevMode::Warm, Some(&previous), &desired, healthy_runtime()),
            DevRestartPlan {
                stop_disabled_ucs_labs: true,
                ..DevRestartPlan::default()
            }
        );
    }

    #[test]
    fn fresh_mode_restarts_every_enabled_group() {
        let state = sample_dev_state();
        assert_eq!(
            plan_dev_restart(DevMode::Fresh, Some(&state), &state, healthy_runtime()),
            DevRestartPlan {
                server: true,
                workers: true,
                ucs_labs: true,
                web: true,
                ..DevRestartPlan::default()
            }
        );
    }

    #[test]
    fn fixture_digest_changes_with_seed_inputs() {
        let baseline = fixture_digest();
        let mut contexts = DEMO_CONTEXTS;
        contexts[0].3 += 1;
        assert_ne!(
            baseline,
            fixture_digest_for(&contexts, &DEMO_IDENTITIES, PLATFORM_PERMISSIONS)
        );
        let mut identities = DEMO_IDENTITIES;
        identities[0].2 += 1;
        assert_ne!(
            baseline,
            fixture_digest_for(&DEMO_CONTEXTS, &identities, PLATFORM_PERMISSIONS)
        );
        let mut permissions = PLATFORM_PERMISSIONS.to_vec();
        permissions.push("test.permission");
        assert_ne!(
            baseline,
            fixture_digest_for(&DEMO_CONTEXTS, &DEMO_IDENTITIES, &permissions)
        );
    }

    #[test]
    fn environment_fingerprint_is_order_independent_but_value_sensitive() {
        let first = vec![("B", "2".to_owned()), ("A", "1".to_owned())];
        let reordered = vec![("A", "1".to_owned()), ("B", "2".to_owned())];
        let changed = vec![("A", "1".to_owned()), ("B", "3".to_owned())];
        assert_eq!(environment_sha256(&first), environment_sha256(&reordered));
        assert_ne!(environment_sha256(&first), environment_sha256(&changed));
    }

    #[test]
    fn reset_accepts_only_loopback_hosts() {
        for host in ["localhost", "127.0.0.1", "::1"] {
            assert!(is_loopback_host(host));
        }
        for host in ["db.example.test", "10.0.0.9", "0.0.0.0"] {
            assert!(!is_loopback_host(host));
        }
    }

    #[test]
    fn tailscale_development_ip_accepts_only_the_cgnat_range() {
        assert_eq!(
            parse_development_tailnet_ip("100.97.9.56"),
            Some(IpAddr::from_str("100.97.9.56").expect("address"))
        );
        for address in [
            "100.63.255.255",
            "100.128.0.1",
            "0.0.0.0",
            "192.168.1.10",
            "fd7a:115c:a1e0::1",
            "tail.example.test",
        ] {
            assert_eq!(
                parse_development_tailnet_ip(address),
                None,
                "accepted {address}"
            );
        }
    }

    #[test]
    fn endpoint_parser_extracts_loopback_hosts() {
        assert_eq!(endpoint_host("http://127.0.0.1:9000"), Some("127.0.0.1"));
        assert_eq!(endpoint_host("https://[::1]:9000"), Some("::1"));
        assert_eq!(
            endpoint_host("https://minio.example.test"),
            Some("minio.example.test")
        );
    }

    #[test]
    fn reset_schema_allowlist_rejects_unknown_names() {
        let allowed = ["public", "n8n", "folio"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert!(validate_schemas(&allowed).is_ok());
        let unknown = ["public", "customer_data"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert!(validate_schemas(&unknown).is_err());
    }

    #[test]
    fn reset_bucket_allowlist_preserves_erpx_namespace() {
        let allowed = [LEGACY_BUCKET, ERPX_BUCKET]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert!(validate_buckets(&allowed).is_ok());
        let unknown = ["unrelated-bucket"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert!(validate_buckets(&unknown).is_err());
    }
}
