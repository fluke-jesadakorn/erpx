//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "customer360_detail_reads.rs"]
mod customer360_detail_reads;
#[path = "hr_payroll_projection.rs"]
mod hr_payroll_projection;
#[path = "hr_payroll_validation.rs"]
mod hr_payroll_validation;
#[path = "hr_persistence_envelope.rs"]
mod hr_persistence_envelope;
#[path = "legal_document_scope_integrity.rs"]
mod legal_document_scope_integrity;
#[path = "legal_persistence.rs"]
mod legal_persistence;
#[path = "legal_projection_read.rs"]
mod legal_projection_read;
#[path = "role_review_persistence.rs"]
mod role_review_persistence;
#[path = "support_persistence.rs"]
mod support_persistence_cases;
#[path = "support_workflow.rs"]
mod support_workflow;
