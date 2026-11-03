//! Cross-owner assembly only. Business rules remain in each owner's public ports.
pub(crate) mod runtime_database;
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

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_journal_review;

#[cfg(feature = "legal-queue")]
pub mod legal_processing_status;

#[cfg(any(feature = "business-http", feature = "finance-field-write"))]
pub mod business_http;

#[cfg(any(feature = "field-service-http", feature = "existing-business"))]
pub mod http_scope;

#[cfg(feature = "private-field-api")]
pub mod private_development_api;

#[cfg(any(feature = "existing-business", feature = "finance-field-read"))]
pub mod finance_journal_detail;

#[cfg(any(feature = "existing-business", feature = "finance-field-read"))]
pub mod finance_presentation;

#[cfg(any(feature = "existing-business", feature = "finance-field-read"))]
pub mod finance_service_invoice_flow;

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_invoice_flow_command;

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_field_journal_flow;
#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_field_work;
#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_field_journal_flow_command;

#[cfg(any(feature = "existing-business", feature = "finance-field-read"))]
pub mod finance_journal_environment;

#[cfg(feature = "field-service")]
#[path = "finance_journal_review/field_service_parts.rs"]
mod finance_field_service_lineage;

#[cfg(any(feature = "business-http", feature = "finance-field-read"))]
pub mod finance_journal_detail_http;

#[cfg(feature = "existing-business")]
pub mod maintenance_parts;

#[cfg(feature = "directory-ldaps")]
pub mod directory_source_administration;
#[cfg(feature = "field-service")]
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

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_journal_create;

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_invoice_post;
#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_no_charge;
#[cfg(feature="field-service")]
mod finance_journal_actor;
#[cfg(any(feature="existing-business",feature="finance-field-read"))]
mod finance_service_no_charge_read;
#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_invoice_post_http;

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_journal_edit;

#[cfg(any(feature = "business-http", feature = "finance-field-write"))]
pub mod finance_journal_create_http;

#[cfg(any(feature = "business-http", feature = "finance-field-write"))]
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

#[cfg(feature = "field-service-http")]
pub mod field_attachment_intake_http;
#[cfg(feature = "field-service-http")]
pub mod field_upload_http;

#[cfg(feature = "field-service")]
pub mod field_attachment_intake_contracts;

#[cfg(feature = "field-service")]
pub mod field_service_source_record;
#[cfg(feature = "field-service")]
mod field_service_records_source;
#[cfg(feature = "field-service")]
pub mod field_service_package_settlement;
#[cfg(feature = "field-service-http")]
pub mod field_service_package_http;
#[cfg(feature = "field-service")]
pub mod field_collaboration_command;
#[cfg(feature = "field-service")]
pub mod field_collaboration_read;
#[cfg(feature = "field-service")]
pub mod field_inbox_read;

#[cfg(feature="field-service")]
mod finance_service_invoice_history;
#[cfg(feature="field-service")]
mod finance_field_journal_history;
#[cfg(feature = "field-service")]
pub mod field_executive_read;
#[cfg(feature = "field-service")]
pub mod field_executive_evidence;
#[cfg(feature = "field-service")]
pub mod field_inbox_command;
#[cfg(feature = "field-service")]
pub mod field_support_read;

pub mod module_assembly;
#[cfg(feature = "module-example")]
mod module_example;

#[cfg(feature = "field-service")]
pub mod service_company_ownership;
#[cfg(feature = "field-service")]
pub mod service_count_worker;
#[cfg(feature="field-service")]
pub mod field_dependency_worker;
#[cfg(feature = "field-service")]
pub mod service_stock_setup;
#[cfg(feature = "field-service")]
pub mod service_customer_preparation;
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

#[cfg(feature = "files-scan-worker")]
pub mod files_scan_worker;

#[cfg(feature = "files-scan-worker")]
pub mod field_files_runtime_configuration;

#[cfg(feature = "files-scan-worker")]
pub mod field_files_cleanup_worker;

#[cfg(feature = "field-service")]
pub mod field_operations_command;

#[cfg(feature="field-push-worker")]
pub mod field_push_worker;

#[cfg(feature="field-service")]
pub mod field_reservation_alerts;

#[cfg(feature="field-service")]
pub mod native_work_roles;

#[cfg(feature="field-service-http")]
pub mod native_work_roles_http;

#[cfg(feature="field-service")]
pub mod native_account_preferences;

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_receipt_create;

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_receipt_review;
#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_receipt_http;

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_receipt_bank;

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_receipt_confirm;

#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_service_receipt_files;
#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_field_journal_files;
#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
mod finance_field_journal_files_bind;
#[cfg(any(feature = "existing-business", feature = "finance-field-write"))]
pub mod finance_field_files_intake_command;

#[cfg(feature = "existing-business")]
pub mod support_incident_command;
#[cfg(feature = "existing-business")]
pub mod support_session_command;
#[cfg(feature = "existing-business")]
pub mod support_session_http;
#[cfg(feature = "existing-business")]
pub mod support_incident_http;
#[cfg(feature = "existing-business")]
pub mod support_ticket_command;
#[cfg(feature = "existing-business")]
pub mod support_ticket_http;
