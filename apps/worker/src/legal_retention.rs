//! Holding-write Legal retention sweep worker.
//!
//! This seam deliberately routes only the holding write database. Retention
//! expiry is a source-state mutation; read projections and object storage are
//! separate concerns and are never opened or touched here.

use chrono::{DateTime, Utc};
use erpx_infrastructure::{
    HoldingDatabaseRouter, LegalRetentionSweepReport, RoutedDatabaseSide, RoutedHoldingDatabase,
    SqlxLegalPersistence,
};
use thiserror::Error;
use uuid::Uuid;

use crate::{NamedWorkerHandler, WorkerHandlerReport, WorkerRuntimeError};

pub const LEGAL_RETENTION_HANDLER_NAME: &str = "legal_retention";
pub const LEGAL_RETENTION_MAX_BATCH_SIZE: u32 = 100;
pub const LEGAL_RETENTION_DEFAULT_BATCH_SIZE: u32 = 100;
const LEGAL_RETENTION_SYSTEM_ACTOR: &str = "system-worker:legal-retention";
const LEGAL_RETENTION_PURPOSE: &str = "expire legal contract retention";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LegalRetentionWorkerError {
    #[error("Legal retention configuration error: {0}")]
    Configuration(String),
    #[error("Legal retention {operation} failed")]
    Retention { operation: &'static str },
}

impl From<LegalRetentionWorkerError> for WorkerRuntimeError {
    fn from(error: LegalRetentionWorkerError) -> Self {
        match error {
            LegalRetentionWorkerError::Configuration(message) => {
                WorkerRuntimeError::Configuration(message)
            }
            LegalRetentionWorkerError::Retention { operation } => {
                WorkerRuntimeError::Configuration(format!("Legal retention {operation} failed"))
            }
        }
    }
}

/// The source-owned worker context used to bind an explicit sweep invocation.
/// The current source API supplies the authoritative system-worker command
/// context internally; these fields prevent an unbound handler from being
/// composed and provide stable correlation metadata for worker operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalRetentionWorkerContext {
    pub actor_ref: String,
    pub purpose: String,
    pub correlation_id: Uuid,
}

impl LegalRetentionWorkerContext {
    pub fn new(
        actor_ref: impl Into<String>,
        purpose: impl Into<String>,
        correlation_id: Uuid,
    ) -> Result<Self, LegalRetentionWorkerError> {
        let context = Self {
            actor_ref: actor_ref.into(),
            purpose: purpose.into(),
            correlation_id,
        };
        validate_reference(&context.actor_ref, "actor_ref")?;
        validate_reference(&context.purpose, "purpose")?;
        if context.correlation_id.is_nil() {
            return Err(LegalRetentionWorkerError::Configuration(
                "Legal retention correlation_id is invalid".into(),
            ));
        }
        Ok(context)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalRetentionRequest {
    pub holding_id: String,
    pub data_profile_key: String,
    pub batch_size: u32,
    pub context: LegalRetentionWorkerContext,
}

impl LegalRetentionRequest {
    pub fn new(
        holding_id: impl Into<String>,
        data_profile_key: impl Into<String>,
        batch_size: u32,
        context: LegalRetentionWorkerContext,
    ) -> Result<Self, LegalRetentionWorkerError> {
        let request = Self {
            holding_id: holding_id.into(),
            data_profile_key: data_profile_key.into(),
            batch_size,
            context,
        };
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), LegalRetentionWorkerError> {
        validate_scope_reference(&self.holding_id, "holding_id")?;
        validate_scope_reference(&self.data_profile_key, "data_profile_key")?;
        if self.context.actor_ref != LEGAL_RETENTION_SYSTEM_ACTOR
            || self.context.purpose != LEGAL_RETENTION_PURPOSE
        {
            return Err(LegalRetentionWorkerError::Configuration(
                "Legal retention worker identity is not authoritative".into(),
            ));
        }
        if self.batch_size == 0 || self.batch_size > LEGAL_RETENTION_MAX_BATCH_SIZE {
            return Err(LegalRetentionWorkerError::Configuration(format!(
                "Legal retention batch size must be 1..={LEGAL_RETENTION_MAX_BATCH_SIZE}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalRetentionRunReport {
    pub holding_id: String,
    pub data_profile_key: String,
    pub scanned: u32,
    pub expired: u32,
    pub conflicts: u32,
}

#[derive(Debug, Clone)]
pub struct LegalRetentionRunner {
    router: HoldingDatabaseRouter,
    request: LegalRetentionRequest,
}

impl LegalRetentionRunner {
    pub fn new(
        router: HoldingDatabaseRouter,
        request: LegalRetentionRequest,
    ) -> Result<Self, LegalRetentionWorkerError> {
        request.validate()?;
        Ok(Self { router, request })
    }

    pub const fn batch_size(&self) -> u32 {
        self.request.batch_size
    }

    pub fn request(&self) -> &LegalRetentionRequest {
        &self.request
    }

    /// Execute only against the routed holding write database. No read route,
    /// MinIO client, object deletion, or authorization decision exists in this
    /// runner; source persistence remains authoritative for the mutation.
    pub async fn run_once_at(
        &self,
        now: DateTime<Utc>,
    ) -> Result<LegalRetentionRunReport, LegalRetentionWorkerError> {
        let write = self
            .router
            .route(
                &self.request.holding_id,
                &self.request.data_profile_key,
                RoutedDatabaseSide::Write,
            )
            .await
            .map_err(|_| redacted_error("write routing"))?;
        let result = self.run_on_write_route(&write, now).await;
        write.close().await;
        result
    }

    async fn run_on_write_route(
        &self,
        write: &RoutedHoldingDatabase,
        now: DateTime<Utc>,
    ) -> Result<LegalRetentionRunReport, LegalRetentionWorkerError> {
        validate_write_route_binding(
            &self.request.holding_id,
            &self.request.data_profile_key,
            write.side,
            &write.holding_id,
            &write.data_profile_key,
        )?;
        let source = SqlxLegalPersistence::new(
            write.database_pool().clone(),
            self.request.data_profile_key.clone(),
        )
        .map_err(|_| redacted_error("source construction"))?;
        let report = source
            .expire_due_contracts_with_correlation(
                now,
                self.request.batch_size,
                self.request.context.correlation_id,
            )
            .await
            .map_err(|_| redacted_error("source expiry"))?;
        Ok(map_report(
            &self.request.holding_id,
            &self.request.data_profile_key,
            report,
        ))
    }
}

#[async_trait::async_trait]
impl NamedWorkerHandler for LegalRetentionRunner {
    fn name(&self) -> &str {
        LEGAL_RETENTION_HANDLER_NAME
    }

    async fn run_once(&self, worker_id: Uuid) -> Result<WorkerHandlerReport, WorkerRuntimeError> {
        validate_worker_id(worker_id)?;
        let report = self.run_once_at(Utc::now()).await?;
        let processed = u64::from(report.expired);
        Ok(WorkerHandlerReport {
            processed,
            has_work: report.scanned != 0,
            ..WorkerHandlerReport::default()
        })
    }
}

fn map_report(
    holding_id: &str,
    data_profile_key: &str,
    report: LegalRetentionSweepReport,
) -> LegalRetentionRunReport {
    LegalRetentionRunReport {
        holding_id: holding_id.into(),
        data_profile_key: data_profile_key.into(),
        scanned: report.scanned,
        expired: report.expired,
        conflicts: report.conflicts,
    }
}

fn validate_reference(value: &str, field: &str) -> Result<(), LegalRetentionWorkerError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
    {
        return Err(LegalRetentionWorkerError::Configuration(format!(
            "Legal retention {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_scope_reference(value: &str, field: &str) -> Result<(), LegalRetentionWorkerError> {
    validate_reference(value, field)?;
    if value.contains(':') || value.contains(' ') {
        return Err(LegalRetentionWorkerError::Configuration(format!(
            "Legal retention {field} is invalid"
        )));
    }
    Ok(())
}

fn validate_write_route_binding(
    expected_holding_id: &str,
    expected_data_profile_key: &str,
    actual_side: RoutedDatabaseSide,
    actual_holding_id: &str,
    actual_data_profile_key: &str,
) -> Result<(), LegalRetentionWorkerError> {
    if actual_side != RoutedDatabaseSide::Write
        || actual_holding_id != expected_holding_id
        || actual_data_profile_key != expected_data_profile_key
    {
        return Err(LegalRetentionWorkerError::Configuration(
            "Legal retention write route binding is invalid".into(),
        ));
    }
    Ok(())
}

fn redacted_error(operation: &'static str) -> LegalRetentionWorkerError {
    tracing::error!(operation, "Legal retention worker seam failed");
    LegalRetentionWorkerError::Retention { operation }
}

fn validate_worker_id(worker_id: Uuid) -> Result<(), WorkerRuntimeError> {
    if worker_id.is_nil() {
        return Err(WorkerRuntimeError::Configuration(
            "Legal retention worker_id is invalid".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> LegalRetentionWorkerContext {
        LegalRetentionWorkerContext::new(
            "system-worker:legal-retention",
            "expire legal contract retention",
            Uuid::new_v4(),
        )
        .expect("valid context")
    }

    #[test]
    fn request_rejects_unbounded_or_unsafe_values() {
        assert!(LegalRetentionRequest::new("holding", "holding", 0, context()).is_err());
        assert!(
            LegalRetentionRequest::new(
                "holding",
                "holding",
                LEGAL_RETENTION_MAX_BATCH_SIZE + 1,
                context(),
            )
            .is_err()
        );
        assert!(LegalRetentionRequest::new("holding/read", "holding", 1, context()).is_err());
    }

    #[test]
    fn request_rejects_non_authoritative_worker_identity() {
        let context = LegalRetentionWorkerContext::new(
            "other-worker",
            LEGAL_RETENTION_PURPOSE,
            Uuid::new_v4(),
        )
        .expect("bounded context");
        assert!(LegalRetentionRequest::new("holding", "holding", 1, context).is_err());

        let context = LegalRetentionWorkerContext::new(
            LEGAL_RETENTION_SYSTEM_ACTOR,
            "other-purpose",
            Uuid::new_v4(),
        )
        .expect("bounded context");
        assert!(LegalRetentionRequest::new("holding", "holding", 1, context).is_err());
    }

    #[test]
    fn context_rejects_nil_or_unbounded_values() {
        assert!(LegalRetentionWorkerContext::new("", "purpose", Uuid::new_v4()).is_err());
        assert!(LegalRetentionWorkerContext::new("actor", "purpose", Uuid::nil()).is_err());
    }

    #[test]
    fn report_maps_to_worker_counts_without_counting_conflicts_as_expired() {
        let report = map_report(
            "holding",
            "holding",
            LegalRetentionSweepReport {
                scanned: 4,
                expired: 2,
                conflicts: 2,
            },
        );
        assert_eq!(report.expired, 2);
        assert_eq!(report.conflicts, 2);
        assert_eq!(report.scanned, 4);
    }

    #[test]
    fn write_route_binding_is_scope_and_side_fenced() {
        assert!(
            validate_write_route_binding(
                "holding",
                "holding",
                RoutedDatabaseSide::Write,
                "holding",
                "holding",
            )
            .is_ok()
        );
        for (side, holding_id, data_profile_key) in [
            (RoutedDatabaseSide::Read, "holding", "holding"),
            (RoutedDatabaseSide::Write, "other-holding", "holding"),
            (RoutedDatabaseSide::Write, "holding", "other-holding"),
        ] {
            assert_eq!(
                validate_write_route_binding(
                    "holding",
                    "holding",
                    side,
                    holding_id,
                    data_profile_key
                ),
                Err(LegalRetentionWorkerError::Configuration(
                    "Legal retention write route binding is invalid".into()
                ))
            );
        }
    }

    #[test]
    fn worker_id_must_be_non_nil_before_routing_or_source_mutation() {
        assert!(validate_worker_id(Uuid::nil()).is_err());
        assert!(validate_worker_id(Uuid::new_v4()).is_ok());
    }
}
