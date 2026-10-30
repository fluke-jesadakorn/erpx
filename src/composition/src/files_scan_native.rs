//! Actual unmounted native Files worker. Operator mapping/provider paths are
//! trusted configuration; jobs select none of these roots, binaries or argv.
use crate::files_system_scan::{
    DevelopmentFilesScanConfiguration, DevelopmentFilesScanTransactions,
    assemble_development_files_scan,
};
use erpx_server_files::public::{
    attachment_scan::{
        AttachmentSanitizationProvider, AttachmentScanRunOutcome, LocalSanitizationStore,
        NativeAttachmentSanitizer, run_next_attachment_scan,
    },
    malware_scanner::{ClamAvConfig, ClamAvScanner},
    preview::{BoundedPreviewRenderer, PreviewToolConfiguration},
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, RecordId};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Mutex;
type Error = PgTransactionError<BusinessError>;

/// Trusted explicit provider configuration. No Deserialize, ambient .env,
/// fallback decoder, arbitrary URL, caller filesystem path or runtime activation.
pub struct NativeFilesScanConfiguration {
    pub files_roots: BTreeMap<String, PathBuf>,
    pub temporary_root: PathBuf,
    pub preview_binary: PathBuf,
    pub preview_tools: PreviewToolConfiguration,
    pub scanner: ClamAvConfig,
}
/// Native operational assembly. The same bounded real provider is shared by all
/// submissions; current database authority is reacquired independently perphase.
pub struct NativeFilesScanWorker<C> {
    transactions: DevelopmentFilesScanTransactions<C>,
    sanitizer: NativeAttachmentSanitizer,
    clock: Arc<C>,
    admission: Arc<Mutex<()>>,
    stopping: Arc<AtomicBool>,
    field_only: bool,
}
impl<C> Clone for NativeFilesScanWorker<C> {
    fn clone(&self) -> Self {
        Self {
            transactions: self.transactions.clone(),
            sanitizer: self.sanitizer.clone(),
            clock: self.clock.clone(),
            admission: self.admission.clone(),
            stopping: self.stopping.clone(),
            field_only: self.field_only,
        }
    }
}
impl<C: Clock + Send + Sync + 'static> NativeFilesScanWorker<C> {
    /// Cursor advancement is operational metadata, never source authority.
    pub async fn next_field_candidate(&self) -> Result<Option<RecordId>, Error> {
        if !self.field_only { return Err(unavailable().into()); }
        self.transactions.next_field_candidate().await
    }
    pub async fn run_next(
        &self,
        source_filter: Option<&RecordId>,
    ) -> Result<AttachmentScanRunOutcome, Error> {
        if self.stopping.load(Ordering::Acquire) {
            return Err(unavailable().into());
        }
        let _admission = self
            .admission
            .clone()
            .try_lock_owned()
            .map_err(|_| busy())?;
        if self.stopping.load(Ordering::Acquire) {
            return Err(unavailable().into());
        }
        run_next_attachment_scan(
            &self.transactions,
            &self.sanitizer,
            self.clock.as_ref(),
            source_filter.map(RecordId::as_str),
        )
        .await
    }
    pub(crate) fn cleanup_worker(&self,writer:erpx_server_files::public::field_visit_upload::LocalFieldQuarantineWriter)->Result<crate::field_files_cleanup_worker::FieldFilesCleanupWorker<C>,BusinessError>{
        if !self.field_only {return Err(unavailable());}
        Ok(crate::field_files_cleanup_worker::FieldFilesCleanupWorker::new(self.transactions.clone(),writer,self.clock.clone()))
    }
    pub(crate) fn cleanup_scope(&self)->&erpx_server_files::public::attachment_scan::AttachmentScanScope {self.transactions.scope()}
    pub fn request_shutdown(&self) {
        self.stopping.store(true, Ordering::Release);
        self.sanitizer.request_shutdown();
    }
    pub async fn shutdown_and_drain(&self) {
        self.request_shutdown();
        // No DB lease or job admission is held by this wait. Active scans keep
        // their own ownership; cancellation never invents a terminal verdict.
        self.sanitizer.drain().await;
        let _finished = self.admission.lock().await;
        self.sanitizer.drain().await;
    }
    pub async fn close(self) {
        self.shutdown_and_drain().await;
        self.transactions.close().await;
    }
}

impl<C: Clock + Send + Sync + 'static> crate::field_attachment_intake_command::FieldIntakeProviderReadiness for NativeFilesScanWorker<C> {
    fn probe(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<crate::field_attachment_intake_command::FieldIntakeProviderStatus, BusinessError>> + Send + '_>> {
        Box::pin(async move {
            if !self.field_only || self.stopping.load(Ordering::Acquire) { return Err(unavailable()); }
            // The provider probes current ClamAV PING/VERSION and the bounded
            // preview executable/profile; these are not caller-supplied flags.
            self.sanitizer.require_ready(self.clock.as_ref()).await?;
            Ok(crate::field_attachment_intake_command::FieldIntakeProviderStatus { scanner_enabled:true, scanner_ready:true, preview_ready:true })
        })
    }
}

pub async fn assemble_native_development_files_scan<C: Clock + Send + Sync + 'static>(
    databases: DevelopmentFilesScanConfiguration,
    providers: NativeFilesScanConfiguration,
    clock: Arc<C>,
) -> Result<NativeFilesScanWorker<C>, Error> {
    assemble_native_files_scan(databases, providers, clock, false).await
}

/// Runtime-owned Field worker: persisted Files lineage limits claims.
pub async fn assemble_native_field_files_scan<C: Clock + Send + Sync + 'static>(
    databases: DevelopmentFilesScanConfiguration,
    providers: NativeFilesScanConfiguration,
    clock: Arc<C>,
) -> Result<NativeFilesScanWorker<C>, Error> {
    assemble_native_files_scan(databases, providers, clock, true).await
}

async fn assemble_native_files_scan<C: Clock + Send + Sync + 'static>(
    databases: DevelopmentFilesScanConfiguration,
    providers: NativeFilesScanConfiguration,
    clock: Arc<C>,
    field_only: bool,
) -> Result<NativeFilesScanWorker<C>, Error> {
    if providers.files_roots.is_empty()
        || providers.files_roots.len() > 64
        || !providers.temporary_root.is_absolute()
        || !providers.preview_binary.is_absolute()
        || providers.files_roots.values().any(|p| !p.is_absolute())
    {
        return Err(unavailable().into());
    }
    // This opens only explicit exact-lease databases and verifies schema/current
    // startup registry. It returns no authority usable across later compute.
    let target = assemble_development_files_scan(databases, clock.clone()).await?;
    let target = if field_only { target.field_only() } else { target };
    let result = async {
        let root = providers
            .files_roots
            .get(target.files_binding())
            .ok_or_else(unavailable)?;
        let storage = LocalSanitizationStore::open(root, 1)?;
        let scanner = ClamAvScanner::new(providers.scanner)?;
        let renderer = BoundedPreviewRenderer::new(
            &providers.preview_binary,
            &providers.temporary_root,
            providers.preview_tools,
        )
        .map_err(|_| unavailable())?;
        let profile = renderer
            .current_provider_profile()
            .await
            .map_err(|_| unavailable())?;
        let sanitizer = NativeAttachmentSanitizer::new(storage, scanner, renderer, profile);
        // Real local PING/VERSION/signature freshness and current executable
        // profile checks happen outside all Control/Holding transactions.
        sanitizer.require_ready(clock.as_ref()).await?;
        Ok::<_, Error>(sanitizer)
    }
    .await;
    match result {
        Ok(sanitizer) => Ok(NativeFilesScanWorker {
            transactions: target,
            sanitizer,
            clock,
            admission: Arc::new(Mutex::new(())),
            stopping: Arc::new(AtomicBool::new(false)),
            field_only,
        }),
        Err(error) => {
            target.close().await;
            Err(error)
        }
    }
}
fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_NATIVE_SCANNER_UNAVAILABLE",
        "The explicitly configured native Files scanner is unavailable.",
    )
}
fn busy() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_NATIVE_SCANNER_BUSY",
        "The native Files scanner is already processing an owned job.",
    )
}
