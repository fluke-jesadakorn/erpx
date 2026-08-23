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
    access::{RiskTier, RoleAssignment},
    contracts::AuthorityScope as Scope,
    holding_administration::HoldingRegistryBinding,
};
use erpx_identity::{
    AuthenticatedAuthority, AuthenticatedPrincipalKind, AuthenticatedRealm, SqlxIdentityStore,
    apply_identity_migration,
};
use erpx_infrastructure::{
    ControlPlanePools, FRESH_INITIAL_HOLDING_PROJECTIONS, HoldingHoldingBinding,
    HoldingProjectionSchedulerRepository, HoldingStoreRegistration, ProjectionJobKind,
    ProjectionPolicy, ProjectionScheduleRequest, ProjectionSchedulerAuthorization,
    SqlxControlPlane, SqlxHoldingAdministrationStore, SqlxHoldingStoreRegistry,
    holding_holding_scope_id,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Connection, Executor, PgConnection, PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;

mod affected;
mod architecture;
mod dev_access;
mod directory_lab;
mod identity_access_fixture;
mod infra;
mod wasm;
mod web_e2e;
mod web_toolchain;

const RESET_CONFIRMATION: &str = "RESET_ERPX_DEV";
const RECONFIGURE_LOCAL_CONFIRMATION: &str = "RECONFIGURE_ERPX_LOCAL";
const ROOT_PASSWORD_RESET_CONFIRMATION: &str = "RESET_ERPX_PLATFORM_ROOT";
const LEGACY_DATABASE: &str = "folio_db";
const LEGACY_BUCKET: &str = "folio-storage";
const ERPX_BUCKET: &str = "erpx-demo";
const WORKER_ID: &str = "1db95d2e-6aa6-5de7-81b2-e650a66be777";
// Runtime fixtures use the same UUID-only holding identity contract as the
// production boundary. Human-readable names remain data-profile/display
// metadata and must never become a routing identity.
const HOLDING_ID: &str = "37098a2d-0d13-5b0e-8a37-f2df2fbe8cba";
const DATA_PROFILE_KEY: &str = "demo-holding-v6";
const RESTRICTED_HOLDING_ID: &str = "4017ec64-6620-50e2-bfdc-14d0a9b3c414";
const RESTRICTED_DATA_PROFILE_KEY: &str = "beacon-holding-v6";
const FIXTURE_VERSION: &str = "erpx-demo-v7";
const DEV_REQUIRED_HOLDING_PROJECTIONS: [&str; 5] = [
    "operations.exceptions",
    "operations.source_signals",
    "operations.work_queue_policies",
    "operations.lifecycle-records",
    "item_directory",
];
const DEV_STATE_VERSION: u32 = 3;
const WEB_LAUNCH_CONTRACT_VERSION: u32 = 4;
const WEB_SESSION_CACHE_CONTRACT_VERSION: u32 = 1;
const DEV_STATE_FILE: &str = "dev-state.json";
const FRESH_RUNTIME_MANIFEST_FILE: &str = "cutover/fresh-state.json";
const DEV_WORKER_POLL_INTERVAL_MS: &str = "5000";
const DEVELOPMENT_TAILSCALE_FQDN_ENV: &str = "ERPX_DEV_TAILSCALE_FQDN";
const LEGACY_DEVELOPMENT_TAILSCALE_IP_ENV: &str = "ERPX_DEV_TAILSCALE_IP";
const DATABASES: [&str; 6] = [
    "erpx_clean_control_write",
    "erpx_clean_control_read",
    "erpx_clean_holding_write",
    "erpx_clean_holding_read",
    "erpx_clean_restricted_holding_write",
    "erpx_clean_restricted_holding_read",
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
const PRESERVED_SCHEMAS: [&str; 1] = ["public"];
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
    "platform.holding.backup",
    "platform.holding.provision",
    "platform.holding.read",
    "platform.holding.recovery",
    "platform.holding.suspend",
    "holding.projection.execute",
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

const STANDARD_WORKERS: [&str; 96] = [
    "support-worker",
    "agent-holding-operations-worker",
    "commercial-catalog-projector",
    "wave5-control-projector",
    "quality-projector",
    "quality-lifecycle-records-projector",
    "identity-access-principals-projector",
    "identity-access-roles-projector",
    "identity-access-governance-projector",
    "organization-projector",
    "organization-sharing-projector",
    "commercial-subscriptions-projector",
    "commercial-entitlements-projector",
    "commercial-lifecycle-records-projector",
    "marketing-lead-candidates-projector",
    "marketing-campaign-budgets-projector",
    "marketing-business-records-projector",
    "crm-leads-projector",
    "crm-opportunities-projector",
    "crm-business-records-projector",
    "procurement-purchase-orders-projector",
    "procurement-workflow-records-projector",
    "procurement-business-records-projector",
    "inventory-stock-items-projector",
    "inventory-lifecycle-records-projector",
    "customer-authority-business-records-projector",
    "customer360-owned-records-projector",
    "support-platform-cases-projector",
    "people-payroll-time-records-projector",
    "legal-cases-projector",
    "shared-signatures-projector",
    "manufacturing-orders-projector",
    "planning-runs-projector",
    "ecommerce-listings-projector",
    "ecommerce-journeys-projector",
    "talent-candidate-projector",
    "talent-workforce-projector",
    "learning-enrollments-projector",
    "benefits-enrollments-projector",
    "field-service-orders-projector",
    "projects-projects-projector",
    "projects-hierarchy-projector",
    "collaboration-threads-projector",
    "collaboration-stream-projector",
    "collaboration-inbox-projector",
    "item-directory-projector",
    "assets-projector",
    "asset-lifecycle-records-projector",
    "maintenance-work-orders-projector",
    "maintenance-lifecycle-records-projector",
    "maintenance-asset-facts-worker",
    "sales-quotes-projector",
    "sales-orders-projector",
    "sales-revenue-arrangements-projector",
    "sales-lifecycle-records-projector",
    "people-payroll-employees-projector",
    "people-payroll-leave-requests-projector",
    "people-payroll-expense-claims-projector",
    "people-payroll-payroll-runs-projector",
    "pos-sales-projector",
    "pos-refunds-projector",
    "pos-lifecycle-records-projector",
    "finance-charts-projector",
    "finance-periods-projector",
    "finance-journals-projector",
    "finance-subledger-projector",
    "finance-trial-balance-projector",
    "finance-bank-transactions-projector",
    "finance-statutory-tax-projector",
    "finance-vendor-invoices-projector",
    "finance-payments-projector",
    "finance-customer-invoices-projector",
    "finance-revenue-schedules-projector",
    "finance-business-records-projector",
    "legal-contracts-projector",
    "legal-documents-projector",
    "legal-access-history-projector",
    "legal-matters-projector",
    "support-tickets-projector",
    "support-incidents-projector",
    "support-sessions-projector",
    "support-capabilities-projector",
    "operations-exceptions-projector",
    "operations-lifecycle-records-projector",
    "operations-source-signals-projector",
    "operations-work-queue-policies-projector",
    "operations-source-ingestion-worker",
    "procure-to-pay-worker",
    "marketing-approval-decisions-worker",
    "customer360-control-customers-projector",
    "customer360-holding-activity-projector",
    "branch-provisioning-worker",
    "holding-projection-status-projector",
    "marketing-public-abuse-cleanup-worker",
    "marketing-delivery-worker",
    "marketing-journey-integration-worker",
];
const DEDICATED_WORKERS: [&str; 99] = [
    "support-worker",
    "agent-holding-operations-worker",
    "commercial-catalog-projector",
    "wave5-control-projector",
    "quality-projector",
    "quality-lifecycle-records-projector",
    "identity-access-principals-projector",
    "identity-access-roles-projector",
    "identity-access-governance-projector",
    "organization-projector",
    "organization-sharing-projector",
    "commercial-subscriptions-projector",
    "commercial-entitlements-projector",
    "commercial-lifecycle-records-projector",
    "marketing-lead-candidates-projector",
    "marketing-campaign-budgets-projector",
    "marketing-business-records-projector",
    "crm-leads-projector",
    "crm-opportunities-projector",
    "crm-business-records-projector",
    "procurement-purchase-orders-projector",
    "procurement-workflow-records-projector",
    "procurement-business-records-projector",
    "inventory-stock-items-projector",
    "inventory-lifecycle-records-projector",
    "customer-authority-business-records-projector",
    "customer360-owned-records-projector",
    "support-platform-cases-projector",
    "people-payroll-time-records-projector",
    "legal-cases-projector",
    "shared-signatures-projector",
    "manufacturing-orders-projector",
    "planning-runs-projector",
    "ecommerce-listings-projector",
    "ecommerce-journeys-projector",
    "talent-candidate-projector",
    "talent-workforce-projector",
    "learning-enrollments-projector",
    "benefits-enrollments-projector",
    "field-service-orders-projector",
    "projects-projects-projector",
    "projects-hierarchy-projector",
    "collaboration-threads-projector",
    "collaboration-stream-projector",
    "collaboration-inbox-projector",
    "item-directory-projector",
    "assets-projector",
    "asset-lifecycle-records-projector",
    "maintenance-work-orders-projector",
    "maintenance-lifecycle-records-projector",
    "maintenance-asset-facts-worker",
    "sales-quotes-projector",
    "sales-orders-projector",
    "sales-revenue-arrangements-projector",
    "sales-lifecycle-records-projector",
    "people-payroll-employees-projector",
    "people-payroll-leave-requests-projector",
    "people-payroll-expense-claims-projector",
    "people-payroll-payroll-runs-projector",
    "pos-sales-projector",
    "pos-refunds-projector",
    "pos-lifecycle-records-projector",
    "finance-charts-projector",
    "finance-periods-projector",
    "finance-journals-projector",
    "finance-subledger-projector",
    "finance-trial-balance-projector",
    "finance-bank-transactions-projector",
    "finance-statutory-tax-projector",
    "finance-vendor-invoices-projector",
    "finance-payments-projector",
    "finance-customer-invoices-projector",
    "finance-revenue-schedules-projector",
    "finance-business-records-projector",
    "legal-contracts-projector",
    "legal-documents-projector",
    "legal-access-history-projector",
    "legal-matters-projector",
    "support-tickets-projector",
    "support-incidents-projector",
    "support-sessions-projector",
    "support-capabilities-projector",
    "operations-exceptions-projector",
    "operations-lifecycle-records-projector",
    "operations-source-signals-projector",
    "operations-work-queue-policies-projector",
    "operations-source-ingestion-worker",
    "procure-to-pay-worker",
    "marketing-approval-decisions-worker",
    "customer360-control-customers-projector",
    "customer360-holding-activity-projector",
    "branch-provisioning-worker",
    "holding-projection-status-projector",
    "marketing-public-abuse-cleanup-worker",
    "marketing-delivery-worker",
    "marketing-journey-integration-worker",
    "directory-supervisor",
    "holding-backup-worker",
    "backup-retention-worker",
];
const LEGACY_LOCAL_WORKERS: [&str; 1] = ["pitr-controller"];
const UCS_LABS: [&str; 1] = ["ucs-lab"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DevMode {
    Warm,
    Fresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DevOptions {
    mode: DevMode,
    web_debug: bool,
    hot_patch: bool,
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
    web_launch_sha256: String,
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
    web_artifacts_ready: bool,
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
    schema_revision: u16,
    service_catalog_generation: u16,
    service_id_range: String,
    state: String,
    prepared_at: String,
    installation_generation: Uuid,
    installation_prefix: String,
    holding_id: String,
    data_profile_key: String,
    control_write_database: String,
    control_read_database: String,
    holding_write_database: String,
    holding_read_database: String,
    object_namespace: String,
    rollback: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct LocalReconfigurationManifest {
    version: u32,
    service_catalog_generation: u16,
    installation_generation: Uuid,
    installation_prefix: String,
    holding_id: String,
    data_profile_key: String,
    control_write_database: String,
    control_read_database: String,
    holding_write_database: String,
    holding_read_database: String,
    object_namespace: String,
}

impl LocalReconfigurationManifest {
    fn database_names(&self) -> Vec<String> {
        vec![
            self.control_write_database.clone(),
            self.control_read_database.clone(),
            self.holding_write_database.clone(),
            self.holding_read_database.clone(),
        ]
    }
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
        let bytes = fs::read(&path)?;
        let raw: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| "fresh-state runtime manifest is invalid")?;
        // The active path may still contain a pre-cutover marker retained from
        // an earlier installation. It is rollback evidence, not an authority
        // for the current runtime. Ignore only this exact legacy shape so the
        // current four-database runtime can reconcile without rewriting it.
        if is_retained_legacy_manifest(&raw) {
            return Ok(None);
        }
        let manifest: Self =
            serde_json::from_value(raw).map_err(|_| "fresh-state runtime manifest is invalid")?;
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
        let holding_id = Uuid::parse_str(&self.holding_id)
            .ok()
            .filter(|value| !value.is_nil());
        let data_profile_key = Uuid::parse_str(&self.data_profile_key)
            .ok()
            .filter(|value| !value.is_nil());
        let rollback_valid = self.rollback.get("retained_resource_authority")
            == Some(&serde_json::Value::String(
                "offline_non_authoritative".into(),
            ))
            && self.rollback.get("write_policy")
                == Some(&serde_json::Value::String("isolated_no_merge".into()));
        let expected_object_namespace = format!(
            "{}-holding-objects",
            self.installation_prefix.replace('_', "-")
        );
        let holding_write_token = self
            .holding_write_database
            .strip_prefix(&format!("{}_", self.installation_prefix))
            .and_then(|value| value.strip_suffix("_holding_write"));
        let holding_read_token = self
            .holding_read_database
            .strip_prefix(&format!("{}_", self.installation_prefix))
            .and_then(|value| value.strip_suffix("_holding_read"));
        let holding_token_valid = holding_write_token == holding_read_token
            && holding_write_token.is_some_and(|token| {
                token.len() == 12 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
        let names_match_prefix = self.control_write_database
            == format!("{}_control_write", self.installation_prefix)
            && self.control_read_database == format!("{}_control_read", self.installation_prefix)
            && holding_token_valid;
        if self.version != 4
            || self.schema_revision != 2
            || self.service_catalog_generation != 4
            || self.service_id_range != "IFC-SVC-201..267"
            || self.state != "prepared"
            || self.installation_generation.is_nil()
            || !valid_prefix
            || !database_names_valid
            || !unique_databases
            || holding_id.is_none()
            || data_profile_key.is_none()
            || chrono::DateTime::parse_from_rfc3339(&self.prepared_at).is_err()
            || self.object_namespace != expected_object_namespace
            || !names_match_prefix
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
            &self.holding_write_database,
            &self.holding_read_database,
        ]
    }

    async fn complete_receipt_root_principal(
        &self,
        _root: &Path,
        config: &DevConfig,
    ) -> TaskResult<String> {
        let pool = pool(&config.database_url(&self.control_write_database)).await?;
        let root_principal: Option<String> = sqlx::query_scalar(
            "SELECT receipt.root_principal_ref
                FROM erpx_control.platform_bootstrap_receipts receipt
                JOIN erpx_control.holding_registry registry
                  ON registry.holding_id=receipt.holding_id
                WHERE receipt.installation_prefix=$1
                  AND receipt.holding_id=$2
                  AND receipt.data_profile_key=$3
                  AND receipt.state='complete'
                  AND registry.data_profile_key=$3
                  AND registry.write_database_key=$4
                  AND registry.read_database_key=$5
                  AND registry.storage_namespace=$6
                  AND registry.state='active'
                LIMIT 1",
        )
        .bind(&self.installation_prefix)
        .bind(&self.holding_id)
        .bind(&self.data_profile_key)
        .bind(&self.holding_write_database)
        .bind(&self.holding_read_database)
        .bind(&self.object_namespace)
        .fetch_optional(&pool)
        .await?;
        pool.close().await;
        let root_principal = root_principal
            .ok_or("fresh-state runtime requires a matching complete receipt and active holding")?;
        if root_principal.trim() != root_principal
            || root_principal.is_empty()
            || root_principal.len() > 255
            || root_principal.chars().any(|character| {
                character.is_control()
                    || character.is_whitespace()
                    || matches!(character, '/' | '\\')
            })
        {
            return Err("fresh-state bootstrap receipt contains an invalid root principal".into());
        }
        Ok(root_principal)
    }

    async fn require_complete_receipt(&self, root: &Path, config: &DevConfig) -> TaskResult {
        self.complete_receipt_root_principal(root, config)
            .await
            .map(|_| ())
    }
}

fn is_retained_legacy_manifest(value: &serde_json::Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("holding_id").is_none()
        && object.get("holding_ref").is_some()
        && object.get("tenant_id").is_some()
        && object.get("tenant_write_database").is_some()
        && object.get("tenant_read_database").is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DevelopmentAccessRuntimeTarget {
    control_write_database: String,
    control_read_database: String,
    controller_target_holding: String,
    fresh_state: bool,
}

fn development_access_target_from_manifest(
    manifest: Option<&FreshRuntimeManifest>,
) -> DevelopmentAccessRuntimeTarget {
    manifest.map_or_else(
        || DevelopmentAccessRuntimeTarget {
            control_write_database: DATABASES[0].into(),
            control_read_database: DATABASES[1].into(),
            controller_target_holding: erpx_identity::DEVELOPMENT_CONTROLLER_TARGET_HOLDING.into(),
            fresh_state: false,
        },
        |manifest| DevelopmentAccessRuntimeTarget {
            control_write_database: manifest.control_write_database.clone(),
            control_read_database: manifest.control_read_database.clone(),
            controller_target_holding: manifest.holding_id.clone(),
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
            &target.controller_target_holding,
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
            && !(self.port == 55_432
                && matches!(
                    self.legacy_database.as_str(),
                    "erpx_clean_control_write" | "erpx_control_write" | "erpxv8_control_write"
                ))
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
    if let Some(options) = parse_dev_options(&args)? {
        let config = DevConfig::load(&root)?;
        return dev(&root, &config, options).await;
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
        [command, subcommand, holding_flag, holding]
            if command == "dev-access"
                && subcommand == "credentials"
                && holding_flag == "--holding" =>
        {
            require_development_access_requested(&root)?;
            dev_access::credentials(&root, Some(holding))
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
            Err("continuous local PITR is disabled; use `cargo xtask pitr rehearsal`".into())
        }
        [command, subcommand, confirm_flag, confirmation]
            if command == "pitr"
                && subcommand == "cleanup-local"
                && confirm_flag == "--confirm" =>
        {
            stop_service_group(&root, &LEGACY_LOCAL_WORKERS)?;
            infra::pitr_cleanup_local(&root, confirmation).await
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
        [command, flag] if command == "reconfigure-local" && flag == "--dry-run" => {
            reconfigure_local(&root, None).await
        }
        [command, confirm_flag, confirmation]
            if command == "reconfigure-local" && confirm_flag == "--confirm" =>
        {
            reconfigure_local(&root, Some(confirmation)).await
        }
        [command] if command == "verify" => verify(&root).await,
        [command] if command == "verify-web-production" => verify_web_production(&root).await,
        [command] if command == "web-e2e" => web_e2e::run(&root).await,
        [command] if command == "verify-core" => verify_core(&root),
        [command] if command == "verify-runtime" => verify_runtime(&root).await,
        [command] if command == "verify-sqlx" => verify_sqlx(&root).await,
        [command] if command == "verify-platforms" => verify_platforms(&root),
        _ => {
            println!(
                "ERPX development tasks\n\n\
                 cargo xtask dev --detach [--fresh] [--hot-patch|--no-hot-patch] [--web-debug]\n\
                 cargo xtask status\n\
                 cargo xtask stop\n\
                 cargo xtask infra init|start|status|stop\n\
                 cargo xtask directory-lab init|start|configure|verify-deny-wins|status|stop\n\
                 cargo xtask directory-lab ucs-real check|seed|verify\n\
                 cargo xtask dev-access init|reconcile|status|credentials [--holding HOLDING]\n\
                 cargo xtask dev-access rotate|reset --confirm ...\n\
                 cargo xtask migrate-all-erpx-databases\n\
                 cargo xtask pitr rehearsal\n\
                 cargo xtask pitr cleanup-local --confirm PURGE_ERPX_LOCAL_PITR\n\
                 cargo xtask pitr promote --confirm PROMOTE_ERPX_PITR\n\
                 cargo xtask reset-dev --confirm {RESET_CONFIRMATION}\n\
                 cargo xtask reconfigure-local --dry-run\n\
                 cargo xtask reconfigure-local --confirm {RECONFIGURE_LOCAL_CONFIRMATION}\n\
                 cargo xtask reset-platform-root --confirm {ROOT_PASSWORD_RESET_CONFIRMATION}\n\
                 cargo xtask seed-demo\n\
                 cargo xtask web-toolchain\n\
                 cargo xtask verify\n\
                 cargo xtask verify-web-production\n\
                 cargo xtask web-e2e\n\
                 cargo xtask verify-core\n\
                 cargo xtask verify-runtime\n\
                 cargo xtask verify-sqlx\n\
                 cargo xtask verify-platforms"
            );
            Ok(())
        }
    }
}

fn parse_dev_options(args: &[String]) -> TaskResult<Option<DevOptions>> {
    if args.first().map(String::as_str) != Some("dev") {
        return Ok(None);
    }
    let mut options = DevOptions {
        mode: DevMode::Warm,
        web_debug: false,
        hot_patch: false,
    };
    let mut seen = BTreeSet::new();
    let mut detach = false;
    for flag in &args[1..] {
        if !seen.insert(flag.as_str()) {
            return Err(format!("duplicate dev flag: {flag}").into());
        }
        match flag.as_str() {
            "--detach" => detach = true,
            "--fresh" => options.mode = DevMode::Fresh,
            "--hot-patch" => options.hot_patch = true,
            "--no-hot-patch" => options.hot_patch = false,
            "--web-debug" => options.web_debug = true,
            _ => return Err(format!("unknown dev flag: {flag}").into()),
        }
    }
    if !detach {
        return Err(
            "usage: cargo xtask dev --detach [--fresh] [--hot-patch|--no-hot-patch] [--web-debug]"
                .into(),
        );
    }
    Ok(Some(options))
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
    let holding_write = pool(&config.database_url(DATABASES[2])).await?;
    let holding_read = pool(&config.database_url(DATABASES[3])).await?;
    let restricted_holding_write = pool(&config.database_url(DATABASES[4])).await?;
    let restricted_holding_read = pool(&config.database_url(DATABASES[5])).await?;
    erpx_migration::migrate_control_plane(&control_write)
        .await
        .map_err(|error| format!("control-write baseline failed: {error}"))?;
    erpx_migration::migrate_control_read(&control_read)
        .await
        .map_err(|error| format!("control-read baseline failed: {error}"))?;
    erpx_migration::migrate_holding_write(&holding_write)
        .await
        .map_err(|error| format!("holding-write baseline failed: {error}"))?;
    erpx_migration::migrate_holding_read(&holding_read)
        .await
        .map_err(|error| format!("holding-read baseline failed: {error}"))?;
    erpx_migration::migrate_holding_write(&restricted_holding_write).await?;
    erpx_migration::migrate_holding_read(&restricted_holding_read).await?;
    apply_identity_migration(&control_write).await?;
    let control = SqlxControlPlane::new(ControlPlanePools {
        write: control_write,
        read: control_read,
    });
    control.require_erpx_command_owner().await?;
    holding_write.close().await;
    holding_read.close().await;
    restricted_holding_write.close().await;
    restricted_holding_read.close().await;
    Ok(())
}

async fn migrate_fresh_runtime(config: &DevConfig, manifest: &FreshRuntimeManifest) -> TaskResult {
    let control_write = pool(&config.database_url(&manifest.control_write_database)).await?;
    let control_read = pool(&config.database_url(&manifest.control_read_database)).await?;
    let holding_write = pool(&config.database_url(&manifest.holding_write_database)).await?;
    let holding_read = pool(&config.database_url(&manifest.holding_read_database)).await?;
    erpx_migration::migrate_control_plane(&control_write).await?;
    erpx_migration::migrate_control_read(&control_read).await?;
    erpx_migration::migrate_holding_write(&holding_write).await?;
    erpx_migration::migrate_holding_read(&holding_read).await?;
    apply_identity_migration(&control_write).await?;
    let control = SqlxControlPlane::new(ControlPlanePools {
        write: control_write,
        read: control_read,
    });
    control.require_erpx_command_owner().await?;
    holding_write.close().await;
    holding_read.close().await;
    Ok(())
}

async fn pool(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new().max_connections(4).connect(url).await
}

fn load_reconfiguration_manifest(
    root: &Path,
) -> TaskResult<(LocalReconfigurationManifest, Vec<u8>)> {
    let path = runtime_dir(root).join(FRESH_RUNTIME_MANIFEST_FILE);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 128 * 1024 {
        return Err("reconfigure-local manifest is not a bounded regular file".into());
    }
    let bytes = fs::read(&path)?;
    let manifest: LocalReconfigurationManifest = serde_json::from_slice(&bytes)?;
    if !matches!(manifest.version, 2..=4)
        || !matches!(manifest.service_catalog_generation, 2..=4)
        || manifest.installation_generation.is_nil()
        || manifest.installation_prefix != "erpxv8"
        || Uuid::parse_str(&manifest.holding_id).is_err()
        || Uuid::parse_str(&manifest.data_profile_key).is_err()
        || manifest.database_names().iter().any(|database| {
            database.is_empty()
                || database.len() > 63
                || !database
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
        || manifest.object_namespace != "erpxv8-holding-objects"
    {
        return Err("reconfigure-local manifest failed the active erpxv8 fence".into());
    }
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let object = value
        .as_object_mut()
        .ok_or("reconfigure-local manifest is not an object")?;
    object.insert("version".into(), serde_json::json!(4));
    object.insert("service_catalog_generation".into(), serde_json::json!(4));
    object.insert(
        "service_id_range".into(),
        serde_json::json!("IFC-SVC-201..267"),
    );
    object.remove("n8n_config_sha256");
    let rollback = object
        .get_mut("rollback")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("reconfigure-local rollback inventory is missing")?;
    rollback.remove("n8n_config_sha256");
    let staged = serde_json::to_vec_pretty(&value)?;
    Ok((manifest, staged))
}

async fn verify_reconfiguration_authority(
    config: &DevConfig,
    manifest: &LocalReconfigurationManifest,
) -> TaskResult {
    let control_write = pool(&config.database_url(&manifest.control_write_database)).await?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM erpx_control.platform_bootstrap_receipts receipt
            JOIN erpx_control.holding_registry registry ON registry.holding_id=receipt.holding_id
            WHERE receipt.installation_prefix=$1
              AND receipt.holding_id=$2 AND receipt.data_profile_key=$3 AND receipt.state='complete'
              AND registry.data_profile_key=$3 AND registry.write_database_key=$4
              AND registry.read_database_key=$5 AND registry.storage_namespace=$6
              AND registry.state='active')",
    )
    .bind(&manifest.installation_prefix)
    .bind(&manifest.holding_id)
    .bind(&manifest.data_profile_key)
    .bind(&manifest.holding_write_database)
    .bind(&manifest.holding_read_database)
    .bind(&manifest.object_namespace)
    .fetch_one(&control_write)
    .await?;
    if !active {
        return Err(
            "reconfigure-local requires the active installation and complete receipt".into(),
        );
    }
    let write_has_n8n: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM erpx_platform_administration_write.connector_guards
            WHERE lower(connector_kind)='n8n' OR lower(connector_kind_type)='n8n'
               OR lower(coalesce(connector_kind_other,''))='n8n'
            UNION ALL SELECT 1 FROM erpx_cqrs_write.command_receipts
            WHERE context_name='platform_administration' AND lower(coalesce(receipt::text,'')) LIKE '%n8n%'
            UNION ALL SELECT 1 FROM erpx_cqrs_write.audit_log
            WHERE context_name='platform_administration' AND lower(details::text) LIKE '%n8n%'
            UNION ALL SELECT 1 FROM erpx_cqrs_write.outbox_events
            WHERE context_name='platform_administration' AND lower(payload::text) LIKE '%n8n%')",
    )
    .fetch_one(&control_write)
    .await?;
    control_write.close().await;
    let control_read = pool(&config.database_url(&manifest.control_read_database)).await?;
    let read_has_n8n: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM erpx_platform_administration_read.connector_projection
            WHERE lower(connector_kind)='n8n' OR lower(state::text) LIKE '%n8n%'
            UNION ALL SELECT 1 FROM erpx_cqrs_read.projection_failures
            WHERE projection_name='platform_administration.connectors' AND lower(last_error) LIKE '%n8n%')",
    )
    .fetch_one(&control_read)
    .await?;
    control_read.close().await;
    if write_has_n8n || read_has_n8n {
        return Err("reconfigure-local found legacy n8n authority or evidence".into());
    }
    Ok(())
}

async fn reconfigure_local(root: &Path, confirmation: Option<&String>) -> TaskResult {
    let config = DevConfig::load(root)?;
    config.validate_local()?;
    if config.port != 55_432 {
        return Err("reconfigure-local requires the fixed dedicated PostgreSQL port 55432".into());
    }
    infra::pitr_reconfiguration_fence(root)?;
    let (manifest, staged_manifest) = load_reconfiguration_manifest(root)?;
    verify_reconfiguration_authority(&config, &manifest).await?;
    println!("ERPX local reconfiguration targets:");
    println!("  Workspace runtime: {}", runtime_dir(root).display());
    println!("  PostgreSQL root: /Users/fluke/erpx-postgres-data (retain all databases)");
    println!("  PostgreSQL roles: erpx_admin, erpx_app, erpx_backup, erpx_restore");
    println!("  Databases: {}", manifest.database_names().join(", "));
    println!("  MinIO root: /Users/fluke/erpx-minio-data (retain all objects)");
    println!("  MinIO namespace probe: {}", manifest.object_namespace);
    println!("  Runtime manifest: v4, service catalog generation 4 (IFC-SVC-201..267)");
    println!("  Retired runtime removal: .runtime/n8n");
    println!("  PITR: inactive; active installation and receipt: verified");
    let Some(confirmation) = confirmation else {
        println!("Dry run complete; no credentials, certificates, configuration, or data changed.");
        return Ok(());
    };
    if confirmation != RECONFIGURE_LOCAL_CONFIRMATION {
        return Err(format!(
            "reconfigure-local requires --confirm {RECONFIGURE_LOCAL_CONFIRMATION}; no changes were made"
        )
        .into());
    }
    stop(root)?;
    infra::reconfigure_local(
        root,
        &manifest.control_write_database,
        &manifest.database_names(),
        &manifest.object_namespace,
        &staged_manifest,
    )
    .await?;
    let clean = Command::new("cargo")
        .arg("clean")
        .current_dir(root)
        .status()?;
    if !clean.success() {
        return Err(format!("cargo clean failed with {clean}").into());
    }
    let refreshed = DevConfig::load(root)?;
    dev(
        root,
        &refreshed,
        DevOptions {
            mode: DevMode::Fresh,
            web_debug: false,
            hot_patch: true,
        },
    )
    .await
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
    if confirmation != RESET_CONFIRMATION {
        return Err(format!(
            "reset-dev requires --confirm {RESET_CONFIRMATION}; no changes were made"
        )
        .into());
    }

    for schema in LEGACY_SCHEMAS {
        legacy
            .execute(format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE").as_str())
            .await?;
    }
    legacy.close().await;
    remove_legacy_bucket(config.minio_root.as_deref())?;
    cleanup_legacy_runtime(root)?;
    initialize_databases(config, true).await?;
    println!("Reset complete.");
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

    let holding_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.holding_registry
         WHERE (holding_id=$1 AND data_profile_key=$2 AND write_database_key=$3
                AND read_database_key=$4 AND storage_namespace=$5 AND state='active')
            OR (holding_id=$6 AND data_profile_key=$7 AND write_database_key=$8
                AND read_database_key=$9 AND storage_namespace=$10 AND state='active')",
    )
    .bind(HOLDING_ID)
    .bind(DATA_PROFILE_KEY)
    .bind(DATABASES[2])
    .bind(DATABASES[3])
    .bind(ERPX_BUCKET)
    .bind(RESTRICTED_HOLDING_ID)
    .bind(RESTRICTED_DATA_PROFILE_KEY)
    .bind(DATABASES[4])
    .bind(DATABASES[5])
    .bind("erpx-demo-restricted")
    .fetch_one(&write)
    .await?;

    let configured_holding_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM erpx_control.holding_summary_projection
         WHERE holding_id = ANY($1)
           AND configuration_state='configured'
           AND customer_id IS NOT NULL
           AND canonical_holding_id IS NOT NULL
           AND ((holding_id=$2 AND cardinality(enabled_modules)=12)
                OR (holding_id=$3 AND cardinality(enabled_modules)=12))",
    )
    .bind([HOLDING_ID, RESTRICTED_HOLDING_ID])
    .bind(HOLDING_ID)
    .bind(RESTRICTED_HOLDING_ID)
    .fetch_one(&write)
    .await?;

    let context_keys = DEMO_CONTEXTS
        .iter()
        .map(|(key, _, _, _)| *key)
        .collect::<Vec<_>>();
    let context_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_control.demo_contexts
         WHERE holding_id=$1 AND data_profile_key=$2 AND context_key = ANY($3)",
    )
    .bind(HOLDING_ID)
    .bind(DATA_PROFILE_KEY)
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
    let identity_access_current = identity_access_fixture::is_current(config).await?;

    let native_secret = runtime_dir(root).join("secrets/native-demo-password.key");
    Ok(holding_count == 2
        && configured_holding_count == 2
        && context_count == i64::try_from(DEMO_CONTEXTS.len())?
        && principal_count == i64::try_from(principal_refs.len())?
        && assignment_count == i64::try_from(assignment_ids.len())?
        && identity_access_current
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

    let registry = SqlxHoldingStoreRegistry::new(control.clone());
    // Older local fixtures used display labels as holding identities. Keep
    // those rows as retained, non-routable evidence while making the active
    // runtime registry UUID-only. This is an idempotent fixture reconciliation
    // and does not delete any database or object data.
    sqlx::query(
        "UPDATE erpx_control.holding_registry
         SET state='suspended', updated_at=now()
         WHERE holding_id IN ('demo-holding', 'demo-holding-restricted')
           AND state='active'",
    )
    .execute(&write)
    .await?;
    if registry.get(HOLDING_ID).await?.is_none() {
        registry
            .register(HoldingStoreRegistration {
                holding_id: HOLDING_ID.into(),
                data_profile_key: DATA_PROFILE_KEY.into(),
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
    if registry.get(RESTRICTED_HOLDING_ID).await?.is_none() {
        registry
            .register(HoldingStoreRegistration {
                holding_id: RESTRICTED_HOLDING_ID.into(),
                data_profile_key: RESTRICTED_DATA_PROFILE_KEY.into(),
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
        "UPDATE erpx_control.holding_registry
         SET credential_key_ref='local_shared',
             updated_at=now()
         WHERE holding_id IN ($1,$2)",
    )
    .bind(HOLDING_ID)
    .bind(RESTRICTED_HOLDING_ID)
    .execute(&write)
    .await?;

    configure_demo_holdings(&control).await?;

    let identity = SqlxIdentityStore::new(write.clone());
    ensure_native_demo_principal(root, &write, &identity).await?;
    let principal_ref = "local:platform-root";
    let principal_authority: Option<(String, String)> = sqlx::query_as(
        "SELECT authenticated_realm, principal_kind
         FROM erpx_identity.principals WHERE principal_ref = $1",
    )
    .bind(principal_ref)
    .fetch_optional(&write)
    .await?;
    if let Some((realm, kind)) = principal_authority {
        if realm != "platform_administration" || kind != "human" {
            return Err("Platform Root authenticated authority is contradictory".into());
        }
    } else {
        let one_time_password = format!(
            "ERPX-{}-{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        identity
            .create_principal_with_password_and_authority(
                principal_ref,
                &one_time_password,
                AuthenticatedAuthority {
                    realm: AuthenticatedRealm::PlatformAdministration,
                    principal_kind: AuthenticatedPrincipalKind::Human,
                },
                Utc::now(),
            )
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
    identity_access_fixture::seed(config).await?;

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
                    (context_key, display_name, holding_id, data_profile_key, state,
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
            .bind(HOLDING_ID)
            .bind(DATA_PROFILE_KEY)
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
                    aggregate_version, active_scope, actor, correlation_id,
                    idempotency_key, payload, occurred_at
                 )
                 VALUES ($1, 'erpx.demo.context.seeded', 1, 'demo_context', $1,
                         1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(event_id)
            .bind(serde_json::json!({"holding_id":HOLDING_ID,"data_profile_key":DATA_PROFILE_KEY}))
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

async fn reconcile_required_projection_jobs(
    config: &DevConfig,
    fresh_runtime: Option<&FreshRuntimeManifest>,
    actor_ref: &str,
) -> TaskResult {
    let (control_write_database, control_read_database, route_bindings) = fresh_runtime
        .map_or_else(
            || {
                (
                    DATABASES[0],
                    DATABASES[1],
                    vec![
                        (HOLDING_ID.to_owned(), DATA_PROFILE_KEY.to_owned()),
                        (
                            RESTRICTED_HOLDING_ID.to_owned(),
                            RESTRICTED_DATA_PROFILE_KEY.to_owned(),
                        ),
                    ],
                )
            },
            |manifest| {
                (
                    manifest.control_write_database.as_str(),
                    manifest.control_read_database.as_str(),
                    vec![(
                        manifest.holding_id.clone(),
                        manifest.data_profile_key.clone(),
                    )],
                )
            },
        );
    let write = pool(&config.database_url(control_write_database)).await?;
    let read = pool(&config.database_url(control_read_database)).await?;
    let mut bindings = Vec::with_capacity(route_bindings.len());
    for (holding_id, data_profile_key) in route_bindings {
        let canonical_holding_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT canonical_holding_id
             FROM erpx_control.holding_summary_projection
             WHERE holding_id=$1 AND configuration_state='configured'",
        )
        .bind(&holding_id)
        .fetch_one(&write)
        .await?;
        bindings.push((holding_id, data_profile_key, canonical_holding_id));
    }
    let control = std::sync::Arc::new(SqlxControlPlane::new(ControlPlanePools {
        write: write.clone(),
        read,
    }));
    let authorization = ProjectionSchedulerAuthorization::new(
        actor_ref,
        "reconcile required local holding projections",
        Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:dev:{FIXTURE_VERSION}:required-holding-projections").as_bytes(),
        ),
    )?;
    let repository = HoldingProjectionSchedulerRepository::new(
        write.clone(),
        std::sync::Arc::new(SqlxHoldingStoreRegistry::new(control)),
        authorization,
    );
    let policy = ProjectionPolicy::new(500, 10, 10, 1, 60)?;
    let now = Utc::now();
    for (holding_id, data_profile_key, canonical_holding_id) in bindings {
        let binding = HoldingHoldingBinding::new(holding_id, data_profile_key)?;
        for projection_name in DEV_REQUIRED_HOLDING_PROJECTIONS {
            let payload = if projection_name == "item_directory" {
                serde_json::json!({
                    "canonical_holding_id": canonical_holding_id,
                })
            } else {
                serde_json::json!({})
            };
            repository
                .schedule(
                    ProjectionScheduleRequest {
                        binding: binding.clone(),
                        projection_name: projection_name.into(),
                        job_key: format!(
                            "dev-reconcile:{FIXTURE_VERSION}:{projection_name}-current-v1"
                        ),
                        kind: ProjectionJobKind::Incremental,
                        available_at: chrono::DateTime::UNIX_EPOCH,
                        policy,
                        payload,
                    },
                    now,
                )
                .await?;
        }
    }
    write.close().await;
    println!("dev required holding projections: scheduled or already current");
    Ok(())
}

async fn configure_demo_holdings(control: &std::sync::Arc<SqlxControlPlane>) -> TaskResult {
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
    let holdings = [
        (
            HOLDING_ID,
            DATA_PROFILE_KEY,
            "DEMO-ATLAS-V6",
            "Atlas Holdings",
            "Atlas Holdings Co., Ltd.",
            ALL_MODULES,
            100_i32,
            24_i32,
            2_i32,
            4_i32,
        ),
        (
            RESTRICTED_HOLDING_ID,
            RESTRICTED_DATA_PROFILE_KEY,
            "DEMO-BEACON-V6",
            "Beacon Services",
            "Beacon Services Co., Ltd.",
            ALL_MODULES,
            40_i32,
            12_i32,
            2_i32,
            4_i32,
        ),
    ];
    let administration = SqlxHoldingAdministrationStore::new(control.clone());

    for (
        holding_id,
        data_profile_key,
        account_code,
        trading_name,
        legal_name,
        modules,
        purchased_seats,
        used_seats,
        company_count,
        branch_count,
    ) in holdings
    {
        let customer_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:demo:customer:{holding_id}").as_bytes(),
        );
        let canonical_holding_id = holding_holding_scope_id(data_profile_key);
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
        .bind(canonical_holding_id)
        .execute(&control.pools().write)
        .await?;

        if administration
            .customer_holding_binding(holding_id)
            .await?
            .is_none()
        {
            let binding = HoldingRegistryBinding::new(
                customer_id,
                holding_id,
                canonical_holding_id,
                data_profile_key,
            )?;
            administration
                .bind_customer_holding(&binding, "xtask:seed-demo")
                .await?;
        }

        sqlx::query(
            "UPDATE erpx_control.holding_summary_projection
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
             WHERE holding_id=$1",
        )
        .bind(holding_id)
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
    let existing_authority: Option<(String, String)> = sqlx::query_as(
        "SELECT authenticated_realm, principal_kind
         FROM erpx_identity.principals WHERE principal_ref=$1",
    )
    .bind(principal_ref)
    .fetch_optional(write)
    .await?;
    if let Some((realm, kind)) = existing_authority {
        if realm != "internal_workforce" || kind != "human" {
            return Err("native demo authenticated authority is contradictory".into());
        }
    } else {
        identity
            .create_principal_with_password_and_authority(
                principal_ref,
                &format!("ERPX-NATIVE-{password_component}"),
                AuthenticatedAuthority::legacy_human(),
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
        let existing_authority: Option<(String, String)> = sqlx::query_as(
            "SELECT authenticated_realm, principal_kind
             FROM erpx_identity.principals WHERE principal_ref = $1",
        )
        .bind(principal_ref)
        .fetch_optional(write)
        .await?;
        let authority = if principal_ref == "local:platform-root" {
            AuthenticatedAuthority {
                realm: AuthenticatedRealm::PlatformAdministration,
                principal_kind: AuthenticatedPrincipalKind::Human,
            }
        } else {
            AuthenticatedAuthority::legacy_human()
        };
        let expected_realm = if authority.realm == AuthenticatedRealm::PlatformAdministration {
            "platform_administration"
        } else {
            "internal_workforce"
        };
        if let Some((realm, kind)) = existing_authority {
            if realm != expected_realm || kind != "human" {
                return Err(format!(
                    "demo principal {principal_ref} authenticated authority is contradictory"
                )
                .into());
            }
        } else {
            let discarded_bootstrap_secret = format!("ERPX-DEMO-{}", Uuid::new_v4().simple());
            identity
                .create_principal_with_password_and_authority(
                    principal_ref,
                    &discarded_bootstrap_secret,
                    authority,
                    now,
                )
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
                            "platform.holding.read".into(),
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
        HOLDING_ID,
        DATA_PROFILE_KEY,
        RESTRICTED_HOLDING_ID,
        RESTRICTED_DATA_PROFILE_KEY,
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

async fn dev(root: &Path, config: &DevConfig, options: DevOptions) -> TaskResult {
    let total_started = Instant::now();
    let mode = options.mode;
    web_toolchain::ensure(root)?;
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
    run_command_with_env(root, "cargo", build_arguments, &[])?;
    println!("dev binaries: built ({:?})", phase.elapsed());

    let phase = Instant::now();
    if dedicated {
        if mode == DevMode::Warm && infra::is_running(root) {
            infra::start(root).await?;
            println!("dev infrastructure: reconciled ({:?})", phase.elapsed());
        } else {
            infra::start(root).await?;
            println!("dev infrastructure: started ({:?})", phase.elapsed());
        }
        stop_service_group(root, &LEGACY_LOCAL_WORKERS)?;
    } else {
        println!(
            "dev infrastructure: external host reused ({:?})",
            phase.elapsed()
        );
    }
    if development_access_enabled {
        dev_access::init(root, config)?;
    }
    let projection_actor_ref = if let Some(manifest) = fresh_runtime.as_ref() {
        if dedicated {
            infra::ensure_runtime_postgres_hba()?;
        }
        let root_principal = manifest
            .complete_receipt_root_principal(root, config)
            .await?;
        println!(
            "dev cutover: verified fresh receipt for installation {}",
            manifest.installation_generation
        );
        root_principal
    } else {
        "local:platform-root".into()
    };

    let phase = Instant::now();
    if let Some(manifest) = fresh_runtime.as_ref() {
        // A retained installation receipt proves the original bootstrap, but
        // it does not prove that migrations added after that receipt have run.
        // Reapply the additive migrators on every warm development start so a
        // hot-reloaded BFF never runs ahead of its durable session schema.
        migrate_fresh_runtime(config, manifest).await?;
        println!("dev schema: reconciled bootstrap databases with additive migrations");
    } else if dedicated {
        let admin_url = infra::admin_runtime_database_url(root)?;
        initialize_databases_with_admin(config, false, &admin_url, Some("erpx_app")).await?;
        infra::reconcile_logical_profiles(root).await?;
    } else {
        initialize_databases(config, false).await?;
    }
    if dedicated && development_access_enabled && fresh_runtime.is_none() {
        dev_access::migrate_active_branch_schemas(root, config).await?;
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
                        &manifest.holding_id,
                    )
                    .await?
                }
                None => dev_access::fixtures_current(root, config).await?,
            });
    if mode == DevMode::Warm && (!demo_current || !access_current) {
        stop_service_group(root, &["server"])?;
        stop_service_group(root, &DEDICATED_WORKERS)?;
        stop_service_group(root, &LEGACY_LOCAL_WORKERS)?;
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
                    &manifest.holding_id,
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
    reconcile_required_projection_jobs(config, fresh_runtime.as_ref(), &projection_actor_ref)
        .await?;

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
    let tailnet_fqdn = configured_development_tailnet_fqdn(root)?;
    if tailnet_fqdn.is_some() && !development_access_enabled {
        return Err(
            "ERPX_DEV_TAILSCALE_FQDN requires ERPX_DEV_PERSONA_MODE=enabled for dev-only access"
                .into(),
        );
    }
    let advertised_web_origin = tailnet_fqdn.as_ref().map_or_else(
        || "http://localhost:3004".to_owned(),
        |hostname| format!("https://{hostname}"),
    );
    let default_platform_origin = tailnet_fqdn.as_ref().map_or_else(
        || "http://platform.localhost:3004".to_owned(),
        |hostname| format!("https://{hostname}"),
    );
    let platform_origin =
        configured_value("ERPX_PLATFORM_ORIGIN").unwrap_or(default_platform_origin);
    let trusted_web_origins = format!(
        "{advertised_web_origin},{platform_origin},http://127.0.0.1:3004,http://localhost:3004"
    );
    let auth_http_origin = configured_value("ERPX_CONSOLE_AUTH_HTTP_ORIGIN")
        .unwrap_or_else(|| "http://localhost:50052".to_owned());
    let grpc_endpoint = configured_value("ERPX_CONSOLE_GRPC_ENDPOINT")
        .unwrap_or_else(|| "http://127.0.0.1:50051".to_owned());
    let mut common = dev_common_environment(
        root,
        config,
        dedicated,
        development_access_enabled,
        fresh_runtime.as_ref(),
    )?;
    common.push(("ERPX_WORKER_ACTOR_REF", projection_actor_ref.clone()));
    let mut allowed_origins = configured_value("ERPX_HTTP_ALLOWED_ORIGINS");
    if tailnet_fqdn.is_some() {
        let origin = advertised_web_origin.clone();
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
    server_environment.extend([
        (
            "ERPX_SUPPORT_CAPABILITY_ESCROW_KEY",
            secret_file(&runtime.join("support-capability-escrow.key"))?,
        ),
        (
            "ERPX_HOLDING_PROJECTION_SCHEDULER_ACTOR_REF",
            projection_actor_ref,
        ),
    ]);
    let mut web_environment = vec![
        ("ERPX_DEV_WEB_BIND_ADDR", "127.0.0.1".to_owned()),
        ("ERPX_PUBLIC_ORIGIN", advertised_web_origin.clone()),
        ("ERPX_PLATFORM_ORIGIN", platform_origin),
        ("ERPX_CONSOLE_API_BASE", auth_http_origin.clone()),
        ("ERPX_CONSOLE_AUTH_HTTP_ORIGIN", auth_http_origin),
        ("ERPX_CONSOLE_GRPC_ENDPOINT", grpc_endpoint),
        ("ERPX_WEB_BFF_ENABLED", "1".into()),
        ("ERPX_WEB_SSR_ENABLED", "1".into()),
        ("ERPX_WEB_TRUSTED_ORIGINS", trusted_web_origins),
        ("ERPX_WEB_SESSION_KEY_REFERENCE", "local-dev-v1".into()),
        (
            "ERPX_WEB_SESSION_KEY",
            STANDARD.encode(Sha256::digest(
                secret_file(&runtime.join("web-session.key"))?.as_bytes(),
            )),
        ),
    ];
    if let Some(tailnet_fqdn) = tailnet_fqdn {
        web_environment.push((DEVELOPMENT_TAILSCALE_FQDN_ENV, tailnet_fqdn));
    }
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
    if options.web_debug {
        web_environment.extend([
            ("CARGO_PROFILE_SERVER_DEV_DEBUG", "1".into()),
            ("CARGO_PROFILE_WASM_DEV_DEBUG", "1".into()),
        ]);
    }
    let desired_state = DevState {
        version: DEV_STATE_VERSION,
        server_binary_sha256: required_file_sha256(&runtime_binary(root, "erpx-server"))?,
        worker_binary_sha256: required_file_sha256(&runtime_binary(root, "erpx-worker"))?,
        ucs_binary_sha256: required_file_sha256(&runtime_binary(root, "erpx-ucs-lab"))?,
        server_environment_sha256: environment_sha256(&server_environment),
        worker_environment_sha256: worker_environment_sha256(&common, dedicated),
        ucs_environment_sha256: if development_access_enabled {
            ucs_environment_sha256(root)?
        } else {
            "disabled".into()
        },
        web_environment_sha256: environment_sha256(&web_environment),
        web_launch_sha256: web_launch_sha256(root, options)?,
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
        options,
    )?;
    println!("dev runtime services: applied ({:?})", phase.elapsed());

    let phase = Instant::now();
    wait_for_http("http://127.0.0.1:50052/health", 90).await?;
    wait_for_http("http://127.0.0.1:50052/ready", 90).await?;
    require_dioxus_web_ready(root, "http://127.0.0.1:3004", 180).await?;
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
        "ERPX is running at {advertised_web_origin} (dev ensure {:?})",
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
            "ERPX_HOLDING_ADMIN_DATABASE_URL",
            if dedicated {
                infra::admin_runtime_database_url(root)?
            } else {
                config.database_url("postgres")
            },
        ),
        (
            "ERPX_HOLDING_DATABASE_URL_TEMPLATE",
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
    ];
    if development_access_enabled {
        let target_holding = fresh_runtime.map_or(
            erpx_identity::DEVELOPMENT_CONTROLLER_TARGET_HOLDING,
            |manifest| manifest.holding_id.as_str(),
        );
        common.extend([
            ("ERPX_DEV_PERSONA_MODE", "enabled".into()),
            ("ERPX_DEVELOPMENT_CLUSTER", "local-ucs-lab".into()),
            (
                "ERPX_DEV_CONTROLLER_ROLE_PROFILE",
                "restricted_holding_full_admin".into(),
            ),
            ("ERPX_DEV_CONTROLLER_TARGET_HOLDING", target_holding.into()),
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
    // holding routing continues to resolve database keys from the fresh
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
            name: "agent-holding-operations-worker",
            handler: "agent_assignment",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b18a",
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
                "quality-lifecycle-records-projector",
                "quality_lifecycle_records",
                "b16f",
            ),
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
            (
                "identity-access-governance-projector",
                "identity_access_governance",
                "b167",
            ),
            ("organization-projector", "organization", "b149"),
            (
                "organization-sharing-projector",
                "organization_sharing",
                "b168",
            ),
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
                "commercial-lifecycle-records-projector",
                "commercial_lifecycle_records",
                "b171",
            ),
            (
                "marketing-lead-candidates-projector",
                "marketing_lead_candidates",
                "b14c",
            ),
            (
                "marketing-campaign-budgets-projector",
                "marketing_campaign_budgets",
                "b161",
            ),
            (
                "marketing-business-records-projector",
                "marketing_business_records",
                "b169",
            ),
            ("crm-leads-projector", "crm_leads", "b14d"),
            ("crm-opportunities-projector", "crm_opportunities", "b14e"),
            (
                "crm-business-records-projector",
                "crm_business_records",
                "b16a",
            ),
            (
                "procurement-purchase-orders-projector",
                "procurement_purchase_orders",
                "b14f",
            ),
            (
                "procurement-workflow-records-projector",
                "procurement_workflow_records",
                "b18b",
            ),
            (
                "procurement-business-records-projector",
                "procurement_business_records",
                "b16b",
            ),
            (
                "inventory-stock-items-projector",
                "inventory_stock_items",
                "b150",
            ),
            (
                "inventory-lifecycle-records-projector",
                "inventory_lifecycle_records",
                "b174",
            ),
            (
                "customer-authority-business-records-projector",
                "customer_authority_business_records",
                "b175",
            ),
            (
                "customer360-owned-records-projector",
                "customer360_owned_records",
                "b176",
            ),
            (
                "support-platform-cases-projector",
                "support_platform_cases",
                "b177",
            ),
            (
                "people-payroll-time-records-projector",
                "people_payroll_time_records",
                "b178",
            ),
            ("legal-cases-projector", "legal_cases", "b179"),
            ("shared-signatures-projector", "shared_signatures", "b17a"),
            (
                "manufacturing-orders-projector",
                "manufacturing_orders",
                "b17b",
            ),
            ("planning-runs-projector", "planning_runs", "b17c"),
            ("ecommerce-listings-projector", "ecommerce_listings", "b17d"),
            ("ecommerce-journeys-projector", "ecommerce_journeys", "b17e"),
            ("talent-candidate-projector", "talent_candidate", "b17f"),
            ("talent-workforce-projector", "talent_workforce", "b180"),
            (
                "learning-enrollments-projector",
                "learning_enrollments",
                "b181",
            ),
            (
                "benefits-enrollments-projector",
                "benefits_enrollments",
                "b182",
            ),
            (
                "field-service-orders-projector",
                "field_service_orders",
                "b183",
            ),
            ("projects-projects-projector", "projects_projects", "b184"),
            ("projects-hierarchy-projector", "projects_hierarchy", "b185"),
            (
                "collaboration-threads-projector",
                "collaboration_threads",
                "b186",
            ),
            (
                "collaboration-stream-projector",
                "collaboration_stream",
                "b187",
            ),
            (
                "collaboration-inbox-projector",
                "collaboration_inbox",
                "b188",
            ),
            ("item-directory-projector", "item_directory", "b189"),
            ("assets-projector", "assets_cqrs", "b151"),
            (
                "asset-lifecycle-records-projector",
                "asset_lifecycle_records",
                "b16d",
            ),
            (
                "maintenance-work-orders-projector",
                "maintenance_work_orders",
                "b152",
            ),
            (
                "maintenance-lifecycle-records-projector",
                "maintenance_lifecycle_records",
                "b16e",
            ),
            (
                "maintenance-asset-facts-worker",
                "maintenance_asset_facts",
                "b153",
            ),
            ("sales-quotes-projector", "sales_quotes", "b154"),
            ("sales-orders-projector", "sales_orders", "b155"),
            (
                "sales-revenue-arrangements-projector",
                "sales_revenue_arrangements",
                "b162",
            ),
            (
                "sales-lifecycle-records-projector",
                "sales_lifecycle_records",
                "b173",
            ),
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
            (
                "pos-lifecycle-records-projector",
                "pos_lifecycle_records",
                "b172",
            ),
        ]
        .into_iter()
        .map(|(name, projection_kind, suffix)| WorkerSpec {
            name,
            handler: "holding_projection_scheduler",
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
                "b161" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b161",
                "b162" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b162",
                "b167" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b167",
                "b168" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b168",
                "b169" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b169",
                "b16a" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b16a",
                "b16b" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b16b",
                "b16d" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b16d",
                "b16e" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b16e",
                "b16f" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b16f",
                "b171" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b171",
                "b172" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b172",
                "b173" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b173",
                "b174" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b174",
                "b175" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b175",
                "b176" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b176",
                "b177" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b177",
                "b178" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b178",
                "b179" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b179",
                "b17a" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b17a",
                "b17b" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b17b",
                "b17c" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b17c",
                "b17d" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b17d",
                "b17e" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b17e",
                "b17f" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b17f",
                "b180" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b180",
                "b181" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b181",
                "b182" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b182",
                "b183" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b183",
                "b184" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b184",
                "b185" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b185",
                "b186" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b186",
                "b187" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b187",
                "b188" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b188",
                "b189" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b189",
                "b18b" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b18b",
                _ => unreachable!("frozen generation-two projection worker suffix"),
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
            (
                "finance-vendor-invoices-projector",
                "finance_vendor_invoices",
                "b18c",
            ),
            ("finance-payments-projector", "finance_payments", "b18d"),
            (
                "finance-customer-invoices-projector",
                "finance_customer_invoices",
                "b163",
            ),
            (
                "finance-revenue-schedules-projector",
                "finance_revenue_schedules",
                "b164",
            ),
            (
                "finance-business-records-projector",
                "finance_business_records",
                "b16c",
            ),
            ("legal-contracts-projector", "legal_contracts", "b137"),
            ("legal-documents-projector", "legal_documents", "b138"),
            (
                "legal-access-history-projector",
                "legal_access_history",
                "b139",
            ),
            ("legal-matters-projector", "legal_matters", "b165"),
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
                "operations-lifecycle-records-projector",
                "operations_lifecycle_records",
                "b170",
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
            handler: "holding_projection_scheduler",
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
                "b163" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b163",
                "b164" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b164",
                "b165" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b165",
                "b16c" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b16c",
                "b170" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b170",
                "b18c" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b18c",
                "b18d" => "8cf8d67f-186e-4b12-9e1c-bc29f5f5b18d",
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
            name: "procure-to-pay-worker",
            handler: "procure_to_pay",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b18e",
            projection_kind: None,
        },
        WorkerSpec {
            name: "marketing-approval-decisions-worker",
            handler: "marketing_approval_decisions",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b166",
            projection_kind: None,
        },
        WorkerSpec {
            name: "customer360-control-customers-projector",
            handler: "customer360_control_customers_projection",
            worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f5b141",
            projection_kind: None,
        },
        WorkerSpec {
            name: "customer360-holding-activity-projector",
            handler: "customer360_holding_activity_projection",
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
            name: "holding-projection-status-projector",
            handler: "holding_projection_status_projection",
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
                name: "holding-backup-worker",
                handler: "holding_backup",
                worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f554a2",
                projection_kind: None,
            },
            WorkerSpec {
                name: "backup-retention-worker",
                handler: "holding_backup_retention",
                worker_id: "8cf8d67f-186e-4b12-9e1c-bc29f5f554a3",
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

fn web_launch_sha256(root: &Path, options: DevOptions) -> TaskResult<String> {
    fn collect_manifests(path: &Path, files: &mut Vec<PathBuf>) -> TaskResult {
        let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                collect_manifests(&path, files)?;
            } else if metadata.is_file() && path.file_name() == Some(OsStr::new("Cargo.toml")) {
                files.push(path);
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain",
        "rust-toolchain.toml",
        "Dioxus.toml",
        "apps/console/Dioxus.toml",
        ".cargo/config",
        ".cargo/config.toml",
    ] {
        let path = root.join(relative);
        if path.is_file() {
            files.push(path);
        }
    }
    for relative in ["apps", "crates", "xtask"] {
        let path = root.join(relative);
        if path.is_dir() {
            collect_manifests(&path, &mut files)?;
        }
    }
    files.sort();
    files.dedup();
    let mut digest = Sha256::new();
    update_digest(&mut digest, &WEB_LAUNCH_CONTRACT_VERSION.to_string());
    update_digest(
        &mut digest,
        if options.hot_patch {
            "monolithic-hot-patch"
        } else {
            "monolithic-stable-hot-reload"
        },
    );
    update_digest(
        &mut digest,
        if options.web_debug {
            "debug-on"
        } else {
            "debug-off"
        },
    );
    let dx = web_toolchain::require(root)?;
    let dx_version = Command::new(dx).arg("--version").output()?;
    if !dx_version.status.success() {
        return Err("dx --version failed while calculating the Web launch fingerprint".into());
    }
    update_digest(&mut digest, &String::from_utf8_lossy(&dx_version.stdout));
    for path in files {
        let relative = path.strip_prefix(root).unwrap_or(&path);
        update_digest(&mut digest, &relative.to_string_lossy());
        update_digest(&mut digest, &required_file_sha256(&path)?);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn ucs_environment_sha256(root: &Path) -> TaskResult<String> {
    let runtime = runtime_dir(root);
    let mut digest = Sha256::new();
    for value in ["platform", HOLDING_ID, RESTRICTED_HOLDING_ID] {
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
        web_artifacts_ready: wasm::artifact_readiness(root, wasm::WasmProfile::Dev).is_ok(),
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
    let web_changed = previous.is_none_or(|state| {
        state.web_environment_sha256 != desired.web_environment_sha256
            || state.web_launch_sha256 != desired.web_launch_sha256
    });
    DevRestartPlan {
        server: fresh || !runtime.server || server_changed,
        workers: fresh || !runtime.all_workers || workers_changed,
        ucs_labs: desired.development_access_enabled
            && (fresh || !runtime.all_ucs_labs || ucs_changed),
        stop_disabled_ucs_labs: !desired.development_access_enabled && runtime.any_ucs_labs,
        web: fresh || !runtime.web || !runtime.web_artifacts_ready || web_changed,
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
        runtime_binary(root, "erpx-server"),
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
    stop_service_group(root, &LEGACY_LOCAL_WORKERS)?;
    for spec in worker_specs(dedicated) {
        let mut environment = common.to_vec();
        environment.extend([
            ("ERPX_WORKER_HANDLER", spec.handler.into()),
            ("ERPX_WORKER_ID", spec.worker_id.into()),
            (
                "ERPX_WORKER_POLL_INTERVAL_MS",
                DEV_WORKER_POLL_INTERVAL_MS.into(),
            ),
        ]);
        if let Some(kind) = spec.projection_kind {
            environment.extend([
                ("ERPX_HOLDING_PROJECTION_KIND", kind.into()),
                (
                    "ERPX_HOLDING_PROJECTION_CLAIM_KEY_PREFIX",
                    format!("holding-projection-{kind}"),
                ),
            ]);
        }
        spawn_service(
            root,
            runtime,
            spec.name,
            runtime_binary(root, "erpx-worker"),
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
                    count(*) FILTER (WHERE state='dead_letter') AS failed,
                    count(*) FILTER (WHERE state='claimed' AND lease_until <= now()) AS expired,
                    max(updated_at) AS newest_updated_at
             FROM erpx_control.holding_projection_jobs
             WHERE holding_id=$1 AND data_profile_key=$2 AND job_key LIKE $3",
        )
        .bind(&manifest.holding_id)
        .bind(&manifest.data_profile_key)
        .bind(&job_prefix)
        .fetch_one(&write)
        .await
        .map_err(|_| "fresh initial projection barrier could not be evaluated")?;
        let expected_job_count = i64::try_from(FRESH_INITIAL_HOLDING_PROJECTIONS.len())?;
        if jobs.0 != expected_job_count {
            return Err(format!(
                "fresh initial projection job inventory is incomplete: expected {expected_job_count}, found {}",
                jobs.0
            )
            .into());
        }
        if jobs.2 != 0 || jobs.3 != 0 {
            return Err("fresh initial projection jobs failed closed".into());
        }

        let status_checkpoint: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
            "SELECT source_updated_at
             FROM erpx_control.holding_projection_status_checkpoints
             WHERE projection_name='holding_projection_status'",
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
                 WHERE context_name='brand' AND event_type='brand.profile_snapshot'
                   AND publish_error IS NULL),
                (SELECT count(DISTINCT command_id) FROM erpx_cqrs_write.outbox_events
                 WHERE context_name='brand' AND event_type='brand.profile_snapshot'
                   AND publish_error IS NULL),
                (SELECT max(global_sequence) FROM erpx_cqrs_write.outbox_events
                 WHERE context_name='brand' AND event_type='brand.profile_snapshot')",
        )
        .fetch_one(&write)
        .await
        .map_err(|_| "fresh command-side smoke evidence could not be read")?;
        if write_evidence.0 != 2
            || write_evidence.1 != 2
            || write_evidence.2 != 2
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
    options: DevOptions,
) -> TaskResult {
    if !restart {
        println!("dev web: reused (Dioxus hot reload remains active)");
        return Ok(());
    }
    let bind_addr = environment
        .iter()
        .find_map(|(key, value)| (*key == "ERPX_DEV_WEB_BIND_ADDR").then_some(value.clone()))
        .unwrap_or_else(|| "127.0.0.1".to_owned());
    fs::create_dir_all(web_session_cache_dir(root))?;
    stop_service_group(root, &["web"])?;
    spawn_service(
        root,
        runtime,
        "web",
        web_toolchain::require(root)?,
        web_serve_arguments(root, &bind_addr, options),
        environment,
    )?;
    println!(
        "dev web: {}",
        if was_running { "restarted" } else { "started" }
    );
    Ok(())
}

fn web_serve_arguments(root: &Path, bind_addr: &str, options: DevOptions) -> Vec<String> {
    let mut arguments = vec!["serve".to_owned()];
    if options.hot_patch {
        arguments.push("--hot-patch".to_owned());
    } else {
        arguments.extend([
            "--hot-patch".to_owned(),
            "false".to_owned(),
            "--hot-reload".to_owned(),
            "true".to_owned(),
        ]);
    }
    arguments.extend([
        "--web".to_owned(),
        "--fullstack".to_owned(),
        "true".to_owned(),
        "--no-default-features".to_owned(),
        "--offline".to_owned(),
        "--debug-symbols".to_owned(),
        options.web_debug.to_string(),
        "--session-cache-dir".to_owned(),
        web_session_cache_dir(root).to_string_lossy().into_owned(),
        "-p".to_owned(),
        "erpx-console".to_owned(),
        "--port".to_owned(),
        "3004".to_owned(),
        "--addr".to_owned(),
        bind_addr.to_owned(),
        "--open".to_owned(),
        "false".to_owned(),
    ]);
    arguments
}

fn web_session_cache_dir(root: &Path) -> PathBuf {
    root.join("target/dx-hotpatch").join(format!(
        "{}-monolithic-v{}",
        web_toolchain::DX_VERSION,
        WEB_SESSION_CACHE_CONTRACT_VERSION,
    ))
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
        .chain(LEGACY_LOCAL_WORKERS)
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
    let local_values = read_dotenv(&root.join(".env.local"))?;
    let tailnet_origin =
        configured_development_tailnet_fqdn(root)?.map(|hostname| format!("https://{hostname}"));
    let public_origin = env::var("ERPX_PUBLIC_ORIGIN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            local_values
                .get("ERPX_PUBLIC_ORIGIN")
                .filter(|value| !value.trim().is_empty())
                .cloned()
        })
        .or_else(|| tailnet_origin.clone())
        .unwrap_or_else(|| "http://127.0.0.1:3004".into());
    println!("public-origin: {public_origin}");
    let default_platform_origin =
        if env::var("ERPX_WEB_PRODUCTION_PROFILE").as_deref() == Ok("enabled") {
            "https://console.tailb48369.ts.net".to_owned()
        } else {
            tailnet_origin.unwrap_or_else(|| "http://platform.localhost:3004".to_owned())
        };
    let platform_origin = env::var("ERPX_PLATFORM_ORIGIN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            local_values
                .get("ERPX_PLATFORM_ORIGIN")
                .filter(|value| !value.trim().is_empty())
                .cloned()
        })
        .unwrap_or(default_platform_origin);
    println!("platform-origin: {platform_origin}");
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
        .chain(LEGACY_LOCAL_WORKERS)
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
        _ if DEDICATED_WORKERS.contains(&name) || LEGACY_LOCAL_WORKERS.contains(&name) => {
            command.contains("erpx-worker")
        }
        _ => false,
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebProductionEvidence {
    schema_version: u32,
    release_revision: String,
    captured_at: String,
    coverage: WebProductionCoverageEvidence,
    providers: Vec<WebProductionProviderEvidence>,
    browser: WebProductionBrowserEvidence,
    security: WebProductionSecurityEvidence,
    recovery: WebProductionRecoveryEvidence,
    operations: WebProductionOperationsEvidence,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebProductionCoverageEvidence {
    baseline_version: String,
    requirement_outcomes: BTreeMap<String, String>,
    scenario_outcomes: BTreeMap<String, String>,
    workspaces: Vec<String>,
    administration_surfaces: Vec<String>,
    external_surfaces: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebProductionProviderEvidence {
    category: String,
    provider_key: String,
    adapter_revision: String,
    endpoint: String,
    callback_origin: String,
    credential_reference: String,
    tls_trust_reference: String,
    scopes: Vec<String>,
    rate_limit_per_minute: u32,
    data_residency: String,
    data_classification: String,
    certification_evidence: String,
    live_tests: BTreeMap<String, bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebProductionBrowserEvidence {
    passed: bool,
    features: Vec<String>,
    locales: Vec<String>,
    viewports: Vec<String>,
    negative_cases: Vec<String>,
    feature_journeys: Vec<WebProductionFeatureJourney>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebProductionFeatureJourney {
    feature: String,
    passed: bool,
    positive_cases: Vec<String>,
    negative_cases: Vec<String>,
    locales: Vec<String>,
    viewports: Vec<String>,
    unexplained_console_errors: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebProductionSecurityEvidence {
    passed: bool,
    critical_findings: u32,
    high_findings: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebProductionRecoveryEvidence {
    backup_restore_passed: bool,
    object_recovery_passed: bool,
    pitr_passed: bool,
    measured_rpo_seconds: u64,
    measured_rto_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WebProductionOperationsEvidence {
    alerts_loaded: bool,
    runbooks_verified: bool,
    projection_lag_seconds: u64,
    unexplained_console_errors: u32,
}

const WEB_PRODUCTION_FEATURES: [&str; 30] = [
    "ai-collaboration",
    "assets",
    "benefits",
    "brand",
    "commercial",
    "crm",
    "customer-360",
    "customer-authority",
    "ecommerce",
    "field-service",
    "finance",
    "identity-access",
    "inventory",
    "legal",
    "learning",
    "maintenance",
    "marketing",
    "manufacturing",
    "operations",
    "organization",
    "people-payroll",
    "planning",
    "platform-administration",
    "pos",
    "procurement",
    "projects",
    "quality",
    "sales",
    "support",
    "talent",
];

const WEB_PRODUCTION_WORKSPACES: [&str; 21] = [
    "operations",
    "finance",
    "sales",
    "procurement",
    "inventory",
    "asset",
    "maintenance",
    "quality",
    "marketing",
    "people",
    "pos",
    "legal",
    "manufacturing",
    "planning",
    "project",
    "commerce",
    "talent",
    "learning",
    "benefits",
    "field-service",
    "ai",
];

const WEB_PRODUCTION_ADMINISTRATION_SURFACES: [&str; 7] = [
    "platform-administration",
    "holding-administration",
    "identity-security",
    "work",
    "processes",
    "insights",
    "modules",
];

const WEB_PRODUCTION_EXTERNAL_SURFACES: [&str; 6] = [
    "commerce",
    "candidate",
    "signatory",
    "shared-public",
    "marketing-form",
    "marketing-preview",
];

const WEB_PRODUCTION_PROVIDER_CATEGORIES: [&str; 9] = [
    "payment",
    "directory",
    "marketing-delivery",
    "banking",
    "payroll",
    "benefits",
    "learning",
    "e-signature",
    "ai",
];

const WEB_PRODUCTION_PROVIDER_LIVE_TESTS: [&str; 6] = [
    "create",
    "read",
    "callback",
    "retry",
    "reconciliation",
    "revocation",
];

fn web_production_requirement_ids(root: &Path) -> TaskResult<BTreeSet<String>> {
    let matrix = fs::read_to_string(root.join("Plan/acceptance/traceability-matrix.md"))?;
    let mut requirements = BTreeSet::new();
    for line in matrix.lines().filter(|line| line.starts_with("| `REQ-")) {
        let cell = line
            .split('|')
            .nth(1)
            .ok_or("requirement traceability row is malformed")?;
        for token in cell
            .split(|character: char| {
                character == '`' || character == ',' || character.is_whitespace()
            })
            .filter(|token| token.starts_with("REQ-"))
        {
            let token = token.trim();
            if let Some((start, end)) = token.split_once("..") {
                let (prefix, start) = start
                    .rsplit_once('-')
                    .ok_or("requirement range start is malformed")?;
                let start = start.parse::<u16>()?;
                let end = end.parse::<u16>()?;
                if end < start {
                    return Err("requirement range is reversed".into());
                }
                requirements.extend((start..=end).map(|number| format!("{prefix}-{number:03}")));
            } else {
                requirements.insert(token.to_owned());
            }
        }
    }
    if requirements.len() != 644 {
        return Err(format!(
            "production requirement baseline 2.0.0 must resolve exactly 644 IDs, found {}",
            requirements.len()
        )
        .into());
    }
    Ok(requirements)
}

fn validate_web_production_coverage(
    root: &Path,
    evidence: &WebProductionCoverageEvidence,
) -> TaskResult {
    if evidence.baseline_version != "2.0.0" {
        return Err("Web production evidence must target requirements baseline 2.0.0".into());
    }
    let expected_requirements = web_production_requirement_ids(root)?;
    let actual_requirements = evidence
        .requirement_outcomes
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual_requirements != expected_requirements
        || evidence
            .requirement_outcomes
            .values()
            .any(|outcome| outcome != "pass")
    {
        return Err("Web production evidence must pass exactly all 644 requirement IDs".into());
    }
    let expected_scenarios = (1..=68)
        .map(|number| format!("E2E-{number:02}"))
        .collect::<BTreeSet<_>>();
    let actual_scenarios = evidence
        .scenario_outcomes
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if actual_scenarios != expected_scenarios
        || evidence
            .scenario_outcomes
            .values()
            .any(|outcome| outcome != "pass")
    {
        return Err("Web production evidence must pass exactly all 68 E2E scenarios".into());
    }
    for (label, actual, expected) in [
        (
            "workspace",
            &evidence.workspaces,
            WEB_PRODUCTION_WORKSPACES.as_slice(),
        ),
        (
            "administration surface",
            &evidence.administration_surfaces,
            WEB_PRODUCTION_ADMINISTRATION_SURFACES.as_slice(),
        ),
        (
            "external surface",
            &evidence.external_surfaces,
            WEB_PRODUCTION_EXTERNAL_SURFACES.as_slice(),
        ),
    ] {
        let actual = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
        let expected = expected.iter().copied().collect::<BTreeSet<_>>();
        if actual != expected {
            return Err(format!("Web production evidence has incomplete {label} coverage").into());
        }
    }
    Ok(())
}

fn validate_web_production_providers(providers: &[WebProductionProviderEvidence]) -> TaskResult {
    let categories = providers
        .iter()
        .map(|provider| provider.category.as_str())
        .collect::<BTreeSet<_>>();
    if providers.len() != WEB_PRODUCTION_PROVIDER_CATEGORIES.len()
        || categories != WEB_PRODUCTION_PROVIDER_CATEGORIES.into_iter().collect()
    {
        return Err(
            "Production launch requires exactly one provider in each of the nine categories".into(),
        );
    }
    for provider in providers {
        let provider_key = provider.provider_key.to_ascii_lowercase();
        if provider.provider_key.is_empty()
            || provider.provider_key.len() > 128
            || ["replace", "fixture", "simulator", "demo", "local"]
                .into_iter()
                .any(|marker| provider_key.contains(marker))
            || provider.adapter_revision.is_empty()
            || provider.adapter_revision.len() > 128
            || !provider.credential_reference.starts_with("secret-ref:")
            || provider.tls_trust_reference.trim().is_empty()
            || provider.scopes.is_empty()
            || provider.scopes.len() > 64
            || provider.rate_limit_per_minute == 0
            || provider.data_residency.trim().is_empty()
            || provider.data_classification.trim().is_empty()
            || provider.certification_evidence.trim().is_empty()
        {
            return Err(format!(
                "Production provider manifest is incomplete for {}",
                provider.category
            )
            .into());
        }
        for value in [&provider.endpoint, &provider.callback_origin] {
            let url = reqwest::Url::parse(value)
                .map_err(|_| "provider manifest contains an invalid HTTPS URL")?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
            {
                return Err(
                    "provider manifest endpoints must be credential-free HTTPS URLs".into(),
                );
            }
        }
        let tests = provider
            .live_tests
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if tests != WEB_PRODUCTION_PROVIDER_LIVE_TESTS.into_iter().collect()
            || provider.live_tests.values().any(|passed| !passed)
        {
            return Err(format!(
                "Live provider certification is incomplete for {}",
                provider.category
            )
            .into());
        }
    }
    Ok(())
}

fn require_secure_production_url(name: &str) -> TaskResult<reqwest::Url> {
    let raw = env::var(name).map_err(|_| format!("{name} is required"))?;
    let url = reqwest::Url::parse(&raw).map_err(|_| format!("{name} is not a valid URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(format!("{name} must be an HTTPS URL without credentials or fragment").into());
    }
    Ok(url)
}

fn validate_web_production_profile() -> TaskResult<String> {
    if env::var("ERPX_WEB_PRODUCTION_PROFILE").as_deref() != Ok("enabled") {
        return Err("ERPX_WEB_PRODUCTION_PROFILE=enabled is required".into());
    }
    if env::var("ERPX_LOG_FORMAT").as_deref() != Ok("json") {
        return Err("ERPX_LOG_FORMAT=json is required".into());
    }
    for forbidden in [
        "ERPX_DEV_PERSONA_MODE",
        "ERPX_DEVELOPMENT_CLUSTER",
        "ERPX_DEV_CONTROLLER_ROLE_PROFILE",
        "ERPX_DEV_CONTROLLER_TARGET_HOLDING",
        "ERPX_DEV_CONTROLLER_MFA_BYPASS",
        "ERPX_DEV_CONTROLLER_SOD_BYPASS",
    ] {
        if env::var_os(forbidden).is_some() {
            return Err(format!("{forbidden} is forbidden in the production profile").into());
        }
    }
    if env::var("ERPX_DEVELOPMENT_PROFILE").is_ok_and(|value| value.starts_with("local")) {
        return Err("local ERPX_DEVELOPMENT_PROFILE is forbidden in production".into());
    }
    for endpoint in [
        "ERPX_CONSOLE_AUTH_HTTP_ORIGIN",
        "ERPX_CONSOLE_GRPC_ENDPOINT",
        "ERPX_WEB_PRODUCTION_BASE_URL",
        "ERPX_WEB_PRODUCTION_HEALTH_URL",
        "ERPX_WEB_PRODUCTION_METRICS_URL",
    ] {
        require_secure_production_url(endpoint)?;
    }
    let origins =
        env::var("ERPX_WEB_TRUSTED_ORIGINS").map_err(|_| "ERPX_WEB_TRUSTED_ORIGINS is required")?;
    let origins = origins
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if origins.is_empty() || origins.len() > 16 {
        return Err("ERPX_WEB_TRUSTED_ORIGINS must contain 1..=16 HTTPS origins".into());
    }
    for origin in origins {
        let parsed = reqwest::Url::parse(origin)
            .map_err(|_| "ERPX_WEB_TRUSTED_ORIGINS contains an invalid origin")?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(
                "ERPX_WEB_TRUSTED_ORIGINS accepts HTTPS origins without path or credentials".into(),
            );
        }
    }
    let key_reference = env::var("ERPX_WEB_SESSION_KEY_REFERENCE")
        .map_err(|_| "ERPX_WEB_SESSION_KEY_REFERENCE is required")?;
    if key_reference.trim().is_empty()
        || key_reference.len() > 128
        || key_reference.starts_with("local-")
        || key_reference.chars().any(char::is_control)
    {
        return Err("ERPX_WEB_SESSION_KEY_REFERENCE must be a non-local key reference".into());
    }
    let session_key =
        env::var("ERPX_WEB_SESSION_KEY").map_err(|_| "ERPX_WEB_SESSION_KEY is required")?;
    if STANDARD
        .decode(session_key)
        .map(|decoded| decoded.len())
        .ok()
        != Some(32)
    {
        return Err("ERPX_WEB_SESSION_KEY must decode to exactly 32 bytes".into());
    }
    let signing_key = env::var("ERPX_ACCESS_TOKEN_SIGNING_KEY")
        .map_err(|_| "ERPX_ACCESS_TOKEN_SIGNING_KEY is required")?;
    if signing_key.len() < 32 {
        return Err("ERPX_ACCESS_TOKEN_SIGNING_KEY must contain at least 32 bytes".into());
    }
    let revision = env::var("ERPX_WEB_PRODUCTION_REVISION")
        .map_err(|_| "ERPX_WEB_PRODUCTION_REVISION is required")?;
    if revision.len() < 7
        || revision.len() > 128
        || revision
            .chars()
            .any(|value| value.is_control() || value.is_whitespace())
    {
        return Err("ERPX_WEB_PRODUCTION_REVISION is not a bounded immutable revision".into());
    }
    Ok(revision)
}

fn load_web_production_evidence(
    root: &Path,
    expected_revision: &str,
) -> TaskResult<WebProductionEvidence> {
    let evidence_path = env::var("ERPX_WEB_PRODUCTION_EVIDENCE")
        .map(PathBuf::from)
        .map_err(|_| "ERPX_WEB_PRODUCTION_EVIDENCE is required")?;
    let metadata =
        fs::metadata(&evidence_path).map_err(|_| "Web production evidence file is unavailable")?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 256 * 1024 {
        return Err(
            "Web production evidence must be a non-empty file no larger than 256 KiB".into(),
        );
    }
    let evidence: WebProductionEvidence = serde_json::from_slice(&fs::read(&evidence_path)?)?;
    if evidence.schema_version != 4 || evidence.release_revision != expected_revision {
        return Err("Web production evidence schema or release revision does not match".into());
    }
    let captured_at = chrono::DateTime::parse_from_rfc3339(&evidence.captured_at)
        .map_err(|_| "Web production evidence captured_at is invalid")?
        .with_timezone(&Utc);
    let age = Utc::now().signed_duration_since(captured_at);
    if age < chrono::Duration::minutes(-5) || age > chrono::Duration::days(7) {
        return Err("Web production evidence must be current within seven days".into());
    }
    validate_web_production_coverage(root, &evidence.coverage)?;
    validate_web_production_providers(&evidence.providers)?;
    let features = evidence
        .browser
        .features
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if features != WEB_PRODUCTION_FEATURES.into_iter().collect() {
        return Err(
            "Browser evidence must cover exactly the 30 registered bounded-context features".into(),
        );
    }
    let locales = evidence
        .browser
        .locales
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if !["en", "th"]
        .into_iter()
        .all(|value| locales.contains(value))
    {
        return Err("Browser evidence must cover English and Thai".into());
    }
    let viewports = evidence
        .browser
        .viewports
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if !["desktop", "390px"]
        .into_iter()
        .all(|value| viewports.contains(value))
    {
        return Err("Browser evidence must cover desktop and 390px layouts".into());
    }
    let negative_cases = evidence
        .browser
        .negative_cases
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if !["403", "404", "validation", "conflict", "duplicate", "retry"]
        .into_iter()
        .all(|value| negative_cases.contains(value))
    {
        return Err("Browser evidence is missing required negative cases".into());
    }
    let journey_features = evidence
        .browser
        .feature_journeys
        .iter()
        .map(|journey| journey.feature.as_str())
        .collect::<BTreeSet<_>>();
    if journey_features != WEB_PRODUCTION_FEATURES.into_iter().collect()
        || evidence.browser.feature_journeys.len() != WEB_PRODUCTION_FEATURES.len()
    {
        return Err(
            "Browser evidence must contain exactly one journey for every registered feature".into(),
        );
    }
    let required_positive = [
        "login",
        "scope",
        "list",
        "detail",
        "create",
        "transition",
        "read_your_writes",
        "audit",
        "session_recovery",
    ];
    let required_negative = [
        "authorization",
        "validation",
        "conflict",
        "duplicate",
        "recovery",
    ];
    for journey in &evidence.browser.feature_journeys {
        let positive = journey
            .positive_cases
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let negative = journey
            .negative_cases
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let locales = journey
            .locales
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let viewports = journey
            .viewports
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if !journey.passed
            || journey.unexplained_console_errors != 0
            || !required_positive
                .into_iter()
                .all(|case| positive.contains(case))
            || !required_negative
                .into_iter()
                .all(|case| negative.contains(case))
            || !["en", "th"]
                .into_iter()
                .all(|value| locales.contains(value))
            || !["desktop", "390px"]
                .into_iter()
                .all(|value| viewports.contains(value))
        {
            return Err(format!(
                "Browser journey evidence is incomplete for feature {}",
                journey.feature
            )
            .into());
        }
    }
    if !evidence.browser.passed
        || !evidence.security.passed
        || evidence.security.critical_findings != 0
        || evidence.security.high_findings != 0
        || !evidence.recovery.backup_restore_passed
        || !evidence.recovery.object_recovery_passed
        || !evidence.recovery.pitr_passed
        || evidence.recovery.measured_rpo_seconds > 86_400
        || evidence.recovery.measured_rto_seconds > 14_400
        || !evidence.operations.alerts_loaded
        || !evidence.operations.runbooks_verified
        || evidence.operations.projection_lag_seconds > 60
        || evidence.operations.unexplained_console_errors != 0
    {
        return Err(
            "Web production evidence did not satisfy functional, security, recovery, or operational gates"
                .into(),
        );
    }
    for required in [
        "ops/observability/erpx-web-production-alerts.yml",
        "ops/runbooks/web-production-incident.md",
        "ops/runbooks/web-production-recovery.md",
    ] {
        if !root.join(required).is_file() {
            return Err(
                format!("required production operations artifact is missing: {required}").into(),
            );
        }
    }
    Ok(evidence)
}

async fn verify(root: &Path) -> TaskResult {
    let config = DevConfig::load(root)?;
    verify_core(root)?;
    verify_local_sqlx(root, &config).await?;
    // The ignored SQLx suite deliberately exercises the dedicated cluster at
    // high concurrency. A fail-fast local worker can exhaust its small pool
    // and exit while that disposable workload is running, after the initial
    // runtime snapshot but before the readiness handoff. Reconcile the warm
    // development runtime after the suite so verification judges the final
    // service set, rather than a stale pre-suite PID snapshot.
    dev(
        root,
        &config,
        DevOptions {
            mode: DevMode::Warm,
            web_debug: false,
            hot_patch: true,
        },
    )
    .await?;
    verify_runtime_readiness(root, &config).await?;
    for (url, recovery_seconds) in [
        ("http://127.0.0.1:50052/health", 5),
        ("http://127.0.0.1:50052/ready", 5),
        // The release WASM build shares Cargo's target graph with the active
        // Dioxus watcher. Give its automatic rebuild time to settle while
        // still failing closed if the recorded Web process never recovers.
        // `/` is intentionally a typed 404 in the v2 route grammar. Probe a
        // canonical public route so readiness does not conflict with routing.
        ("http://127.0.0.1:3004/auth/sign-in", 60),
    ] {
        wait_for_http(url, recovery_seconds).await?;
    }
    verify_web_ssr_and_bff().await?;
    affected::record_verified_snapshot(root)?;
    println!("ERPX Web verification completed.");
    Ok(())
}

fn production_metric_value(body: &str, metric: &str) -> TaskResult<f64> {
    body.lines()
        .find_map(|line| {
            let (name, value) = line.rsplit_once(' ')?;
            (name == metric)
                .then(|| value.parse::<f64>().ok())
                .flatten()
        })
        .ok_or_else(|| format!("production metrics omitted {metric}").into())
}

async fn verify_web_production_endpoints() -> TaskResult {
    let base = require_secure_production_url("ERPX_WEB_PRODUCTION_BASE_URL")?;
    if base.path() != "/" || base.query().is_some() {
        return Err("ERPX_WEB_PRODUCTION_BASE_URL must be an HTTPS origin".into());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let login = client
        .get(base.join("auth/sign-in")?)
        .header(reqwest::header::ACCEPT_LANGUAGE, "th")
        .send()
        .await?;
    if !login.status().is_success() {
        return Err(format!("production login route returned {}", login.status()).into());
    }
    for (header, expected) in [
        ("strict-transport-security", "max-age="),
        ("content-security-policy", "default-src 'self'"),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        ("referrer-policy", "no-referrer"),
        ("cross-origin-opener-policy", "same-origin"),
    ] {
        let value = login
            .headers()
            .get(header)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| format!("production response omitted {header}"))?;
        if !value.contains(expected) {
            return Err(format!("production response {header} is not closed").into());
        }
    }
    let login_html = login.text().await?;
    if !login_html.contains("ERPX")
        || login_html.to_ascii_lowercase().contains("access_token")
        || login_html.to_ascii_lowercase().contains("refresh_token")
    {
        return Err("production login HTML failed the credential-boundary check".into());
    }
    let protected = client.get(base.join("finance")?).send().await?;
    if protected.status() != reqwest::StatusCode::TEMPORARY_REDIRECT
        || protected
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            != Some("/auth/sign-in?return_to=%2Ffinance")
    {
        return Err("anonymous production module route did not fail closed to login".into());
    }
    let unknown = client
        .get(base.join("__erpx-production-route-must-not-exist")?)
        .send()
        .await?;
    if unknown.status() != reqwest::StatusCode::NOT_FOUND {
        return Err(format!("unknown production route returned {}", unknown.status()).into());
    }
    let health = client
        .get(require_secure_production_url(
            "ERPX_WEB_PRODUCTION_HEALTH_URL",
        )?)
        .send()
        .await?;
    if !health.status().is_success() {
        return Err(format!("production health endpoint returned {}", health.status()).into());
    }
    let metrics = client
        .get(require_secure_production_url(
            "ERPX_WEB_PRODUCTION_METRICS_URL",
        )?)
        .send()
        .await?;
    if !metrics.status().is_success() {
        return Err(format!("production metrics endpoint returned {}", metrics.status()).into());
    }
    let metrics = metrics.text().await?;
    for (metric, expected) in [
        ("erpx_server_up", 1.0),
        ("erpx_runtime_metrics_observation_up", 1.0),
        (
            "erpx_worker_heartbeats{state=\"stale_or_unavailable\"}",
            0.0,
        ),
        ("erpx_outbox_dead_letters", 0.0),
        ("erpx_projection_dead_letters", 0.0),
    ] {
        if production_metric_value(&metrics, metric)? != expected {
            return Err(format!("production metric {metric} failed its release threshold").into());
        }
    }
    if production_metric_value(&metrics, "erpx_projection_lag_seconds")? > 60.0 {
        return Err("production projection lag exceeds 60 seconds".into());
    }
    for metric in [
        "erpx_last_verified_backup_timestamp_seconds",
        "erpx_last_verified_pitr_timestamp_seconds",
    ] {
        if production_metric_value(&metrics, metric)? <= 0.0 {
            return Err(format!("production metric {metric} has no verified evidence").into());
        }
    }
    Ok(())
}

async fn verify_web_production(root: &Path) -> TaskResult {
    // This release-only command deliberately includes the normal Web gate.
    // The production checks are additive and may never replace it.
    verify(root).await?;
    let revision = validate_web_production_profile()?;
    let _evidence = load_web_production_evidence(root, &revision)?;
    verify_web_production_endpoints().await?;
    println!("ERPX Web production verification completed for revision {revision}.");
    Ok(())
}

fn verify_core(root: &Path) -> TaskResult {
    let dx = web_toolchain::ensure(root)?;
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
    ] {
        run_command(root, program, args)?;
    }
    wasm::reconcile_release_generated_assets(root)?;
    run_command(
        root,
        dx.to_str()
            .ok_or("standard pinned Dioxus CLI path is not UTF-8")?,
        [
            "build",
            "--web",
            "--fullstack",
            "true",
            "--no-default-features",
            "--features",
            "web,web-split",
            "--wasm-split",
            "--release",
            "--locked",
            "--offline",
            "-p",
            "erpx-console",
            "--debug-symbols",
            "false",
        ],
    )?;
    wasm::compress_and_measure(root, wasm::WasmProfile::Release)?;
    architecture::scan(root)?;
    repository_scan(root)?;
    println!("ERPX offline core verification completed.");
    Ok(())
}

async fn verify_sqlx(root: &Path) -> TaskResult {
    let config = DevConfig::load(root)?;
    verify_local_sqlx(root, &config).await?;
    println!("ERPX PostgreSQL acceptance verification completed.");
    Ok(())
}

async fn verify_runtime(root: &Path) -> TaskResult {
    let config = DevConfig::load(root)?;
    verify_runtime_readiness(root, &config).await
}

async fn verify_runtime_readiness(root: &Path, config: &DevConfig) -> TaskResult {
    config.validate_local()?;
    let dedicated = config.port == 55_432;
    let development_access_enabled = development_access_requested(root)?;
    require_runtime_services(root, dedicated, development_access_enabled)?;

    let manifest = FreshRuntimeManifest::load(root)?;
    let control_write_database = manifest.as_ref().map_or(DATABASES[0], |manifest| {
        manifest.control_write_database.as_str()
    });
    let write = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url(control_write_database))
        .await
        .map_err(|_| "runtime verification could not connect to control-write")?;

    let heartbeat_rows = sqlx::query_as::<_, (Uuid, String, bool)>(
        "SELECT worker_id, status, last_seen_at >= now() - interval '60 seconds'
         FROM erpx_control.worker_heartbeats",
    )
    .fetch_all(&write)
    .await
    .map_err(|_| "runtime verification could not evaluate worker heartbeats")?;
    let heartbeat_by_id = heartbeat_rows
        .into_iter()
        .map(|(worker_id, status, fresh)| (worker_id, (status, fresh)))
        .collect::<BTreeMap<_, _>>();
    let mut unhealthy_workers = Vec::new();
    for spec in worker_specs(dedicated) {
        let worker_id = Uuid::parse_str(spec.worker_id)
            .map_err(|_| "runtime worker catalog contains an invalid worker ID")?;
        match heartbeat_by_id.get(&worker_id) {
            Some((status, true)) if status == "ready" => {}
            Some((status, fresh)) => unhealthy_workers.push(format!(
                "{} (status={}, heartbeat={})",
                spec.name,
                status,
                if *fresh { "fresh" } else { "stale" }
            )),
            None => unhealthy_workers.push(format!("{} (heartbeat=missing)", spec.name)),
        }
    }
    if !unhealthy_workers.is_empty() {
        write.close().await;
        return Err(format!(
            "runtime worker heartbeat gate failed: {}",
            unhealthy_workers.join(", ")
        )
        .into());
    }

    // Dead-letter rows remain durable evidence. A newer successful run of the
    // same projection resolves the current readiness incident without
    // rewriting that history.
    let projection_states: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            count(*) FILTER (WHERE job.state='retryable'),
            count(*) FILTER (
                WHERE job.state='dead_letter'
                  AND NOT EXISTS (
                      SELECT 1
                      FROM erpx_control.holding_projection_jobs AS recovery
                      WHERE recovery.holding_id=job.holding_id
                        AND recovery.data_profile_key=job.data_profile_key
                        AND recovery.projection_name=job.projection_name
                        AND recovery.state IN ('succeeded','rebuild_succeeded')
                        AND recovery.updated_at > job.updated_at
                  )
            ),
            count(*) FILTER (WHERE job.state='claimed' AND job.lease_until <= now()),
            count(*) FILTER (
                WHERE job.state IN ('pending', 'rebuild_pending')
                  AND job.available_at < now() - interval '60 seconds'
            )
         FROM erpx_control.holding_projection_jobs AS job",
    )
    .fetch_one(&write)
    .await
    .map_err(|_| "runtime verification could not evaluate projection jobs")?;
    write.close().await;
    if projection_states != (0, 0, 0, 0) {
        return Err(format!(
            "runtime projection gate failed: retryable={}, dead_letter={}, expired_claims={}, overdue={}",
            projection_states.0,
            projection_states.1,
            projection_states.2,
            projection_states.3
        )
        .into());
    }

    println!(
        "runtime readiness verified: {} required worker processes and heartbeats",
        worker_specs(dedicated).len()
    );
    Ok(())
}

fn verify_platforms(root: &Path) -> TaskResult {
    let android_environment = android_cross_environment()?;
    for args in [
        vec![
            "check",
            "-p",
            "erpx-console",
            "--no-default-features",
            "--features",
            "native",
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
            "native",
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
            "native",
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
        if args.contains(&"aarch64-linux-android") {
            run_command_with_env(root, "cargo", args, &android_environment)?;
        } else {
            run_command(root, "cargo", args)?;
        }
    }
    verify_macos_native_evidence(root)?;
    println!("Dioxus Native macOS and mobile compile certification completed.");
    Ok(())
}

fn android_cross_environment() -> TaskResult<Vec<(&'static str, String)>> {
    let mut candidates = ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"]
        .into_iter()
        .filter_map(|key| env::var_os(key).map(PathBuf::from))
        .chain([
            PathBuf::from("/opt/homebrew/share/android-ndk"),
            PathBuf::from("/usr/local/share/android-ndk"),
        ])
        .collect::<Vec<_>>();

    for sdk_key in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        let Some(sdk) = env::var_os(sdk_key).map(PathBuf::from) else {
            continue;
        };
        candidates.push(sdk.join("ndk-bundle"));
        let ndk_versions = sdk.join("ndk");
        if let Ok(entries) = fs::read_dir(ndk_versions) {
            let mut installed = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            installed.sort();
            candidates.extend(installed.into_iter().rev());
        }
    }

    let ndk = candidates
        .into_iter()
        .find(|candidate| candidate.join("toolchains/llvm/prebuilt").is_dir())
        .ok_or("Android NDK is required for the aarch64-linux-android certification lane")?;
    let prebuilt = fs::read_dir(ndk.join("toolchains/llvm/prebuilt"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|candidate| {
            candidate
                .join("bin/aarch64-linux-android24-clang")
                .is_file()
        })
        .ok_or("Android NDK has no API 24 aarch64 Clang toolchain")?;
    let bin = prebuilt.join("bin");
    let clang = bin.join("aarch64-linux-android24-clang");
    let archive = bin.join("llvm-ar");
    let ranlib = bin.join("llvm-ranlib");

    Ok(vec![
        ("ANDROID_NDK_HOME", ndk.display().to_string()),
        ("CC_aarch64_linux_android", clang.display().to_string()),
        ("AR_aarch64_linux_android", archive.display().to_string()),
        ("RANLIB_aarch64_linux_android", ranlib.display().to_string()),
        ("RANLIB", ranlib.display().to_string()),
        (
            "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER",
            clang.display().to_string(),
        ),
    ])
}

fn verify_macos_native_evidence(root: &Path) -> TaskResult {
    let evidence_path = root.join("audits/DIOXUS-NATIVE-MACOS/evidence.json");
    if !evidence_path.is_file() {
        return Err(
            "macOS Native certification evidence is absent; copy evidence.template.json to evidence.json and capture the M1 8 GiB suite"
                .into(),
        );
    }
    let evidence: serde_json::Value = serde_json::from_slice(&fs::read(&evidence_path)?)?;
    let text = |pointer: &str| {
        evidence
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("native evidence omitted {pointer}"))
    };
    let number = |pointer: &str| {
        evidence
            .pointer(pointer)
            .and_then(serde_json::Value::as_f64)
            .ok_or_else(|| format!("native evidence omitted {pointer}"))
    };
    let count = |pointer: &str| {
        evidence
            .pointer(pointer)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("native evidence omitted {pointer}"))
    };
    let truth = |pointer: &str| {
        evidence
            .pointer(pointer)
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| format!("native evidence omitted {pointer}"))
    };

    if evidence
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
        || !text("/host/mac_model")?.contains("M1")
        || number("/host/memory_gib")? != 8.0
        || number("/host/display_refresh_hz")? != 60.0
    {
        return Err("native evidence is not from the locked Apple M1 8 GiB / 60 Hz profile".into());
    }
    for section in [
        "screenshots",
        "startup",
        "rendering",
        "memory",
        "accessibility",
        "virtual_table",
        "crash_recovery",
    ] {
        if text(&format!("/{section}/status"))? != "passed" {
            return Err(format!("native evidence section {section} is not passed").into());
        }
    }
    if count("/startup/sample_count")? < 30
        || number("/startup/p75_cold_start_ms")? > 2_000.0
        || count("/rendering/sample_count")? < 30
        || number("/rendering/p95_input_to_paint_ms")? > 100.0
        || number("/rendering/p95_frame_ms")? > 16.7
        || number("/rendering/frames_within_two_refresh_intervals_percent")? < 99.0
        || count("/memory/sample_count")? < 30
        || number("/memory/reduction_percent")? < 20.0
    {
        return Err("native startup, rendering, or memory threshold failed".into());
    }
    for pointer in [
        "/accessibility/voiceover",
        "/accessibility/button_actions",
        "/accessibility/focus",
        "/accessibility/text_edit_actions",
        "/accessibility/dialog_focus_restoration",
        "/accessibility/selected_expanded_disabled_states",
        "/accessibility/thai_labels",
        "/virtual_table/virtualized",
        "/virtual_table/keyboard",
        "/virtual_table/focus",
        "/virtual_table/ime_thai",
        "/virtual_table/ime_english",
        "/virtual_table/voiceover",
        "/crash_recovery/passed",
    ] {
        if !truth(pointer)? {
            return Err(format!("native functional evidence failed {pointer}").into());
        }
    }
    if count("/virtual_table/row_count")? != 10_000 {
        return Err("native virtual-table evidence must cover exactly 10,000 rows".into());
    }
    for section in [
        "startup",
        "rendering",
        "memory",
        "accessibility",
        "virtual_table",
        "crash_recovery",
    ] {
        let artifact = text(&format!("/{section}/artifact"))?;
        let expected = text(&format!("/{section}/sha256"))?;
        let relative = Path::new(artifact);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            || !artifact.starts_with("audits/DIOXUS-NATIVE-MACOS/artifacts/")
        {
            return Err(format!("native evidence artifact path is unsafe: {artifact}").into());
        }
        if required_file_sha256(&root.join(relative))? != expected {
            return Err(format!("native evidence artifact hash changed: {artifact}").into());
        }
    }
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
    let root = redirect_client.get("http://127.0.0.1:3004/").send().await?;
    if root.status() != reqwest::StatusCode::TEMPORARY_REDIRECT
        || root
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            != Some("/auth/sign-in?return_to=%2Fwork")
    {
        return Err("anonymous root did not redirect before protected SSR".into());
    }
    let protected = redirect_client
        .get("http://127.0.0.1:3004/finance?ignored=1")
        .send()
        .await?;
    if protected.status() != reqwest::StatusCode::TEMPORARY_REDIRECT
        || protected
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            != Some("/auth/sign-in?return_to=%2Ffinance")
        || protected
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok())
            != Some("private, no-store")
    {
        return Err("anonymous protected page did not redirect to canonical login".into());
    }
    let legacy_login = redirect_client
        .get("http://127.0.0.1:3004/login")
        .send()
        .await?;
    if legacy_login.status() != reqwest::StatusCode::NOT_FOUND {
        return Err("legacy /login alias was not rejected".into());
    }
    let sign_in = client
        .get("http://127.0.0.1:3004/auth/sign-in")
        .header(reqwest::header::ACCEPT_LANGUAGE, "th")
        .send()
        .await?;
    if !sign_in.status().is_success() {
        return Err(format!("SSR sign-in returned {}", sign_in.status()).into());
    }
    let sign_in_html = sign_in.text().await?;
    if !sign_in_html.contains("Sign in · ERPX")
        || sign_in_html.contains("<main class=\"locale-bootstrap-surface\"")
    {
        return Err("SSR sign-in did not render the localized route content".into());
    }
    if !sign_in_html.contains("data-erpx-component=\"provider\"")
        || !sign_in_html.contains("/assets/tokens-")
        || !sign_in_html.contains("/assets/foundation-")
        || !sign_in_html.contains("/assets/console-surfaces-")
    {
        return Err(
            "canonical Dioxus SSR surface omitted the ERPX provider or style layers".into(),
        );
    }
    if !sign_in_html.contains("<form")
        || !sign_in_html.contains("method=\"post\"")
        || !sign_in_html.contains("action=\"/api/auth/password/form\"")
        || !sign_in_html.contains("type=\"submit\"")
        || !sign_in_html.contains("name=\"principal_ref\"")
        || !sign_in_html.contains("name=\"password\"")
        || !sign_in_html.contains("name=\"device_label\"")
        || sign_in_html.contains("name=\"account\"")
        || sign_in_html.contains("name=\"return_path\"")
    {
        return Err(
            "SSR password form did not render the exact typed BFF fields and submit control".into(),
        );
    }
    let native_form_entry = redirect_client
        .get("http://127.0.0.1:3004/api/auth/password/form")
        .send()
        .await?;
    if native_form_entry.status() != reqwest::StatusCode::SEE_OTHER
        || native_form_entry
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            != Some("/auth/sign-in")
    {
        return Err("native password form entry did not redirect to /auth/sign-in".into());
    }
    let native_form = redirect_client
        .post("http://127.0.0.1:3004/api/auth/password/form")
        .header(reqwest::header::ORIGIN, "http://127.0.0.1:3004")
        .form(&[
            ("principal_ref", "local:dev-controller"),
            ("password", ""),
            ("device_label", "erpx-console"),
        ])
        .send()
        .await?;
    if native_form.status() != reqwest::StatusCode::SEE_OTHER
        || native_form
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            != Some("/auth/sign-in?auth_error=invalid_request")
    {
        return Err(format!(
            "native password form did not return a safe validation redirect: {}",
            native_form.status(),
        )
        .into());
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

    for retired in ["/api/session", "/auth/bff/upgrade"] {
        let get_response = redirect_client
            .get(format!("http://127.0.0.1:3004{retired}"))
            .send()
            .await?;
        if get_response.status() != reqwest::StatusCode::NOT_FOUND {
            return Err(format!(
                "retired delivery endpoint {retired} accepted GET with {}",
                get_response.status(),
            )
            .into());
        }
        let post_response = redirect_client
            .post(format!("http://127.0.0.1:3004{retired}"))
            .send()
            .await?;
        if !matches!(
            post_response.status(),
            reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
        ) {
            return Err(format!(
                "retired delivery endpoint {retired} accepted POST with {}",
                post_response.status(),
            )
            .into());
        }
    }

    let spoofed_grpc = client
        .post("http://127.0.0.1:3004/api/internal/grpc/erpx.finance.FinanceQueryService/GetChart")
        .header(reqwest::header::ORIGIN, "http://127.0.0.1:3004")
        .header(reqwest::header::CONTENT_TYPE, "application/grpc-web+proto")
        .header("x-erpx-access-token", "browser-spoof")
        .header("x-erpx-actor", "browser-spoof")
        .header("x-erpx-session", Uuid::new_v4().to_string())
        .body(Vec::<u8>::new())
        .send()
        .await?;
    if !matches!(
        spoofed_grpc.status(),
        reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::METHOD_NOT_ALLOWED
    ) {
        return Err(format!(
            "removed arbitrary gRPC proxy returned {}, expected 404 or 405",
            spoofed_grpc.status()
        )
        .into());
    }

    let csrf_login = client
        .post("http://127.0.0.1:3004/api/auth/password")
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
    // The authenticated-authority upgrade test drops only its own identity
    // schema and additionally requires this suffix as a destructive-test
    // fence. Keep the random component short enough for PostgreSQL's
    // 63-byte identifier limit.
    let identity_upgrade_key = format!("erpx_verify_{}_identity_upgrade_test", &stamp[..16]);
    let database_keys = [
        format!("erpx_verify_cw_{stamp}"),
        format!("erpx_verify_cr_{stamp}"),
        format!("erpx_verify_tw_{stamp}"),
        format!("erpx_verify_tr_{stamp}"),
        format!("erpx_verify_dir_{stamp}"),
        format!("erpx_verify_com_{stamp}"),
        format!("erpx_verify_mkt_{stamp}"),
        format!("erpx_verify_sup_{stamp}"),
        identity_upgrade_key,
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
    let application_database_template = config.database_template();
    let application_url =
        |database: &str| application_database_template.replace("{database}", database);
    let control_write_url = verification_url(&database_keys[0]);
    let control_read_url = verification_url(&database_keys[1]);
    let holding_write_url = verification_url(&database_keys[2]);
    let holding_read_url = verification_url(&database_keys[3]);
    let directory_url = verification_url(&database_keys[4]);
    let commercial_url = verification_url(&database_keys[5]);
    let marketing_url = verification_url(&database_keys[6]);
    let support_url = verification_url(&database_keys[7]);
    let identity_upgrade_url = verification_url(&database_keys[8]);
    let application_control_write_url = application_url(&database_keys[0]);
    let application_control_read_url = application_url(&database_keys[1]);
    let application_holding_write_url = application_url(&database_keys[2]);
    let application_holding_read_url = application_url(&database_keys[3]);
    let control_write = pool(&application_control_write_url).await?;
    let control_read = pool(&application_control_read_url).await?;
    let holding_write = pool(&application_holding_write_url).await?;
    let holding_read = pool(&application_holding_read_url).await?;
    erpx_migration::migrate_control_plane(&control_write)
        .await
        .map_err(|error| format!("control-write baseline failed: {error}"))?;
    erpx_migration::migrate_control_read(&control_read)
        .await
        .map_err(|error| format!("control-read baseline failed: {error}"))?;
    erpx_migration::migrate_holding_write(&holding_write)
        .await
        .map_err(|error| format!("holding-write baseline failed: {error}"))?;
    erpx_migration::migrate_holding_read(&holding_read)
        .await
        .map_err(|error| format!("holding-read baseline failed: {error}"))?;

    let local_values = read_dotenv(&root.join(".env.local"))?;
    let minio_endpoint = required_value(&local_values, "ERPX_MINIO_ENDPOINT")?;
    let minio_access_key = required_value(&local_values, "ERPX_MINIO_ACCESS_KEY")?;
    let minio_secret_key = required_value(&local_values, "ERPX_MINIO_SECRET_KEY")?;
    let minio_region = required_value(&local_values, "ERPX_MINIO_REGION")?;
    let minio_ca = required_value(&local_values, "ERPX_MINIO_CA_CERTIFICATE_PATH")?;
    // SQLx acceptance must not depend on, or write through, the retained
    // runtime cutover manifest. The live MinIO probe creates a unique object
    // key and removes it before returning, inside this dedicated namespace.
    let object_namespace = "erpx-sqlx-acceptance".to_owned();
    let test_environment = vec![
        ("ERPX_TEST_CONTROL_WRITE_DATABASE_URL", control_write_url),
        ("ERPX_TEST_CONTROL_READ_DATABASE_URL", control_read_url),
        (
            "ERPX_TEST_HOLDING_WRITE_DATABASE_URL",
            holding_write_url.clone(),
        ),
        (
            "ERPX_TEST_HOLDING_READ_DATABASE_URL",
            holding_read_url.clone(),
        ),
        (
            "ERPX_TEST_RLS_HOLDING_WRITE_DATABASE_URL",
            application_holding_write_url,
        ),
        (
            "ERPX_TEST_RLS_HOLDING_READ_DATABASE_URL",
            application_holding_read_url,
        ),
        (
            "ERPX_TEST_DIRECTORY_RBAC_DATABASE_URL",
            directory_url.clone(),
        ),
        ("ERPX_G14_LOCAL_DIRECTORY_DATABASE_URL", directory_url),
        ("ERPX_TEST_COMMERCIAL_DATABASE_URL", commercial_url),
        ("ERPX_TEST_MARKETING_SCHEDULER_DATABASE_URL", marketing_url),
        ("ERPX_SUPPORT_TEST_DATABASE_URL", support_url),
        (
            "ERPX_TEST_IDENTITY_UPGRADE_DATABASE_URL",
            identity_upgrade_url,
        ),
        ("ERPX_TEST_ADMIN_DATABASE_URL", admin_url.clone()),
        ("ERPX_G13_DISPOSABLE_ADMIN_DATABASE_URL", admin_url.clone()),
        (
            "ERPX_TEST_STRIPE_SEMANTIC_ADMIN_DATABASE_URL",
            admin_url.clone(),
        ),
        (
            "ERPX_TEST_HOLDING_DATABASE_URL_TEMPLATE",
            verification_database_template,
        ),
        (
            "ERPX_TEST_PHYSICAL_HOLDING_WRITE_KEY",
            physical_write_key.clone(),
        ),
        (
            "ERPX_TEST_PHYSICAL_HOLDING_READ_KEY",
            physical_read_key.clone(),
        ),
        (
            "ERPX_TEST_ORGANIZATION_HOLDING_WRITE_KEY",
            organization_write_key.clone(),
        ),
        (
            "ERPX_TEST_ORGANIZATION_HOLDING_READ_KEY",
            organization_read_key.clone(),
        ),
        ("ERPX_TEST_READINESS_SUFFIX", stamp.clone()),
        (
            "ERPX_TEST_G9_INVENTORY_WRITE_DATABASE_URL",
            holding_write_url.clone(),
        ),
        (
            "ERPX_TEST_G9_INVENTORY_READ_DATABASE_URL",
            holding_read_url.clone(),
        ),
        (
            "ERPX_TEST_G9_LOGISTICS_WRITE_DATABASE_URL",
            holding_write_url.clone(),
        ),
        (
            "ERPX_TEST_G9_LOGISTICS_READ_DATABASE_URL",
            holding_read_url.clone(),
        ),
        (
            "ERPX_TEST_G9_PROCUREMENT_WRITE_DATABASE_URL",
            holding_write_url.clone(),
        ),
        (
            "ERPX_TEST_G9_PROCUREMENT_READ_DATABASE_URL",
            holding_read_url.clone(),
        ),
        ("ERPX_TEST_G9_SALES_WRITE_DATABASE_URL", holding_write_url),
        ("ERPX_TEST_G9_SALES_READ_DATABASE_URL", holding_read_url),
        (
            "ERPX_TEST_CALLBACK_SECRET",
            "local-verification-callback-secret".into(),
        ),
        ("ERPX_TEST_AGENT_MINIO_ENDPOINT", minio_endpoint),
        ("ERPX_TEST_AGENT_MINIO_ACCESS_KEY", minio_access_key),
        ("ERPX_TEST_AGENT_MINIO_SECRET_KEY", minio_secret_key),
        ("ERPX_TEST_AGENT_MINIO_REGION", minio_region),
        ("ERPX_TEST_AGENT_MINIO_NAMESPACE", object_namespace),
        ("ERPX_TEST_AGENT_MINIO_CA_CERTIFICATE_PATH", minio_ca),
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
            "-g",
            "!*.base64",
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

    let architecture_output = Command::new("rg")
        .args([
            "-n",
            "-o",
            "--hidden",
            "-g",
            "!.git/**",
            "-g",
            "!target/**",
            "-g",
            "!.runtime/**",
            "-g",
            "!*.base64",
            "TenantId|TenantScope|tenant_id|holding_ref|root_holding_id|CustomerTenantBinding|CustomerHoldingBinding|PlatformCustomerService|/platform/customers|(?i:generation[-_ ](?:v?[0-9]+|two))|erpx\\.[a-z0-9_.]+\\.v[123]\\b|erpx[-_a-z0-9]*[-_]v[123]\\b|erpxv[123]\\b|current-workspace-v[123]\\b|/api/v[123]\\b|:[vV][123]:|\\b[A-Za-z][A-Za-z0-9]*(V1|V2|V3)\\b",
            "apps",
            "crates",
            "fixtures",
            "adr",
            "audits",
            "README.md",
        ])
        .current_dir(root)
        .output()?;
    if architecture_output.status.success() {
        let output = String::from_utf8_lossy(&architecture_output.stdout);
        let violations = output
            .lines()
            .filter(|line| {
                let matched = line.rsplit(':').next().unwrap_or(line);
                !allowed_phase2_version_symbol(matched)
            })
            .collect::<Vec<_>>();
        if !violations.is_empty() {
            return Err(format!(
                "forbidden pre-Holding or version-branded architecture remains:\n{}",
                violations.join("\n")
            )
            .into());
        }
    }

    let path_output = Command::new("rg")
        .args(["--files", "apps", "crates", "fixtures", "adr", "audits"])
        .current_dir(root)
        .output()?;
    if !path_output.status.success() {
        return Err("repository path scan could not enumerate source files".into());
    }
    let path_listing = String::from_utf8_lossy(&path_output.stdout);
    let forbidden_paths = path_listing
        .lines()
        .filter(|path| forbidden_architecture_path(path))
        .collect::<Vec<_>>();
    if !forbidden_paths.is_empty() {
        return Err(format!(
            "forbidden pre-Holding or generation-branded paths remain:\n{}",
            forbidden_paths.join("\n")
        )
        .into());
    }
    Ok(())
}

fn allowed_phase2_version_symbol(symbol: &str) -> bool {
    matches!(
        symbol,
        "ItemDescriptorDraftV2"
            | "GetThreadV2"
            | "ListThreadsByRecordV2"
            | "ThreadProjectionV2"
            | "CommentProjectionV2"
            | "ThreadRetentionPolicyV2"
            | "ThreadStateV2"
            | "CommentVisibilityV2"
    )
}

fn forbidden_architecture_path(path: &str) -> bool {
    let normalized = path.to_ascii_lowercase().replace('\\', "/");
    if normalized.contains("tenant") {
        return true;
    }
    if normalized.contains("stripe") {
        return false;
    }
    normalized.split('/').any(|segment| {
        segment == "v1"
            || segment == "v2"
            || segment == "v3"
            || ["-v1", "-v2", "-v3", "_v1", "_v2", "_v3"]
                .iter()
                .any(|marker| segment.contains(marker))
    })
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

fn html_asset_urls(
    html: &str,
    tag: &str,
    required_attribute: (&str, &str),
    url_attribute: &str,
) -> Vec<String> {
    let mut urls = Vec::new();
    let needle = format!("<{tag}");
    let mut remaining = html;
    while let Some(start) = remaining.find(&needle) {
        remaining = &remaining[start + needle.len()..];
        let Some(end) = remaining.find('>') else {
            break;
        };
        let attributes = &remaining[..end];
        if html_attribute(attributes, required_attribute.0).as_deref() == Some(required_attribute.1)
            && let Some(url) = html_attribute(attributes, url_attribute)
        {
            urls.push(url);
        }
        remaining = &remaining[end + 1..];
    }
    urls
}

fn html_attribute(attributes: &str, name: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let prefix = format!("{name}={quote}");
        if let Some(start) = attributes.find(&prefix) {
            let value = &attributes[start + prefix.len()..];
            return value.find(quote).map(|end| value[..end].to_owned());
        }
    }
    None
}

fn public_asset_url(public: &Path, asset: &Path) -> TaskResult<String> {
    let relative = asset
        .strip_prefix(public)
        .map_err(|_| format!("Dioxus asset escaped public directory: {}", asset.display()))?;
    Ok(format!(
        "/{}",
        relative.to_string_lossy().replace('\\', "/")
    ))
}

fn content_type_matches(path: &str, content_type: &str) -> bool {
    let mime = content_type.split(';').next().unwrap_or_default().trim();
    if path.ends_with(".css") {
        mime == "text/css"
    } else if path.ends_with(".wasm") {
        mime == "application/wasm"
    } else if path.ends_with(".js") {
        matches!(mime, "text/javascript" | "application/javascript")
    } else {
        false
    }
}

async fn dioxus_web_readiness_once(
    root: &Path,
    client: &reqwest::Client,
    base_url: &str,
) -> TaskResult {
    let artifacts = wasm::artifact_readiness(root, wasm::WasmProfile::Dev)?;
    let login_url = reqwest::Url::parse(base_url)?.join("/auth/sign-in")?;
    let response = client.get(login_url.clone()).send().await?;
    if !response.status().is_success() {
        return Err(format!("Dioxus login SSR returned {}", response.status()).into());
    }
    let html_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !html_type.starts_with("text/html") {
        return Err(format!("Dioxus login SSR returned invalid MIME {html_type:?}").into());
    }
    let html = response.text().await?;
    if !html.contains("window.initial_dioxus_hydration_data=")
        || !html.contains("window.hydrate_queue=")
        || !html.contains("data-node-hydration=")
    {
        return Err("Dioxus login SSR omitted its hydration payload".into());
    }
    let stylesheet_urls = html_asset_urls(&html, "link", ("rel", "stylesheet"), "href");
    let module_urls = html_asset_urls(&html, "script", ("type", "module"), "src");
    if stylesheet_urls.is_empty() || module_urls.is_empty() {
        return Err("Dioxus login SSR omitted stylesheet or module-script assets".into());
    }

    let generated_paths = artifacts
        .stylesheets
        .iter()
        .chain(&artifacts.javascript)
        .map(|asset| {
            public_asset_url(&artifacts.public, asset)
                .and_then(|url| Ok(reqwest::Url::parse(base_url)?.join(&url)?.path().to_owned()))
        })
        .collect::<TaskResult<BTreeSet<_>>>()?;
    for asset_url in stylesheet_urls.iter().chain(&module_urls) {
        let path = login_url.join(asset_url)?.path().to_owned();
        if !generated_paths.contains(&path) {
            return Err(format!("Dioxus SSR references missing generated asset {path}").into());
        }
    }

    let mut urls = stylesheet_urls;
    urls.extend(module_urls);
    for wasm in &artifacts.wasm_files {
        urls.push(public_asset_url(&artifacts.public, wasm)?);
    }
    for asset_url in urls {
        let canonical_url = login_url.join(&asset_url)?;
        let route_relative_url = login_url.join(asset_url.trim_start_matches('/'))?;
        for url in [canonical_url, route_relative_url] {
            let response = client.get(url.clone()).send().await?;
            let status = response.status();
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            if !status.is_success() {
                return Err(format!("Dioxus asset {} returned {status}", url.path()).into());
            }
            if !content_type_matches(url.path(), &content_type) {
                return Err(format!(
                    "Dioxus asset {} returned invalid MIME {content_type:?}",
                    url.path()
                )
                .into());
            }
        }
    }
    Ok(())
}

async fn require_dioxus_web_ready(root: &Path, base_url: &str, seconds: u64) -> TaskResult {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut last_error = "readiness probe has not completed".to_owned();
    while Instant::now() < deadline {
        match dioxus_web_readiness_once(root, &client, base_url).await {
            Ok(()) => {
                println!("dev web artifacts/hydration: ready");
                return Ok(());
            }
            Err(error) => last_error = error.to_string(),
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(format!("Dioxus Web readiness timed out: {last_error}").into())
}

async fn wait_for_ucs_authorities(root: &Path, seconds: u64) -> TaskResult {
    let certificate = runtime_dir(root).join("ucs-lab/tls/ucs.crt");
    for _ in 0..seconds {
        let mut ready = true;
        for authority in ["platform", HOLDING_ID, RESTRICTED_HOLDING_ID] {
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

pub(crate) fn runtime_target_dir(root: &Path) -> PathBuf {
    root.join("target")
}

pub(crate) fn runtime_binary(root: &Path, name: &str) -> PathBuf {
    runtime_target_dir(root).join("debug").join(name)
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

fn configured_development_tailnet_fqdn(root: &Path) -> TaskResult<Option<String>> {
    let values = read_dotenv(&root.join(".env.local"))?;
    let legacy_ip = env::var(LEGACY_DEVELOPMENT_TAILSCALE_IP_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            values
                .get(LEGACY_DEVELOPMENT_TAILSCALE_IP_ENV)
                .filter(|value| !value.trim().is_empty())
                .cloned()
        });
    let configured = env::var(DEVELOPMENT_TAILSCALE_FQDN_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            values
                .get(DEVELOPMENT_TAILSCALE_FQDN_ENV)
                .filter(|value| !value.trim().is_empty())
                .cloned()
        });
    if configured.is_none() && legacy_ip.is_some() {
        return Err(
            "ERPX_DEV_TAILSCALE_IP is no longer supported; configure ERPX_DEV_TAILSCALE_FQDN with the machine's exact .ts.net hostname"
                .into(),
        );
    }
    configured
        .map(|value| {
            parse_development_tailnet_fqdn(&value).ok_or_else(|| {
                "ERPX_DEV_TAILSCALE_FQDN must be an exact Tailscale hostname below .ts.net".into()
            })
        })
        .transpose()
}

fn parse_development_tailnet_fqdn(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let hostname = trimmed.strip_suffix('.').unwrap_or(trimmed);
    if hostname.ends_with('.') {
        return None;
    }
    let hostname = hostname.to_ascii_lowercase();
    let labels = hostname.split('.').collect::<Vec<_>>();
    if hostname.len() > 253
        || labels.len() < 4
        || !hostname.ends_with(".ts.net")
        || labels.iter().any(|label| {
            label.is_empty()
                || label.len() > 63
                || !label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                || !label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                || !label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
    {
        return None;
    }
    Some(hostname)
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

    #[test]
    fn architecture_path_scan_is_path_aware() {
        assert!(forbidden_architecture_path("apps/server/src/tenant.rs"));
        assert!(forbidden_architecture_path(
            "crates/proto/proto/erpx/finance/v2/service.proto"
        ));
        assert!(forbidden_architecture_path(
            "audits/DIOXUS-V2-TRACEABILITY.md"
        ));
        assert!(!forbidden_architecture_path(
            "apps/console/assets/fonts/Sarabun-Regular.ttf.base64"
        ));
        assert!(!forbidden_architecture_path(
            "crates/infrastructure/src/stripe_v1_boundary.rs"
        ));
    }

    #[test]
    fn retained_legacy_manifest_is_ignored_but_unrelated_invalid_json_is_not() {
        let legacy = serde_json::json!({
            "holding_ref": "opaque-retained-reference",
            "tenant_id": "opaque-retained-id",
            "tenant_write_database": "retained_write",
            "tenant_read_database": "retained_read"
        });
        assert!(is_retained_legacy_manifest(&legacy));
        assert!(!is_retained_legacy_manifest(&serde_json::json!({
            "holding_ref": "opaque-retained-reference"
        })));
    }

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
            web_launch_sha256: "web-launch".into(),
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
            web_artifacts_ready: true,
        }
    }

    fn sample_fresh_runtime_manifest() -> FreshRuntimeManifest {
        FreshRuntimeManifest {
            version: 4,
            schema_revision: 2,
            service_catalog_generation: 4,
            service_id_range: "IFC-SVC-201..267".into(),
            state: "prepared".into(),
            prepared_at: "2026-08-05T01:02:03Z".into(),
            installation_generation: Uuid::parse_str("4fbf88d8-ea38-51b6-93f6-8cdb6c7585ec")
                .expect("generation"),
            installation_prefix: "next".into(),
            holding_id: "a5c86431-c0b8-58ea-bb5f-5af2f940918d".into(),
            data_profile_key: "ad093a73-d3d0-5532-8a03-2f2a29ca0f3c".into(),
            control_write_database: "next_control_write".into(),
            control_read_database: "next_control_read".into(),
            holding_write_database: "next_1234567890ab_holding_write".into(),
            holding_read_database: "next_1234567890ab_holding_read".into(),
            object_namespace: "next-holding-objects".into(),
            rollback: serde_json::json!({
                "retained_resource_authority": "offline_non_authoritative",
                "write_policy": "isolated_no_merge"
            }),
        }
    }

    #[test]
    fn fresh_runtime_manifest_requires_exact_v4_catalog_and_no_merge_contract() {
        let valid = sample_fresh_runtime_manifest();
        assert!(valid.validate().is_ok());

        let mut legacy = valid.clone();
        legacy.version = 1;
        assert!(legacy.validate().is_err());

        let mut legacy_schema = valid.clone();
        legacy_schema.schema_revision = 1;
        assert!(legacy_schema.validate().is_err());

        let mut retired_services = valid.clone();
        retired_services.service_id_range = "IFC-SVC-001..065".into();
        assert!(retired_services.validate().is_err());

        let mut mismatched_database = valid.clone();
        mismatched_database.holding_read_database = "retained_holding_read".into();
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
            retained.controller_target_holding,
            erpx_identity::DEVELOPMENT_CONTROLLER_TARGET_HOLDING
        );
        assert!(!retained.fresh_state);

        let manifest = sample_fresh_runtime_manifest();
        let fresh = development_access_target_from_manifest(Some(&manifest));
        assert_eq!(
            fresh.control_write_database,
            manifest.control_write_database
        );
        assert_eq!(fresh.control_read_database, manifest.control_read_database);
        assert_eq!(fresh.controller_target_holding, manifest.holding_id);
        assert!(fresh.fresh_state);
    }

    #[test]
    fn development_starts_the_complete_generation_two_worker_catalog() {
        let standard = worker_specs(false);
        assert_eq!(standard.len(), 96);
        assert_eq!(worker_specs(true).len(), 99);
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
        assert!(
            !worker_specs(true)
                .iter()
                .any(|spec| spec.name == "pitr-controller")
        );
        let projection_kinds = standard
            .iter()
            .filter_map(|spec| spec.projection_kind)
            .collect::<BTreeSet<_>>();
        assert_eq!(projection_kinds.len(), 82);
        assert!(projection_kinds.contains("quality"));
        assert!(projection_kinds.contains("quality_lifecycle_records"));
        assert!(projection_kinds.contains("identity_access_principals"));
        assert!(projection_kinds.contains("identity_access_governance"));
        assert!(projection_kinds.contains("organization_sharing"));
        assert!(projection_kinds.contains("commercial_entitlements"));
        assert!(projection_kinds.contains("commercial_lifecycle_records"));
        assert!(projection_kinds.contains("marketing_lead_candidates"));
        assert!(projection_kinds.contains("marketing_campaign_budgets"));
        assert!(projection_kinds.contains("marketing_business_records"));
        assert!(projection_kinds.contains("crm_business_records"));
        assert!(projection_kinds.contains("procurement_business_records"));
        assert!(projection_kinds.contains("procurement_workflow_records"));
        assert!(projection_kinds.contains("inventory_lifecycle_records"));
        assert!(projection_kinds.contains("customer_authority_business_records"));
        assert!(projection_kinds.contains("customer360_owned_records"));
        assert!(projection_kinds.contains("support_platform_cases"));
        assert!(projection_kinds.contains("people_payroll_time_records"));
        assert!(projection_kinds.contains("legal_cases"));
        assert!(projection_kinds.contains("shared_signatures"));
        assert!(projection_kinds.contains("manufacturing_orders"));
        assert!(projection_kinds.contains("planning_runs"));
        assert!(projection_kinds.contains("ecommerce_listings"));
        assert!(projection_kinds.contains("ecommerce_journeys"));
        assert!(projection_kinds.contains("talent_candidate"));
        assert!(projection_kinds.contains("talent_workforce"));
        assert!(projection_kinds.contains("learning_enrollments"));
        assert!(projection_kinds.contains("benefits_enrollments"));
        assert!(projection_kinds.contains("field_service_orders"));
        assert!(projection_kinds.contains("projects_projects"));
        assert!(projection_kinds.contains("projects_hierarchy"));
        assert!(projection_kinds.contains("collaboration_threads"));
        assert!(projection_kinds.contains("collaboration_stream"));
        assert!(projection_kinds.contains("collaboration_inbox"));
        assert!(projection_kinds.contains("item_directory"));
        assert!(projection_kinds.contains("maintenance_asset_facts"));
        assert!(projection_kinds.contains("asset_lifecycle_records"));
        assert!(projection_kinds.contains("maintenance_lifecycle_records"));
        assert!(projection_kinds.contains("people_payroll_payroll_runs"));
        assert!(projection_kinds.contains("pos_refunds"));
        assert!(projection_kinds.contains("pos_lifecycle_records"));
        assert!(projection_kinds.contains("finance_charts"));
        assert!(projection_kinds.contains("finance_customer_invoices"));
        assert!(projection_kinds.contains("finance_revenue_schedules"));
        assert!(projection_kinds.contains("finance_business_records"));
        assert!(projection_kinds.contains("finance_vendor_invoices"));
        assert!(projection_kinds.contains("finance_payments"));
        assert!(projection_kinds.contains("legal_access_history"));
        assert!(projection_kinds.contains("legal_matters"));
        assert!(projection_kinds.contains("sales_revenue_arrangements"));
        assert!(projection_kinds.contains("sales_lifecycle_records"));
        assert!(projection_kinds.contains("support_capabilities"));
        assert!(projection_kinds.contains("operations_exceptions"));
        assert!(projection_kinds.contains("operations_lifecycle_records"));
        assert!(projection_kinds.contains("operations_source_signals"));
        assert!(projection_kinds.contains("operations_work_queue_policies"));
        assert!(standard.iter().any(|spec| {
            spec.handler == "operations_source_ingestion" && spec.projection_kind.is_none()
        }));
        assert!(
            standard
                .iter()
                .any(|spec| { spec.handler == "procure_to_pay" && spec.projection_kind.is_none() })
        );
        assert!(standard.iter().any(|spec| {
            spec.handler == "marketing_approval_decisions" && spec.projection_kind.is_none()
        }));
        assert!(standard.iter().any(|spec| {
            spec.handler == "customer360_control_customers_projection"
                && spec.projection_kind.is_none()
        }));
        assert!(standard.iter().any(|spec| {
            spec.handler == "customer360_holding_activity_projection"
                && spec.projection_kind.is_none()
        }));
        assert!(
            standard
                .iter()
                .filter(|spec| spec.projection_kind.is_some())
                .all(|spec| spec.handler == "holding_projection_scheduler")
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
    fn development_cli_flags_accept_any_order_and_reject_invalid_input() {
        assert_eq!(
            parse_dev_options(&args(&["dev", "--detach"])).expect("default dev options"),
            Some(DevOptions {
                mode: DevMode::Warm,
                web_debug: false,
                hot_patch: false,
            })
        );
        assert_eq!(
            parse_dev_options(&args(&["dev", "--web-debug", "--fresh", "--detach",]))
                .expect("reordered dev options"),
            Some(DevOptions {
                mode: DevMode::Fresh,
                web_debug: true,
                hot_patch: false,
            })
        );
        assert_eq!(
            parse_dev_options(&args(&["dev", "--detach", "--no-hot-patch"]))
                .expect("stable fallback options"),
            Some(DevOptions {
                mode: DevMode::Warm,
                web_debug: false,
                hot_patch: false,
            })
        );
        assert_eq!(
            parse_dev_options(&args(&["dev", "--detach", "--hot-patch"]))
                .expect("hot-patch opt-in options"),
            Some(DevOptions {
                mode: DevMode::Warm,
                web_debug: false,
                hot_patch: true,
            })
        );
        assert!(parse_dev_options(&args(&["dev"])).is_err());
        assert!(parse_dev_options(&args(&["dev", "--detach", "--detach"])).is_err());
        assert!(parse_dev_options(&args(&["dev", "--detach", "--reset"])).is_err());
        assert_eq!(
            parse_dev_options(&args(&["status"])).expect("non-dev command"),
            None
        );
    }

    #[test]
    fn web_arguments_use_stable_hot_reload_by_default_and_allow_hot_patch_opt_in() {
        let root = Path::new("/workspace/erpx");
        let arguments = web_serve_arguments(
            root,
            "127.0.0.1",
            DevOptions {
                mode: DevMode::Warm,
                web_debug: false,
                hot_patch: true,
            },
        );
        assert!(arguments.iter().any(|value| value == "--hot-patch"));
        assert!(!arguments.iter().any(|value| value == "--wasm-split"));
        assert!(!arguments.iter().any(|value| value == "web-split"));
        assert!(
            arguments
                .windows(2)
                .any(|values| { values == ["--debug-symbols", "false"] })
        );
        assert!(arguments.windows(2).any(|values| {
            values
                == [
                    "--session-cache-dir",
                    "/workspace/erpx/target/dx-hotpatch/0.8.0-alpha.1-monolithic-v1",
                ]
        }));

        let debug = web_serve_arguments(
            root,
            "127.0.0.1",
            DevOptions {
                mode: DevMode::Fresh,
                web_debug: true,
                hot_patch: true,
            },
        );
        assert!(debug.iter().any(|value| value == "--hot-patch"));
        assert!(!debug.iter().any(|value| value == "--wasm-split"));
        assert!(
            debug
                .windows(2)
                .any(|values| { values == ["--debug-symbols", "true"] })
        );

        let stable = web_serve_arguments(
            root,
            "127.0.0.1",
            DevOptions {
                mode: DevMode::Warm,
                web_debug: false,
                hot_patch: false,
            },
        );
        assert!(!stable.iter().any(|value| value == "--wasm-split"));
        assert!(!stable.iter().any(|value| value == "web-split"));
        assert!(
            stable
                .windows(2)
                .any(|values| values == ["--hot-patch", "false"])
        );
        assert!(
            stable
                .windows(2)
                .any(|values| values == ["--hot-reload", "true"])
        );
    }

    #[test]
    fn dioxus_asset_parser_selects_stylesheets_and_module_scripts() {
        let html = r#"<link rel="stylesheet" href="/assets/app.css"><link rel="icon" href="/favicon.ico"><script>inline()</script><script type="module" src="/wasm/app.js"></script>"#;
        assert_eq!(
            html_asset_urls(html, "link", ("rel", "stylesheet"), "href"),
            ["/assets/app.css"]
        );
        assert_eq!(
            html_asset_urls(html, "script", ("type", "module"), "src"),
            ["/wasm/app.js"]
        );
        assert!(content_type_matches(
            "/assets/app.css",
            "text/css; charset=utf-8"
        ));
        assert!(content_type_matches("/wasm/app.js", "text/javascript"));
        assert!(content_type_matches("/wasm/app.wasm", "application/wasm"));
        assert!(!content_type_matches("/wasm/app.wasm", "text/plain"));
    }

    #[test]
    fn native_runtime_binaries_use_the_workspace_target() {
        assert_eq!(
            runtime_binary(Path::new("/workspace/erpx"), "erpx-server"),
            PathBuf::from("/workspace/erpx/target/debug/erpx-server")
        );
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

        let mut desired = previous.clone();
        desired.web_launch_sha256 = "new-web-launch".into();
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
            web_artifacts_ready: false,
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
    fn running_web_with_missing_artifacts_restarts_only_web() {
        let state = sample_dev_state();
        let runtime = RuntimeSnapshot {
            web_artifacts_ready: false,
            ..healthy_runtime()
        };
        assert_eq!(
            plan_dev_restart(DevMode::Warm, Some(&state), &state, runtime),
            DevRestartPlan {
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
    fn tailscale_development_fqdn_accepts_only_exact_ts_net_hostnames() {
        assert_eq!(
            parse_development_tailnet_fqdn("Fluke-MacBook16.tailb48369.ts.net."),
            Some("fluke-macbook16.tailb48369.ts.net".into())
        );
        for hostname in [
            "100.97.9.56",
            "https://fluke-macbook16.tailb48369.ts.net",
            "*.tailb48369.ts.net",
            "tailb48369.ts.net",
            "fluke-macbook16.tailb48369.ts.net:443",
            "fluke-macbook16.tailb48369.ts.net/login",
            "fluke-macbook16.tailb48369.ts.net..",
            "tail.example.test",
        ] {
            assert_eq!(
                parse_development_tailnet_fqdn(hostname),
                None,
                "accepted {hostname}"
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
        let allowed = ["public", "folio"].into_iter().map(str::to_owned).collect();
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

    #[test]
    fn production_evidence_example_is_versioned_and_covers_every_feature() {
        let evidence: WebProductionEvidence = serde_json::from_str(include_str!(
            "../../ops/web-production-evidence.example.json"
        ))
        .expect("versioned production evidence example");
        assert_eq!(evidence.schema_version, 4);
        assert!(evidence.browser.passed);
        assert_eq!(
            evidence
                .browser
                .features
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            WEB_PRODUCTION_FEATURES.into_iter().collect()
        );
        assert!(evidence.recovery.backup_restore_passed);
        assert!(evidence.recovery.object_recovery_passed);
        assert!(evidence.recovery.pitr_passed);
        assert_eq!(
            evidence
                .browser
                .feature_journeys
                .iter()
                .map(|journey| journey.feature.as_str())
                .collect::<BTreeSet<_>>(),
            WEB_PRODUCTION_FEATURES.into_iter().collect()
        );
        assert!(
            evidence
                .browser
                .feature_journeys
                .iter()
                .all(|journey| journey.passed && journey.unexplained_console_errors == 0)
        );
        assert_eq!(evidence.coverage.requirement_outcomes.len(), 644);
        assert_eq!(evidence.coverage.scenario_outcomes.len(), 68);
        assert_eq!(evidence.coverage.workspaces.len(), 21);
        assert_eq!(evidence.providers.len(), 9);
        assert!(
            evidence
                .coverage
                .requirement_outcomes
                .values()
                .all(|outcome| outcome == "not_executed")
        );
    }

    #[test]
    fn production_metric_parser_requires_exact_metric_names() {
        let body = "erpx_server_up 1\nerpx_projection_lag_seconds 4.250\n";
        assert_eq!(
            production_metric_value(body, "erpx_server_up").unwrap(),
            1.0
        );
        assert_eq!(
            production_metric_value(body, "erpx_projection_lag_seconds").unwrap(),
            4.25
        );
        assert!(production_metric_value(body, "erpx_server").is_err());
    }
}
