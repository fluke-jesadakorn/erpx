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
#[cfg(feature = "existing-business")]
pub mod legal_queue_source;

#[cfg(any(feature = "legal-pdf-worker", feature = "legal-document-worker"))]
pub mod legal_system_worker;

#[cfg(feature = "legal-document-worker")]
pub mod legal_worker_bootstrap;

#[cfg(feature = "legal-queue")]
pub mod legal_queue_administration;

#[cfg(feature = "existing-business")]
pub mod finance_journal_review;

#[cfg(feature = "legal-queue")]
pub mod legal_processing_status;

#[cfg(feature = "business-http")]
pub mod business_http;

#[cfg(feature = "field-service-http")]
pub mod http_scope;

#[cfg(feature = "private-field-api")]
pub mod private_development_api;

#[cfg(any(feature = "existing-business", feature = "finance-field-read"))]
pub mod finance_journal_detail;

#[cfg(any(feature = "existing-business", feature = "finance-field-read"))]
pub mod finance_journal_environment;

#[cfg(any(feature = "existing-business", feature = "finance-field-read"))]
#[path = "finance_journal_review/field_service_parts.rs"]
mod finance_field_service_lineage;

#[cfg(any(feature = "business-http", feature = "finance-field-read"))]
pub mod finance_journal_detail_http;

#[cfg(feature = "existing-business")]
pub mod maintenance_parts;

#[cfg(feature = "directory-ldaps")]
pub mod directory_source_administration;
#[cfg(feature = "existing-business")]
pub mod files_system_scan;

#[cfg(feature = "files-scan-worker")]
pub mod files_scan_native;

#[cfg(feature = "existing-business")]
pub mod maintenance_transitions;

#[cfg(all(feature = "directory-ldaps", feature = "business-http"))]
pub mod directory_source_http;
#[cfg(feature = "directory-ldaps")]
pub mod directory_source_wire;

#[cfg(feature = "existing-business")]
pub mod maintenance_settlement;

#[cfg(feature = "field-service")]
pub mod field_service_parts;

#[cfg(feature = "field-service")]
pub mod service_parts_posting_setup;
#[cfg(feature = "field-service")]
pub mod service_parts_prerequisite_setup;

#[cfg(feature = "field-service-http")]
pub mod field_service_http;
#[cfg(feature = "field-service-http")]
mod field_service_http_wire;

#[cfg(feature = "field-service-http")]
mod field_service_http_core;

#[cfg(feature = "existing-business")]
pub mod finance_journal_create;

#[cfg(feature = "existing-business")]
pub mod finance_journal_edit;

#[cfg(feature = "business-http")]
pub mod finance_journal_create_http;

#[cfg(feature = "business-http")]
pub mod finance_journal_edit_http;

#[cfg(feature = "field-service")]
pub mod field_service_reservation_read;
#[cfg(feature = "field-service-http")]
pub mod field_service_reservation_read_http;

#[cfg(feature = "field-service")]
pub mod field_service_workspace;

#[cfg(feature = "field-service")]
pub mod field_service_detail;

#[cfg(feature = "field-service")]
pub mod field_attachment;

#[cfg(feature = "field-service")]
pub mod field_attachment_bind;

#[cfg(feature = "field-service")]
pub mod field_attachment_intake;

#[cfg(feature = "field-service")]
pub mod field_service_source_record;

pub mod module_assembly;
#[cfg(feature = "module-example")]
mod module_example;

#[cfg(feature = "field-service")]
pub mod service_company_ownership;
#[cfg(feature = "field-service")]
pub mod service_count_worker;
#[cfg(feature = "field-service")]
pub mod service_stock_setup;
#[cfg(feature = "field-service")]
mod service_initial_physical_stock_sources;
#[cfg(feature = "field-service")]
mod service_initial_physical_stock_purchases;
#[cfg(feature = "field-service")]
mod service_initial_physical_stock_read;
#[cfg(feature = "field-service")]
mod service_initial_physical_stock_command;
#[cfg(feature = "field-service")]
mod service_initial_physical_stock_flow;
#[cfg(feature = "field-service")]
mod service_initial_physical_stock_record;
#[cfg(feature = "field-service")]
mod service_initial_physical_stock_create;
#[cfg(feature = "field-service")]
mod service_initial_physical_stock_action;
#[cfg(feature = "field-service")]
pub mod service_initial_physical_stock_update;
#[cfg(feature = "field-service")]
pub mod service_initial_physical_stock;
#[cfg(feature = "field-service-http")]
pub mod service_initial_physical_stock_transport;
#[cfg(feature = "field-service")]
pub mod service_initial_physical_stock_contracts;

#[cfg(feature = "field-service")]
pub mod field_attachment_intake_command;
