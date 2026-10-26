use std::future::Future;

use erpx_server_shared::public::{Clock, TimestampMillis};

use crate::domain::pdf_verification::{
    AuthorizedPdfSnapshot, PDF_BUSINESS_LIMIT, PdfVerificationEvidence, PdfVerificationFailure,
    PdfVerificationJob, TRUST_ITEM_BYTES_LIMIT, TRUST_ITEM_LIMIT, TrustMaterialState,
    VERIFICATION_ADMISSION_LIMIT,
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

/// Infrastructure failure is not evidence of a valid or invalid signature. A job
/// adapter must retain/retry it as indeterminate and must never release a document.
pub async fn verify_company_pdf<Repository: PdfVerificationRepository>(
    repository: &mut Repository,
    verifier: &impl PdfVerifierProcess,
    clock: &impl Clock,
    job: &PdfVerificationJob,
) -> Result<PdfVerificationEvidence, Repository::Error> {
    let snapshot = repository.load_authorized(job).await?;
    if snapshot.holding_id != job.holding_id
        || snapshot.company_id != job.company_id
        || snapshot.document_version_id != job.document_version_id
    {
        return Err(PdfVerificationFailure::InvalidScope.into());
    }
    validate_admission(&snapshot).map_err(Repository::Error::from)?;
    let evidence = verifier
        .verify(snapshot, clock.now())
        .await
        .map_err(Repository::Error::from)?;
    let retained = repository.retain_if_current(job, evidence).await?;
    if retained.inspection_id.is_none() {
        return Err(PdfVerificationFailure::InvalidSnapshot.into());
    }
    Ok(retained)
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
