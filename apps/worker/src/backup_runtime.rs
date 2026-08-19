//! Worker composition for the production-oriented PostgreSQL/MinIO backup
//! adapter. Durable state and replacement-read activation remain owned by
//! `HoldingDatabaseProvisioner`.

use std::{env, sync::Arc};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use erpx_infrastructure::{
    BackupAdapterError, BackupExecutionRequest, BackupExecutionResult, BackupExecutor,
    BackupExecutorError, DatabaseSide, HoldingBackupJobClaim, HoldingBackupRun,
    HoldingDatabaseProvisioner, HoldingDatabaseProvisioningConfig, HoldingDatabaseRouter,
    HoldingProvisioner, HoldingReadDatabaseReplacement, HoldingSourceProjection,
    MAX_HOLDING_BACKUP_JOB_ATTEMPTS, MAX_HOLDING_BACKUP_JOB_LEASE_SECONDS,
    MAX_HOLDING_BACKUP_JOB_PAGE_SIZE, MAX_HOLDING_BACKUP_JOB_PROJECTION_BATCH_SIZE,
    MIN_HOLDING_BACKUP_JOB_LEASE_SECONDS, PitrBackupAdapter, PitrBackupTarget,
    PitrRestoreTargetResolver, PostgresMinioBackupAdapter, RestoreExecutor,
    RestoreTestExecutionRequest, RestoreTestExecutionResult, SqlxControlPlane,
};

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const HOLDING_BACKUP_HANDLER_NAME: &str = "holding_backup";
pub const BACKUP_EXTERNAL_CREDENTIALS_BLOCKER_CODE: &str =
    "backup_external_credentials_unavailable";
const BACKUP_CLAIM_INVALID_CODE: &str = "backup_claim_invalid";
const BACKUP_EXTERNAL_CREDENTIALS_ERROR: &str = "backup external credentials unavailable";
const BACKUP_CLAIM_INVALID_ERROR: &str = "backup job claim invalid";
const BACKUP_PITR_ADAPTER_UNAVAILABLE_ERROR: &str = "physical PITR adapter unavailable";

#[derive(Debug, Clone)]
pub struct BackupRuntime {
    adapter: Arc<PostgresMinioBackupAdapter>,
    pitr_adapter: Option<Arc<PitrBackupRuntime>>,
}

impl BackupRuntime {
    pub fn from_env() -> Result<Self, BackupAdapterError> {
        let pitr_adapter = if env::var_os("ERPX_BACKUP_WAL_ARCHIVE_DIRECTORY").is_some() {
            Some(Arc::new(PitrBackupRuntime::from_env()?))
        } else {
            None
        };
        Ok(Self {
            adapter: Arc::new(PostgresMinioBackupAdapter::from_env()?),
            pitr_adapter,
        })
    }

    pub fn from_adapter(adapter: PostgresMinioBackupAdapter) -> Self {
        Self {
            adapter: Arc::new(adapter),
            pitr_adapter: None,
        }
    }

    pub fn adapter(&self) -> &PostgresMinioBackupAdapter {
        &self.adapter
    }

    /// Execute a full or PITR request through the provisioner's durable
    /// started/completed/failed state machine. Physical PITR capture is
    /// fail-closed unless its explicit WAL archive configuration produced an
    /// adapter.
    pub async fn execute_backup(
        &self,
        provisioner: &HoldingProvisioner,
        run: HoldingBackupRun,
    ) -> Result<HoldingBackupRun, erpx_infrastructure::PersistenceError> {
        validate_backup_run_for_execution(&run)?;
        if run.kind == "pitr" {
            let Some(pitr_adapter) = self.pitr_adapter.as_ref() else {
                // The durable claim already owns a started run. Persist the
                // unavailable PITR capability before returning so a worker
                // retry cannot leave authoritative backup history stuck in
                // `started` while the job is retried/dead-lettered.
                provisioner
                    .record_failed_backup_run(run.clone(), BACKUP_PITR_ADAPTER_UNAVAILABLE_ERROR)
                    .await?;
                return Err(erpx_infrastructure::PersistenceError::Configuration(
                    BACKUP_PITR_ADAPTER_UNAVAILABLE_ERROR,
                ));
            };
            return provisioner
                .execute_backup_with_executor(run, pitr_adapter.as_ref())
                .await;
        }
        provisioner
            .execute_backup_with_executor(run, self.adapter.as_ref())
            .await
    }

    /// Execute restore and replacement verification through the existing
    /// typed restore seam.
    ///
    /// This legacy shape does not carry a holding scope or source projection,
    /// so it cannot prove the source fence or safely activate a replacement
    /// read database. Keep the method as a compatibility boundary, but refuse
    /// it rather than allowing the provisioner's metadata-only path to be
    /// reached. Durable workers must use the routed method below.
    pub async fn execute_restore_test(
        &self,
        _provisioner: &HoldingProvisioner,
        _run: HoldingBackupRun,
        _replacement: HoldingReadDatabaseReplacement,
        _source_version: u64,
    ) -> Result<HoldingBackupRun, erpx_infrastructure::PersistenceError> {
        Err(erpx_infrastructure::PersistenceError::InvalidData(
            "worker restore tests require the holding-scoped routed replacement path".into(),
        ))
    }

    /// Run the real physical PITR restore and target-evidence verification
    /// against an explicitly resolved isolated target. This method never
    /// prepares, activates, or routes a holding read replacement; credentialed
    /// acceptance can call it once the resolver and target are supplied.
    /// Physical PITR remains fail-closed until those production credentials and
    /// the approved external restore rehearsal are present.
    pub async fn execute_physical_pitr_restore_without_activation(
        &self,
        target: &PitrBackupTarget,
        manifest_object_key: &str,
        restore_root: std::path::PathBuf,
        resolver: Arc<dyn PitrRestoreTargetResolver>,
    ) -> Result<(), erpx_infrastructure::PersistenceError> {
        let Some(pitr_runtime) = self.pitr_adapter.as_ref() else {
            return Err(erpx_infrastructure::PersistenceError::Configuration(
                BACKUP_PITR_ADAPTER_UNAVAILABLE_ERROR,
            ));
        };
        pitr_runtime
            .adapter
            .restore_physical_without_activation(
                target,
                manifest_object_key,
                restore_root,
                resolver,
            )
            .await
            .map_err(map_physical_restore_error)
    }
}

/// Worker-facing composition for physical PostgreSQL PITR.  The durable
/// backup run remains owned by `HoldingProvisioner`; this adapter only binds a
/// persisted recovery point and holding registry target to the real
/// `pg_basebackup`/WAL/object-store implementation.
#[derive(Debug, Clone)]
struct PitrBackupRuntime {
    adapter: Arc<PitrBackupAdapter>,
    source_side: DatabaseSide,
}

impl PitrBackupRuntime {
    fn from_env() -> Result<Self, BackupAdapterError> {
        let source_side = match env::var("ERPX_BACKUP_PITR_SOURCE_SIDE")
            .or_else(|_| env::var("ERPX_BACKUP_SOURCE_SIDE"))
            .unwrap_or_else(|_| "write".into())
            .as_str()
        {
            "write" => DatabaseSide::Write,
            "read" => DatabaseSide::Read,
            _ => return Err(BackupAdapterError::Configuration),
        };
        Ok(Self {
            adapter: Arc::new(PitrBackupAdapter::from_env()?),
            source_side,
        })
    }
}

#[async_trait]
impl BackupExecutor for PitrBackupRuntime {
    async fn execute(
        &self,
        request: BackupExecutionRequest,
    ) -> Result<BackupExecutionResult, BackupExecutorError> {
        if request.kind() != erpx_infrastructure::BackupRunKind::Pitr {
            return Err(BackupExecutorError::Failed);
        }
        let target = request.target();
        let recovery_point = target
            .pitr_recovery_point()
            .cloned()
            .ok_or(BackupExecutorError::Failed)?;
        let source_database_key = match self.source_side {
            DatabaseSide::Write => &target.databases().write_database_key,
            DatabaseSide::Read => &target.databases().read_database_key,
        };
        // PITR must use the registry-owned namespace carried by the target.
        // Falling back to the process-wide adapter namespace would allow a
        // legacy or malformed target to read/write another holding's object
        // prefix. The normal provisioner path always binds this field through
        // `HoldingBackupTarget::for_run_in_namespace`; missing evidence is a
        // hard execution failure.
        let storage_namespace = require_holding_storage_namespace(target.storage_namespace())?;
        let pitr_target = PitrBackupTarget::new(
            target.holding_id(),
            source_database_key,
            storage_namespace,
            target.backup_chain_ref(),
            target.backup_ref(),
            recovery_point,
        )
        .map_err(map_backup_adapter_error)?;
        self.adapter
            .capture(&pitr_target)
            .await
            .map_err(map_backup_adapter_error)?;
        Ok(BackupExecutionResult::for_target(target, true))
    }
}

fn require_holding_storage_namespace(
    storage_namespace: Option<&str>,
) -> Result<&str, BackupExecutorError> {
    storage_namespace.ok_or(BackupExecutorError::Failed)
}

fn map_backup_adapter_error(error: BackupAdapterError) -> BackupExecutorError {
    match error {
        BackupAdapterError::Expired => BackupExecutorError::Expired,
        BackupAdapterError::Unavailable
        | BackupAdapterError::Transient
        | BackupAdapterError::ObjectMissing => BackupExecutorError::Unavailable,
        BackupAdapterError::Configuration
        | BackupAdapterError::Permanent
        | BackupAdapterError::ArtifactTooLarge
        | BackupAdapterError::ChecksumMismatch
        | BackupAdapterError::IdempotencyConflict
        | BackupAdapterError::InventoryUnavailable => BackupExecutorError::Failed,
    }
}

fn map_physical_restore_error(error: BackupAdapterError) -> erpx_infrastructure::PersistenceError {
    match error {
        BackupAdapterError::Configuration => erpx_infrastructure::PersistenceError::Configuration(
            "physical PITR restore configuration is unavailable",
        ),
        BackupAdapterError::Unavailable
        | BackupAdapterError::Expired
        | BackupAdapterError::Transient
        | BackupAdapterError::ObjectMissing => erpx_infrastructure::PersistenceError::InvalidData(
            "physical PITR restore verification is unavailable".into(),
        ),
        BackupAdapterError::Permanent
        | BackupAdapterError::ArtifactTooLarge
        | BackupAdapterError::ChecksumMismatch
        | BackupAdapterError::IdempotencyConflict
        | BackupAdapterError::InventoryUnavailable => {
            erpx_infrastructure::PersistenceError::InvalidData(
                "physical PITR restore verification failed".into(),
            )
        }
    }
}

#[async_trait]
impl BackupExecutor for BackupRuntime {
    async fn execute(
        &self,
        request: BackupExecutionRequest,
    ) -> Result<BackupExecutionResult, BackupExecutorError> {
        BackupExecutor::execute(self.adapter.as_ref(), request).await
    }
}

#[async_trait]
impl RestoreExecutor for BackupRuntime {
    async fn execute(
        &self,
        request: RestoreTestExecutionRequest,
    ) -> Result<RestoreTestExecutionResult, erpx_infrastructure::RestoreExecutorError> {
        RestoreExecutor::execute(self.adapter.as_ref(), request).await
    }
}

/// Durable worker composition for scheduled full backups and restore tests.
/// Claims, run history, retry state, and dead letters live in the control
/// write database; this handler owns no production in-memory queue.
#[derive(Clone)]
pub struct HoldingBackupJobRuntimeHandler {
    provisioner: HoldingProvisioner,
    database_provisioner: HoldingDatabaseProvisioner,
    router: HoldingDatabaseRouter,
    runtime: Option<BackupRuntime>,
    batch_size: u32,
    lease: Duration,
}

impl HoldingBackupJobRuntimeHandler {
    pub fn from_env(
        control: Arc<SqlxControlPlane>,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MAX_HOLDING_BACKUP_JOB_PAGE_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "holding backup batch size is outside its bound".into(),
            ));
        }
        if !(MIN_HOLDING_BACKUP_JOB_LEASE_SECONDS..=MAX_HOLDING_BACKUP_JOB_LEASE_SECONDS)
            .contains(&lease.num_seconds())
        {
            return Err(WorkerRuntimeError::Configuration(
                "holding backup lease is outside its bound".into(),
            ));
        }
        let config = HoldingDatabaseProvisioningConfig::from_env()?;
        let database_provisioner = HoldingDatabaseProvisioner::new(Arc::clone(&control), config)?;
        let router = HoldingDatabaseRouter::new(
            Arc::clone(&control),
            HoldingDatabaseProvisioningConfig::from_env()?,
        )?;
        let runtime = match BackupRuntime::from_env() {
            Ok(runtime) => Some(runtime),
            Err(error) => {
                tracing::warn!(
                    blocker = BACKUP_EXTERNAL_CREDENTIALS_BLOCKER_CODE,
                    "holding backup certification blocker queued; native external backup credentials are unavailable"
                );
                tracing::debug!(?error, "holding backup adapter is disabled");
                None
            }
        };
        Ok(Self {
            provisioner: HoldingProvisioner::new(control),
            database_provisioner,
            router,
            runtime,
            batch_size,
            lease,
        })
    }

    pub fn from_parts(
        provisioner: HoldingProvisioner,
        database_provisioner: HoldingDatabaseProvisioner,
        router: HoldingDatabaseRouter,
        runtime: BackupRuntime,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MAX_HOLDING_BACKUP_JOB_PAGE_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "holding backup batch size is outside its bound".into(),
            ));
        }
        if !(MIN_HOLDING_BACKUP_JOB_LEASE_SECONDS..=MAX_HOLDING_BACKUP_JOB_LEASE_SECONDS)
            .contains(&lease.num_seconds())
        {
            return Err(WorkerRuntimeError::Configuration(
                "holding backup lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            provisioner,
            database_provisioner,
            router,
            runtime: Some(runtime),
            batch_size,
            lease,
        })
    }

    /// Build a worker that can drain the durable queue into truthful failed
    /// run/dead-letter evidence while the native external backup credentials
    /// are unavailable. No external operation is attempted in this mode.
    pub fn from_parts_without_external_adapter(
        provisioner: HoldingProvisioner,
        database_provisioner: HoldingDatabaseProvisioner,
        router: HoldingDatabaseRouter,
        batch_size: u32,
        lease: Duration,
    ) -> Result<Self, WorkerRuntimeError> {
        if batch_size == 0 || batch_size > MAX_HOLDING_BACKUP_JOB_PAGE_SIZE {
            return Err(WorkerRuntimeError::Configuration(
                "holding backup batch size is outside its bound".into(),
            ));
        }
        if !(MIN_HOLDING_BACKUP_JOB_LEASE_SECONDS..=MAX_HOLDING_BACKUP_JOB_LEASE_SECONDS)
            .contains(&lease.num_seconds())
        {
            return Err(WorkerRuntimeError::Configuration(
                "holding backup lease is outside its bound".into(),
            ));
        }
        Ok(Self {
            provisioner,
            database_provisioner,
            router,
            runtime: None,
            batch_size,
            lease,
        })
    }

    async fn execute_claim(
        &self,
        claim: &HoldingBackupJobClaim,
    ) -> Result<(), erpx_infrastructure::PersistenceError> {
        if validate_backup_claim_for_execution(claim).is_err() {
            // A claim is created with a durable started run. If the worker's
            // stricter execution boundary rejects that claim, preserve the
            // run receipt before releasing the job. Otherwise the job could
            // become a retry/dead-letter while its run remains falsely
            // started in the authoritative history/read model.
            self.provisioner
                .record_failed_backup_run(claim.run.clone(), BACKUP_CLAIM_INVALID_ERROR)
                .await?;
            return Err(erpx_infrastructure::PersistenceError::InvalidData(
                BACKUP_CLAIM_INVALID_ERROR.into(),
            ));
        }
        if let Err(error) = self
            .provisioner
            .validate_active_backup_job_binding(&claim.holding_id, &claim.data_profile_key)
            .await
        {
            self.provisioner
                .record_failed_backup_run(claim.run.clone(), "holding_binding_invalid")
                .await?;
            return Err(error);
        }
        let Some(runtime) = self.runtime.as_ref() else {
            self.provisioner
                .record_failed_backup_run(claim.run.clone(), BACKUP_EXTERNAL_CREDENTIALS_ERROR)
                .await?;
            return Err(erpx_infrastructure::PersistenceError::InvalidData(
                BACKUP_EXTERNAL_CREDENTIALS_ERROR.into(),
            ));
        };
        match claim.kind.as_str() {
            "full" | "pitr" => {
                runtime
                    .execute_backup(&self.provisioner, claim.run.clone())
                    .await?;
            }
            "restore_test" => {
                let (candidate, source_version, source_projection) = restore_claim_inputs(claim)?;
                if let Err(error) = self
                    .database_provisioner
                    .provision_restore_test_candidate_database(&claim.holding_id, candidate)
                    .await
                {
                    self.provisioner
                        .record_failed_backup_run(
                            claim.run.clone(),
                            "restore_candidate_prepare_failed",
                        )
                        .await?;
                    return Err(self.fail_closed_restore(claim, error).await);
                }
                let activation = match self
                    .provisioner
                    .execute_restore_test_with_routed_read_replacement_executor(
                        &self.router,
                        claim.run.clone(),
                        &claim.data_profile_key,
                        candidate,
                        source_projection,
                        source_version,
                        runtime.adapter(),
                    )
                    .await
                {
                    Ok(activation) => activation,
                    Err(error) => {
                        return Err(self.fail_closed_restore(claim, error).await);
                    }
                };
                if let Err(error) = validate_restore_activation_evidence(claim, &activation) {
                    return Err(self.fail_closed_restore(claim, error).await);
                }
            }
            _ => {
                let error = erpx_infrastructure::PersistenceError::InvalidData(
                    "backup job kind is invalid".into(),
                );
                self.provisioner
                    .record_failed_backup_run(claim.run.clone(), "backup_kind_invalid")
                    .await?;
                return Err(error);
            }
        }
        Ok(())
    }

    /// Establish a read-route proof after any restore failure. The routed
    /// replacement API records a failed generation when one exists; this
    /// additional check prevents a worker retry when the candidate somehow
    /// became active or when the old route cannot be probed. Both cases are
    /// fail-closed and become durable dead-letter evidence at the job seam.
    async fn fail_closed_restore(
        &self,
        claim: &HoldingBackupJobClaim,
        original_error: erpx_infrastructure::PersistenceError,
    ) -> erpx_infrastructure::PersistenceError {
        let candidate = claim
            .candidate_read_database_key
            .as_deref()
            .unwrap_or_default();
        let routed = match self
            .router
            .route(
                &claim.holding_id,
                &claim.data_profile_key,
                erpx_infrastructure::RoutedDatabaseSide::Read,
            )
            .await
        {
            Ok(routed) => routed,
            Err(_) => {
                return erpx_infrastructure::PersistenceError::InvalidData(
                    "restore rollback evidence is unavailable".into(),
                );
            }
        };
        let active_database_key = routed.database_key.clone();
        let probe = routed.probe().await;
        routed.close().await;
        if probe.is_err() || active_database_key == candidate {
            return erpx_infrastructure::PersistenceError::InvalidData(
                "restore rollback evidence is unavailable".into(),
            );
        }
        original_error
    }
}

#[async_trait]
impl NamedWorkerHandler for HoldingBackupJobRuntimeHandler {
    fn name(&self) -> &str {
        HOLDING_BACKUP_HANDLER_NAME
    }

    async fn run_once(
        &self,
        worker_id: uuid::Uuid,
    ) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        let now = Utc::now();
        self.provisioner
            .project_backup_jobs_to_read(MAX_HOLDING_BACKUP_JOB_PROJECTION_BATCH_SIZE)
            .await?;
        let claims = self
            .provisioner
            .claim_backup_jobs(worker_id, now, self.lease, self.batch_size)
            .await?;
        let mut report = WorkerHandlerReport {
            claimed: claims.len() as u64,
            has_work: !claims.is_empty(),
            ..WorkerHandlerReport::default()
        };
        for claim in claims {
            match self.execute_claim(&claim).await {
                Ok(()) => {
                    self.provisioner
                        .complete_backup_job(&claim, Utc::now())
                        .await?;
                    report.processed = report.processed.saturating_add(1);
                }
                Err(error) => {
                    let (error_code, retryable) = classify_backup_failure(&error);
                    let dead_lettered = self
                        .provisioner
                        .fail_backup_job(&claim, Utc::now(), error_code, retryable)
                        .await?;
                    if dead_lettered {
                        report.dead_lettered = report.dead_lettered.saturating_add(1);
                    } else {
                        report.retried = report.retried.saturating_add(1);
                    }
                }
            }
        }
        Ok(report)
    }
}

fn classify_backup_failure(error: &erpx_infrastructure::PersistenceError) -> (&'static str, bool) {
    match error {
        erpx_infrastructure::PersistenceError::InvalidData(message)
            if message == BACKUP_CLAIM_INVALID_ERROR =>
        {
            (BACKUP_CLAIM_INVALID_CODE, false)
        }
        erpx_infrastructure::PersistenceError::InvalidData(message)
            if message == BACKUP_EXTERNAL_CREDENTIALS_ERROR =>
        {
            (BACKUP_EXTERNAL_CREDENTIALS_BLOCKER_CODE, true)
        }
        erpx_infrastructure::PersistenceError::Configuration(_) => {
            ("backup_configuration_rejected", false)
        }
        erpx_infrastructure::PersistenceError::InvalidData(message)
            if message.contains("rollback evidence")
                || message.contains("source watermark")
                || message.contains("candidate")
                || message.contains("projection")
                || message.contains("restore")
                || message.contains("binding") =>
        {
            ("backup_restore_validation_failed", false)
        }
        erpx_infrastructure::PersistenceError::InvalidData(_) => {
            ("backup_executor_retryable_failure", true)
        }
        _ => ("backup_persistence_unavailable", true),
    }
}

fn source_projection(value: &str) -> Option<HoldingSourceProjection> {
    match value {
        "operations" => Some(HoldingSourceProjection::Operations),
        "finance_banking" => Some(HoldingSourceProjection::FinanceBanking),
        "finance_subledger" => Some(HoldingSourceProjection::FinanceSubledger),
        "finance_close" => Some(HoldingSourceProjection::FinanceClose),
        _ => None,
    }
}

fn validate_database_key(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 63
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

fn validate_opaque_ref(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 255
        && value.chars().all(|character| {
            !character.is_control() && !matches!(character, '/' | '\\' | ':' | ' ')
        })
}

fn validate_backup_run_for_execution(
    run: &HoldingBackupRun,
) -> Result<(), erpx_infrastructure::PersistenceError> {
    if run.id.is_nil()
        || !validate_opaque_ref(&run.holding_id)
        || !validate_opaque_ref(&run.backup_ref)
        || run.status != "started"
        || run.completed_at.is_some()
        || run.verified
        || run.error.is_some()
        || !matches!(run.kind.as_str(), "full" | "pitr")
        || run.restore_target_ref.is_some()
        || (run.kind == "full" && run.pitr_recovery_point.is_some())
        || (run.kind == "pitr" && run.pitr_recovery_point.is_none())
    {
        return Err(erpx_infrastructure::PersistenceError::InvalidData(
            "worker backup run shape is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_backup_claim_for_execution(
    claim: &HoldingBackupJobClaim,
) -> Result<(), erpx_infrastructure::PersistenceError> {
    if claim.id.is_nil()
        || claim.claim_id.is_nil()
        || claim.worker_id.is_nil()
        || claim.run.id.is_nil()
        || claim.attempt == 0
        || claim.attempt > MAX_HOLDING_BACKUP_JOB_ATTEMPTS
    {
        return Err(erpx_infrastructure::PersistenceError::InvalidData(
            "backup job claim identifiers are required".into(),
        ));
    }
    if !validate_opaque_ref(&claim.holding_id)
        || !validate_opaque_ref(&claim.data_profile_key)
        || claim.run.holding_id != claim.holding_id
        || claim.run.kind != claim.kind
        || claim.run.status != "started"
        || claim.run.completed_at.is_some()
        || claim.run.verified
        || claim.run.error.is_some()
        || !validate_opaque_ref(&claim.run.backup_ref)
        || !validate_opaque_ref(&claim.source_projection)
    {
        return Err(erpx_infrastructure::PersistenceError::InvalidData(
            "backup job claim run binding is invalid".into(),
        ));
    }
    match claim.kind.as_str() {
        "full" if claim.run.restore_target_ref.is_none() => {
            if claim.candidate_read_database_key.is_some()
                || claim.source_version.is_some()
                || claim.run.pitr_recovery_point.is_some()
            {
                return Err(erpx_infrastructure::PersistenceError::InvalidData(
                    "full backup claim contains restore or PITR fields".into(),
                ));
            }
            if source_projection(&claim.source_projection).is_none() {
                return Err(erpx_infrastructure::PersistenceError::InvalidData(
                    "backup source projection is invalid".into(),
                ));
            }
            Ok(())
        }
        "pitr" if claim.run.restore_target_ref.is_none() => {
            if claim.candidate_read_database_key.is_some()
                || claim.source_version.is_some()
                || claim.run.pitr_recovery_point.is_none()
            {
                return Err(erpx_infrastructure::PersistenceError::InvalidData(
                    "PITR claim recovery point is invalid".into(),
                ));
            }
            if source_projection(&claim.source_projection).is_none() {
                return Err(erpx_infrastructure::PersistenceError::InvalidData(
                    "backup source projection is invalid".into(),
                ));
            }
            Ok(())
        }
        "restore_test" => {
            if claim.run.pitr_recovery_point.is_some() {
                return Err(erpx_infrastructure::PersistenceError::InvalidData(
                    "restore test claim contains PITR metadata".into(),
                ));
            }
            let candidate = claim
                .candidate_read_database_key
                .as_deref()
                .ok_or_else(|| {
                    erpx_infrastructure::PersistenceError::InvalidData(
                        "restore candidate is missing".into(),
                    )
                })?;
            if !validate_database_key(candidate) {
                return Err(erpx_infrastructure::PersistenceError::InvalidData(
                    "restore candidate database key is invalid".into(),
                ));
            }
            let source_version = claim.source_version.ok_or_else(|| {
                erpx_infrastructure::PersistenceError::InvalidData(
                    "restore source version is missing".into(),
                )
            })?;
            if source_version == 0 {
                return Err(erpx_infrastructure::PersistenceError::InvalidData(
                    "restore source version is invalid".into(),
                ));
            }
            let restore_target = claim.run.restore_target_ref.as_deref().ok_or_else(|| {
                erpx_infrastructure::PersistenceError::InvalidData(
                    "restore target reference is missing".into(),
                )
            })?;
            if !validate_opaque_ref(restore_target)
                || restore_target == claim.run.backup_ref
                || candidate == claim.run.backup_ref
                || candidate == restore_target
                || source_projection(&claim.source_projection).is_none()
            {
                return Err(erpx_infrastructure::PersistenceError::InvalidData(
                    "restore claim target or source projection is invalid".into(),
                ));
            }
            Ok(())
        }
        _ => Err(erpx_infrastructure::PersistenceError::InvalidData(
            "backup job kind is invalid".into(),
        )),
    }
}

fn restore_claim_inputs(
    claim: &HoldingBackupJobClaim,
) -> Result<(&str, u64, HoldingSourceProjection), erpx_infrastructure::PersistenceError> {
    let candidate = claim
        .candidate_read_database_key
        .as_deref()
        .ok_or_else(|| {
            erpx_infrastructure::PersistenceError::InvalidData(
                "restore candidate is missing".into(),
            )
        })?;
    let source_version = claim.source_version.ok_or_else(|| {
        erpx_infrastructure::PersistenceError::InvalidData(
            "restore source version is missing".into(),
        )
    })?;
    let source_projection = source_projection(&claim.source_projection).ok_or_else(|| {
        erpx_infrastructure::PersistenceError::InvalidData(
            "backup source projection is invalid".into(),
        )
    })?;
    Ok((candidate, source_version, source_projection))
}

fn validate_restore_activation_evidence(
    claim: &HoldingBackupJobClaim,
    activation: &erpx_infrastructure::HoldingRestoreTestActivation,
) -> Result<(), erpx_infrastructure::PersistenceError> {
    let candidate = claim
        .candidate_read_database_key
        .as_deref()
        .ok_or_else(|| {
            erpx_infrastructure::PersistenceError::InvalidData(
                "restore activation evidence is incomplete".into(),
            )
        })?;
    let source_version = claim.source_version.ok_or_else(|| {
        erpx_infrastructure::PersistenceError::InvalidData(
            "restore activation evidence is incomplete".into(),
        )
    })?;
    let run = &activation.run;
    if run.id != claim.run.id
        || run.holding_id != claim.holding_id
        || run.kind != "restore_test"
        || run.backup_ref != claim.run.backup_ref
        || run.restore_target_ref != claim.run.restore_target_ref
        || run.pitr_recovery_point != claim.run.pitr_recovery_point
        || run.started_at != claim.run.started_at
        || run.status != "completed"
        || !run.verified
        || run.completed_at.is_none()
        || run.error.is_some()
        || activation.candidate_read_database_key != candidate
        || activation.source_version != source_version
        || activation.generation == 0
    {
        return Err(erpx_infrastructure::PersistenceError::InvalidData(
            "restore activation evidence is incomplete".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        BACKUP_CLAIM_INVALID_CODE, BACKUP_CLAIM_INVALID_ERROR,
        BACKUP_EXTERNAL_CREDENTIALS_BLOCKER_CODE, HOLDING_BACKUP_HANDLER_NAME,
        classify_backup_failure, require_holding_storage_namespace, source_projection,
        validate_backup_claim_for_execution, validate_backup_run_for_execution,
        validate_database_key, validate_restore_activation_evidence,
    };
    use chrono::Utc;
    use erpx_domain::platform_completion::PitrRecoveryPoint;
    use erpx_infrastructure::{
        BackupExecutorError, HoldingBackupJobClaim, HoldingBackupRun, HoldingRestoreTestActivation,
        PersistenceError,
    };
    use uuid::Uuid;

    #[test]
    fn backup_failure_classification_is_bounded_and_fail_closed() {
        assert_eq!(
            classify_backup_failure(&PersistenceError::Configuration("invalid config")),
            ("backup_configuration_rejected", false)
        );
        assert_eq!(
            classify_backup_failure(&PersistenceError::InvalidData(
                "restore source watermark is stale".into(),
            )),
            ("backup_restore_validation_failed", false)
        );
        assert_eq!(
            classify_backup_failure(&PersistenceError::InvalidData(
                "backup executor failed".into(),
            )),
            ("backup_executor_retryable_failure", true)
        );
        assert_eq!(
            classify_backup_failure(&PersistenceError::InvalidData(
                "backup external credentials unavailable".into(),
            )),
            (BACKUP_EXTERNAL_CREDENTIALS_BLOCKER_CODE, true)
        );
        assert_eq!(
            classify_backup_failure(&PersistenceError::InvalidData(
                BACKUP_CLAIM_INVALID_ERROR.into(),
            )),
            (BACKUP_CLAIM_INVALID_CODE, false)
        );
        assert_eq!(HOLDING_BACKUP_HANDLER_NAME, "holding_backup");
    }

    #[test]
    fn pitr_requires_registry_owned_storage_namespace() {
        assert_eq!(
            require_holding_storage_namespace(Some("holding-storage-a")),
            Ok("holding-storage-a")
        );
        assert_eq!(
            require_holding_storage_namespace(None),
            Err(BackupExecutorError::Failed)
        );
    }

    #[test]
    fn malformed_claim_is_not_allowed_to_retry_without_a_run_receipt() {
        let mut claim = HoldingBackupJobClaim {
            id: Uuid::new_v4(),
            holding_id: "holding-a".into(),
            data_profile_key: "holding-a".into(),
            kind: "full".into(),
            run: HoldingBackupRun {
                id: Uuid::new_v4(),
                holding_id: "holding-a".into(),
                kind: "full".into(),
                status: "started".into(),
                backup_ref: "backup-a".into(),
                restore_target_ref: None,
                pitr_recovery_point: None,
                started_at: Utc::now(),
                completed_at: None,
                verified: false,
                error: None,
            },
            candidate_read_database_key: None,
            source_version: None,
            source_projection: "operations".into(),
            attempt: 1,
            claim_id: Uuid::new_v4(),
            worker_id: Uuid::new_v4(),
        };
        claim.source_projection = "untrusted_projection".into();

        assert!(validate_backup_claim_for_execution(&claim).is_err());
        assert_eq!(
            classify_backup_failure(&PersistenceError::InvalidData(
                BACKUP_CLAIM_INVALID_ERROR.into(),
            )),
            (BACKUP_CLAIM_INVALID_CODE, false)
        );
    }

    #[test]
    fn restore_claim_validation_rejects_missing_or_unsafe_restore_inputs() {
        let mut claim = HoldingBackupJobClaim {
            id: Uuid::new_v4(),
            holding_id: "holding-a".into(),
            data_profile_key: "holding-a".into(),
            kind: "restore_test".into(),
            run: HoldingBackupRun {
                id: Uuid::new_v4(),
                holding_id: "holding-a".into(),
                kind: "restore_test".into(),
                status: "started".into(),
                backup_ref: "backup-a".into(),
                restore_target_ref: Some("restore-a".into()),
                pitr_recovery_point: None,
                started_at: Utc::now(),
                completed_at: None,
                verified: false,
                error: None,
            },
            candidate_read_database_key: Some("candidate_a".into()),
            source_version: Some(7),
            source_projection: "operations".into(),
            attempt: 1,
            claim_id: Uuid::new_v4(),
            worker_id: Uuid::new_v4(),
        };
        assert!(validate_backup_claim_for_execution(&claim).is_ok());
        assert!(validate_database_key("candidate_a"));
        assert!(!validate_database_key("candidate/a"));
        assert!(source_projection("finance_close").is_some());

        claim.run.pitr_recovery_point = Some(PitrRecoveryPoint {
            target_time: Some(Utc::now()),
            target_lsn: None,
            wal_archive_ref: "wal-archive-a".into(),
            physical_restore_target_ref: "physical-target-a".into(),
        });
        assert!(validate_backup_claim_for_execution(&claim).is_err());

        let mut unsafe_claim = claim;
        unsafe_claim.run.pitr_recovery_point = None;
        unsafe_claim.candidate_read_database_key = Some("candidate/a".into());
        assert!(validate_backup_claim_for_execution(&unsafe_claim).is_err());
    }

    #[test]
    fn restore_claim_validation_is_fail_closed_for_full_and_stale_runs() {
        let mut claim = HoldingBackupJobClaim {
            id: Uuid::new_v4(),
            holding_id: "holding-a".into(),
            data_profile_key: "holding-a".into(),
            kind: "full".into(),
            run: HoldingBackupRun {
                id: Uuid::new_v4(),
                holding_id: "holding-a".into(),
                kind: "full".into(),
                status: "started".into(),
                backup_ref: "backup-a".into(),
                restore_target_ref: None,
                pitr_recovery_point: None,
                started_at: Utc::now(),
                completed_at: None,
                verified: false,
                error: None,
            },
            candidate_read_database_key: None,
            source_version: None,
            source_projection: "operations".into(),
            attempt: 1,
            claim_id: Uuid::new_v4(),
            worker_id: Uuid::new_v4(),
        };
        assert!(validate_backup_claim_for_execution(&claim).is_ok());
        claim.run.pitr_recovery_point = Some(PitrRecoveryPoint {
            target_time: Some(Utc::now()),
            target_lsn: None,
            wal_archive_ref: "wal-archive-full".into(),
            physical_restore_target_ref: "physical-target-full".into(),
        });
        assert!(validate_backup_claim_for_execution(&claim).is_err());
        claim.run.pitr_recovery_point = None;
        claim.run.status = "completed".into();
        assert!(validate_backup_claim_for_execution(&claim).is_err());
        assert_eq!(
            classify_backup_failure(&PersistenceError::InvalidData(
                "restore rollback evidence is unavailable".into(),
            )),
            ("backup_restore_validation_failed", false)
        );
    }

    #[test]
    fn backup_run_boundary_rejects_metadata_only_or_malformed_execution() {
        let mut run = HoldingBackupRun {
            id: Uuid::new_v4(),
            holding_id: "holding-a".into(),
            kind: "full".into(),
            status: "started".into(),
            backup_ref: "backup-a".into(),
            restore_target_ref: None,
            pitr_recovery_point: None,
            started_at: Utc::now(),
            completed_at: None,
            verified: false,
            error: None,
        };
        assert!(validate_backup_run_for_execution(&run).is_ok());

        run.completed_at = Some(Utc::now());
        assert!(validate_backup_run_for_execution(&run).is_err());
        run.completed_at = None;
        run.restore_target_ref = Some("restore-a".into());
        assert!(validate_backup_run_for_execution(&run).is_err());
        run.restore_target_ref = None;
        run.backup_ref = "backup/a".into();
        assert!(validate_backup_run_for_execution(&run).is_err());
    }

    #[test]
    fn restore_activation_evidence_binds_run_candidate_generation_and_watermark() {
        let claim = HoldingBackupJobClaim {
            id: Uuid::new_v4(),
            holding_id: "holding-a".into(),
            data_profile_key: "holding-a".into(),
            kind: "restore_test".into(),
            run: HoldingBackupRun {
                id: Uuid::new_v4(),
                holding_id: "holding-a".into(),
                kind: "restore_test".into(),
                status: "started".into(),
                backup_ref: "backup-a".into(),
                restore_target_ref: Some("restore-a".into()),
                pitr_recovery_point: None,
                started_at: Utc::now(),
                completed_at: None,
                verified: false,
                error: None,
            },
            candidate_read_database_key: Some("candidate_a".into()),
            source_version: Some(7),
            source_projection: "operations".into(),
            attempt: 1,
            claim_id: Uuid::new_v4(),
            worker_id: Uuid::new_v4(),
        };
        let activation = HoldingRestoreTestActivation {
            run: HoldingBackupRun {
                status: "completed".into(),
                completed_at: Some(Utc::now()),
                verified: true,
                ..claim.run.clone()
            },
            candidate_read_database_key: "candidate_a".into(),
            generation: 3,
            source_version: 7,
        };
        assert!(validate_restore_activation_evidence(&claim, &activation).is_ok());

        let mut stale_activation = activation.clone();
        stale_activation.source_version = 8;
        assert!(validate_restore_activation_evidence(&claim, &stale_activation).is_err());

        let mut mismatched_target = activation.clone();
        mismatched_target.run.restore_target_ref = Some("other-restore".into());
        assert!(validate_restore_activation_evidence(&claim, &mismatched_target).is_err());

        let mut mismatched_pitr_metadata = activation.clone();
        mismatched_pitr_metadata.run.pitr_recovery_point = Some(PitrRecoveryPoint {
            target_time: Some(Utc::now()),
            target_lsn: None,
            wal_archive_ref: "wal-archive-a".into(),
            physical_restore_target_ref: "physical-target-a".into(),
        });
        assert!(validate_restore_activation_evidence(&claim, &mismatched_pitr_metadata).is_err());

        let mut mismatched_started_at = activation;
        mismatched_started_at.run.started_at += chrono::Duration::seconds(1);
        assert!(validate_restore_activation_evidence(&claim, &mismatched_started_at).is_err());
    }
}
