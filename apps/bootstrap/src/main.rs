use std::{
    env, fs, net::IpAddr, path::PathBuf, str::FromStr, sync::Arc, time::Duration as StdDuration,
};

use chrono::{Duration, Utc};
use erpx_application::{RoleLifecycleService, SystemClock};
use erpx_domain::{
    access::{RiskTier, RoleAssignment, Scope},
    data_plane::BusinessUnitId,
    tenant_administration::CustomerTenantBinding,
};
use erpx_identity::{SqlxIdentityStore, apply_identity_migration};
use erpx_infrastructure::{
    ControlPlanePools, DatabaseProfileRegistration, FRESH_INITIAL_TENANT_PROJECTIONS,
    MinioObjectStoreConfig, ProjectionJobKind, ProjectionPolicy, ProjectionScheduleRequest,
    ProjectionSchedulerAuthorization, SqlxBranchProvisioningStore, SqlxControlPlane,
    SqlxTenantAdministrationStore, SqlxTenantStoreRegistry, TenantDatabaseHealth,
    TenantHoldingBinding, TenantProjectionSchedulerRepository, TenantProvisioner,
    TenantStorageHealth, initialize_tenant_organization_in_pools,
};
use serde::Serialize;
use sqlx::{
    Connection, Executor, PgConnection, PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions, PgSslMode},
};
use uuid::Uuid;

mod bootstrap_progress;
mod cutover_manifest;
mod fresh_state;
mod resource_guard;
mod typed_seed;

use bootstrap_progress::{BootstrapPhase, BootstrapProgressMarker};
use cutover_manifest::RuntimeCutoverManifest;
use fresh_state::{
    DatabaseInventoryEntry, FreshResourcePlan, require_fresh_targets, require_unused_prefix,
    validate_development_endpoint,
};
use resource_guard::{
    FreshNamespacePlan, LoopbackMinioNamespaceAdmin, NamespaceVerificationReceipt,
    OldResourceInventory, RollbackMetadata, create_and_verify_new_namespace,
    verify_new_namespace_absent,
};

const HELP: &str = "ERPX first-install bootstrap\n\n\
USAGE:\n  erpx-bootstrap <check|bootstrap|status>\n\n\
The command reads ERPX_SETUP_PREFIX, ERPX_DATABASE_PROFILE_DEFAULT_URL,\n\
ERPX_DATABASE_PROFILE_DEFAULT_USERNAME, ERPX_DATABASE_PROFILE_DEFAULT_PASSWORD,\n\
ERPX_SETUP_DATABASE_ADMIN_USERNAME, ERPX_SETUP_DATABASE_ADMIN_PASSWORD,\n\
ERPX_SETUP_TENANT_NAME, ERPX_SETUP_HOLDING_NAME, ERPX_SETUP_ROOT_PRINCIPAL, and\n\
ERPX_SETUP_HOLDING_OWNER_PRINCIPAL from the environment. Fresh object storage\n\
also requires ERPX_SETUP_MINIO_ENDPOINT, credentials, ERPX_SETUP_WORKSPACE_ROOT,\n\
and the ERPX_SETUP_RETAINED_* rollback inventory. Database URLs must not contain\n\
credentials. Plaintext bootstrap credentials are printed only when a principal\n\
is first created.\n";

const ROOT_PERMISSIONS: &[&str] = &[
    "commercial.entitlement.activate",
    "commercial.entitlement.allocate",
    "commercial.billing.read",
    "diagnostics:read",
    "platform.customer.access_audit.read",
    "platform.customer.notes.write",
    "platform.customer.read",
    "platform.brand.lock",
    "platform.brand.manage",
    "platform.brand.publish",
    "platform.brand.read",
    "platform.directory.activate",
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
];

const OWNER_PERMISSIONS: &[&str] = &[
    "branch.infrastructure.provision",
    "business-unit.brand.manage",
    "business-unit.brand.publish",
    "business-unit.brand.read",
    "business-unit.configuration.manage",
    "business-unit.configuration.read",
    "organization.admin",
    "organization.read",
    "role.assignment.approve",
    "role.assignment.read",
    "role.assignment.recover",
    "role.assignment.renew",
    "role.assignment.review",
    "tenant.directory.activate",
    "tenant.directory.manage",
    "tenant.directory.read",
    "tenant.infrastructure.apply",
    "tenant.infrastructure.approve",
    "tenant.infrastructure.propose",
    "tenant.infrastructure.read",
];

#[derive(Debug)]
struct BootstrapError(&'static str);

impl std::fmt::Display for BootstrapError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for BootstrapError {}

type Result<T> = std::result::Result<T, BootstrapError>;

#[derive(Clone)]
struct SetupConfig {
    resources: FreshResourcePlan,
    prefix: String,
    admin_options: PgConnectOptions,
    runtime_options: PgConnectOptions,
    runtime_database_owner: String,
    tenant_name: String,
    holding_name: String,
    root_principal: String,
    owner_principal: String,
    tenant_id: String,
    holding_ref: String,
    holding_id: Uuid,
    customer_id: Uuid,
    control_write: String,
    control_read: String,
    tenant_write: String,
    tenant_read: String,
}

impl std::fmt::Debug for SetupConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SetupConfig")
            .field("prefix", &self.prefix)
            .field("tenant_id", &self.tenant_id)
            .field("holding_ref", &self.holding_ref)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct CheckReport<'a> {
    valid: bool,
    schema_generation: i16,
    prefix: &'a str,
    installation_generation: Uuid,
    tenant_id: &'a str,
    holding_ref: &'a str,
    tls: &'static str,
    database_create_allowed: bool,
    runtime_database_role_ready: bool,
    existing_database_count: i64,
    fresh_targets: bool,
    databases: &'a [DatabaseInventoryEntry],
    unexpected_prefix_databases: &'a [String],
    object_namespace_fresh: bool,
    retained_database_count: usize,
}

#[derive(Serialize)]
struct BootstrapCompleteReport<'a> {
    state: &'static str,
    schema_generation: i16,
    service_catalog_generation: u16,
    service_id_range: &'static str,
    installation_generation: Uuid,
    tenant_id: &'a str,
    holding_ref: &'a str,
    resources: &'a FreshResourcePlan,
    object_namespace: &'a NamespaceVerificationReceipt,
    rollback: &'a RollbackMetadata,
    runtime_manifest: String,
    seed_policy: [&'static str; 5],
    copied_previous_application_state: bool,
    mutated_previous_resources: bool,
}

struct FreshObjectSetup {
    endpoint: String,
    access_key: String,
    secret_key: String,
    region: String,
    ca_certificate_pem: Option<Vec<u8>>,
    workspace_root: PathBuf,
    old_resources: OldResourceInventory,
}

#[derive(Serialize)]
struct StatusReport {
    installed: bool,
    state: String,
    prefix: String,
    tenant_id: Option<String>,
    holding_ref: Option<String>,
    completed_steps: Vec<String>,
    failure_category: Option<String>,
    bootstrap_version: Option<u64>,
    progress_generation: Option<Uuid>,
    progress_phase: Option<&'static str>,
    progress_state: Option<&'static str>,
    progress_failure_category: Option<&'static str>,
}

#[tokio::main]
async fn main() {
    let command = env::args().nth(1);
    if matches!(command.as_deref(), None | Some("-h") | Some("--help")) {
        println!("{HELP}");
        return;
    }
    let result = match command.as_deref() {
        Some("check") => check().await,
        Some("bootstrap") => bootstrap().await,
        Some("status") => status().await,
        _ => Err(BootstrapError("unknown command; use --help for usage")),
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
}

async fn check() -> Result<()> {
    let config = SetupConfig::from_env()?;
    let object_setup = FreshObjectSetup::from_env()?;
    RuntimeCutoverManifest::require_absent(&object_setup.workspace_root)?;
    let namespace_plan = FreshNamespacePlan::new(
        &object_setup.endpoint,
        &config.resources.prefix,
        &config.resources.storage_namespace,
        &object_setup.old_resources,
    )
    .map_err(|_| BootstrapError("fresh object namespace validation failed"))?;
    let namespace_admin = LoopbackMinioNamespaceAdmin::new(
        namespace_plan.endpoint(),
        MinioObjectStoreConfig {
            endpoint: object_setup.endpoint.clone(),
            access_key: object_setup.access_key.clone(),
            secret_key: object_setup.secret_key.clone(),
            region: object_setup.region.clone(),
        },
        object_setup.ca_certificate_pem.as_deref(),
    )
    .map_err(|_| BootstrapError("fresh object namespace client could not be configured"))?;
    let object_namespace_fresh = verify_new_namespace_absent(
        &namespace_plan,
        &namespace_admin,
        StdDuration::from_secs(30),
    )
    .await
    .is_ok();
    let mut admin = PgConnection::connect_with(&config.admin_options)
        .await
        .map_err(|_| BootstrapError("database connectivity validation failed"))?;
    let database_create_allowed: bool = sqlx::query_scalar(
        "SELECT r.rolcreatedb OR r.rolsuper FROM pg_roles r WHERE r.rolname=current_user",
    )
    .fetch_one(&mut admin)
    .await
    .map_err(|_| BootstrapError("database privilege validation failed"))?;
    let runtime_database_role_ready: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1 AND rolcanlogin)",
    )
    .bind(&config.runtime_database_owner)
    .fetch_one(&mut admin)
    .await
    .map_err(|_| BootstrapError("runtime database role validation failed"))?;
    let (inventory, prefix_collisions) = inspect_database_inventory(&mut admin, &config).await?;
    let existing_database_count =
        i64::try_from(inventory.iter().filter(|database| database.exists).count())
            .map_err(|_| BootstrapError("database inventory validation failed"))?;
    let fresh_targets = require_fresh_targets(&inventory).is_ok()
        && require_unused_prefix(&prefix_collisions).is_ok();
    let valid = database_create_allowed
        && runtime_database_role_ready
        && fresh_targets
        && object_namespace_fresh;
    emit(&CheckReport {
        valid,
        schema_generation: erpx_migration::SCHEMA_GENERATION,
        prefix: &config.prefix,
        installation_generation: config.resources.installation_generation,
        tenant_id: &config.tenant_id,
        holding_ref: &config.holding_ref,
        tls: if is_local_host(config.admin_options.get_host()) {
            "loopback"
        } else {
            "verify-full"
        },
        database_create_allowed,
        runtime_database_role_ready,
        existing_database_count,
        fresh_targets,
        databases: &inventory,
        unexpected_prefix_databases: &prefix_collisions,
        object_namespace_fresh,
        retained_database_count: object_setup.old_resources.database_names().len(),
    })?;
    require_valid_preflight(valid)
}

async fn bootstrap() -> Result<()> {
    let config = SetupConfig::from_env()?;
    let object_setup = FreshObjectSetup::from_env()?;
    RuntimeCutoverManifest::require_absent(&object_setup.workspace_root)?;
    let namespace_plan = FreshNamespacePlan::new(
        &object_setup.endpoint,
        &config.resources.prefix,
        &config.resources.storage_namespace,
        &object_setup.old_resources,
    )
    .map_err(|_| BootstrapError("fresh object namespace validation failed"))?;
    let mut admin = PgConnection::connect_with(&config.admin_options)
        .await
        .map_err(|_| BootstrapError("database connectivity validation failed"))?;
    let lock_key = format!("erpx-bootstrap:{}", config.prefix);
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1, 0))")
        .bind(&lock_key)
        .execute(&mut admin)
        .await
        .map_err(|_| BootstrapError("bootstrap lock could not be acquired"))?;

    let database_create_allowed: bool = sqlx::query_scalar(
        "SELECT r.rolcreatedb OR r.rolsuper FROM pg_roles r WHERE r.rolname=current_user",
    )
    .fetch_one(&mut admin)
    .await
    .map_err(|_| BootstrapError("database privilege validation failed"))?;
    if !database_create_allowed {
        return Err(BootstrapError(
            "database principal is not allowed to create fresh targets",
        ));
    }
    let runtime_database_role_ready: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1 AND rolcanlogin)",
    )
    .bind(&config.runtime_database_owner)
    .fetch_one(&mut admin)
    .await
    .map_err(|_| BootstrapError("runtime database role validation failed"))?;
    if !runtime_database_role_ready {
        return Err(BootstrapError(
            "runtime database role must exist and permit login",
        ));
    }
    let (inventory, prefix_collisions) = inspect_database_inventory(&mut admin, &config).await?;
    require_fresh_targets(&inventory)?;
    require_unused_prefix(&prefix_collisions)?;

    let namespace_admin = LoopbackMinioNamespaceAdmin::new(
        namespace_plan.endpoint(),
        MinioObjectStoreConfig {
            endpoint: object_setup.endpoint.clone(),
            access_key: object_setup.access_key.clone(),
            secret_key: object_setup.secret_key.clone(),
            region: object_setup.region.clone(),
        },
        object_setup.ca_certificate_pem.as_deref(),
    )
    .map_err(|_| BootstrapError("fresh object namespace client could not be configured"))?;
    verify_new_namespace_absent(
        &namespace_plan,
        &namespace_admin,
        StdDuration::from_secs(30),
    )
    .await
    .map_err(|_| BootstrapError("fresh object namespace preflight failed"))?;

    let mut progress =
        bootstrap_progress::begin(&object_setup.workspace_root, &config.resources, Utc::now())?;

    for name in config.database_names() {
        create_fresh_database(&mut admin, name, &config.runtime_database_owner).await?;
    }

    progress.update_phase(BootstrapPhase::DatabaseMigration, Utc::now())?;

    let control_write = connect_database(&config, &config.control_write).await?;
    let control_read = connect_database(&config, &config.control_read).await?;
    let tenant_write = connect_database(&config, &config.tenant_write).await?;
    let tenant_read = connect_database(&config, &config.tenant_read).await?;
    erpx_migration::migrate_control_plane(&control_write)
        .await
        .map_err(|_| BootstrapError("control write migration failed"))?;
    erpx_migration::migrate_control_read(&control_read)
        .await
        .map_err(|_| BootstrapError("control read migration failed"))?;
    apply_identity_migration(&control_write)
        .await
        .map_err(|_| BootstrapError("identity migration failed"))?;
    erpx_migration::migrate_tenant_write(&tenant_write)
        .await
        .map_err(|_| BootstrapError("tenant write migration failed"))?;
    erpx_migration::migrate_tenant_read(&tenant_read)
        .await
        .map_err(|_| BootstrapError("tenant read migration failed"))?;
    verify_generation_v2_database_roles(&control_write, &control_read, &tenant_write, &tenant_read)
        .await?;

    progress.update_phase(BootstrapPhase::ObjectNamespaceCreation, Utc::now())?;

    let namespace_receipt = create_and_verify_new_namespace(
        &namespace_plan,
        &namespace_admin,
        Utc::now(),
        StdDuration::from_secs(30),
    )
    .await
    .map_err(|_| BootstrapError("fresh object namespace could not be verified"))?;
    config.resources.require_matching_storage_receipt(
        namespace_receipt.prefix(),
        namespace_receipt.namespace(),
    )?;

    progress.update_phase(BootstrapPhase::ControlRegistration, Utc::now())?;

    let control = Arc::new(SqlxControlPlane::new(ControlPlanePools {
        write: control_write.clone(),
        read: control_read.clone(),
    }));
    register_tenant_and_receipt(&config, &control_write).await?;
    let profiles = SqlxBranchProvisioningStore::new(control.clone());
    profiles
        .register_profile(&DatabaseProfileRegistration {
            profile_key: "default".into(),
            database_prefix: config.prefix.clone(),
            deployment_mode: "erpx_managed".into(),
            credential_profile_key: "default".into(),
            state: "active".into(),
            profile_version: 1,
            created_by: config.root_principal.clone(),
        })
        .await
        .map_err(|_| BootstrapError("default database profile registration failed"))?;
    profiles
        .bind_profile(&config.tenant_id, "default", &config.root_principal)
        .await
        .map_err(|_| BootstrapError("default database profile binding failed"))?;
    upsert_receipt(&config, &control_write, "profiles_registered").await?;

    progress.update_phase(BootstrapPhase::PrincipalSeed, Utc::now())?;

    let identity = SqlxIdentityStore::new(control_write.clone());
    let root_credentials =
        ensure_principal(&identity, &control_write, &config.root_principal).await?;
    let owner_credentials =
        ensure_principal(&identity, &control_write, &config.owner_principal).await?;
    sqlx::query(
        "INSERT INTO erpx_identity.protected_local_principals
            (principal_id, protection_kind, designated_by)
         SELECT id, 'platform_root', 'bootstrap:system'
         FROM erpx_identity.principals WHERE principal_ref=$1
         ON CONFLICT (principal_id) DO NOTHING",
    )
    .bind(&config.root_principal)
    .execute(&control_write)
    .await
    .map_err(|_| BootstrapError("Platform Root protection failed"))?;
    ensure_roles(&config, control.clone()).await?;
    ensure_only_bootstrap_principals(&config, &control_write).await?;
    upsert_receipt(&config, &control_write, "principals_and_roles_seeded").await?;

    progress.update_phase(BootstrapPhase::BusinessSeed, Utc::now())?;

    seed_customer_and_units(&config, control.clone()).await?;
    verify_neutral_brand_seed(&control_write).await?;
    typed_seed::seed_typed_neutral_brand(&control_write, &config.root_principal).await?;
    upsert_receipt(
        &config,
        &control_write,
        "registry_units_and_neutral_brand_seeded",
    )
    .await?;
    progress.update_phase(BootstrapPhase::OrganizationSeed, Utc::now())?;
    initialize_tenant_organization_in_pools(
        &tenant_write,
        &tenant_read,
        &config.tenant_id,
        &config.holding_ref,
        config.holding_id,
        &config.owner_principal,
        Utc::now(),
    )
    .await
    .map_err(|_| BootstrapError("holding organization initialization failed"))?;
    upsert_receipt(&config, &control_write, "organization_seeded").await?;

    progress.update_phase(BootstrapPhase::TenantActivation, Utc::now())?;

    activate_tenant(&config, control.clone(), &namespace_receipt).await?;
    schedule_initial_projection_jobs(&config, control.clone()).await?;
    upsert_receipt(&config, &control_write, "initial_projection_jobs_scheduled").await?;

    let rollback = RollbackMetadata::new(Utc::now(), object_setup.old_resources)
        .map_err(|_| BootstrapError("rollback metadata could not be created"))?;
    progress.update_phase(BootstrapPhase::ManifestPublication, Utc::now())?;
    let runtime_manifest = RuntimeCutoverManifest::new(&config.resources, Utc::now(), &rollback)?;
    // Publish the non-secret runtime handoff before committing the database
    // receipt. Consumers require both artifacts, so an interrupted bootstrap
    // can never make a prepared manifest authoritative on its own.
    let runtime_manifest_path = runtime_manifest.write_once(&object_setup.workspace_root)?;
    progress.update_phase(BootstrapPhase::ReceiptCompletion, Utc::now())?;
    sqlx::query(
        "UPDATE erpx_control.platform_bootstrap_receipts
         SET state='complete', completed_at=COALESCE(completed_at, now()),
             completed_steps=(SELECT ARRAY(SELECT DISTINCT value FROM unnest(
                 completed_steps || ARRAY['tenant_activated']) AS value ORDER BY value)),
             failure_category=NULL, updated_at=now()
         WHERE installation_prefix=$1",
    )
    .bind(&config.prefix)
    .execute(&control_write)
    .await
    .map_err(|_| BootstrapError("bootstrap receipt completion failed"))?;
    progress.complete(Utc::now())?;
    emit(&BootstrapCompleteReport {
        state: "complete",
        schema_generation: erpx_migration::SCHEMA_GENERATION,
        service_catalog_generation: 3,
        service_id_range: "IFC-SVC-101..165",
        installation_generation: config.resources.installation_generation,
        tenant_id: &config.tenant_id,
        holding_ref: &config.holding_ref,
        resources: &config.resources,
        object_namespace: &namespace_receipt,
        rollback: &rollback,
        runtime_manifest: runtime_manifest_path.display().to_string(),
        seed_policy: [
            "platform_root",
            "holding_owner",
            "tenant_holding_registry",
            "canonical_business_units",
            "neutral_brand",
        ],
        copied_previous_application_state: false,
        mutated_previous_resources: false,
    })?;
    print_credentials("Platform Root", root_credentials);
    print_credentials("Holding Owner", owner_credentials);
    Ok(())
}

async fn verify_generation_v2_database_roles(
    control_write: &PgPool,
    control_read: &PgPool,
    tenant_write: &PgPool,
    tenant_read: &PgPool,
) -> Result<()> {
    for (pool, role) in [
        (control_write, erpx_migration::DatabaseRole::ControlWrite),
        (control_read, erpx_migration::DatabaseRole::ControlRead),
        (tenant_write, erpx_migration::DatabaseRole::TenantWrite),
        (tenant_read, erpx_migration::DatabaseRole::TenantRead),
    ] {
        erpx_migration::verify_schema_manifest(pool, role)
            .await
            .map_err(|_| BootstrapError("database schema generation verification failed"))?;
    }
    Ok(())
}

async fn status() -> Result<()> {
    let config = SetupConfig::from_env()?;
    let workspace_root = PathBuf::from(required("ERPX_SETUP_WORKSPACE_ROOT")?);
    let progress = bootstrap_progress::read(&workspace_root)?;
    if let Some(marker) = progress.as_ref() {
        marker.require_plan(&config.resources)?;
    }
    let mut admin = PgConnection::connect_with(&config.admin_options)
        .await
        .map_err(|_| BootstrapError("database connectivity validation failed"))?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(&config.control_write)
            .fetch_one(&mut admin)
            .await
            .map_err(|_| BootstrapError("bootstrap status could not be read"))?;
    if !exists {
        return emit(&StatusReport {
            installed: false,
            state: "not_started".into(),
            prefix: config.prefix,
            tenant_id: None,
            holding_ref: None,
            completed_steps: Vec::new(),
            failure_category: None,
            bootstrap_version: None,
            progress_generation: progress.as_ref().map(BootstrapProgressMarker::generation),
            progress_phase: progress
                .as_ref()
                .map(|marker| marker.current_phase().as_str()),
            progress_state: progress.as_ref().map(|marker| marker.state().as_str()),
            progress_failure_category: progress
                .as_ref()
                .and_then(BootstrapProgressMarker::failure_category)
                .map(|category| category.as_str()),
        });
    }
    let pool = connect_database(&config, &config.control_write).await?;
    let table_exists: bool = sqlx::query_scalar(
        "SELECT to_regclass('erpx_control.platform_bootstrap_receipts') IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .map_err(|_| BootstrapError("bootstrap status could not be read"))?;
    if !table_exists {
        return emit(&StatusReport {
            installed: false,
            state: "database_created".into(),
            prefix: config.prefix,
            tenant_id: None,
            holding_ref: None,
            completed_steps: vec!["control_database_created".into()],
            failure_category: None,
            bootstrap_version: None,
            progress_generation: progress.as_ref().map(BootstrapProgressMarker::generation),
            progress_phase: progress
                .as_ref()
                .map(|marker| marker.current_phase().as_str()),
            progress_state: progress.as_ref().map(|marker| marker.state().as_str()),
            progress_failure_category: progress
                .as_ref()
                .and_then(BootstrapProgressMarker::failure_category)
                .map(|category| category.as_str()),
        });
    }
    let row = sqlx::query(
        "SELECT tenant_id, holding_ref, state, completed_steps,
                failure_category, bootstrap_version
         FROM erpx_control.platform_bootstrap_receipts
         WHERE installation_prefix=$1",
    )
    .bind(&config.prefix)
    .fetch_optional(&pool)
    .await
    .map_err(|_| BootstrapError("bootstrap status could not be read"))?;
    let Some(row) = row else {
        return emit(&StatusReport {
            installed: false,
            state: "migrated".into(),
            prefix: config.prefix,
            tenant_id: None,
            holding_ref: None,
            completed_steps: vec!["control_schema_migrated".into()],
            failure_category: None,
            bootstrap_version: None,
            progress_generation: progress.as_ref().map(BootstrapProgressMarker::generation),
            progress_phase: progress
                .as_ref()
                .map(|marker| marker.current_phase().as_str()),
            progress_state: progress.as_ref().map(|marker| marker.state().as_str()),
            progress_failure_category: progress
                .as_ref()
                .and_then(BootstrapProgressMarker::failure_category)
                .map(|category| category.as_str()),
        });
    };
    emit(&StatusReport {
        installed: row.get::<String, _>("state") == "complete",
        state: row.get("state"),
        prefix: config.prefix,
        tenant_id: Some(row.get("tenant_id")),
        holding_ref: Some(row.get("holding_ref")),
        completed_steps: row.get("completed_steps"),
        failure_category: row.get("failure_category"),
        bootstrap_version: u64::try_from(row.get::<i64, _>("bootstrap_version")).ok(),
        progress_generation: progress.as_ref().map(BootstrapProgressMarker::generation),
        progress_phase: progress
            .as_ref()
            .map(|marker| marker.current_phase().as_str()),
        progress_state: progress.as_ref().map(|marker| marker.state().as_str()),
        progress_failure_category: progress
            .as_ref()
            .and_then(BootstrapProgressMarker::failure_category)
            .map(|category| category.as_str()),
    })
}

impl SetupConfig {
    fn from_env() -> Result<Self> {
        let prefix = required("ERPX_SETUP_PREFIX")?;
        validate_prefix(&prefix)?;
        let url = required("ERPX_DATABASE_PROFILE_DEFAULT_URL")?;
        reject_credentials_in_url(&url)?;
        let runtime_username = required("ERPX_DATABASE_PROFILE_DEFAULT_USERNAME")?;
        let runtime_password = required("ERPX_DATABASE_PROFILE_DEFAULT_PASSWORD")?;
        let admin_username = required_named(
            "ERPX_SETUP_DATABASE_ADMIN_USERNAME",
            "ERPX_SETUP_DATABASE_ADMIN_USERNAME is required",
        )?;
        let admin_password = required_named(
            "ERPX_SETUP_DATABASE_ADMIN_PASSWORD",
            "ERPX_SETUP_DATABASE_ADMIN_PASSWORD is required",
        )?;
        validate_database_role(&runtime_username)?;
        validate_database_role(&admin_username)?;
        if runtime_password.is_empty()
            || runtime_password.len() > 4096
            || admin_password.is_empty()
            || admin_password.len() > 4096
        {
            return Err(BootstrapError("database password is invalid"));
        }
        let base_options = PgConnectOptions::from_str(&url)
            .map_err(|_| BootstrapError("database URL is invalid"))?;
        validate_development_endpoint(base_options.get_host(), cfg!(debug_assertions))?;
        let mut runtime_options = base_options
            .clone()
            .username(&runtime_username)
            .password(&runtime_password);
        let mut admin_options = base_options
            .username(&admin_username)
            .password(&admin_password);
        if !is_local_host(runtime_options.get_host()) {
            runtime_options = runtime_options.ssl_mode(PgSslMode::VerifyFull);
            admin_options = admin_options.ssl_mode(PgSslMode::VerifyFull);
        }
        let tenant_name = required("ERPX_SETUP_TENANT_NAME")?;
        let holding_name = required("ERPX_SETUP_HOLDING_NAME")?;
        let root_principal = required("ERPX_SETUP_ROOT_PRINCIPAL")?;
        let owner_principal = required("ERPX_SETUP_HOLDING_OWNER_PRINCIPAL")?;
        validate_field(&tenant_name, 255, "tenant name is invalid")?;
        validate_field(&holding_name, 255, "holding name is invalid")?;
        validate_principal(&root_principal)?;
        validate_principal(&owner_principal)?;
        if root_principal == owner_principal {
            return Err(BootstrapError(
                "root and Holding Owner principals must differ",
            ));
        }
        let resources = FreshResourcePlan::new(&prefix, &tenant_name, &holding_name);
        let customer_id = stable_uuid("customer", &format!("{prefix}:{tenant_name}"));
        let tenant_id = resources.tenant_id.to_string();
        let holding_ref = resources.holding_ref.to_string();
        let holding_id = resources.holding_id;
        Ok(Self {
            control_write: resources.databases[0].name.clone(),
            control_read: resources.databases[1].name.clone(),
            tenant_write: resources.databases[2].name.clone(),
            tenant_read: resources.databases[3].name.clone(),
            resources,
            prefix,
            admin_options,
            runtime_options,
            runtime_database_owner: runtime_username,
            tenant_name,
            holding_name,
            root_principal,
            owner_principal,
            tenant_id,
            holding_ref,
            holding_id,
            customer_id,
        })
    }

    fn database_names(&self) -> [&str; 4] {
        self.resources.database_names()
    }
}

impl FreshObjectSetup {
    fn from_env() -> Result<Self> {
        if required_named(
            "ERPX_SETUP_CUTOVER_CONFIRM",
            "ERPX_SETUP_CUTOVER_CONFIRM is required",
        )? != "FRESH_STATE_NO_COPY_NO_MERGE"
        {
            return Err(BootstrapError(
                "fresh-state cutover confirmation literal is invalid",
            ));
        }
        if required_named(
            "ERPX_SETUP_OLD_RUNTIME_STATE",
            "ERPX_SETUP_OLD_RUNTIME_STATE is required",
        )? != "stopped"
        {
            return Err(BootstrapError(
                "old ERPX runtime must be stopped before fresh-state cutover",
            ));
        }
        let endpoint = required_named(
            "ERPX_SETUP_MINIO_ENDPOINT",
            "ERPX_SETUP_MINIO_ENDPOINT is required",
        )?;
        let access_key = required_named(
            "ERPX_SETUP_MINIO_ACCESS_KEY",
            "ERPX_SETUP_MINIO_ACCESS_KEY is required",
        )?;
        let secret_key = required_named(
            "ERPX_SETUP_MINIO_SECRET_KEY",
            "ERPX_SETUP_MINIO_SECRET_KEY is required",
        )?;
        let region = env::var("ERPX_SETUP_MINIO_REGION")
            .ok()
            .filter(|value| !value.trim().is_empty() && value.trim() == value)
            .unwrap_or_else(|| "us-east-1".into());
        validate_field(&region, 64, "MinIO region is invalid")?;

        let workspace_root = PathBuf::from(required_named(
            "ERPX_SETUP_WORKSPACE_ROOT",
            "ERPX_SETUP_WORKSPACE_ROOT is required",
        )?);
        let workspace_metadata = fs::symlink_metadata(&workspace_root)
            .map_err(|_| BootstrapError("setup workspace root is invalid"))?;
        if !workspace_root.is_absolute()
            || workspace_metadata.file_type().is_symlink()
            || !workspace_metadata.is_dir()
        {
            return Err(BootstrapError("setup workspace root is invalid"));
        }

        let ca_certificate_pem = env::var("ERPX_SETUP_MINIO_CA_CERT_PATH")
            .ok()
            .filter(|value| !value.is_empty())
            .map(|value| read_public_ca(&PathBuf::from(value)))
            .transpose()?;
        let old_resources = OldResourceInventory::new(
            Utc::now(),
            required_csv(
                "ERPX_SETUP_RETAINED_PREFIXES",
                "ERPX_SETUP_RETAINED_PREFIXES is required",
            )?,
            required_csv(
                "ERPX_SETUP_RETAINED_DATABASES",
                "ERPX_SETUP_RETAINED_DATABASES is required",
            )?,
            required_csv(
                "ERPX_SETUP_RETAINED_OBJECT_NAMESPACES",
                "ERPX_SETUP_RETAINED_OBJECT_NAMESPACES is required",
            )?,
            required_csv(
                "ERPX_SETUP_RETAINED_BINARY_RELEASES",
                "ERPX_SETUP_RETAINED_BINARY_RELEASES is required",
            )?,
            required_named(
                "ERPX_SETUP_RETAINED_ENVIRONMENT_REF",
                "ERPX_SETUP_RETAINED_ENVIRONMENT_REF is required",
            )?,
        )
        .map_err(|_| BootstrapError("retained rollback inventory is invalid"))?;
        Ok(Self {
            endpoint,
            access_key,
            secret_key,
            region,
            ca_certificate_pem,
            workspace_root,
            old_resources,
        })
    }
}

fn required_named(name: &str, message: &'static str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty() && value.trim() == value)
        .ok_or(BootstrapError(message))
}

fn required_csv(name: &str, message: &'static str) -> Result<Vec<String>> {
    let value = required_named(name, message)?;
    let values = value.split(',').map(str::to_owned).collect::<Vec<_>>();
    if values
        .iter()
        .any(|value| value.is_empty() || value.trim() != value)
    {
        return Err(BootstrapError(message));
    }
    Ok(values)
}

fn read_public_ca(path: &PathBuf) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| BootstrapError("MinIO CA certificate path is invalid"))?;
    if !path.is_absolute()
        || metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > 1024 * 1024
    {
        return Err(BootstrapError("MinIO CA certificate path is invalid"));
    }
    fs::read(path).map_err(|_| BootstrapError("MinIO CA certificate could not be read"))
}

fn required(name: &'static str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty() && value.trim() == value)
        .ok_or(BootstrapError(match name {
            "ERPX_SETUP_PREFIX" => "ERPX_SETUP_PREFIX is required",
            "ERPX_DATABASE_PROFILE_DEFAULT_URL" => "ERPX_DATABASE_PROFILE_DEFAULT_URL is required",
            "ERPX_DATABASE_PROFILE_DEFAULT_USERNAME" => {
                "ERPX_DATABASE_PROFILE_DEFAULT_USERNAME is required"
            }
            "ERPX_DATABASE_PROFILE_DEFAULT_PASSWORD" => {
                "ERPX_DATABASE_PROFILE_DEFAULT_PASSWORD is required"
            }
            "ERPX_SETUP_TENANT_NAME" => "ERPX_SETUP_TENANT_NAME is required",
            "ERPX_SETUP_HOLDING_NAME" => "ERPX_SETUP_HOLDING_NAME is required",
            "ERPX_SETUP_ROOT_PRINCIPAL" => "ERPX_SETUP_ROOT_PRINCIPAL is required",
            _ => "ERPX_SETUP_HOLDING_OWNER_PRINCIPAL is required",
        }))
}

fn validate_prefix(prefix: &str) -> Result<()> {
    let valid = (2..=16).contains(&prefix.len())
        && prefix.as_bytes()[0].is_ascii_lowercase()
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(BootstrapError(
            "setup prefix must be 2-16 lowercase ASCII letters, digits, or underscores and start with a letter",
        ))
    }
}

fn validate_field(value: &str, max: usize, message: &'static str) -> Result<()> {
    if value.len() > max || value.chars().any(char::is_control) {
        Err(BootstrapError(message))
    } else {
        Ok(())
    }
}

fn validate_principal(value: &str) -> Result<()> {
    validate_field(value, 255, "principal reference is invalid")?;
    if value.contains(char::is_whitespace) || !value.contains(':') {
        return Err(BootstrapError("principal reference is invalid"));
    }
    Ok(())
}

fn validate_database_role(value: &str) -> Result<()> {
    let valid = (1..=63).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(BootstrapError("database role is invalid"))
    }
}

fn reject_credentials_in_url(url: &str) -> Result<()> {
    let rest = url
        .strip_prefix("postgresql://")
        .or_else(|| url.strip_prefix("postgres://"))
        .ok_or(BootstrapError("database URL must use postgresql://"))?;
    let authority = rest.split(['/', '?']).next().unwrap_or_default();
    let lower = rest.to_ascii_lowercase();
    if authority.contains('@')
        || lower.contains("password=")
        || lower.contains("user=")
        || lower.contains("username=")
    {
        return Err(BootstrapError("database URL must not contain credentials"));
    }
    Ok(())
}

fn is_local_host(host: &str) -> bool {
    host == "localhost"
        || host.starts_with('/')
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn stable_uuid(kind: &str, source: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:bootstrap:{kind}:v1:{source}").as_bytes(),
    )
}

async fn create_fresh_database(
    admin: &mut PgConnection,
    name: &str,
    runtime_owner: &str,
) -> Result<()> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(name)
            .fetch_one(&mut *admin)
            .await
            .map_err(|_| BootstrapError("database inventory validation failed"))?;
    if exists {
        return Err(BootstrapError(
            "fresh-state target appeared after preflight; bootstrap stopped",
        ));
    }
    admin
        .execute(format!("CREATE DATABASE \"{name}\" OWNER \"{runtime_owner}\"").as_str())
        .await
        .map_err(|_| BootstrapError("database creation failed"))?;
    admin
        .execute(format!("REVOKE CONNECT ON DATABASE \"{name}\" FROM PUBLIC").as_str())
        .await
        .map_err(|_| BootstrapError("database access isolation failed"))?;
    admin
        .execute(format!("GRANT CONNECT ON DATABASE \"{name}\" TO \"{runtime_owner}\"").as_str())
        .await
        .map_err(|_| BootstrapError("runtime database access grant failed"))?;
    Ok(())
}

async fn inspect_database_inventory(
    admin: &mut PgConnection,
    config: &SetupConfig,
) -> Result<(Vec<DatabaseInventoryEntry>, Vec<String>)> {
    let names = config.database_names();
    let existing_names: Vec<String> =
        sqlx::query_scalar("SELECT datname FROM pg_database WHERE datname = ANY($1)")
            .bind(names)
            .fetch_all(&mut *admin)
            .await
            .map_err(|_| BootstrapError("database inventory validation failed"))?;
    let prefix_namespace = format!("{}_", config.prefix);
    let prefix_collisions: Vec<String> = sqlx::query_scalar(
        "SELECT datname FROM pg_database WHERE starts_with(datname, $1) ORDER BY datname",
    )
    .bind(prefix_namespace)
    .fetch_all(&mut *admin)
    .await
    .map_err(|_| BootstrapError("database prefix inventory validation failed"))?;
    Ok((
        config.resources.inventory(&existing_names),
        prefix_collisions,
    ))
}

async fn connect_database(config: &SetupConfig, name: &str) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect_with(config.runtime_options.clone().database(name))
        .await
        .map_err(|_| BootstrapError("database connection failed"))
}

async fn register_tenant_and_receipt(config: &SetupConfig, pool: &PgPool) -> Result<()> {
    let storage_namespace = &config.resources.storage_namespace;
    let credential_key = "database-profile:default";
    let backup_chain = format!("{}-bootstrap-backup", config.prefix.replace('_', "-"));
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| BootstrapError("tenant registration transaction failed"))?;
    let row = sqlx::query(
        "INSERT INTO erpx_control.tenant_registry
            (tenant_id, holding_ref, write_database_key, read_database_key,
             storage_namespace, credential_key_ref, backup_chain_ref, state,
             registry_version, isolation_mode)
         VALUES ($1,$2,$3,$4,$5,$6,$7,'provisioning',1,'holding_and_branch_pairs')
         ON CONFLICT (tenant_id) DO UPDATE SET updated_at=now()
         WHERE erpx_control.tenant_registry.holding_ref=EXCLUDED.holding_ref
           AND erpx_control.tenant_registry.write_database_key=EXCLUDED.write_database_key
           AND erpx_control.tenant_registry.read_database_key=EXCLUDED.read_database_key
           AND erpx_control.tenant_registry.storage_namespace=EXCLUDED.storage_namespace
           AND erpx_control.tenant_registry.isolation_mode='holding_and_branch_pairs'
         RETURNING tenant_id",
    )
    .bind(&config.tenant_id)
    .bind(&config.holding_ref)
    .bind(&config.tenant_write)
    .bind(&config.tenant_read)
    .bind(storage_namespace)
    .bind(credential_key)
    .bind(&backup_chain)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| BootstrapError("tenant registration failed"))?;
    if row.is_none() {
        return Err(BootstrapError(
            "tenant registration conflicts with an existing installation",
        ));
    }
    sqlx::query(
        "INSERT INTO erpx_control.tenant_provisioning
            (tenant_id, state, write_database_key, read_database_key, storage_namespace, version)
         VALUES ($1,'provisioning',$2,$3,$4,1)
         ON CONFLICT (tenant_id) DO UPDATE SET updated_at=now()
         WHERE erpx_control.tenant_provisioning.write_database_key=EXCLUDED.write_database_key
           AND erpx_control.tenant_provisioning.read_database_key=EXCLUDED.read_database_key
           AND erpx_control.tenant_provisioning.storage_namespace=EXCLUDED.storage_namespace",
    )
    .bind(&config.tenant_id)
    .bind(&config.tenant_write)
    .bind(&config.tenant_read)
    .bind(storage_namespace)
    .execute(&mut *transaction)
    .await
    .map_err(|_| BootstrapError("tenant provisioning registration failed"))?;
    transaction
        .commit()
        .await
        .map_err(|_| BootstrapError("tenant registration commit failed"))?;
    Ok(())
}

async fn upsert_receipt(config: &SetupConfig, pool: &PgPool, step: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO erpx_control.platform_bootstrap_receipts
            (installation_prefix, tenant_id, holding_ref, root_principal_ref,
             owner_principal_ref, profile_key, state, completed_steps, bootstrap_version)
         VALUES ($1,$2,$3,$4,$5,'default','provisioning',ARRAY[$6],1)
         ON CONFLICT (installation_prefix) DO UPDATE
         SET completed_steps=(SELECT ARRAY(SELECT DISTINCT value FROM unnest(
                 erpx_control.platform_bootstrap_receipts.completed_steps || ARRAY[$6]) AS value
                 ORDER BY value)), updated_at=now()
         WHERE erpx_control.platform_bootstrap_receipts.tenant_id=EXCLUDED.tenant_id
           AND erpx_control.platform_bootstrap_receipts.holding_ref=EXCLUDED.holding_ref
           AND erpx_control.platform_bootstrap_receipts.root_principal_ref=EXCLUDED.root_principal_ref
           AND erpx_control.platform_bootstrap_receipts.owner_principal_ref=EXCLUDED.owner_principal_ref",
    )
    .bind(&config.prefix)
    .bind(&config.tenant_id)
    .bind(&config.holding_ref)
    .bind(&config.root_principal)
    .bind(&config.owner_principal)
    .bind(step)
    .execute(pool)
    .await
    .map_err(|_| BootstrapError("bootstrap receipt update failed"))?;
    Ok(())
}

struct OneTimeCredentials {
    principal: String,
    password: String,
    recovery_codes: Vec<String>,
}

async fn ensure_principal(
    identity: &SqlxIdentityStore,
    pool: &PgPool,
    principal: &str,
) -> Result<Option<OneTimeCredentials>> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM erpx_identity.principals WHERE principal_ref=$1)",
    )
    .bind(principal)
    .fetch_one(pool)
    .await
    .map_err(|_| BootstrapError("principal state could not be read"))?;
    if exists {
        return Ok(None);
    }
    let password = format!(
        "ERPX-{}-{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    identity
        .create_principal_with_password(principal, &password, Utc::now())
        .await
        .map_err(|_| BootstrapError("principal creation failed"))?;
    let recovery = identity
        .issue_recovery_codes(principal, 8, Utc::now())
        .await
        .map_err(|_| BootstrapError("recovery code creation failed"))?;
    Ok(Some(OneTimeCredentials {
        principal: principal.into(),
        password,
        recovery_codes: recovery.codes().map(str::to_owned).collect(),
    }))
}

async fn ensure_only_bootstrap_principals(config: &SetupConfig, pool: &PgPool) -> Result<()> {
    let mut actual: Vec<String> = sqlx::query_scalar(
        "SELECT principal_ref FROM erpx_identity.principals ORDER BY principal_ref",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| BootstrapError("bootstrap principal inventory could not be read"))?;
    let mut expected = vec![
        config.root_principal.clone(),
        config.owner_principal.clone(),
    ];
    actual.sort();
    expected.sort();
    if actual != expected {
        return Err(BootstrapError(
            "fresh-state seed contains principals outside Platform Root and Holding Owner",
        ));
    }
    let actual_roles: Vec<(String, String)> = sqlx::query_as(
        "SELECT principal_ref, role_key FROM erpx_control.role_assignments
         ORDER BY principal_ref, role_key",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| BootstrapError("bootstrap role inventory could not be read"))?;
    let mut expected_roles = vec![
        (config.root_principal.clone(), "platform_root".to_owned()),
        (config.owner_principal.clone(), "holding_owner".to_owned()),
    ];
    expected_roles.sort();
    if actual_roles != expected_roles {
        return Err(BootstrapError(
            "fresh-state seed contains roles outside Platform Root and Holding Owner",
        ));
    }
    Ok(())
}

async fn ensure_roles(config: &SetupConfig, control: Arc<SqlxControlPlane>) -> Result<()> {
    let lifecycle =
        RoleLifecycleService::new(control.clone(), Arc::new(SystemClock), control.clone());
    let now = Utc::now();
    for assignment in [
        RoleAssignment {
            id: stable_uuid("role", &format!("{}:platform-root", config.root_principal)),
            principal_ref: config.root_principal.clone(),
            role_key: "platform_root".into(),
            permissions: ROOT_PERMISSIONS
                .iter()
                .map(|value| (*value).into())
                .collect(),
            functional_domain: "platform".into(),
            authority_level: u8::MAX,
            approval_limit_minor: u64::MAX,
            scope: Scope::Platform,
            valid_from: now,
            valid_until: now + Duration::days(90),
            timezone: "Asia/Bangkok".into(),
            risk_tier: RiskTier::PlatformRoot,
            grant_reason: "first ERPX installation".into(),
            ticket_reference: Some("bootstrap:v1".into()),
            grantor: "bootstrap:system".into(),
            approver: Some("bootstrap:independent-approver".into()),
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        },
        RoleAssignment {
            id: stable_uuid("role", &format!("{}:holding-owner", config.owner_principal)),
            principal_ref: config.owner_principal.clone(),
            role_key: "holding_owner".into(),
            permissions: OWNER_PERMISSIONS
                .iter()
                .map(|value| (*value).into())
                .collect(),
            functional_domain: "organization".into(),
            authority_level: 100,
            approval_limit_minor: 0,
            scope: Scope::Holding {
                holding_id: config.holding_id,
            },
            valid_from: now,
            valid_until: now + Duration::days(90),
            timezone: "Asia/Bangkok".into(),
            risk_tier: RiskTier::Administrator,
            grant_reason: "first ERPX holding owner".into(),
            ticket_reference: Some("bootstrap:v1".into()),
            grantor: config.root_principal.clone(),
            approver: Some("bootstrap:independent-approver".into()),
            renewal_reviewer: None,
            parent_assignment_id: None,
            assignment_version: 1,
        },
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM erpx_control.role_assignments WHERE id=$1)",
        )
        .bind(assignment.id)
        .fetch_one(&control.pools().write)
        .await
        .map_err(|_| BootstrapError("role state could not be read"))?;
        if !exists {
            lifecycle
                .grant(assignment, None)
                .await
                .map_err(|_| BootstrapError("bootstrap role grant failed"))?;
        }
    }
    Ok(())
}

async fn seed_customer_and_units(
    config: &SetupConfig,
    control: Arc<SqlxControlPlane>,
) -> Result<()> {
    let pool = &control.pools().write;
    let account_code = format!("BOOTSTRAP-{}", config.customer_id.simple());
    sqlx::query(
        "INSERT INTO erpx_control.customers
            (id, account_code, trading_name, legal_name, country, status,
             billing_state, payload)
         VALUES ($1,$2,$3,$4,'UN','active','active',$5)
         ON CONFLICT (id) DO UPDATE SET updated_at=now()",
    )
    .bind(config.customer_id)
    .bind(account_code)
    .bind(&config.tenant_name)
    .bind(&config.holding_name)
    .bind(serde_json::json!({
        "id": config.customer_id,
        "account_code": format!("BOOTSTRAP-{}", config.customer_id.simple()),
        "trading_name": config.tenant_name,
        "legal_name": config.holding_name,
        "country": "UN",
        "status": "active",
        "billing_state": "active",
        "version": 1
    }))
    .execute(pool)
    .await
    .map_err(|_| BootstrapError("bootstrap customer seed failed"))?;
    sqlx::query(
        "INSERT INTO erpx_control.customer_scope_bindings
            (customer_id, holding_id, company_id, branch_id)
         VALUES ($1,$2,NULL,NULL)",
    )
    .bind(config.customer_id)
    .bind(config.holding_id)
    .execute(pool)
    .await
    .map_err(|_| BootstrapError("customer root holding scope seed failed"))?;
    let binding = CustomerTenantBinding::new(
        config.customer_id,
        config.tenant_id.clone(),
        config.holding_id,
        config.holding_ref.clone(),
    )
    .map_err(|_| BootstrapError("customer tenant binding is invalid"))?;
    SqlxTenantAdministrationStore::new(control.clone())
        .bind_customer_tenant(&binding, &config.root_principal)
        .await
        .map_err(|_| BootstrapError("customer tenant binding failed"))?;
    for unit in BusinessUnitId::ALL {
        sqlx::query(
            "INSERT INTO erpx_control.business_unit_configurations
                (tenant_id, root_holding_id, business_unit, configuration_version,
                 active, organization_scope, updated_by)
             VALUES ($1,$2,$3,1,TRUE,$4,$5)
             ON CONFLICT (tenant_id, business_unit) DO NOTHING",
        )
        .bind(&config.tenant_id)
        .bind(config.holding_id)
        .bind(unit.as_str())
        .bind(serde_json::json!({"Holding": {"holding_id": config.holding_id}}))
        .bind(&config.owner_principal)
        .execute(pool)
        .await
        .map_err(|_| BootstrapError("business unit configuration seed failed"))?;
    }
    let mut actual: Vec<String> = sqlx::query_scalar(
        "SELECT business_unit FROM erpx_control.business_unit_configurations
         WHERE tenant_id=$1 ORDER BY business_unit",
    )
    .bind(&config.tenant_id)
    .fetch_all(pool)
    .await
    .map_err(|_| BootstrapError("business unit seed inventory could not be read"))?;
    let mut expected = BusinessUnitId::ALL
        .iter()
        .map(|unit| unit.as_str().to_owned())
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    if actual != expected {
        return Err(BootstrapError(
            "fresh-state seed does not contain exactly the canonical business units",
        ));
    }
    let registry_shape_valid: bool = sqlx::query_scalar(
        "SELECT
             (SELECT count(*) FROM erpx_control.customers) = 1
             AND (SELECT count(*) FROM erpx_control.customer_tenant_bindings) = 1
             AND EXISTS(
                 SELECT 1 FROM erpx_control.customer_tenant_bindings
                 WHERE customer_id=$1 AND tenant_id=$2 AND root_holding_id=$3
                   AND holding_ref=$4
             )",
    )
    .bind(config.customer_id)
    .bind(&config.tenant_id)
    .bind(config.holding_id)
    .bind(&config.holding_ref)
    .fetch_one(pool)
    .await
    .map_err(|_| BootstrapError("tenant/holding registry seed could not be read"))?;
    if !registry_shape_valid {
        return Err(BootstrapError(
            "fresh-state tenant/holding registry seed contains unexpected records",
        ));
    }
    Ok(())
}

async fn verify_neutral_brand_seed(pool: &PgPool) -> Result<()> {
    let valid: bool = sqlx::query_scalar(
        "SELECT
             (SELECT count(*) FROM erpx_control.corporate_identity_profiles) = 1
             AND (SELECT count(*) FROM erpx_control.corporate_identity_versions) = 1
             AND (SELECT count(*) FROM erpx_control.corporate_identity_assets) = 0
             AND (SELECT count(*) FROM erpx_control.corporate_identity_domains) = 0
             AND EXISTS(
             SELECT 1
             FROM erpx_control.corporate_identity_profiles profile
             JOIN erpx_control.corporate_identity_versions version
               ON version.profile_id=profile.profile_id
              AND version.version=profile.current_published_version
             WHERE profile.scope_key='platform'
               AND profile.scope_kind='platform'
               AND version.lifecycle='published'
               AND version.display_name='ERPX Platform'
               AND version.short_name='ERPX'
               AND version.assets='{}'::jsonb
         )",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| BootstrapError("neutral brand seed could not be verified"))?;
    if !valid {
        return Err(BootstrapError(
            "fresh-state seed is missing the canonical neutral brand",
        ));
    }
    Ok(())
}

async fn activate_tenant(
    config: &SetupConfig,
    control: Arc<SqlxControlPlane>,
    namespace_receipt: &NamespaceVerificationReceipt,
) -> Result<()> {
    let provisioner = TenantProvisioner::new(control.clone());
    let checked_at = namespace_receipt.verified_at();
    config.resources.require_matching_storage_receipt(
        namespace_receipt.prefix(),
        namespace_receipt.namespace(),
    )?;
    for (side, key) in [
        (
            erpx_infrastructure::DatabaseSide::Write,
            &config.tenant_write,
        ),
        (erpx_infrastructure::DatabaseSide::Read, &config.tenant_read),
    ] {
        provisioner
            .record_database_health(&TenantDatabaseHealth {
                tenant_id: config.tenant_id.clone(),
                side,
                database_key: key.clone(),
                schema_version: "bootstrap-current".into(),
                reachable: true,
                checked_at,
                details: serde_json::json!({"migration": "applied", "rls": "forced"}),
            })
            .await
            .map_err(|_| BootstrapError("tenant database readiness record failed"))?;
    }
    provisioner
        .record_storage_health(&TenantStorageHealth {
            tenant_id: config.tenant_id.clone(),
            storage_namespace: namespace_receipt.namespace().to_owned(),
            reachable: true,
            checked_at,
            details: serde_json::json!({
                "bootstrap": "namespace_present_and_accessible",
                "installation_prefix": namespace_receipt.prefix(),
                "verified_at": namespace_receipt.verified_at(),
            }),
        })
        .await
        .map_err(|_| BootstrapError("tenant storage readiness record failed"))?;
    let state: String =
        sqlx::query_scalar("SELECT state FROM erpx_control.tenant_registry WHERE tenant_id=$1")
            .bind(&config.tenant_id)
            .fetch_one(&control.pools().write)
            .await
            .map_err(|_| BootstrapError("tenant lifecycle state could not be read"))?;
    if state != "active" {
        provisioner
            .activate(&config.tenant_id)
            .await
            .map_err(|_| BootstrapError("tenant activation failed"))?;
    }
    Ok(())
}

async fn schedule_initial_projection_jobs(
    config: &SetupConfig,
    control: Arc<SqlxControlPlane>,
) -> Result<()> {
    let authorization = ProjectionSchedulerAuthorization::new(
        config.root_principal.clone(),
        "establish initial generation-v2 read-model checkpoints",
        config.resources.installation_generation,
    )
    .map_err(|_| BootstrapError("initial projection authorization is invalid"))?;
    let repository = TenantProjectionSchedulerRepository::new(
        control.pools().write.clone(),
        Arc::new(SqlxTenantStoreRegistry::new(control)),
        authorization,
    );
    let binding = TenantHoldingBinding::new(&config.tenant_id, &config.holding_ref)
        .map_err(|_| BootstrapError("initial projection binding is invalid"))?;
    let policy = ProjectionPolicy::new(500, 10, 10, 1, 60)
        .map_err(|_| BootstrapError("initial projection policy is invalid"))?;
    let now = Utc::now();
    for projection_name in FRESH_INITIAL_TENANT_PROJECTIONS {
        repository
            .schedule(
                ProjectionScheduleRequest {
                    binding: binding.clone(),
                    projection_name: projection_name.into(),
                    job_key: format!(
                        "fresh-bootstrap:{}:{projection_name}",
                        config.resources.installation_generation
                    ),
                    kind: ProjectionJobKind::Incremental,
                    available_at: now,
                    policy,
                    payload: serde_json::json!({}),
                },
                now,
            )
            .await
            .map_err(|_| BootstrapError("initial projection job scheduling failed"))?;
    }
    Ok(())
}

fn print_credentials(label: &str, credentials: Option<OneTimeCredentials>) {
    if let Some(credentials) = credentials {
        println!("{label} credentials (shown once):");
        println!("  principal: {}", credentials.principal);
        println!("  one-time password: {}", credentials.password);
        println!("  recovery codes:");
        for code in credentials.recovery_codes {
            println!("    {code}");
        }
    }
}

fn emit<T: Serialize>(value: &T) -> Result<()> {
    let output = serde_json::to_string_pretty(value)
        .map_err(|_| BootstrapError("redacted output could not be serialized"))?;
    println!("{output}");
    Ok(())
}

fn require_valid_preflight(valid: bool) -> Result<()> {
    if valid {
        Ok(())
    } else {
        Err(BootstrapError("fresh-state preflight did not pass"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_prefix_bounds_and_alphabet() {
        for valid in ["er", "erpx", "erpx_01", "a234567890123456"] {
            assert!(validate_prefix(valid).is_ok(), "{valid}");
        }
        for invalid in ["e", "ERPX", "1erpx", "erpx-01", "a2345678901234567"] {
            assert!(validate_prefix(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn invalid_preflight_is_a_process_error_after_reporting() {
        assert!(require_valid_preflight(true).is_ok());
        assert_eq!(
            require_valid_preflight(false)
                .expect_err("invalid preflight must fail")
                .to_string(),
            "fresh-state preflight did not pass"
        );
    }

    #[test]
    fn rejects_all_url_credential_forms() {
        assert!(reject_credentials_in_url("postgresql://db.example.test/postgres").is_ok());
        assert!(reject_credentials_in_url("postgresql://user@db.example.test/postgres").is_err());
        assert!(
            reject_credentials_in_url("postgresql://db.example.test/postgres?password=x").is_err()
        );
    }

    #[test]
    fn fresh_bootstrap_source_has_no_copy_or_destructive_database_paths() {
        let production = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("bootstrap has production source");
        for forbidden in [
            concat!("DROP", " DATABASE"),
            concat!("DROP", " SCHEMA"),
            concat!("TRUNC", "ATE"),
            concat!("DELETE", " FROM"),
            concat!("CREATE DATABASE", " AS TEMPLATE"),
            concat!("pg_", "dump"),
            concat!("pg_", "restore"),
        ] {
            assert!(!production.contains(forbidden), "found {forbidden}");
        }
        assert!(production.contains("require_fresh_targets(&inventory)"));
        assert!(production.contains("require_unused_prefix(&prefix_collisions)"));
    }
}
