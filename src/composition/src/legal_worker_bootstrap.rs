//! Development-only, one-job native bootstrap. Production activation/config,
//! interactive queue administration and durable multi-Holding scheduling remain
//! separate work; no installed-service or environment-variable fallback exists.
use crate::{
    legal_document_bridge::run_current_legal_extraction,
    legal_owner_policy::LegalOwnerConfiguration,
    legal_queue_source::LegalQueuedSource,
    legal_system_worker::{LegalNativeAdmission, LegalSystemWorker},
};
use erpx_server_commercial::public::purchases::current_package_catalogue;
use erpx_server_files::public::canonical_original::LocalOriginalStore;
use erpx_server_identity_access::public::effective_authority::PgQueuedWorkAuthorizer;
use erpx_server_legal::public::{
    BoundedDocumentExtractor, DocumentProcessLimits, DocumentToolConfiguration,
    LegalProcessingKind, document_processing::LegalExtractionRetention,
    record_policy::LegalCommercialMode,
};
use erpx_server_platform_administration::public::legal_release::current_legal_release_catalogue;
use erpx_server_pms::public::scheduled_legal::ScheduledReleaseMode;
use erpx_server_runtime::public::postgres::{
    LeasedDevelopmentDatabase, PgTransactionError, PoolLimits,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConfigurationDocument {
    mode: String,
    commercial_mode: String,
    holding_id: String,
    company_id: String,
    database_url: String,
    lease_token: String,
    files_root: PathBuf,
    temporary_root: PathBuf,
    document_binary: PathBuf,
    pdf_binary: Option<PathBuf>,
    tools: DocumentToolConfiguration,
}
/// No Deserialize or public fields: only the bounded, explicit Development
/// parser can produce a bootstrap configuration. Debug never prints secrets.
pub struct DevelopmentLegalWorkerConfiguration {
    database: LeasedDevelopmentDatabase,
    owner: LegalOwnerConfiguration,
    files_root: PathBuf,
    temporary_root: PathBuf,
    document_binary: PathBuf,
    #[cfg(feature = "legal-pdf-worker")]
    pdf_binary: Option<PathBuf>,
    tools: DocumentToolConfiguration,
}
impl DevelopmentLegalWorkerConfiguration {
    pub fn parse(json: &str) -> Result<Self, &'static str> {
        if json.len() > 32 * 1024 {
            return Err("LEGAL_DEVELOPMENT_CONFIGURATION_INVALID");
        }
        let doc: ConfigurationDocument =
            serde_json::from_str(json).map_err(|_| "LEGAL_DEVELOPMENT_CONFIGURATION_INVALID")?;
        if doc.mode != "development"
            || doc.commercial_mode != "enforced"
            || [
                &doc.files_root,
                &doc.temporary_root,
                &doc.document_binary,
                &doc.tools.pdftotext,
                &doc.tools.pdfinfo,
                &doc.tools.pdftoppm,
                &doc.tools.tesseract,
                &doc.tools.tessdata,
            ]
            .into_iter()
            .any(|p| !p.is_absolute())
        {
            return Err("EXPLICIT_LEASED_DEVELOPMENT_CONFIGURATION_REQUIRED");
        }
        if doc
            .pdf_binary
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
            || doc.pdf_binary.is_some() && !cfg!(feature = "legal-pdf-worker")
        {
            return Err("LEGAL_PDF_PROVIDER_NOT_CONFIGURED");
        }
        let database = LeasedDevelopmentDatabase::parse(&doc.database_url, &doc.lease_token)
            .map_err(|_| "EXACT_LEASED_DEVELOPMENT_DATABASE_REQUIRED")?;
        let owner = LegalOwnerConfiguration {
            holding_id: HoldingId::new(doc.holding_id)
                .map_err(|_| "LEGAL_DEVELOPMENT_CONFIGURATION_INVALID")?,
            company_id: CompanyId::new(doc.company_id)
                .map_err(|_| "LEGAL_DEVELOPMENT_CONFIGURATION_INVALID")?,
            release_mode: ScheduledReleaseMode::Development,
            commercial_mode: LegalCommercialMode::Enforced,
            artifact_version: "unbuilt".into(),
            release_catalogue: Arc::new(
                current_legal_release_catalogue().map_err(|_| "LEGAL_RELEASE_CATALOGUE_INVALID")?,
            ),
            purchase_catalogue: Arc::new(
                current_package_catalogue().map_err(|_| "LEGAL_PURCHASE_CATALOGUE_INVALID")?,
            ),
            engineering: None,
        };
        Ok(Self {
            database,
            owner,
            files_root: doc.files_root,
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
            .map_err(|_| "LEGAL_DEVELOPMENT_CONFIGURATION_INVALID")?;
        Self::parse(&json)
    }
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
/// One invocation probes its configured tools outside DB, recovers expired
/// leases, scans with a continued cursor, and runs at most one job. Optional PDF
/// admission needs its own independently probed profile; successful OCR probes
/// or mere child-binary existence never establish PDF provider readiness.
pub async fn run_once_file(path: &Path) -> Result<serde_json::Value, &'static str> {
    let config = DevelopmentLegalWorkerConfiguration::load(path)?;
    let pool = config
        .database
        .connect(PoolLimits {
            max_connections: 2,
            acquire_timeout: Duration::from_secs(5),
        })
        .await
        .map_err(|_| "LEGAL_DEVELOPMENT_DATABASE_LEASE_UNAVAILABLE")?;
    let extractor = match BoundedDocumentExtractor::new(
        &config.document_binary,
        &config.temporary_root,
        config.tools,
        DocumentProcessLimits::default(),
    ) {
        Ok(v) => v,
        Err(_) => {
            pool.close().await;
            return Err("LEGAL_DOCUMENT_PROVIDER_UNAVAILABLE");
        }
    };
    #[cfg(feature = "legal-pdf-worker")]
    let verifier = match config.pdf_binary.as_deref() {
        None => None,
        Some(binary) => match erpx_server_legal::public::BoundedPdfVerifier::new(
            binary,
            &config.temporary_root,
            erpx_server_legal::public::PdfProcessLimits::default(),
        ) {
            Ok(verifier) => Some(verifier),
            Err(_) => {
                extractor.shutdown_and_drain().await;
                pool.close().await;
                return Err("LEGAL_PDF_PROVIDER_UNAVAILABLE");
            }
        },
    };
    #[cfg(feature = "legal-pdf-worker")]
    let result = run_once(
        &pool,
        config.owner,
        &config.files_root,
        &extractor,
        verifier.as_ref(),
    )
    .await;
    #[cfg(not(feature = "legal-pdf-worker"))]
    let result = run_once(&pool, config.owner, &config.files_root, &extractor).await;
    #[cfg(feature = "legal-pdf-worker")]
    if let Some(verifier) = verifier {
        verifier.shutdown_and_drain().await;
    }
    extractor.shutdown_and_drain().await;
    pool.close().await;
    result
}
async fn run_once(
    pool: &erpx_server_runtime::public::postgres::DatabasePool,
    owner: LegalOwnerConfiguration,
    files_root: &Path,
    extractor: &BoundedDocumentExtractor,
    #[cfg(feature = "legal-pdf-worker")] verifier: Option<
        &erpx_server_legal::public::BoundedPdfVerifier,
    >,
) -> Result<serde_json::Value, &'static str> {
    let clock = Arc::new(SystemClock);
    let ready = extractor
        .probe_readiness(clock.as_ref())
        .await
        .map_err(|_| "LEGAL_DOCUMENT_PROVIDER_UNREADY")?;
    if !extractor.can_accept_work() {
        return Err("LEGAL_DOCUMENT_PROVIDER_UNREADY");
    }
    let admission = LegalNativeAdmission::documents(ready.clone());
    #[cfg(feature = "legal-pdf-worker")]
    let admission = if let Some(verifier) = verifier {
        let proof = verifier
            .probe_readiness(clock.as_ref())
            .await
            .map_err(|_| "LEGAL_PDF_PROVIDER_UNREADY")?;
        if !verifier.can_accept_work() {
            return Err("LEGAL_PDF_PROVIDER_UNREADY");
        }
        admission.with_pdf(proof)
    } else {
        admission
    };
    let worker = LegalSystemWorker::new(pool.clone(), owner.clone(), admission, clock.clone());
    let store = Arc::new(
        LocalOriginalStore::open(files_root, 1).map_err(|_| "LEGAL_FILES_ROOT_UNAVAILABLE")?,
    );
    let recovered = worker
        .recover_expired()
        .await
        .map_err(|_| "LEGAL_RECOVERY_FAILED")?;
    let started = std::time::Instant::now();
    let mut cursor = None;
    let claim = loop {
        if started.elapsed() > Duration::from_secs(20) {
            return Err("LEGAL_SCAN_BUDGET_EXHAUSTED");
        }
        let scan = worker
            .claim(cursor)
            .await
            .map_err(|_| "LEGAL_CLAIM_FAILED")?;
        if let Some(claim) = scan.claim {
            break claim;
        }
        cursor = scan.next_cursor;
        if cursor.is_none() {
            return Ok(
                serde_json::json!({"acceptance":false,"mode":"leased-development","claimed":false,"recovered":recovered,"documentProviderProfile":ready.profile().sha256()}),
            );
        }
    };
    let source = LegalQueuedSource::new(
        owner.holding_id.clone(),
        owner.company_id.clone(),
        claim.lease().job.id.clone(),
        claim.lease().token.clone(),
    );
    let queued = PgQueuedWorkAuthorizer::new(pool.clone(), owner.holding_id.clone(), clock.clone());
    enum Retained {
        Completed,
        Failed(String),
    }
    let kind = claim.lease().job.kind;
    let running = async {
        match kind {
            LegalProcessingKind::Ocr => {
                run_current_legal_extraction(&queued, source, owner, store, extractor, clock)
                    .await
                    .map(|result| match result {
                        LegalExtractionRetention::Completed(_) => Retained::Completed,
                        LegalExtractionRetention::Failed { code, .. } => Retained::Failed(code),
                    })
            }
            LegalProcessingKind::Verify => {
                #[cfg(feature = "legal-pdf-worker")]
                {
                    match verifier {
                        Some(verifier) => crate::legal_pdf_bridge::run_current_legal_pdf(
                            &queued, source, owner, store, verifier, clock,
                        )
                        .await
                        .map(|_| Retained::Completed),
                        None => Err(erpx_server_shared::public::BusinessError::new(
                            erpx_server_shared::public::FailureKind::Unavailable,
                            "LEGAL_PROVIDER_KIND_NOT_ADMITTED",
                            "No independently probed PDF provider is configured.",
                        )
                        .into()),
                    }
                }
                #[cfg(not(feature = "legal-pdf-worker"))]
                {
                    Err(erpx_server_shared::public::BusinessError::new(
                        erpx_server_shared::public::FailureKind::Unavailable,
                        "LEGAL_PROVIDER_KIND_NOT_ADMITTED",
                        "No PDF provider is compiled.",
                    )
                    .into())
                }
            }
        }
    };
    let outcome = tokio::select! {outcome=running=>outcome,_=stop_requested()=>{
        extractor.shutdown_and_drain().await;
        #[cfg(feature="legal-pdf-worker")]if let Some(verifier)=verifier{verifier.shutdown_and_drain().await;}
        return Err("LEGAL_WORKER_CANCELLED");
    }};
    match outcome {
        Ok(Retained::Completed) => Ok(
            serde_json::json!({"acceptance":false,"mode":"leased-development","claimed":true,"recovered":recovered,"state":"completed"}),
        ),
        Ok(Retained::Failed(code)) => Ok(
            serde_json::json!({"acceptance":false,"mode":"leased-development","claimed":true,"recovered":recovered,"state":"failed","code":code}),
        ),
        Err(PgTransactionError::Business(error))
            if ["DOCUMENT_EXTRACT_CANCELLED", "VERIFIER_CANCELLED"].contains(&error.code) =>
        {
            Err("LEGAL_WORKER_CANCELLED")
        }
        Err(PgTransactionError::Business(_)) => {
            worker
                .fail_claim(claim, "LEGAL_CURRENT_AUTHORITY_OR_SOURCE_CHANGED")
                .await
                .map_err(|_| "LEGAL_SYSTEM_FAILURE_RETENTION_DENIED")?;
            Ok(
                serde_json::json!({"acceptance":false,"mode":"leased-development","claimed":true,"recovered":recovered,"state":"failed","code":"LEGAL_CURRENT_AUTHORITY_OR_SOURCE_CHANGED"}),
            )
        }
        Err(PgTransactionError::Persistence(_)) => Err("LEGAL_WORKER_PERSISTENCE_FAILED"),
    }
}
async fn stop_requested() {
    #[cfg(unix)]
    {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=signal.recv()=>{}}
        } else {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
