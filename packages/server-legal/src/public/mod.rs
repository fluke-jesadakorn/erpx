//! The legal owner's supported Rust interface.
pub use crate::adapters::pdf_validation::{OperatorTrustStore, TrustMaterial, validate_signed_pdf};
pub use crate::domain::pdf_validation::{
    PdfValidationReport, SignatureValidation, ValidationState,
};
