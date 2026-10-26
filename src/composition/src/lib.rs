//! Cross-owner assembly only. Business rules remain in each owner's public ports.
#[cfg(feature = "legal-pdf-worker")]
pub mod legal_owner_policy;
#[cfg(feature = "legal-pdf-worker")]
pub mod legal_pdf_bridge;
#[cfg(feature = "legal-pdf-worker")]
pub mod legal_pdf_worker;
pub mod legal_queue_source;
