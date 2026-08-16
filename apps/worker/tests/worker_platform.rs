//! Domain-grouped integration test harness. Individual cases remain in their original files.

#[path = "backup_external_blocker.rs"]
mod backup_external_blocker;
#[path = "backup_retention.rs"]
mod backup_retention;
#[path = "backup_retention_blocker.rs"]
mod backup_retention_blocker;
#[path = "customer_access_audit.rs"]
mod customer_access_audit;
#[path = "legal_projection.rs"]
mod legal_projection;
#[path = "legal_retention.rs"]
mod legal_retention;
#[path = "role_review_notifications.rs"]
mod role_review_notifications;
#[path = "support_attachments.rs"]
mod support_attachments;
#[path = "support_workflow.rs"]
mod support_workflow;
