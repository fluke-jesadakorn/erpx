//! Durable native Legal processing worker for explicitly configured runtime
//! holdings.
//!
//! One reviewed private file names the Control/Holding databases, files roots,
//! Company scope and native document/PDF providers. The worker reloads that
//! file per tick, verifies the current schema and Holding bindings, probes the
//! providers and runs at most one claimed OCR/PDF job per Holding. It never
//! reads HTTP, a personal session or a TypeScript worker, and it has no
//! environment-variable fallback for any target.
use crate::legal_owner_policy::LegalOwnerConfiguration;
use crate::legal_worker_bootstrap::run_once;
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_identity_access::public::{
    AccountPolicy, HoldingOrigin,
    account_holding_bindings::{
        ConfiguredAccountHolding, ConfiguredHoldingRegistry, lock_holding_binding_sources,
    },
    effective_authority::PgQueuedWorkAuthorizer,
};
use erpx_server_legal::public::{
    BoundedDocumentExtractor, DocumentProcessLimits, DocumentToolConfiguration,
    record_policy::LegalCommercialMode,
};
#[cfg(feature = "legal-pdf-worker")]
use erpx_server_legal::public::{BoundedPdfVerifier, PdfProcessLimits};
use erpx_server_platform_administration::public::legal_release::current_legal_release_catalogue;
use erpx_server_pms::public::scheduled_legal::ScheduledReleaseMode;
use erpx_server_runtime::public::postgres::{
    DatabaseConfig, DatabasePool, PgRequestLease, PoolLimits, connect_pool,
};
use erpx_server_shared::public::{Clock, CompanyId, HoldingId, TimestampMillis};
use serde::Deserialize;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_INTERVAL_MS: i64 = 5_000;
pub const MIN_INTERVAL_MS: i64 = 1_000;
pub const MAX_INTERVAL_MS: i64 = 300_000;
pub const MAX_BACKOFF_MS: i64 = 300_000;
const MAX_BACKOFF_SHIFT: u32 = 4;

/// Durable worker cadence. A restart runs immediately, successful ticks keep
/// the configured interval, and transient failures back off exponentially up to
/// the same five-minute ceiling instead of stopping the worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegalWorkerSchedule {
    interval_ms: i64,
    next_due_at: i64,
    consecutive_failures: u32,
}

impl LegalWorkerSchedule {
    pub fn start(now: i64, interval_ms: i64) -> Result<Self, &'static str> {
        if !(MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&interval_ms) {
            return Err("LEGAL_WORKER_INTERVAL_INVALID");
        }
        Ok(Self {
            interval_ms,
            next_due_at: now,
            consecutive_failures: 0,
        })
    }
    pub fn interval_ms(&self) -> i64 {
        self.interval_ms
    }
    pub fn next_due_at(&self) -> i64 {
        self.next_due_at
    }
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
    pub fn due(&self, now: i64) -> bool {
        now >= self.next_due_at
    }
    pub fn next_delay_ms(&self, now: i64) -> i64 {
        self.next_due_at.saturating_sub(now).max(0)
    }
    pub fn succeeded(&mut self, now: i64) {
        self.consecutive_failures = 0;
        self.next_due_at = now.saturating_add(self.interval_ms);
    }
    pub fn failed(&mut self, now: i64) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let shift = (self.consecutive_failures - 1).min(MAX_BACKOFF_SHIFT);
        let delay = self
            .interval_ms
            .saturating_mul(1_i64 << shift)
            .min(MAX_BACKOFF_MS);
        self.next_due_at = now.saturating_add(delay);
    }
    pub fn restart(&mut self, now: i64) {
        self.consecutive_failures = 0;
        self.next_due_at = now;
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeConfigurationDocument {
    version: u8,
    mode: String,
    environment: String,
    account_host: String,
    commercial_mode: String,
    control_database_url: String,
    temporary_root: PathBuf,
    document_binary: PathBuf,
    pdf_binary: Option<PathBuf>,
    tools: DocumentToolConfiguration,
    holdings: Vec<RuntimeHoldingDocument>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeHoldingDocument {
    holding_id: String,
    slug: String,
    host: String,
    company_id: String,
    database_url: String,
    files_root: PathBuf,
    write_binding: String,
    read_binding: String,
    files_binding: String,
}

#[derive(Clone)]
struct RuntimeHolding {
    registration: ConfiguredAccountHolding,
    host: String,
    company_id: CompanyId,
    database: DatabaseConfig,
    files_root: PathBuf,
}

/// No Deserialize or public fields: only the bounded parser below can produce a
/// runtime configuration. Debug never prints database credentials.
pub struct RuntimeLegalWorkerConfiguration {
    environment: String,
    control: DatabaseConfig,
    holdings: Arc<ConfiguredHoldingRegistry>,
    entries: Vec<RuntimeHolding>,
    temporary_root: PathBuf,
    document_binary: PathBuf,
    #[cfg(feature = "legal-pdf-worker")]
    pdf_binary: Option<PathBuf>,
    tools: DocumentToolConfiguration,
}

impl RuntimeLegalWorkerConfiguration {
    pub fn parse(json: &str) -> Result<Self, &'static str> {
        if json.len() > 32 * 1024 {
            return Err("LEGAL_RUNTIME_CONFIGURATION_INVALID");
        }
        let doc: RuntimeConfigurationDocument =
            serde_json::from_str(json).map_err(|_| "LEGAL_RUNTIME_CONFIGURATION_INVALID")?;
        if doc.version != 1
            || doc.mode != "runtime"
            || !["development", "production"].contains(&doc.environment.as_str())
            || doc.commercial_mode != "enforced"
            || doc.account_host.is_empty()
            || doc.account_host.len() > 253
            || doc
                .account_host
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !matches!(b, b'.' | b'-' | b':'))
            || doc.holdings.is_empty()
            || doc.holdings.len() > 8
        {
            return Err("LEGAL_RUNTIME_CONFIGURATION_INVALID");
        }
        for path in [
            &doc.temporary_root,
            &doc.document_binary,
            &doc.tools.pdftotext,
            &doc.tools.pdfinfo,
            &doc.tools.pdftoppm,
            &doc.tools.tesseract,
            &doc.tools.tessdata,
        ] {
            if !path.is_absolute() {
                return Err("LEGAL_RUNTIME_CONFIGURATION_INVALID");
            }
        }
        if doc.pdf_binary.as_ref().is_some_and(|path| !path.is_absolute())
            || doc.pdf_binary.is_some() && !cfg!(feature = "legal-pdf-worker")
        {
            return Err("LEGAL_PDF_PROVIDER_NOT_CONFIGURED");
        }
        doc.tools
            .validate()
            .map_err(|_| "LEGAL_RUNTIME_TOOL_CONFIGURATION_INVALID")?;
        if !doc.document_binary.is_file() {
            return Err("LEGAL_DOCUMENT_PROVIDER_NOT_CONFIGURED");
        }
        #[cfg(feature = "legal-pdf-worker")]
        if doc.pdf_binary.as_ref().is_some_and(|path| !path.is_file()) {
            return Err("LEGAL_PDF_PROVIDER_NOT_CONFIGURED");
        }
        let control = runtime_database(&doc.control_database_url)?;
        let mut ids = std::collections::BTreeSet::new();
        let mut slugs = std::collections::BTreeSet::new();
        let mut hosts = std::collections::BTreeSet::new();
        let mut databases = std::collections::BTreeSet::new();
        let mut entries = Vec::with_capacity(doc.holdings.len());
        for holding in doc.holdings {
            let holding_id = HoldingId::new(&holding.holding_id)
                .map_err(|_| "LEGAL_RUNTIME_HOLDING_INVALID")?;
            let company_id =
                CompanyId::new(&holding.company_id).map_err(|_| "LEGAL_RUNTIME_HOLDING_INVALID")?;
            if !ids.insert(holding.holding_id.clone())
                || !slugs.insert(holding.slug.clone())
                || !hosts.insert(holding.host.clone())
                || !databases.insert(holding.database_url.clone())
                || holding.slug.is_empty()
                || holding.slug.len() > 128
                || !holding
                    .slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
                || holding.host.is_empty()
                || holding.host.len() > 253
                || holding
                    .host
                    .bytes()
                    .any(|b| !b.is_ascii_alphanumeric() && !matches!(b, b'.' | b'-' | b':'))
                || [&holding.write_binding, &holding.read_binding, &holding.files_binding]
                    .iter()
                    .any(|value| {
                        value.is_empty()
                            || value.len() > 128
                            || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    })
                || !holding.files_root.is_absolute()
                || !holding.files_root.is_dir()
            {
                return Err("LEGAL_RUNTIME_HOLDING_INVALID");
            }
            entries.push(RuntimeHolding {
                registration: ConfiguredAccountHolding {
                    holding_id,
                    slug: holding.slug,
                    write_binding: holding.write_binding,
                    read_binding: holding.read_binding,
                    files_binding: holding.files_binding,
                },
                host: holding.host,
                company_id,
                database: runtime_database(&holding.database_url)?,
                files_root: holding.files_root,
            });
        }
        let policy = AccountPolicy {
            account_origin: format!("https://{}", doc.account_host),
            account_host: doc.account_host,
            environment: doc.environment.clone(),
            require_email_verification: true,
            holdings: entries
                .iter()
                .map(|holding| HoldingOrigin {
                    origin: format!("https://{}", holding.host),
                    host: holding.host.clone(),
                    audience: format!(
                        "holding:{}:{}:{}",
                        doc.environment,
                        holding.registration.holding_id.as_str(),
                        holding.host
                    ),
                })
                .collect(),
            products: vec![],
        };
        let holdings = Arc::new(
            ConfiguredHoldingRegistry::new(
                &policy,
                entries
                    .iter()
                    .map(|holding| holding.registration.clone())
                    .collect(),
            )
            .map_err(|_| "LEGAL_RUNTIME_HOLDING_INVALID")?,
        );
        Ok(Self {
            environment: doc.environment,
            control,
            holdings,
            entries,
            temporary_root: doc.temporary_root,
            document_binary: doc.document_binary,
            #[cfg(feature = "legal-pdf-worker")]
            pdf_binary: doc.pdf_binary,
            tools: doc.tools,
        })
    }

    pub fn load(path: &Path) -> Result<Self, &'static str> {
        if !path.is_absolute() {
            return Err("PRIVATE_ABSOLUTE_WORKER_CONFIGURATION_REQUIRED");
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits());
        }
        let file = options
            .open(path)
            .map_err(|_| "PRIVATE_WORKER_CONFIGURATION_REQUIRED")?;
        let metadata = file
            .metadata()
            .map_err(|_| "PRIVATE_WORKER_CONFIGURATION_REQUIRED")?;
        if !metadata.is_file() || metadata.len() > 32 * 1024 {
            return Err("PRIVATE_WORKER_CONFIGURATION_REQUIRED");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err("PRIVATE_WORKER_CONFIGURATION_REQUIRED");
            }
        }
        let mut json = String::new();
        file.take(32 * 1024 + 1)
            .read_to_string(&mut json)
            .map_err(|_| "LEGAL_RUNTIME_CONFIGURATION_INVALID")?;
        Self::parse(&json)
    }
}

fn runtime_database(url: &str) -> Result<DatabaseConfig, &'static str> {
    let endpoint = url::Url::parse(url).map_err(|_| "LEGAL_RUNTIME_DATABASE_INVALID")?;
    if endpoint.scheme() != "postgresql"
        || endpoint.host_str() != Some("127.0.0.1")
        || endpoint.port() != Some(5432)
        || endpoint.password().is_none_or(str::is_empty)
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err("LEGAL_RUNTIME_DATABASE_INVALID");
    }
    DatabaseConfig::from_url(url).map_err(|_| "LEGAL_RUNTIME_DATABASE_INVALID")
}

struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> TimestampMillis {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        TimestampMillis::new(i64::try_from(millis).unwrap_or(i64::MAX))
    }
}

fn owner_configuration(
    holding: &RuntimeHolding,
    environment: &str,
) -> Result<LegalOwnerConfiguration, &'static str> {
    Ok(LegalOwnerConfiguration {
        holding_id: holding.registration.holding_id.clone(),
        company_id: holding.company_id.clone(),
        release_mode: if environment == "production" {
            ScheduledReleaseMode::Production
        } else {
            ScheduledReleaseMode::Development
        },
        commercial_mode: LegalCommercialMode::Enforced,
        artifact_version: "unbuilt".into(),
        release_catalogue: Arc::new(
            current_legal_release_catalogue().map_err(|_| "LEGAL_RELEASE_CATALOGUE_INVALID")?,
        ),
        purchase_catalogue: Arc::new(
            current_package_catalogue().map_err(|_| "LEGAL_PURCHASE_CATALOGUE_INVALID")?,
        ),
        engineering: None,
    })
}

/// The installed jobs/app roles intentionally cannot read `erpx_migrations`.
/// Runtime schema verification therefore checks the exact Legal/Files relations
/// the worker will touch; the installer retains the migration-set gate.
async fn verify_startup(
    control: &DatabasePool,
    config: &RuntimeLegalWorkerConfiguration,
) -> Result<(), &'static str> {
    for holding in &config.entries {
        let pool = connect_pool(&holding.database, PoolLimits::default())
            .await
            .map_err(|_| "LEGAL_RUNTIME_DATABASE_UNAVAILABLE")?;
        let present: bool = sqlx::query_scalar(
            "SELECT to_regclass('public.legal_processing_jobs') IS NOT NULL \
             AND to_regclass('public.legal_documents') IS NOT NULL \
             AND to_regclass('public.legal_document_inspections') IS NOT NULL \
             AND to_regclass('public.legal_access_events') IS NOT NULL \
             AND to_regclass('public.legal_items') IS NOT NULL \
             AND to_regclass('public.legal_settings') IS NOT NULL \
             AND to_regclass('public.company_package_grants') IS NOT NULL \
             AND to_regclass('public.attachments') IS NOT NULL \
             AND to_regclass('public.attachment_sets') IS NOT NULL \
             AND to_regclass('public.attachment_links') IS NOT NULL",
        )
        .fetch_one(pool.raw())
        .await
        .map_err(|_| "LEGAL_RUNTIME_SCHEMA_UNAVAILABLE")?;
        pool.close().await;
        if !present {
            return Err("LEGAL_RUNTIME_SCHEMA_UNAVAILABLE");
        }
    }
    let registry = config.holdings.clone();
    PgRequestLease::new(control.clone())
        .run(move |connection| {
            Box::pin(async move {
                lock_holding_binding_sources(connection).await?;
                registry.verify_startup(connection).await
            })
        })
        .await
        .map_err(|_| "LEGAL_RUNTIME_HOLDING_BINDINGS_UNAVAILABLE")
}

async fn run_cycle(
    config: &RuntimeLegalWorkerConfiguration,
    only: Option<&str>,
) -> Result<serde_json::Value, &'static str> {
    let limits = PoolLimits {
        max_connections: 2,
        acquire_timeout: Duration::from_secs(5),
    };
    let control = connect_pool(&config.control, limits)
        .await
        .map_err(|_| "LEGAL_RUNTIME_DATABASE_UNAVAILABLE")?;
    let startup = verify_startup(&control, config).await;
    if let Err(error) = startup {
        control.close().await;
        return Err(error);
    }
    let extractor = match BoundedDocumentExtractor::new(
        &config.document_binary,
        &config.temporary_root,
        config.tools.clone(),
        DocumentProcessLimits::default(),
    ) {
        Ok(extractor) => extractor,
        Err(_) => {
            control.close().await;
            return Err("LEGAL_DOCUMENT_PROVIDER_UNAVAILABLE");
        }
    };
    #[cfg(feature = "legal-pdf-worker")]
    let verifier = match config.pdf_binary.as_deref() {
        None => None,
        Some(binary) => match BoundedPdfVerifier::new(
            binary,
            &config.temporary_root,
            PdfProcessLimits::default(),
        ) {
            Ok(verifier) => Some(verifier),
            Err(_) => {
                extractor.shutdown_and_drain().await;
                control.close().await;
                return Err("LEGAL_PDF_PROVIDER_UNAVAILABLE");
            }
        },
    };
    let mut results = Vec::new();
    for holding in &config.entries {
        if only.is_some_and(|id| id != holding.registration.holding_id.as_str()) {
            continue;
        }
        let pool = match connect_pool(&holding.database, limits).await {
            Ok(pool) => pool,
            Err(_) => {
                #[cfg(feature = "legal-pdf-worker")]
                if let Some(verifier) = verifier {
                    verifier.shutdown_and_drain().await;
                }
                extractor.shutdown_and_drain().await;
                control.close().await;
                return Err("LEGAL_RUNTIME_DATABASE_UNAVAILABLE");
            }
        };
        let now = SystemClock.now().get();
        let pending: bool = match sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM legal_processing_jobs WHERE company_id=$1 AND attempts<3 AND (state='queued' OR (state='running' AND lease_until<$2)))",
        )
        .bind(holding.company_id.as_str())
        .bind(now)
        .fetch_one(pool.raw())
        .await
        {
            Ok(pending) => pending,
            Err(_) => {
                pool.close().await;
                #[cfg(feature = "legal-pdf-worker")]
                if let Some(verifier) = verifier {
                    verifier.shutdown_and_drain().await;
                }
                extractor.shutdown_and_drain().await;
                control.close().await;
                return Err("LEGAL_RUNTIME_QUEUE_UNAVAILABLE");
            }
        };
        if !pending {
            pool.close().await;
            results.push(serde_json::json!({
                "holdingId": holding.registration.holding_id.as_str(),
                "companyId": holding.company_id.as_str(),
                "outcome": {"acceptance": false, "claimed": false, "skipped": "no-current-work"},
            }));
            continue;
        }
        let owner = owner_configuration(holding, &config.environment)?;
        let outcome = run_once(
            &control,
            config.holdings.clone(),
            &pool,
            owner,
            &holding.files_root,
            &extractor,
            #[cfg(feature = "legal-pdf-worker")]
            verifier.as_ref(),
        )
        .await;
        pool.close().await;
        results.push(match outcome {
            Ok(report) => serde_json::json!({
                "holdingId": holding.registration.holding_id.as_str(),
                "companyId": holding.company_id.as_str(),
                "outcome": report,
            }),
            Err(code) => {
                #[cfg(feature = "legal-pdf-worker")]
                if let Some(verifier) = verifier {
                    verifier.shutdown_and_drain().await;
                }
                extractor.shutdown_and_drain().await;
                control.close().await;
                return Err(code);
            }
        });
    }
    #[cfg(feature = "legal-pdf-worker")]
    if let Some(verifier) = verifier {
        verifier.shutdown_and_drain().await;
    }
    extractor.shutdown_and_drain().await;
    control.close().await;
    Ok(serde_json::json!({
        "acceptance": false,
        "mode": "runtime",
        "holdings": results,
    }))
}

/// One bounded native cycle over every configured Holding (or one exact
/// Holding). Each invocation re-reads the private configuration and re-probes
/// the providers, so a replaced binary or revoked trust cannot be reused.
pub async fn run_once_file(
    path: &Path,
    only: Option<&str>,
) -> Result<serde_json::Value, &'static str> {
    let config = RuntimeLegalWorkerConfiguration::load(path)?;
    run_cycle(&config, only).await
}

/// Durable native Legal worker. Each tick re-reads the reviewed configuration;
/// a failure backs off and restarts instead of stopping the supervisor.
pub async fn run_loop_file(
    path: &Path,
    shutdown: impl std::future::Future<Output = ()> + Send,
) -> Result<(), &'static str> {
    let mut shutdown = Box::pin(shutdown);
    let mut schedule = LegalWorkerSchedule::start(SystemClock.now().get(), DEFAULT_INTERVAL_MS)?;
    loop {
        let delay = schedule.next_delay_ms(SystemClock.now().get());
        if delay > 0 {
            tokio::select! {
                _ = &mut shutdown => return Ok(()),
                _ = tokio::time::sleep(Duration::from_millis(delay as u64)) => continue,
            }
        }
        match run_once_file(path, None).await {
            Ok(_) => schedule.succeeded(SystemClock.now().get()),
            Err(code) => {
                eprintln!("native Legal worker tick failed: {code}; retrying");
                schedule.failed(SystemClock.now().get());
            }
        }
    }
}
