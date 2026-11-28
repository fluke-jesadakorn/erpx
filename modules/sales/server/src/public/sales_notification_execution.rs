//! The Sales owner's native customer-notice delivery interface.
//!
//! Composition binds one `EmailDispatchPort` and one `NotificationExecutionUnit`
//! per Holding, then calls `drain_notifications`/`deliver_notification`. The
//! owner keeps every claim, lease, authority and terminal rule.
pub use crate::adapters::sales_notification_email::{
    ResendEmailDispatch, UnconfiguredEmailDispatch,
};
pub use crate::adapters::sales_notification_execution::PgNotificationExecution;
pub use crate::application::sales_notification_execution::{
    DeliveryStage, DispatchOutcome, DispatchPlan, DispatchTrace, EmailDispatchPort,
    ExecutionFuture, NotificationExecutionUnit, OutboundEmail, deliver_notification,
    drain_notifications,
};
pub use crate::domain::sales_notification_execution::{
    ATTEMPT_UNCERTAIN, AUTHORIZATION_REQUIRED, ClaimDecision, ExecutionAttempt,
    ExecutionAuthorization, HELD_UNTIL, LEASE_MILLIS, MAX_ATTEMPTS, NOT_CONFIGURED,
    NotificationSource, QueuedActor, attempt_uncertain, authorization_current, claim_decision,
    invalid_reason, not_configured, parse_queued_actor,
};
