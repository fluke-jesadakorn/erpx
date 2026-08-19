//! Bounded holding-write privacy sweep for Marketing form submissions.
//!
//! This runner deliberately opens only the routed holding write database. The
//! read pool passed to [`SqlxMarketingContentStore`] is the same write-side
//! pool handle; no read route, object store, or object bytes are accessed by
//! the privacy mutation.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
use erpx_infrastructure::{
    HoldingDatabaseRouter, MarketingContentPrivacySweepReport, PersistenceError,
    RoutedDatabaseSide, RoutedHoldingDatabase, SqlxMarketingContentStore,
};
use thiserror::Error;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const MARKETING_CONTENT_PRIVACY_HANDLER_NAME: &str = "marketing_content_privacy";
pub use erpx_infrastructure::MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE;
pub const MARKETING_CONTENT_PRIVACY_DEFAULT_BATCH_SIZE: u32 =
    MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MarketingContentPrivacyWorkerError {
    #[error("marketing content privacy configuration error: {0}")]
    Configuration(String),
    #[error("marketing content privacy operation failed")]
    Operation,
}

impl From<MarketingContentPrivacyWorkerError> for WorkerRuntimeError {
    fn from(error: MarketingContentPrivacyWorkerError) -> Self {
        match error {
            MarketingContentPrivacyWorkerError::Configuration(message) => {
                WorkerRuntimeError::Configuration(message)
            }
            MarketingContentPrivacyWorkerError::Operation => WorkerRuntimeError::Configuration(
                "marketing content privacy operation failed".into(),
            ),
        }
    }
}

/// Configuration used to construct the authoritative system-worker command
/// context. The correlation ID is supplied by the durable worker composition
/// boundary and is required to be non-nil for audit and tracing joins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingContentPrivacyWorkerContext {
    pub actor_ref: String,
    pub purpose: String,
    pub correlation_id: Uuid,
}

impl MarketingContentPrivacyWorkerContext {
    pub fn new(
        actor_ref: impl Into<String>,
        purpose: impl Into<String>,
        correlation_id: Uuid,
    ) -> Result<Self, MarketingContentPrivacyWorkerError> {
        let context = Self {
            actor_ref: actor_ref.into(),
            purpose: purpose.into(),
            correlation_id,
        };
        validate_reference(&context.actor_ref, "actor_ref", true)?;
        validate_reference(&context.purpose, "purpose", false)?;
        if context.correlation_id.is_nil() {
            return Err(MarketingContentPrivacyWorkerError::Configuration(
                "marketing content privacy correlation_id is invalid".into(),
            ));
        }
        Ok(context)
    }

    fn command_context(
        &self,
        holding_id: Uuid,
    ) -> Result<CommandContext, MarketingContentPrivacyWorkerError> {
        if holding_id.is_nil() {
            return Err(MarketingContentPrivacyWorkerError::Configuration(
                "marketing content privacy holding_id is invalid".into(),
            ));
        }
        let actor = ActorAttribution {
            principal_ref: self.actor_ref.clone(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let mut command = CommandContext::try_new(
            format!(
                "marketing-content-privacy:{}",
                self.correlation_id.as_hyphenated()
            ),
            actor,
            AuthorityScope::Holding { holding_id },
            self.purpose.clone(),
        )
        .map_err(|_| {
            MarketingContentPrivacyWorkerError::Configuration(
                "marketing content privacy command context is invalid".into(),
            )
        })?;
        command.correlation_id = self.correlation_id;
        command.audit.correlation_id = self.correlation_id;
        Ok(command)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingContentPrivacyRequest {
    pub holding_key: String,
    pub data_profile_key: String,
    pub holding_id: Uuid,
    pub batch_size: u32,
    pub context: MarketingContentPrivacyWorkerContext,
}

impl MarketingContentPrivacyRequest {
    pub fn new(
        holding_key: impl Into<String>,
        data_profile_key: impl Into<String>,
        holding_id: Uuid,
        batch_size: u32,
        context: MarketingContentPrivacyWorkerContext,
    ) -> Result<Self, MarketingContentPrivacyWorkerError> {
        let request = Self {
            holding_key: holding_key.into(),
            data_profile_key: data_profile_key.into(),
            holding_id,
            batch_size,
            context,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), MarketingContentPrivacyWorkerError> {
        validate_scope_reference(&self.holding_key, "holding_key")?;
        validate_scope_reference(&self.data_profile_key, "data_profile_key")?;
        if self.holding_id.is_nil() {
            return Err(MarketingContentPrivacyWorkerError::Configuration(
                "marketing content privacy holding_id is invalid".into(),
            ));
        }
        if self.batch_size == 0 || self.batch_size > MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE {
            return Err(MarketingContentPrivacyWorkerError::Configuration(format!(
                "marketing content privacy batch size must be 1..={MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE}"
            )));
        }
        // Validate the context at request construction so an invalid
        // configuration cannot reach the database route.
        self.context.command_context(self.holding_id).map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketingContentPrivacyRunReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub scanned: u64,
    pub redacted: u64,
}

#[derive(Debug, Clone)]
pub struct MarketingContentPrivacyRunner {
    router: HoldingDatabaseRouter,
    request: MarketingContentPrivacyRequest,
}

impl MarketingContentPrivacyRunner {
    pub fn new(
        router: HoldingDatabaseRouter,
        request: MarketingContentPrivacyRequest,
    ) -> Result<Self, MarketingContentPrivacyWorkerError> {
        request.validate()?;
        Ok(Self { router, request })
    }

    pub const fn batch_size(&self) -> u32 {
        self.request.batch_size
    }

    pub fn request(&self) -> &MarketingContentPrivacyRequest {
        &self.request
    }

    /// Redact only source rows in the routed holding write database. The route
    /// is always closed after the operation, including when store creation or
    /// the SQLx mutation fails.
    pub async fn run_once_at(
        &self,
        now: DateTime<Utc>,
    ) -> Result<MarketingContentPrivacyRunReport, MarketingContentPrivacyWorkerError> {
        let write = self
            .router
            .route(
                &self.request.holding_key,
                &self.request.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(redacted_persistence_error)?;
        let result = self.run_on_write_route(&write, now).await;
        write.close().await;
        result
    }

    async fn run_on_write_route(
        &self,
        write: &RoutedHoldingDatabase,
        now: DateTime<Utc>,
    ) -> Result<MarketingContentPrivacyRunReport, MarketingContentPrivacyWorkerError> {
        let store = SqlxMarketingContentStore::new(
            write.database_pool().clone(),
            write.database_pool().clone(),
            self.request.data_profile_key.clone(),
        )
        .map_err(redacted_persistence_error)?;
        let context = self
            .request
            .context
            .command_context(self.request.holding_id)?;
        let report = store
            .redact_expired_form_submissions(now, self.request.batch_size, &context)
            .await
            .map_err(redacted_persistence_error)?;
        Ok(map_report(
            &self.request.holding_key,
            &self.request.data_profile_key,
            report,
        ))
    }
}

#[async_trait]
impl NamedWorkerHandler for MarketingContentPrivacyRunner {
    fn name(&self) -> &str {
        MARKETING_CONTENT_PRIVACY_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        if worker_id.is_nil() {
            return Err(WorkerRuntimeError::Configuration(
                "marketing content privacy worker_id is invalid".into(),
            ));
        }
        let report = self.run_once_at(Utc::now()).await?;
        Ok(WorkerHandlerReport {
            claimed: report.scanned,
            processed: report.redacted,
            has_work: report.scanned != 0,
            ..WorkerHandlerReport::default()
        })
    }
}

fn map_report(
    holding_id: &str,
    data_profile_key: &str,
    report: MarketingContentPrivacySweepReport,
) -> MarketingContentPrivacyRunReport {
    MarketingContentPrivacyRunReport {
        holding_id: holding_id.into(),
        data_profile_key: data_profile_key.into(),
        scanned: report.scanned,
        redacted: report.redacted,
    }
}

fn validate_reference(
    value: &str,
    field: &str,
    reject_whitespace: bool,
) -> Result<(), MarketingContentPrivacyWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value.chars().any(|character| {
            character.is_control()
                || matches!(character, '/' | '\\')
                || reject_whitespace && character.is_whitespace()
        })
    {
        return Err(MarketingContentPrivacyWorkerError::Configuration(format!(
            "marketing content privacy {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_scope_reference(
    value: &str,
    field: &str,
) -> Result<(), MarketingContentPrivacyWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '/' | '\\' | ':')
        })
    {
        return Err(MarketingContentPrivacyWorkerError::Configuration(format!(
            "marketing content privacy {field} is invalid"
        )));
    }
    Ok(())
}

fn redacted_persistence_error(_error: PersistenceError) -> MarketingContentPrivacyWorkerError {
    MarketingContentPrivacyWorkerError::Operation
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> MarketingContentPrivacyWorkerContext {
        MarketingContentPrivacyWorkerContext::new(
            "system-worker:marketing-content-privacy",
            "marketing content form submission privacy",
            Uuid::new_v4(),
        )
        .expect("valid context")
    }

    #[test]
    fn request_rejects_invalid_bounds_and_scope_references() {
        let holding_id = Uuid::new_v4();
        assert!(
            MarketingContentPrivacyRequest::new("holding", "holding", holding_id, 0, context())
                .is_err()
        );
        assert!(
            MarketingContentPrivacyRequest::new(
                "holding",
                "holding",
                holding_id,
                MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE + 1,
                context(),
            )
            .is_err()
        );
        assert!(
            MarketingContentPrivacyRequest::new(
                "holding/unsafe",
                "holding",
                holding_id,
                1,
                context(),
            )
            .is_err()
        );
        assert!(
            MarketingContentPrivacyRequest::new("holding", "holding ref", holding_id, 1, context(),)
                .is_err()
        );
        assert!(
            MarketingContentPrivacyRequest::new("holding", "holding", Uuid::nil(), 1, context(),)
                .is_err()
        );
    }

    #[test]
    fn context_rejects_nil_correlation_and_unsafe_actor_reference() {
        assert!(
            MarketingContentPrivacyWorkerContext::new("system/worker", "purpose", Uuid::new_v4(),)
                .is_err()
        );
        assert!(
            MarketingContentPrivacyWorkerContext::new("system-worker", "purpose", Uuid::nil(),)
                .is_err()
        );

        let worker_context = context();
        let holding_id = Uuid::new_v4();
        let command = worker_context
            .command_context(holding_id)
            .expect("command context");
        assert_eq!(command.actor.kind, ActorKind::SystemWorker);
        assert_eq!(command.active_scope, AuthorityScope::Holding { holding_id });
        assert_eq!(command.correlation_id, worker_context.correlation_id);
        assert_eq!(command.audit.correlation_id, worker_context.correlation_id);
    }

    #[test]
    fn handler_name_and_batch_default_are_stable() {
        assert_eq!(
            MARKETING_CONTENT_PRIVACY_HANDLER_NAME,
            "marketing_content_privacy"
        );
        assert_eq!(
            MARKETING_CONTENT_PRIVACY_DEFAULT_BATCH_SIZE,
            MARKETING_CONTENT_PRIVACY_MAX_BATCH_SIZE
        );
    }

    #[test]
    fn persistence_errors_are_redacted() {
        let error = redacted_persistence_error(PersistenceError::InvalidData(
            "postgres://secret.example/password".into(),
        ));
        assert_eq!(error, MarketingContentPrivacyWorkerError::Operation);
        assert!(!error.to_string().contains("secret.example"));
        let runtime: WorkerRuntimeError = error.into();
        assert_eq!(
            runtime.to_string(),
            "worker configuration error: marketing content privacy operation failed"
        );
    }

    #[test]
    fn report_mapping_preserves_only_bounded_counts() {
        let mapped = map_report(
            "holding",
            "holding",
            MarketingContentPrivacySweepReport {
                scanned: 4,
                redacted: 3,
            },
        );
        assert_eq!(mapped.scanned, 4);
        assert_eq!(mapped.redacted, 3);
        assert_eq!(mapped.holding_id, "holding");
        assert_eq!(mapped.data_profile_key, "holding");
    }
}
