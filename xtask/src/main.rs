use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::IpAddr,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    time::Duration,
};

use chrono::Utc;
use erpx_application::{RoleLifecycleService, SystemClock};
use erpx_domain::access::{RiskTier, RoleAssignment, Scope};
use erpx_identity::{SqlxIdentityStore, apply_identity_migration};
use erpx_infrastructure::{
    ControlPlanePools, SqlxControlPlane, SqlxTenantStoreRegistry, TenantStoreRegistration,
};
use sha2::{Digest, Sha256};
use sqlx::{Connection, Executor, PgConnection, PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;

const RESET_CONFIRMATION: &str = "RESET_ERPX_DEV";
const LEGACY_DATABASE: &str = "folio_db";
const LEGACY_BUCKET: &str = "folio-storage";
const ERPX_BUCKET: &str = "erpx-demo";
const WORKER_ID: &str = "1db95d2e-6aa6-5de7-81b2-e650a66be777";
const TENANT_ID: &str = "demo-tenant";
const HOLDING_REF: &str = "demo-holding";
const FIXTURE_VERSION: &str = "erpx-demo-v1";
const DATABASES: [&str; 4] = [
    "erpx_control_write",
    "erpx_control_read",
    "erpx_tenant_write",
    "erpx_tenant_read",
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

type TaskResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone)]
struct DevConfig {
    host: String,
    port: u16,
    user: String,
    password: String,
    legacy_database: String,
    minio_endpoint: String,
    minio_root: Option<PathBuf>,
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
        Ok(Self {
            host,
            port,
            user,
            password,
            legacy_database,
            minio_endpoint,
            minio_root,
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
        if self.legacy_database != LEGACY_DATABASE {
            return Err("reset-dev refuses an unexpected legacy database".into());
        }
        Ok(())
    }

    fn database_url(&self, database: &str) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode=disable",
            percent_encode(&self.user),
            percent_encode(&self.password),
            self.host,
            self.port,
            database
        )
    }

    fn database_template(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{{database}}?sslmode=disable",
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
    match args.as_slice() {
        [command] if command == "status" => status(&root),
        [command] if command == "stop" => stop(&root),
        [command] if command == "seed-demo" => {
            let config = DevConfig::load(&root)?;
            initialize_databases(&config, false).await?;
            seed_demo(&config).await
        }
        [command, flag] if command == "dev" && flag == "--detach" => {
            let config = DevConfig::load(&root)?;
            dev(&root, &config).await
        }
        [command, confirm_flag, confirmation]
            if command == "reset-dev" && confirm_flag == "--confirm" =>
        {
            let config = DevConfig::load(&root)?;
            reset_dev(&root, &config, confirmation).await
        }
        [command] if command == "verify" => verify(&root).await,
        _ => {
            println!(
                "ERPX development tasks\n\n\
                 cargo xtask dev --detach\n\
                 cargo xtask status\n\
                 cargo xtask stop\n\
                 cargo xtask reset-dev --confirm {RESET_CONFIRMATION}\n\
                 cargo xtask seed-demo\n\
                 cargo xtask verify"
            );
            Ok(())
        }
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
    let mut admin = PgConnection::connect(&admin_url).await?;
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
            admin
                .execute(format!("CREATE DATABASE \"{database}\"").as_str())
                .await?;
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
    erpx_migration::migrate_control_plane(&control_write).await?;
    erpx_migration::migrate_control_read(&control_read).await?;
    erpx_migration::migrate_tenant_write(&tenant_write).await?;
    erpx_migration::migrate_tenant_read(&tenant_read).await?;
    apply_identity_migration(&control_write).await?;
    let control = SqlxControlPlane::new(ControlPlanePools {
        write: control_write,
        read: control_read,
    });
    control.require_erpx_command_owner().await?;
    tenant_write.close().await;
    tenant_read.close().await;
    Ok(())
}

async fn pool(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new().max_connections(4).connect(url).await
}

async fn reset_dev(root: &Path, config: &DevConfig, confirmation: &str) -> TaskResult {
    config.validate_local()?;
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

async fn seed_demo(config: &DevConfig) -> TaskResult {
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
                credential_key_ref: "local-development-disabled".into(),
                backup_chain_ref: "local-development".into(),
                state: "active".into(),
                registry_version: 1,
            })
            .await?;
    }

    let identity = SqlxIdentityStore::new(write.clone());
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
    .bind("seed-demo:erpx-demo-v1")
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
        .bind("seed-demo:erpx-demo-v1")
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
    if connector_count != 0 {
        return Err("development seed refuses pre-existing directory connectors".into());
    }
    println!(
        "Demo fixture ready: {context_count} contexts, directory disabled, providers unconfigured."
    );
    write.close().await;
    Ok(())
}

async fn seed_demo_identities_and_roles(
    write: &PgPool,
    identity: &SqlxIdentityStore,
    control: &std::sync::Arc<SqlxControlPlane>,
) -> TaskResult {
    let now = Utc::now();
    let identities = [
        ("local:platform-root", "platform_root", 89_i64),
        ("local:demo-operations", "operations_demo", 45),
        ("local:demo-finance", "finance_demo", 30),
        ("local:demo-sales", "sales_demo", 60),
    ];
    for (principal_ref, _, _) in identities.iter().copied() {
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
    let platform_permissions = [
        "commercial.entitlement.activate",
        "commercial.entitlement.allocate",
        "diagnostics:read",
        "platform.customer.access_audit.read",
        "platform.customer.read",
        "platform.directory.manage",
        "platform.directory.read",
        "platform.outbox.replay",
        "platform.projection.rebuild",
        "platform.tenant.backup",
        "platform.tenant.provision",
        "platform.tenant.read",
        "platform.tenant.recovery",
        "platform.tenant.suspend",
        "support.incident.create",
        "support.incident.update",
        "support.session.request",
        "support.ticket.note",
        "support.ticket.reply",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    for (principal_ref, role_key, lifetime_days) in identities.iter().copied() {
        let assignment_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("erpx:demo:role:{principal_ref}:{role_key}").as_bytes(),
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
    let mut digest = Sha256::new();
    for context in DEMO_CONTEXTS {
        digest.update(format!("{context:?}\n"));
    }
    format!("{:x}", digest.finalize())
}

async fn dev(root: &Path, config: &DevConfig) -> TaskResult {
    config.validate_local()?;
    stop(root)?;
    initialize_databases(config, false).await?;
    seed_demo(config).await?;
    let runtime = runtime_dir(root);
    fs::create_dir_all(&runtime)?;
    run_command(
        root,
        "cargo",
        [
            "build",
            "--offline",
            "-p",
            "erpx-server",
            "-p",
            "erpx-worker",
        ],
    )?;
    let access_key = secret_file(&runtime.join("access-token.key"))?;
    let totp_key = secret_file(&runtime.join("totp.key"))?;
    let common = vec![
        (
            "ERPX_CONTROL_WRITE_DATABASE_URL",
            config.database_url(DATABASES[0]),
        ),
        (
            "ERPX_CONTROL_READ_DATABASE_URL",
            config.database_url(DATABASES[1]),
        ),
        (
            "ERPX_TENANT_ADMIN_DATABASE_URL",
            config.database_url("postgres"),
        ),
        (
            "ERPX_TENANT_DATABASE_URL_TEMPLATE",
            config.database_template(),
        ),
        ("ERPX_ACCESS_TOKEN_SIGNING_KEY", access_key),
        ("ERPX_IDENTITY_TOTP_KEY", totp_key),
        ("ERPX_DIRECTORY_RUNTIME", "disabled".into()),
        ("ERPX_DEVELOPMENT_PROFILE", "local".into()),
        (
            "ERPX_MARKETING_CALLBACK_SECRET_RESOLVER",
            "unconfigured".into(),
        ),
        ("ERPX_WEBAUTHN_PROVIDER", "webauthn-rs".into()),
        ("ERPX_WEBAUTHN_REQUIRED", "true".into()),
        ("ERPX_WEBAUTHN_RP_ID", "localhost".into()),
        ("ERPX_WEBAUTHN_ORIGINS", "http://localhost:3004".into()),
    ];
    spawn_service(
        root,
        &runtime,
        "server",
        root.join("target/debug/erpx-server"),
        std::iter::empty::<&str>(),
        common.clone(),
    )?;
    let mut worker_env = common.clone();
    worker_env.extend([
        ("ERPX_WORKER_HANDLER", "support_workflow".into()),
        ("ERPX_WORKER_ID", WORKER_ID.into()),
        ("ERPX_WORKER_POLL_INTERVAL_MS", "500".into()),
    ]);
    spawn_service(
        root,
        &runtime,
        "worker",
        root.join("target/debug/erpx-worker"),
        ["run"],
        worker_env,
    )?;
    spawn_service(
        root,
        &runtime,
        "web",
        PathBuf::from("dx"),
        [
            "serve",
            "--web",
            "-p",
            "erpx-console",
            "--port",
            "3004",
            "--addr",
            "0.0.0.0",
            "--open",
            "false",
        ],
        vec![("ERPX_CONSOLE_API_BASE", "http://127.0.0.1:50052".into())],
    )?;
    wait_for_http("http://127.0.0.1:50052/health", 90).await?;
    wait_for_http("http://127.0.0.1:3004", 180).await?;
    println!("ERPX is running at http://localhost:3004");
    status(root)
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
    for name in ["server", "worker", "web"] {
        let pid = read_pid(&runtime.join(format!("{name}.pid")))?;
        match pid {
            Some(pid) if recorded_process_matches(name, pid) => {
                println!("{name}: running (pid {pid})");
            }
            Some(pid) => println!("{name}: stopped (stale pid {pid})"),
            None => println!("{name}: stopped"),
        }
    }
    Ok(())
}

fn stop(root: &Path) -> TaskResult {
    let runtime = runtime_dir(root);
    for name in ["web", "worker", "server"] {
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
    Ok(())
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
    match name {
        "server" => command.contains("erpx-server"),
        "worker" => command.contains("erpx-worker"),
        "web" => command.contains("dx serve"),
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
                "--all-targets",
                "--offline",
                "--",
                "-D",
                "warnings",
            ],
        ),
        (
            "cargo",
            vec!["test", "--workspace", "--offline", "--no-fail-fast"],
        ),
        (
            "cargo",
            vec![
                "check",
                "-p",
                "erpx-console",
                "--offline",
                "--no-default-features",
                "--features",
                "web",
            ],
        ),
        (
            "cargo",
            vec![
                "check",
                "-p",
                "erpx-console",
                "--offline",
                "--no-default-features",
                "--features",
                "desktop",
            ],
        ),
        (
            "cargo",
            vec![
                "check",
                "-p",
                "erpx-console",
                "--offline",
                "--no-default-features",
                "--features",
                "mobile",
            ],
        ),
        (
            "cargo",
            vec![
                "check",
                "-p",
                "erpx-console",
                "--offline",
                "--target",
                "aarch64-apple-ios",
                "--no-default-features",
                "--features",
                "mobile",
            ],
        ),
        (
            "cargo",
            vec![
                "check",
                "-p",
                "erpx-console",
                "--offline",
                "--target",
                "aarch64-linux-android",
                "--no-default-features",
                "--features",
                "mobile",
            ],
        ),
        (
            "cargo",
            vec!["check", "-p", "erpx-android-bridge", "--offline"],
        ),
        (
            "cargo",
            vec![
                "check",
                "-p",
                "erpx-android-bridge",
                "--offline",
                "--target",
                "aarch64-linux-android",
            ],
        ),
        (
            "dx",
            vec![
                "build",
                "--web",
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
    println!("ERPX verification completed.");
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
    create_disposable_databases(config, &database_keys).await?;

    let control_write_url = config.database_url(&database_keys[0]);
    let control_read_url = config.database_url(&database_keys[1]);
    let tenant_write_url = config.database_url(&database_keys[2]);
    let tenant_read_url = config.database_url(&database_keys[3]);
    let directory_url = config.database_url(&database_keys[4]);
    let commercial_url = config.database_url(&database_keys[5]);
    let marketing_url = config.database_url(&database_keys[6]);
    let support_url = config.database_url(&database_keys[7]);
    let admin_url = config.database_url("postgres");
    let template = config.database_template();
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
        ("ERPX_TEST_STRIPE_SEMANTIC_ADMIN_DATABASE_URL", admin_url),
        ("ERPX_TEST_TENANT_DATABASE_URL_TEMPLATE", template),
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
    let cleanup_result = drop_disposable_databases(config, &cleanup_keys).await;
    test_result?;
    cleanup_result?;
    println!(
        "Local SQLx verification passed; credentialed MinIO/PITR and LDAP/AD/UCS certification gates remain explicitly excluded."
    );
    Ok(())
}

async fn create_disposable_databases(config: &DevConfig, database_keys: &[String]) -> TaskResult {
    let mut admin = PgConnection::connect(&config.database_url("postgres")).await?;
    for database_key in database_keys {
        validate_disposable_database_key(database_key)?;
        admin
            .execute(format!("CREATE DATABASE \"{database_key}\"").as_str())
            .await?;
    }
    Ok(())
}

async fn drop_disposable_databases(config: &DevConfig, database_keys: &[String]) -> TaskResult {
    let mut admin = PgConnection::connect(&config.database_url("postgres")).await?;
    for database_key in database_keys {
        validate_disposable_database_key(database_key)?;
        admin
            .execute(format!("DROP DATABASE IF EXISTS \"{database_key}\" WITH (FORCE)").as_str())
            .await?;
    }
    Ok(())
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

fn runtime_dir(root: &Path) -> PathBuf {
    root.join(".runtime/erpx")
}

fn secret_file(path: &Path) -> TaskResult<String> {
    if path.exists() {
        return Ok(fs::read_to_string(path)?.trim().to_owned());
    }
    let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
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
    fn reset_accepts_only_loopback_hosts() {
        for host in ["localhost", "127.0.0.1", "::1"] {
            assert!(is_loopback_host(host));
        }
        for host in ["db.example.test", "10.0.0.9", "0.0.0.0"] {
            assert!(!is_loopback_host(host));
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
