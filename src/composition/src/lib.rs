//! Cross-owner assembly only. Business rules remain in each owner's public ports.
#[cfg(feature = "legal-document-worker")]
pub mod legal_document_bridge;
#[cfg(feature = "legal-document-worker")]
pub mod legal_document_worker;
#[cfg(any(feature = "legal-pdf-worker", feature = "legal-document-worker"))]
mod legal_owner_bridge;
#[cfg(any(
    feature = "legal-queue",
    feature = "legal-pdf-worker",
    feature = "legal-document-worker"
))]
pub mod legal_owner_policy;
#[cfg(feature = "legal-pdf-worker")]
pub mod legal_pdf_bridge;
#[cfg(feature = "legal-pdf-worker")]
pub mod legal_pdf_worker;
pub mod legal_queue_source;

#[cfg(any(feature = "legal-pdf-worker", feature = "legal-document-worker"))]
pub mod legal_system_worker;

#[cfg(feature = "legal-document-worker")]
pub mod legal_worker_bootstrap;

#[cfg(feature = "legal-queue")]
pub mod legal_queue_administration;

pub mod finance_journal_review;

#[cfg(feature = "legal-queue")]
pub mod legal_processing_status;

#[cfg(feature = "business-http")]
pub mod business_http;

#[cfg(feature = "private-development-api")]
pub mod private_development_api;

pub mod finance_journal_detail;

#[cfg(feature = "business-http")]
pub mod finance_journal_detail_http;

pub mod maintenance_parts;

#[cfg(feature = "directory-ldaps")]
pub mod directory_source_administration;
pub mod files_system_scan;

#[cfg(feature = "files-scan-worker")]
pub mod files_scan_native;

pub mod maintenance_transitions;

#[cfg(all(feature = "directory-ldaps", feature = "business-http"))]
pub mod directory_source_http;
#[cfg(feature = "directory-ldaps")]
pub mod directory_source_wire;

pub mod maintenance_settlement;

pub mod field_service_parts;

#[cfg(feature = "business-http")]
pub mod field_service_http;
#[cfg(feature = "business-http")]
mod field_service_http_wire;

#[cfg(feature = "business-http")]
mod field_service_http_core;

pub mod finance_journal_create;

pub mod finance_journal_edit;

#[cfg(feature = "business-http")]
pub mod finance_journal_create_http;

#[cfg(feature = "business-http")]
pub mod finance_journal_edit_http;

pub mod field_service_reservation_read;
#[cfg(feature = "business-http")]
pub mod field_service_reservation_read_http;

pub mod field_service_workspace;

pub mod field_service_detail;

pub mod field_attachment;

pub mod field_attachment_bind;
