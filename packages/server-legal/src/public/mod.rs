//! The legal owner's supported Rust interface.
#[cfg(feature = "pdf-verifier")]
pub use crate::adapters::pdf_process::{BoundedPdfVerifier, PdfProcessLimits};
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
pub use crate::adapters::postgres_trust::{PgLegalTrustCommitAuthority, PgLegalTrustStore};
pub use crate::adapters::postgres_trust_commands::{
    PgLegalTrustWorkflow, execute_legal_trust_command,
};
#[cfg(feature = "trust-materials")]
pub use crate::adapters::trust_material_inspector::RustTrustMaterialInspector;
pub use crate::application::pdf_verification::{
    PdfVerificationRepository, PdfVerifierProcess, verify_company_pdf,
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
    LegalOriginalRead, PDF_BUSINESS_LIMIT, PdfVerificationEvidence, PdfVerificationFailure,
    PdfVerificationJob, PdfVerificationSnapshotStamp, TRUST_ITEM_BYTES_LIMIT, TRUST_ITEM_LIMIT,
    TrustMaterialKind, TrustMaterialReceipt, TrustMaterialState, VERIFICATION_ADMISSION_LIMIT,
    VERIFICATION_DEADLINE_SECONDS, VERIFICATION_OUTPUT_LIMIT,
};
pub use crate::domain::trust_management::{
    InspectedTrustMaterial, LegalTrustActor, LegalTrustCommand, LegalTrustCommandRequest,
    LegalTrustReceipt, LegalTrustRecord,
};
