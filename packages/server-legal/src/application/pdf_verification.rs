use std::future::Future;

#[cfg(feature = "pdf-verifier")]
use crate::domain::pdf_verification::{ComputedLegalPdf, PreparedLegalPdf};
use erpx_server_shared::public::{Clock, TimestampMillis};

use crate::domain::{
    pdf_validation::{PdfValidationReport, ValidationState},
    pdf_verification::{
        AuthorizedPdfSnapshot, PDF_BUSINESS_LIMIT, PdfVerificationEvidence, PdfVerificationFailure,
        PdfVerificationJob, PdfVerificationSnapshotStamp, TRUST_ITEM_BYTES_LIMIT, TRUST_ITEM_LIMIT,
        TrustMaterialReceipt, TrustMaterialState, VERIFICATION_ADMISSION_LIMIT,
    },
};

/// Implement against the current Company, immutable document version and actual
/// person-based trust approval. Bound database/file reads to the admission limits.
pub trait PdfVerificationRepository {
    /// Keep persistence/serialization errors intact for the outer Holding UoW.
    type Error: From<PdfVerificationFailure> + Send;
    fn load_authorized(
        &mut self,
        job: &PdfVerificationJob,
    ) -> impl Future<Output = Result<AuthorizedPdfSnapshot, Self::Error>> + Send;

    /// Atomically recheck scope, authorization, document hash and trust approval
    /// revisions before persisting evidence and its material IDs/checksums.
    fn retain_if_current(
        &mut self,
        job: &PdfVerificationJob,
        evidence: PdfVerificationEvidence,
    ) -> impl Future<Output = Result<PdfVerificationEvidence, Self::Error>> + Send;
}

pub trait PdfVerifierProcess {
    fn verify(
        &self,
        snapshot: AuthorizedPdfSnapshot,
        validation_time: TimestampMillis,
    ) -> impl Future<Output = Result<PdfVerificationEvidence, PdfVerificationFailure>> + Send;
}

/// No transaction, connection, current source or authority is borrowed here.
#[cfg(feature = "pdf-verifier")]
pub async fn compute_prepared_legal_pdf(
    prepared: PreparedLegalPdf,
    verifier: &impl PdfVerifierProcess,
    clock: &impl Clock,
) -> Result<ComputedLegalPdf, PdfVerificationFailure> {
    prepared
        .lease
        .require_live(
            &prepared.lease.job.holding_id,
            &prepared.lease.job.company_id,
            clock.now().get(),
        )
        .map_err(|_| PdfVerificationFailure::SnapshotChanged)?;
    let evidence = compute_pdf_snapshot(prepared.snapshot, verifier, clock.now(), true).await?;
    Ok(ComputedLegalPdf {
        lease: prepared.lease,
        document: prepared.document,
        source_data_sha256: prepared.source_data_sha256,
        prepared_at: prepared.prepared_at,
        evidence,
    })
}

/// Infrastructure failure is not evidence of a valid or invalid signature. A job
/// adapter must retain/retry it as indeterminate and must never release a document.
pub async fn verify_company_pdf<Repository: PdfVerificationRepository>(
    repository: &mut Repository,
    verifier: &impl PdfVerifierProcess,
    clock: &impl Clock,
    job: &PdfVerificationJob,
) -> Result<PdfVerificationEvidence, Repository::Error> {
    verify_with_failure_policy(repository, verifier, clock, job, false).await
}

/// B0's processing workflow retains tool failure as an explicit indeterminate
/// inspection. Source/authority/trust are still revalidated atomically. Admission,
/// approval, checksum and cancellation failures are never converted into evidence.
pub async fn inspect_company_pdf_for_processing<Repository: PdfVerificationRepository>(
    repository: &mut Repository,
    verifier: &impl PdfVerifierProcess,
    clock: &impl Clock,
    job: &PdfVerificationJob,
) -> Result<PdfVerificationEvidence, Repository::Error> {
    verify_with_failure_policy(repository, verifier, clock, job, true).await
}

async fn verify_with_failure_policy<Repository: PdfVerificationRepository>(
    repository: &mut Repository,
    verifier: &impl PdfVerifierProcess,
    clock: &impl Clock,
    job: &PdfVerificationJob,
    retain_tool_failure: bool,
) -> Result<PdfVerificationEvidence, Repository::Error> {
    let snapshot = repository.load_authorized(job).await?;
    if snapshot.holding_id != job.holding_id
        || snapshot.company_id != job.company_id
        || snapshot.document_version_id != job.document_version_id
    {
        return Err(PdfVerificationFailure::InvalidScope.into());
    }
    validate_admission(&snapshot).map_err(Repository::Error::from)?;
    let evidence = compute_pdf_snapshot(snapshot, verifier, clock.now(), retain_tool_failure)
        .await
        .map_err(Repository::Error::from)?;
    let retained = repository.retain_if_current(job, evidence).await?;
    if retained.inspection_id.is_none() {
        return Err(PdfVerificationFailure::InvalidSnapshot.into());
    }
    Ok(retained)
}

/// Owned computation data only. This function never borrows a database or
/// retains an authority/source witness across the process boundary.
pub(crate) async fn compute_pdf_snapshot(
    snapshot: AuthorizedPdfSnapshot,
    verifier: &impl PdfVerifierProcess,
    moment: TimestampMillis,
    retain_tool_failure: bool,
) -> Result<PdfVerificationEvidence, PdfVerificationFailure> {
    validate_admission(&snapshot)?;
    let mut trust: Vec<_> = snapshot
        .trust
        .iter()
        .map(|material| TrustMaterialReceipt {
            id: material.id.clone(),
            version: material.version,
            kind: material.kind,
            checksum_sha256: material.checksum_sha256.to_ascii_lowercase(),
        })
        .collect();
    trust.sort_by(|left, right| left.id.cmp(&right.id));
    let stamp = PdfVerificationSnapshotStamp {
        document_checksum_sha256: snapshot.document_checksum_sha256.to_ascii_lowercase(),
        authorization_revision: snapshot.authorization_revision.clone(),
        trust_revision: snapshot.trust_revision.clone(),
        trust,
    };
    let evidence = match verifier.verify(snapshot, moment).await {
        Ok(evidence) => evidence,
        Err(failure)
            if retain_tool_failure
                && matches!(
                    failure,
                    PdfVerificationFailure::WorkerUnavailable
                        | PdfVerificationFailure::WorkerTimeout
                        | PdfVerificationFailure::WorkerOutputLimit
                        | PdfVerificationFailure::WorkerMemoryLimit
                        | PdfVerificationFailure::WorkerFailed
                        | PdfVerificationFailure::WorkerProtocol
                ) =>
        {
            PdfVerificationEvidence {
                computation: None,
                snapshot: stamp.clone(),
                inspection_id: None,
                report: PdfValidationReport {
                    status: ValidationState::Indeterminate,
                    signatures: Vec::new(),
                    reasons: vec![
                        "VALIDATION_FAILED".to_owned(),
                        failure.code().to_owned(),
                        "TRUST_DIGEST_UNAVAILABLE".to_owned(),
                    ],
                    validated_at: time::OffsetDateTime::from_unix_timestamp_nanos(
                        i128::from(moment.get()) * 1_000_000,
                    )
                    .ok()
                    .and_then(|moment| {
                        moment
                            .format(&time::format_description::well_known::Rfc3339)
                            .ok()
                    })
                    .ok_or(PdfVerificationFailure::InvalidValidationTime)?,
                    // No child report/digest is available. Retained raw material
                    // receipts bind the attempted inputs without inventing proof.
                    trust_store_digest: String::new(),
                    mode: "offline-strict-revocation".to_owned(),
                    engine: "ERPX Rust / OpenSSL / lopdf".to_owned(),
                },
            }
        }
        Err(failure) => return Err(failure),
    };
    if evidence.snapshot != stamp
        || evidence.inspection_id.is_some()
        || evidence.computation.is_some()
    {
        return Err(PdfVerificationFailure::InvalidSnapshot);
    }
    Ok(evidence)
}

pub(crate) fn validate_admission(
    snapshot: &AuthorizedPdfSnapshot,
) -> Result<(), PdfVerificationFailure> {
    if snapshot.pdf.len() > PDF_BUSINESS_LIMIT {
        return Err(PdfVerificationFailure::DocumentSizeLimit);
    }
    if snapshot.trust.len() > TRUST_ITEM_LIMIT {
        return Err(PdfVerificationFailure::AdmissionLimit);
    }
    if snapshot.authorization_revision.is_empty()
        || snapshot.authorization_revision.len() > 200
        || snapshot.trust_revision.is_empty()
        || snapshot.trust_revision.len() > 200
        || !is_sha256(&snapshot.document_checksum_sha256)
    {
        return Err(PdfVerificationFailure::InvalidSnapshot);
    }
    let mut total = snapshot.pdf.len() + 2048;
    let mut ids = std::collections::BTreeSet::new();
    for material in &snapshot.trust {
        if material.company_id != snapshot.company_id {
            return Err(PdfVerificationFailure::InvalidScope);
        }
        if material.state != TrustMaterialState::Approved
            || !material.independent_person_review
            || material.approved_by.is_none()
            || material.approved_by.as_ref() == Some(&material.created_by)
        {
            return Err(PdfVerificationFailure::UnapprovedTrust);
        }
        if material.version == 0
            || !ids.insert(&material.id)
            || !is_sha256(&material.checksum_sha256)
        {
            return Err(PdfVerificationFailure::InvalidSnapshot);
        }
        if material.content.len() > TRUST_ITEM_BYTES_LIMIT {
            return Err(PdfVerificationFailure::AdmissionLimit);
        }
        total = total
            .checked_add(material.content.len() + 1024)
            .ok_or(PdfVerificationFailure::AdmissionLimit)?;
        if total > VERIFICATION_ADMISSION_LIMIT {
            return Err(PdfVerificationFailure::AdmissionLimit);
        }
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
