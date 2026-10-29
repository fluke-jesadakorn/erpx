use super::postgres_processing_source::CurrentLegalProcessingSource;
use super::{
    postgres_pdf_verification::{PgLegalPdfVerificationBridge, PgPdfVerificationRepository},
    postgres_processing::{PgLegalProcessingBridge, PgLegalProcessingRepository},
};
use crate::{
    application::pdf_verification::{
        PdfVerificationRepository, PdfVerifierProcess, inspect_company_pdf_for_processing,
        validate_admission,
    },
    domain::{
        pdf_verification::{
            ComputedLegalPdf, PdfComputationReceipt, PdfVerificationFailure, PdfVerificationJob,
            PreparedLegalPdf,
        },
        processing::{LegalProcessingKind, LegalProcessingLease},
    },
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource,
};
use sqlx::PgConnection;

pub struct LegalPdfProcessingRequest<'a> {
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub lease: &'a LegalProcessingLease,
}

/// Short, currently authorized preparation only. Commit the caller unit before
/// running compute_prepared_legal_pdf; no current witness escapes this function.
pub async fn prepare_current_legal_pdf<
    Bridge: PgLegalProcessingBridge + PgLegalPdfVerificationBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
>(
    connection: &mut PgConnection,
    bridge: &mut Bridge,
    clock: &ClockSource,
    identifiers: &mut Identifiers,
    source: &CurrentLegalProcessingSource,
) -> Result<PreparedLegalPdf, PgTransactionError<BusinessError>> {
    source
        .require_current(connection, clock.now().get())
        .await?;
    if source.job().kind != LegalProcessingKind::Verify {
        return Err(BusinessError::invalid(
            "LEGAL_PROCESSING_KIND_MISMATCH",
            "Signature verification requires a verify job.",
        )
        .into());
    }
    PgLegalProcessingRepository::new(
        source.holding_id().clone(),
        source.company_id().clone(),
        connection,
        bridge,
        clock,
        identifiers,
    )
    .authorize_work(source.lease())
    .await?;
    let job = source_job(source);
    let mut repository = PgPdfVerificationRepository::new(
        source.holding_id().clone(),
        source.company_id().clone(),
        connection,
        bridge,
        clock,
        identifiers,
    );
    let mut snapshot = repository.load_authorized(&job).await?;
    validate_admission(&snapshot)?;
    let (document, source_data_sha256) = repository.prepared_source_data()?;
    // The worker's framing field contains an explicit data-only marker. The
    // initial transaction/authority witness is dropped with this repository.
    snapshot.authorization_revision = "erpx-pdf-prepared-data-v1".to_owned();
    Ok(PreparedLegalPdf {
        lease: source.lease().clone(),
        document,
        source_data_sha256,
        prepared_at: clock.now().get(),
        snapshot,
    })
}

/// Fresh final owner unit only. Remint current Files/Identity/Commercial/trust
/// witnesses, compare actual data identities, then retain the dated computation.
/// No subprocess or long computation is performed while these locks are held.
pub async fn retain_current_legal_pdf<
    Bridge: PgLegalProcessingBridge + PgLegalPdfVerificationBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
>(
    connection: &mut PgConnection,
    bridge: &mut Bridge,
    clock: &ClockSource,
    identifiers: &mut Identifiers,
    source: &CurrentLegalProcessingSource,
    computed: &ComputedLegalPdf,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    source
        .require_current(connection, clock.now().get())
        .await?;
    if source.lease() != &computed.lease {
        return Err(PdfVerificationFailure::SnapshotChanged.into());
    }
    PgLegalProcessingRepository::new(
        source.holding_id().clone(),
        source.company_id().clone(),
        connection,
        bridge,
        clock,
        identifiers,
    )
    .authorize_work(source.lease())
    .await?;
    let job = source_job(source);
    let inspection = {
        let mut repository = PgPdfVerificationRepository::new(
            source.holding_id().clone(),
            source.company_id().clone(),
            connection,
            bridge,
            clock,
            identifiers,
        );
        let current = repository.load_authorized(&job).await?;
        validate_admission(&current)?;
        let (document, source_data_sha256) = repository.prepared_source_data()?;
        let stamp = repository.retention_stamp()?;
        if computed.document != document
            || computed.source_data_sha256 != source_data_sha256
            || computed.evidence.snapshot.document_checksum_sha256 != stamp.document_checksum_sha256
            || computed.evidence.snapshot.trust_revision != stamp.trust_revision
            || computed.evidence.snapshot.trust != stamp.trust
            || computed.evidence.snapshot.authorization_revision != "erpx-pdf-prepared-data-v1"
            || computed.evidence.inspection_id.is_some()
            || computed.evidence.computation.is_some()
        {
            return Err(PdfVerificationFailure::SnapshotChanged.into());
        }
        // Old input is data, not authority. Bind the retained row to the fresh
        // authorized snapshot while preserving the original dated computation.
        let mut evidence = computed.evidence.clone();
        evidence.snapshot = stamp;
        evidence.computation = Some(PdfComputationReceipt {
            job_id: job.job_id.clone(),
            attempt: computed.lease.attempts,
            document_version_id: document.id,
            document_version: document.version,
            document_checksum_sha256: document.checksum_sha256,
            original_source_data_sha256: source_data_sha256,
            prepared_at: computed.prepared_at,
        });
        repository
            .retain_if_current(&job, evidence)
            .await?
            .inspection_id
            .ok_or(PdfVerificationFailure::InvalidSnapshot)?
    };
    PgLegalProcessingRepository::new(
        source.holding_id().clone(),
        source.company_id().clone(),
        connection,
        bridge,
        clock,
        identifiers,
    )
    .complete_current_source(source, &inspection)
    .await
}

fn source_job(source: &CurrentLegalProcessingSource) -> PdfVerificationJob {
    PdfVerificationJob {
        job_id: source.job().id.clone(),
        holding_id: source.holding_id().clone(),
        company_id: source.company_id().clone(),
        document_version_id: source.job().document.document_id.clone(),
        requested_by: source.job().created_by.clone(),
    }
}

/// Atomic component/conformance helper, deliberately unmounted. Production
/// workers use short prepare/compute/fresh-retain phases instead of retaining
/// owner/Identity locks during the subprocess.
pub async fn inspect_current_legal_pdf<
    Bridge: PgLegalProcessingBridge + PgLegalPdfVerificationBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
>(
    connection: &mut PgConnection,
    bridge: &mut Bridge,
    verifier: &(impl PdfVerifierProcess + Sync),
    clock: &ClockSource,
    identifiers: &mut Identifiers,
    source: &CurrentLegalProcessingSource,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    source
        .require_current(connection, clock.now().get())
        .await?;
    let result = inspect_claimed_legal_pdf(
        connection,
        bridge,
        verifier,
        clock,
        identifiers,
        LegalPdfProcessingRequest {
            holding_id: source.holding_id().clone(),
            company_id: source.company_id().clone(),
            lease: source.lease(),
        },
    )
    .await?;
    source
        .record_completed(connection, clock.now().get(), &result)
        .await?;
    Ok(result)
}

/// Atomic component helper, not the production worker topology. It executes one
/// caller-owned transaction retaining current authority through commit. The caller
/// owns retry, terminal failure recording and runtime shutdown. All work here is
/// offline computation or owner-row effects; no provider message is dispatched.
pub async fn inspect_claimed_legal_pdf<
    Bridge: PgLegalProcessingBridge + PgLegalPdfVerificationBridge + Send,
    ClockSource: Clock + Sync,
    Identifiers: IdentifierSource + Send,
>(
    connection: &mut PgConnection,
    bridge: &mut Bridge,
    verifier: &(impl PdfVerifierProcess + Sync),
    clock: &ClockSource,
    identifiers: &mut Identifiers,
    request: LegalPdfProcessingRequest<'_>,
) -> Result<serde_json::Value, PgTransactionError<BusinessError>> {
    let LegalPdfProcessingRequest {
        holding_id,
        company_id,
        lease,
    } = request;
    lease.require_live(&holding_id, &company_id, clock.now().get())?;
    if lease.job.kind != LegalProcessingKind::Verify {
        return Err(BusinessError::new(
            FailureKind::InvalidInput,
            "LEGAL_PROCESSING_KIND_MISMATCH",
            "Signature verification requires a verify job.",
        )
        .into());
    }
    PgLegalProcessingRepository::new(
        holding_id.clone(),
        company_id.clone(),
        connection,
        bridge,
        clock,
        identifiers,
    )
    .authorize_work(lease)
    .await?;
    let job = PdfVerificationJob {
        job_id: lease.job.id.clone(),
        holding_id: holding_id.clone(),
        company_id: company_id.clone(),
        document_version_id: lease.job.document.document_id.clone(),
        requested_by: lease.job.created_by.clone(),
    };
    let evidence = {
        let mut repository = PgPdfVerificationRepository::new(
            holding_id.clone(),
            company_id.clone(),
            connection,
            bridge,
            clock,
            identifiers,
        );
        inspect_company_pdf_for_processing(&mut repository, verifier, clock, &job).await?
    };
    let inspection_id = evidence
        .inspection_id
        .ok_or(PdfVerificationFailure::InvalidSnapshot)?;
    PgLegalProcessingRepository::new(
        holding_id,
        company_id,
        connection,
        bridge,
        clock,
        identifiers,
    )
    .complete(lease, &inspection_id)
    .await
}
