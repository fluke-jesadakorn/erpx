use crate::{
    application::document_extraction::{DocumentExtractor, require_extraction_report},
    domain::{
        document_extraction::{
            DocumentExtractionFailure, DocumentExtractionKind, DocumentExtractionSource,
        },
        document_processing::{
            ComputedLegalExtraction, PreparedLegalExtraction, extraction_changed,
            extraction_compute_budget, extraction_failure,
        },
    },
};
use erpx_server_shared::public::{BusinessError, Clock};

/// Computation accepts owned data only and never borrows a database/authority
/// lease. Reduced deadlines finish owned kill/reap before returning.
pub async fn compute_prepared_legal_extraction(
    prepared: PreparedLegalExtraction,
    extractor: &impl DocumentExtractor,
    clock: &impl Clock,
) -> Result<ComputedLegalExtraction, BusinessError> {
    let now = clock.now().get();
    if now < prepared.prepared_at {
        return Err(extraction_changed());
    }
    let kind = match &prepared.input.source {
        DocumentExtractionSource::InlineText { .. } => DocumentExtractionKind::InlineText,
        DocumentExtractionSource::Original { media_type, .. } => {
            DocumentExtractionKind::Original(*media_type)
        }
    };
    let outcome = match extraction_compute_budget(prepared.lease.until, now) {
        Some(budget) => match extractor.extract_with_budget(prepared.input, budget).await {
            Ok(report) => {
                match require_extraction_report(&prepared.document.checksum_sha256, kind, &report) {
                    Ok(()) => Ok(report),
                    Err(error) => Err(error),
                }
            }
            Err(DocumentExtractionFailure::WorkerCancelled) => {
                return Err(extraction_failure(
                    DocumentExtractionFailure::WorkerCancelled,
                ));
            }
            Err(error) => Err(error),
        },
        None => Err(DocumentExtractionFailure::DeadlineExceeded),
    };
    let computed_at = clock.now().get();
    if computed_at < now {
        return Err(extraction_changed());
    }
    Ok(ComputedLegalExtraction {
        lease: prepared.lease,
        document: prepared.document,
        source_data_sha256: prepared.source_data_sha256,
        prepared_at: prepared.prepared_at,
        computed_at,
        outcome,
    })
}
