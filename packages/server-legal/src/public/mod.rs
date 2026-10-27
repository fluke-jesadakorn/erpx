//! The legal owner's supported Rust interface.
pub mod document_processing;
#[cfg(feature = "pdf-verifier")]
pub use crate::adapters::pdf_process::{BoundedPdfVerifier, PdfProcessLimits};
#[cfg(feature = "pdf-verifier")]
pub use crate::adapters::pdf_readiness::{PdfProviderProfile, PdfProviderReadiness};
#[cfg(feature = "pdf-verifier")]
pub use crate::adapters::pdf_resource_limits::{
    PDF_CHILD_DATA_LIMIT, enforce_pdf_child_resource_limits,
};
#[cfg(feature = "pdf-verifier")]
pub use crate::adapters::pdf_validation::{OperatorTrustStore, TrustMaterial, validate_signed_pdf};
#[cfg(feature = "pdf-verifier")]
pub use crate::adapters::postgres_pdf_verification::{
    PgLegalPdfVerificationBridge, PgPdfVerificationRepository,
};
pub use crate::adapters::postgres_processing::{
    PgLegalProcessingBridge, PgLegalProcessingRepository,
};
pub use crate::adapters::postgres_trust::{PgLegalTrustCommitAuthority, PgLegalTrustStore};
pub use crate::adapters::postgres_trust_commands::{
    PgLegalTrustWorkflow, execute_legal_trust_command,
};
#[cfg(feature = "trust-materials")]
pub use crate::adapters::trust_material_inspector::RustTrustMaterialInspector;
pub use crate::application::pdf_verification::{
    PdfVerificationRepository, PdfVerifierProcess, inspect_company_pdf_for_processing,
    verify_company_pdf,
};
pub use crate::application::trust_management::{
    LegalTrustChangePlan, LegalTrustCommandPorts, LegalTrustInspector, PreparedLegalTrustCommand,
    prepare_legal_trust_command,
};
pub use crate::domain::pdf_validation::{
    PdfValidationReport, SignatureValidation, ValidationState,
};
pub use crate::domain::pdf_verification::{
    ApprovedCompanyTrustMaterial, AuthorizedPdfSnapshot, LegalInspectionDocument,
    LegalOriginalRead, PDF_BUSINESS_LIMIT, PdfComputationReceipt, PdfComputeBudget,
    PdfVerificationEvidence, PdfVerificationFailure, PdfVerificationJob,
    PdfVerificationSnapshotStamp, TRUST_ITEM_BYTES_LIMIT, TRUST_ITEM_LIMIT, TrustMaterialKind,
    TrustMaterialReceipt, TrustMaterialState, VERIFICATION_ADMISSION_LIMIT,
    VERIFICATION_DEADLINE_SECONDS, VERIFICATION_OUTPUT_LIMIT,
};
pub use crate::domain::processing::{
    LegalProcessingActor, LegalProcessingCursor, LegalProcessingDocument, LegalProcessingJob,
    LegalProcessingKind, LegalProcessingLease, LegalProcessingReceipt, LegalProcessingScan,
    LegalProcessingStatus, PROCESSING_ATTEMPTS, PROCESSING_LEASE_MILLIS, PROCESSING_SCAN_LIMIT,
};
pub use crate::domain::trust_management::{
    InspectedTrustMaterial, LegalTrustActor, LegalTrustCommand, LegalTrustCommandRequest,
    LegalTrustReceipt, LegalTrustRecord,
};

#[cfg(feature = "document-provider")]
pub use crate::adapters::document_docx::extract_docx_text;
#[cfg(feature = "document-provider")]
pub use crate::adapters::document_engine::{
    DocumentChildRequest, DocumentChildResponse, DocumentToolConfiguration,
    enforce_document_child_resource_limits, extract_document_in_owned_child,
};
#[cfg(feature = "document-provider")]
pub use crate::adapters::document_process::{BoundedDocumentExtractor, DocumentProcessLimits};
#[cfg(feature = "document-provider")]
pub use crate::adapters::document_readiness::{
    DocumentProviderProfile, DocumentProviderReadiness, DocumentProviderResource,
};
#[cfg(feature = "pdf-verifier")]
pub use crate::adapters::postgres_pdf_processing::{
    LegalPdfProcessingRequest, inspect_claimed_legal_pdf, inspect_current_legal_pdf,
    prepare_current_legal_pdf, retain_current_legal_pdf,
};
pub use crate::adapters::postgres_processing_source::{
    CurrentLegalProcessingSource, LegalProcessingSourceRequest,
    load_current_legal_processing_source,
};
pub use crate::application::document_extraction::{
    DocumentExtractor, finish_document_extraction, unresolved_document_extraction,
};
pub use crate::application::processing::{LegalProcessingCommandPorts, queue_legal_processing};
pub use crate::domain::document_extraction::{
    DOCUMENT_INPUT_LIMIT, DocumentComputeBudget, DocumentExtractionFailure,
    DocumentExtractionInput, DocumentExtractionKind, DocumentExtractionReport,
    DocumentExtractionSource, EXTRACTION_DEADLINE_SECONDS, EXTRACTION_OUTPUT_LIMIT,
    EXTRACTION_PAGE_MAP_LIMIT, EXTRACTION_RETENTION_HEADROOM_MILLIS, EXTRACTION_TEXT_LIMIT,
    ExtractedDocument, ExtractedPage, ExtractionMethod, ExtractionStatus, LegalDocumentMediaType,
    SCANNED_PDF_PAGE_LIMIT,
};
pub use crate::domain::processing::{LegalProcessingQueueAuthority, PreparedLegalProcessingQueue};

#[cfg(feature = "pdf-verifier")]
pub use crate::application::pdf_verification::compute_prepared_legal_pdf;
#[cfg(feature = "pdf-verifier")]
pub use crate::domain::pdf_verification::{ComputedLegalPdf, PreparedLegalPdf};
pub mod processing_maintenance;
pub mod record_policy;

pub use crate::adapters::postgres_processing_queue::StagedLegalProcessingQueue;
