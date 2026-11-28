//! The Sales owner's native customer-notice reconciliation interface.
//!
//! Composition binds one current-approver authorizer, package mode and bounded
//! provider observation port per Holding, then calls
//! `reconcile_customer_notification` inside its serializable owner unit.
pub use crate::adapters::sales_notification_observation::{
    ResendEmailObservation, UnconfiguredObservation, configured_observation,
};
pub use crate::adapters::sales_notification_reconciliation::PgNotificationReconciliation;
pub use crate::adapters::sales_notification_reconciliation_http::{
    ReconcileHttpBackend, ReconcileHttpFuture, ReconcileHttpRequest, ReconcileHttpResponse,
    ReconcileHttpTarget, sales_notification_reconciliation_router,
};
pub use crate::application::sales_notification_reconciliation::{
    NotificationReconciliationRepository, ProviderObservationPort, ReconcileFuture,
    ReconcilePlan, ReconcileReceiptSnapshot, reconcile_customer_notification,
};
pub use crate::domain::sales_notification_reconciliation::{
    MAX_ATTEMPT_ID, MAX_REASON, MIN_REASON, OPERATION, OBSERVED_WINDOW_MILLIS, ProviderEvent,
    ProviderObservation, ReconcileActor, ReconcileAttempt, ReconcileDecision, ReconcileInput,
    ReconcileReceipt, authority_changed, disabled, idempotency_conflict,
    independent_reviewer_required, not_found, not_uncertain, provider_id_required,
    provider_mismatch, provider_unconfirmed, readback_invalid, request_invalid, source_changed,
    version_conflict,
};
